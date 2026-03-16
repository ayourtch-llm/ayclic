use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use digest::Digest;
use md5::Md5;
use tokio::net::TcpListener;
use tracing::{debug, info};

use crate::error::CiscoIosError;

/// Determine the local IP address that can reach a given target.
/// Uses the UDP-connect trick: bind a UDP socket, "connect" to the target
/// (no actual traffic), and read back the local address the OS chose.
pub fn local_ip_for_target(target: &str) -> Result<String, CiscoIosError> {
    // Strip port if present, default to port 22 for the probe
    let host = target
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(target)
        .trim_matches(|c| c == '[' || c == ']');
    let probe_addr = format!("{}:22", host);

    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| CiscoIosError::HttpUploadError(format!("bind UDP: {}", e)))?;
    socket
        .connect(&probe_addr)
        .map_err(|e| CiscoIosError::HttpUploadError(format!("connect UDP to {}: {}", probe_addr, e)))?;
    let local_addr = socket
        .local_addr()
        .map_err(|e| CiscoIosError::HttpUploadError(format!("local_addr: {}", e)))?;

    Ok(local_addr.ip().to_string())
}

/// Start a one-shot HTTP server that serves `content` at any path.
/// Returns the (ip, port) the server is listening on, plus a shutdown handle.
/// The server runs as a background tokio task and shuts down after one GET.
pub async fn start_one_shot_http(
    content: Vec<u8>,
    bind_ip: &str,
) -> Result<(String, u16, tokio::task::JoinHandle<()>), CiscoIosError> {
    let bind_addr = format!("{}:0", bind_ip);
    let listener = TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| CiscoIosError::HttpUploadError(format!("bind TCP {}: {}", bind_addr, e)))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| CiscoIosError::HttpUploadError(format!("local_addr: {}", e)))?;
    let ip = local_addr.ip().to_string();
    let port = local_addr.port();

    info!("One-shot HTTP server listening on {}:{}", ip, port);

    let content = Arc::new(content);
    let content_len = content.len();

    // Shutdown signal: fires after first successful GET
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown_tx = Arc::new(tokio::sync::Mutex::new(Some(shutdown_tx)));

    let app = axum::Router::new()
        .fallback(get({
            let shutdown_tx = shutdown_tx.clone();
            move |State(data): State<Arc<Vec<u8>>>| {
                let shutdown_tx = shutdown_tx.clone();
                async move {
                    info!("HTTP: serving {} bytes", data.len());
                    // Signal shutdown after this response
                    if let Some(tx) = shutdown_tx.lock().await.take() {
                        let _ = tx.send(());
                    }
                    (
                        [(header::CONTENT_TYPE, "text/plain")],
                        (*data).clone(),
                    ).into_response()
                }
            }
        }))
        .with_state(content);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
                // Give the response time to fully flush
                tokio::time::sleep(Duration::from_secs(1)).await;
            })
            .await
            .ok();
        debug!("One-shot HTTP server shut down after serving {} bytes", content_len);
    });

    Ok((ip, port, handle))
}

/// Compute MD5 hex digest of a byte slice
pub fn md5_hex_bytes(data: &[u8]) -> String {
    let result = Md5::digest(data);
    format!("{:x}", result)
}

/// Compute MD5 hex digest of a string
pub fn md5_hex(data: &str) -> String {
    md5_hex_bytes(data.as_bytes())
}

/// Compute MD5 of the file content that TCL `puts` will produce.
/// Each line gets a trailing `\n` (TCL `puts` appends a newline).
pub fn md5_hex_as_flash_content(config: &str) -> String {
    let mut content = Vec::new();
    for line in config.lines() {
        content.extend_from_slice(line.as_bytes());
        content.push(b'\n');
    }
    md5_hex_bytes(&content)
}

/// Parse the MD5 hash from Cisco IOS `verify /md5` output.
///
/// The output looks like:
/// ```text
/// verify /md5 (flash:_ayclic_abc123.cfg) = d41d8cd98f00b204e9800998ecf8427e
/// ```
///
/// Returns the lowercase hex hash string.
pub fn parse_verify_md5(output: &str) -> Option<String> {
    // Look for "= " followed by a 32-char hex string
    for line in output.lines() {
        if let Some(pos) = line.find("= ") {
            let hash_part = line[pos + 2..].trim();
            // MD5 is 32 hex characters
            let hash: String = hash_part
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if hash.len() == 32 {
                return Some(hash.to_lowercase());
            }
        }
    }
    None
}

/// Escape a string for use inside TCL double quotes.
/// Escapes: backslash, double quote, dollar sign, square brackets.
pub fn tcl_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

/// Number of config lines to accumulate per TCL `append` command.
/// Each chunk is joined with `\n` inside a TCL string, so the device
/// only does in-memory string operations. The single `puts` at the end
/// does one flash write.
const TCL_BATCH_SIZE: usize = 20;

/// Build the sequence of TCL commands to write config lines to a flash file.
/// Returns a Vec of individual commands to be sent via run_cmd.
///
/// Strategy: accumulate the entire file content in a TCL variable using
/// `set`/`append` (fast in-memory ops), then write it all at once with
/// a single `puts` (one flash I/O).
pub fn build_tclsh_write_commands(filename: &str, config: &str) -> Vec<String> {
    let mut cmds = Vec::new();
    cmds.push("tclsh".to_string());

    // Build content in a TCL variable, batching lines with \n separators
    let lines: Vec<&str> = config.lines().collect();
    if lines.is_empty() {
        cmds.push(r#"set c """#.to_string());
    } else {
        for (i, chunk) in lines.chunks(TCL_BATCH_SIZE).enumerate() {
            let escaped: Vec<String> = chunk.iter().map(|l| tcl_escape(l)).collect();
            let joined = escaped.join(r"\n");
            if i == 0 {
                cmds.push(format!(r#"set c "{}""#, joined));
            } else {
                // Prepend \n to join with previous chunk
                cmds.push(format!(r#"append c "\n{}""#, joined));
            }
        }
    }

    // Write all at once — single flash I/O
    cmds.push(format!(r#"set fd [open "{}" w]"#, filename));
    cmds.push(r#"puts $fd $c"#.to_string());
    cmds.push("close $fd".to_string());
    cmds.push("unset c".to_string());
    cmds
}

/// Connection method for Cisco IOS devices
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionType {
    /// TELNET connection
    Telnet,
    /// SSH with password authentication
    Ssh,
    /// SSH with RSA public key authentication
    SshKey,
    /// SSH with keyboard-interactive authentication
    SshKbdInteractive,
}

/// Configuration for a CiscoIosConn
#[derive(Debug, Clone)]
pub struct CiscoIosConfig {
    pub target: String,
    pub conntype: ConnectionType,
    pub username: String,
    pub password: String,
    pub private_key: Option<Vec<u8>>,
    pub timeout: Duration,
    pub read_timeout: Duration,
}

impl Default for CiscoIosConfig {
    fn default() -> Self {
        Self {
            target: String::new(),
            conntype: ConnectionType::Ssh,
            username: String::new(),
            password: String::new(),
            private_key: None,
            timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(30),
        }
    }
}

/// Inner connection - either telnet or SSH
#[derive(Debug)]
enum InnerConn {
    Telnet(aytelnet::CiscoConn),
    Ssh(ayssh::CiscoConn),
}

/// Unified Cisco IOS connection supporting both Telnet and SSH
///
/// Provides a single type that can connect to Cisco IOS devices via
/// telnet, SSH password, SSH public key, or SSH keyboard-interactive
/// authentication. All methods share the same `run_cmd` / `disconnect` API.
#[derive(Debug)]
pub struct CiscoIosConn {
    config: CiscoIosConfig,
    inner: InnerConn,
}

impl CiscoIosConn {
    /// Create a new connection with password authentication and default timeouts (30s)
    ///
    /// Works for Telnet, Ssh, and SshKbdInteractive connection types.
    pub async fn new(
        target: &str,
        conntype: ConnectionType,
        username: &str,
        password: &str,
    ) -> Result<Self, CiscoIosError> {
        Self::with_timeouts(
            target,
            conntype,
            username,
            password,
            Duration::from_secs(30),
            Duration::from_secs(30),
        )
        .await
    }

    /// Create a new connection with password authentication and custom timeouts
    pub async fn with_timeouts(
        target: &str,
        conntype: ConnectionType,
        username: &str,
        password: &str,
        timeout: Duration,
        read_timeout: Duration,
    ) -> Result<Self, CiscoIosError> {
        if conntype == ConnectionType::SshKey {
            return Err(CiscoIosError::InvalidConnectionType(
                "SshKey requires new_with_key constructor".to_string(),
            ));
        }

        info!("Connecting to {} via {:?}", target, conntype);

        let inner = match conntype {
            ConnectionType::Telnet => {
                let conn = aytelnet::CiscoConn::with_timeouts(
                    target,
                    aytelnet::ConnectionType::CiscoTelnet,
                    username,
                    password,
                    timeout,
                    read_timeout,
                )
                .await?;
                InnerConn::Telnet(conn)
            }
            ConnectionType::Ssh => {
                let conn = ayssh::CiscoConn::with_timeouts(
                    target,
                    ayssh::ConnectionType::CiscoSsh,
                    username,
                    password,
                    timeout,
                    read_timeout,
                )
                .await?;
                InnerConn::Ssh(conn)
            }
            ConnectionType::SshKbdInteractive => {
                let conn = ayssh::CiscoConn::with_timeouts(
                    target,
                    ayssh::ConnectionType::CiscoSshKbdInteractive,
                    username,
                    password,
                    timeout,
                    read_timeout,
                )
                .await?;
                InnerConn::Ssh(conn)
            }
            ConnectionType::SshKey => unreachable!(),
        };

        debug!("Connected to {} successfully", target);

        Ok(Self {
            config: CiscoIosConfig {
                target: target.to_string(),
                conntype,
                username: username.to_string(),
                password: password.to_string(),
                private_key: None,
                timeout,
                read_timeout,
            },
            inner,
        })
    }

    /// Create a new connection with RSA public key authentication
    pub async fn new_with_key(
        target: &str,
        username: &str,
        private_key: &[u8],
    ) -> Result<Self, CiscoIosError> {
        info!("Connecting to {} via SSH key auth", target);

        let conn = ayssh::CiscoConn::new_with_key(target, username, private_key).await?;

        debug!("Connected to {} with key auth successfully", target);

        Ok(Self {
            config: CiscoIosConfig {
                target: target.to_string(),
                conntype: ConnectionType::SshKey,
                username: username.to_string(),
                password: String::new(),
                private_key: Some(private_key.to_vec()),
                timeout: Duration::from_secs(30),
                read_timeout: Duration::from_secs(30),
            },
            inner: InnerConn::Ssh(conn),
        })
    }

    /// Execute a command on the connected device and return its output
    pub async fn run_cmd(&mut self, cmd: &str) -> Result<String, CiscoIosError> {
        debug!("run_cmd on {}: {}", self.config.target, cmd);
        match &mut self.inner {
            InnerConn::Telnet(conn) => Ok(conn.run_cmd(cmd).await?),
            InnerConn::Ssh(conn) => Ok(conn.run_cmd(cmd).await?),
        }
    }

    /// Atomically apply a configuration snippet to the device.
    ///
    /// This method:
    /// 1. Computes an MD5 hash of the config for a unique temp filename
    /// 2. Spins up a one-shot HTTP server to serve the file content
    /// 3. Tells the device to `copy http://our_ip:port/file flash:<tempfile>`
    /// 4. Runs `verify /md5` to confirm file integrity
    /// 5. Only if the MD5 matches, applies with `copy flash:<tempfile> running-config`
    /// 6. Cleans up the temp file
    ///
    /// Returns the output of the copy command.
    pub async fn config_atomic(&mut self, config: &str) -> Result<String, CiscoIosError> {
        let expected_md5 = md5_hex_as_flash_content(config);
        let flash_file = format!("_ayclic_{}.cfg", expected_md5);
        let flash_path = format!("flash:{}", flash_file);
        info!(
            "config_atomic: uploading config to {} on {}",
            flash_path, self.config.target
        );

        // Build the file content (same as what tclsh puts would produce)
        let mut file_content = Vec::new();
        for line in config.lines() {
            file_content.extend_from_slice(line.as_bytes());
            file_content.push(b'\n');
        }

        // Determine our local IP reachable from the device
        let local_ip = local_ip_for_target(&self.config.target)?;
        info!("config_atomic: local IP for device is {}", local_ip);

        // Start one-shot HTTP server
        let (ip, port, http_handle) =
            start_one_shot_http(file_content, &local_ip).await?;
        let http_url = format!("http://{}:{}/{}", ip, port, flash_file);
        info!("config_atomic: serving config at {}", http_url);

        // Suppress interactive prompts for copy commands
        self.run_cmd("configure terminal").await?;
        self.run_cmd("file prompt quiet").await?;
        self.run_cmd("end").await?;

        // Download file from our HTTP server to flash
        let copy_cmd = format!("copy {} {}", http_url, flash_path);
        let copy_to_flash = self.run_cmd(&copy_cmd).await?;
        info!("copy to flash output: {}", copy_to_flash);

        // Wait for HTTP server to finish
        let _ = tokio::time::timeout(Duration::from_secs(5), http_handle).await;

        // Restore default file prompt behavior
        self.run_cmd("configure terminal").await?;
        self.run_cmd("no file prompt quiet").await?;
        self.run_cmd("end").await?;

        // Verify MD5 of the file on flash
        let verify_cmd = format!("verify /md5 {}", flash_path);
        let verify_output = self.run_cmd(&verify_cmd).await?;
        debug!("verify output: {}", verify_output);

        let actual_md5 = parse_verify_md5(&verify_output).ok_or_else(|| {
            CiscoIosError::Md5ParseError(verify_output.clone())
        })?;

        if actual_md5 != expected_md5 {
            // Keep the file on flash for investigation
            info!(
                "config_atomic: MD5 mismatch! Retaining {} on flash for debugging",
                flash_path
            );
            return Err(CiscoIosError::Md5Mismatch {
                expected: expected_md5,
                actual: actual_md5,
            });
        }

        info!("config_atomic: MD5 verified ({}), applying config", expected_md5);

        // Apply config: copy from flash to running-config
        self.run_cmd("configure terminal").await?;
        self.run_cmd("file prompt quiet").await?;
        self.run_cmd("end").await?;

        let copy_output = self
            .run_cmd(&format!("copy {} running-config", flash_path))
            .await?;

        self.run_cmd("configure terminal").await?;
        self.run_cmd("no file prompt quiet").await?;
        self.run_cmd("end").await?;

        // Clean up temp file from flash
        let delete_cmd = format!("delete /force {}", flash_path);
        self.run_cmd(&delete_cmd).await?;

        info!(
            "config_atomic: applied successfully on {}",
            self.config.target
        );
        Ok(copy_output)
    }

    /// Disconnect from the device
    pub async fn disconnect(&mut self) -> Result<(), CiscoIosError> {
        info!("Disconnecting from {}", self.config.target);
        match &mut self.inner {
            InnerConn::Telnet(conn) => Ok(conn.disconnect().await?),
            InnerConn::Ssh(conn) => Ok(conn.disconnect().await?),
        }
    }

    /// Get the target address
    pub fn target(&self) -> &str {
        &self.config.target
    }

    /// Get the username
    pub fn username(&self) -> &str {
        &self.config.username
    }

    /// Get the connection type
    pub fn conntype(&self) -> &ConnectionType {
        &self.config.conntype
    }

    /// Get the connection timeout
    pub fn timeout(&self) -> Duration {
        self.config.timeout
    }

    /// Get the read timeout
    pub fn read_timeout(&self) -> Duration {
        self.config.read_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === ConnectionType enum tests ===

    #[test]
    fn test_connection_type_equality() {
        assert_eq!(ConnectionType::Telnet, ConnectionType::Telnet);
        assert_eq!(ConnectionType::Ssh, ConnectionType::Ssh);
        assert_eq!(ConnectionType::SshKey, ConnectionType::SshKey);
        assert_eq!(
            ConnectionType::SshKbdInteractive,
            ConnectionType::SshKbdInteractive
        );
    }

    #[test]
    fn test_connection_type_inequality() {
        assert_ne!(ConnectionType::Telnet, ConnectionType::Ssh);
        assert_ne!(ConnectionType::Ssh, ConnectionType::SshKey);
        assert_ne!(ConnectionType::SshKey, ConnectionType::SshKbdInteractive);
        assert_ne!(ConnectionType::Telnet, ConnectionType::SshKbdInteractive);
    }

    #[test]
    fn test_connection_type_clone() {
        let ct = ConnectionType::Ssh;
        let ct2 = ct.clone();
        assert_eq!(ct, ct2);
    }

    #[test]
    fn test_connection_type_debug() {
        let s = format!("{:?}", ConnectionType::Telnet);
        assert_eq!(s, "Telnet");
        let s = format!("{:?}", ConnectionType::Ssh);
        assert_eq!(s, "Ssh");
        let s = format!("{:?}", ConnectionType::SshKey);
        assert_eq!(s, "SshKey");
        let s = format!("{:?}", ConnectionType::SshKbdInteractive);
        assert_eq!(s, "SshKbdInteractive");
    }

    // === CiscoIosConfig tests ===

    #[test]
    fn test_config_default() {
        let config = CiscoIosConfig::default();
        assert_eq!(config.target, "");
        assert_eq!(config.conntype, ConnectionType::Ssh);
        assert_eq!(config.username, "");
        assert_eq!(config.password, "");
        assert!(config.private_key.is_none());
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.read_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_config_clone() {
        let config = CiscoIosConfig {
            target: "192.168.1.1".to_string(),
            conntype: ConnectionType::Ssh,
            username: "admin".to_string(),
            password: "secret".to_string(),
            private_key: None,
            timeout: Duration::from_secs(60),
            read_timeout: Duration::from_secs(10),
        };
        let config2 = config.clone();
        assert_eq!(config.target, config2.target);
        assert_eq!(config.conntype, config2.conntype);
        assert_eq!(config.username, config2.username);
        assert_eq!(config.password, config2.password);
        assert_eq!(config.timeout, config2.timeout);
        assert_eq!(config.read_timeout, config2.read_timeout);
    }

    #[test]
    fn test_config_with_private_key() {
        let config = CiscoIosConfig {
            private_key: Some(vec![1, 2, 3]),
            ..Default::default()
        };
        assert_eq!(config.private_key, Some(vec![1, 2, 3]));
    }

    // === Constructor validation tests (SshKey via new() should fail) ===

    #[tokio::test]
    async fn test_new_rejects_ssh_key_type() {
        let result = CiscoIosConn::new("192.168.1.1", ConnectionType::SshKey, "admin", "pass").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            CiscoIosError::InvalidConnectionType(msg) => {
                assert!(msg.contains("new_with_key"));
            }
            other => panic!("Expected InvalidConnectionType, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_with_timeouts_rejects_ssh_key_type() {
        let result = CiscoIosConn::with_timeouts(
            "192.168.1.1",
            ConnectionType::SshKey,
            "admin",
            "pass",
            Duration::from_secs(10),
            Duration::from_secs(5),
        )
        .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::InvalidConnectionType(_) => {}
            other => panic!("Expected InvalidConnectionType, got: {:?}", other),
        }
    }

    // === Connection tests (these will fail to connect without a real device,
    //     but verify the API accepts the right parameters and returns
    //     the expected error type) ===

    #[tokio::test]
    async fn test_telnet_connection_returns_telnet_error() {
        let result =
            CiscoIosConn::new("127.0.0.1:19999", ConnectionType::Telnet, "admin", "pass").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::Telnet(_) => {} // correct error variant
            other => panic!("Expected Telnet error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_ssh_connection_returns_ssh_error() {
        let result =
            CiscoIosConn::new("127.0.0.1:19999", ConnectionType::Ssh, "admin", "pass").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::Ssh(_) => {} // correct error variant
            other => panic!("Expected Ssh error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_ssh_kbd_interactive_returns_ssh_error() {
        let result = CiscoIosConn::new(
            "127.0.0.1:19999",
            ConnectionType::SshKbdInteractive,
            "admin",
            "pass",
        )
        .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::Ssh(_) => {}
            other => panic!("Expected Ssh error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_ssh_key_connection_returns_ssh_error() {
        let fake_key = b"not a real key";
        let result =
            CiscoIosConn::new_with_key("127.0.0.1:19999", "admin", fake_key).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::Ssh(_) => {}
            other => panic!("Expected Ssh error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_with_custom_timeouts_returns_telnet_error() {
        let result = CiscoIosConn::with_timeouts(
            "127.0.0.1:19999",
            ConnectionType::Telnet,
            "admin",
            "pass",
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::Telnet(_) => {}
            other => panic!("Expected Telnet error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_with_custom_timeouts_ssh_returns_ssh_error() {
        let result = CiscoIosConn::with_timeouts(
            "127.0.0.1:19999",
            ConnectionType::Ssh,
            "admin",
            "pass",
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::Ssh(_) => {}
            other => panic!("Expected Ssh error, got: {:?}", other),
        }
    }

    // === MD5 helper tests ===

    #[test]
    fn test_md5_hex_empty_string() {
        assert_eq!(md5_hex(""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_md5_hex_hello() {
        assert_eq!(md5_hex("hello"), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_md5_hex_deterministic() {
        let config = "interface loopback1\n ip address 192.0.2.1 255.255.255.0\n";
        assert_eq!(md5_hex(config), md5_hex(config));
    }

    #[test]
    fn test_md5_hex_different_inputs() {
        assert_ne!(md5_hex("config1"), md5_hex("config2"));
    }

    #[test]
    fn test_md5_hex_as_flash_content_single_line() {
        // "hello" -> file contains "hello\n"
        assert_eq!(md5_hex_as_flash_content("hello"), md5_hex_bytes(b"hello\n"));
    }

    #[test]
    fn test_md5_hex_as_flash_content_multi_line() {
        let config = "interface loopback1\n ip address 192.0.2.1 255.255.255.0";
        // Each line gets \n appended by puts
        let expected_bytes = b"interface loopback1\n ip address 192.0.2.1 255.255.255.0\n";
        assert_eq!(
            md5_hex_as_flash_content(config),
            md5_hex_bytes(expected_bytes)
        );
    }

    #[test]
    fn test_md5_hex_as_flash_content_trailing_newline() {
        // Input with trailing newline: lines() will produce an empty last element
        // only if there are chars after the last \n. "a\n" -> lines() yields ["a"]
        let config = "line1\nline2\n";
        // lines() on "line1\nline2\n" yields ["line1", "line2"]
        let expected_bytes = b"line1\nline2\n";
        assert_eq!(
            md5_hex_as_flash_content(config),
            md5_hex_bytes(expected_bytes)
        );
    }

    // === parse_verify_md5 tests ===

    #[test]
    fn test_parse_verify_md5_typical_output() {
        let output =
            r#"verify /md5 (flash:_ayclic_abc123.cfg) = d41d8cd98f00b204e9800998ecf8427e"#;
        assert_eq!(
            parse_verify_md5(output),
            Some("d41d8cd98f00b204e9800998ecf8427e".to_string())
        );
    }

    #[test]
    fn test_parse_verify_md5_with_surrounding_output() {
        let output = "some preamble\nverify /md5 (flash:test.cfg) = ABCDEF1234567890abcdef1234567890\nSomeRouter#";
        assert_eq!(
            parse_verify_md5(output),
            Some("abcdef1234567890abcdef1234567890".to_string())
        );
    }

    #[test]
    fn test_parse_verify_md5_no_match() {
        assert_eq!(parse_verify_md5("no hash here"), None);
    }

    #[test]
    fn test_parse_verify_md5_truncated_hash() {
        let output = "verify /md5 (flash:test.cfg) = abcdef12";
        assert_eq!(parse_verify_md5(output), None); // too short
    }

    #[test]
    fn test_parse_verify_md5_uppercase_normalized() {
        let output = "verify /md5 (flash:test.cfg) = D41D8CD98F00B204E9800998ECF8427E";
        assert_eq!(
            parse_verify_md5(output),
            Some("d41d8cd98f00b204e9800998ecf8427e".to_string())
        );
    }

    // === TCL escape tests ===

    #[test]
    fn test_tcl_escape_simple() {
        assert_eq!(tcl_escape("simple text"), "simple text");
    }

    #[test]
    fn test_tcl_escape_double_quotes() {
        assert_eq!(tcl_escape(r#"has "quotes""#), r#"has \"quotes\""#);
    }

    #[test]
    fn test_tcl_escape_dollar() {
        assert_eq!(tcl_escape("has $var"), "has \\$var");
    }

    #[test]
    fn test_tcl_escape_brackets() {
        assert_eq!(tcl_escape("has [cmd]"), "has \\[cmd\\]");
    }

    #[test]
    fn test_tcl_escape_backslash() {
        assert_eq!(tcl_escape("has \\backslash"), "has \\\\backslash");
    }

    #[test]
    fn test_tcl_escape_cisco_config_line() {
        // Typical Cisco config lines should pass through mostly unchanged
        assert_eq!(
            tcl_escape(" ip address 192.0.2.1 255.255.255.0"),
            " ip address 192.0.2.1 255.255.255.0"
        );
        assert_eq!(tcl_escape("interface loopback1"), "interface loopback1");
        assert_eq!(tcl_escape("!"), "!");
    }

    // === build_tclsh_write_commands tests ===

    #[test]
    fn test_build_tclsh_write_commands_basic() {
        let config = "interface loopback1\n ip address 192.0.2.1 255.255.255.0\n!";
        let cmds = build_tclsh_write_commands("flash:test.cfg", config);

        assert_eq!(cmds[0], "tclsh");
        // 3 lines fit in one batch -> single set command with \n separators
        assert_eq!(
            cmds[1],
            r#"set c "interface loopback1\n ip address 192.0.2.1 255.255.255.0\n!""#
        );
        assert_eq!(cmds[2], r#"set fd [open "flash:test.cfg" w]"#);
        assert_eq!(cmds[3], r#"puts $fd $c"#);
        assert_eq!(cmds[4], "close $fd");
        assert_eq!(cmds[5], "unset c");
        assert_eq!(cmds.len(), 6);
    }

    #[test]
    fn test_build_tclsh_write_commands_empty_config() {
        let cmds = build_tclsh_write_commands("flash:test.cfg", "");
        assert_eq!(cmds[0], "tclsh");
        assert_eq!(cmds[1], r#"set c """#);
        assert_eq!(cmds[2], r#"set fd [open "flash:test.cfg" w]"#);
        assert_eq!(cmds[3], r#"puts $fd $c"#);
        assert_eq!(cmds[4], "close $fd");
        assert_eq!(cmds[5], "unset c");
        assert_eq!(cmds.len(), 6);
    }

    #[test]
    fn test_build_tclsh_write_commands_escapes_special_chars() {
        let config = r#"description has "quotes" and $vars"#;
        let cmds = build_tclsh_write_commands("flash:test.cfg", config);
        assert_eq!(
            cmds[1],
            r#"set c "description has \"quotes\" and \$vars""#
        );
    }

    #[test]
    fn test_build_tclsh_write_commands_batches_large_config() {
        // Create config with more lines than TCL_BATCH_SIZE
        let lines: Vec<String> = (0..250).map(|i| format!("line {}", i)).collect();
        let config = lines.join("\n");
        let cmds = build_tclsh_write_commands("flash:test.cfg", &config);

        // 250 lines / 20 per batch = 13 batches (1 set + 12 append)
        // Total: tclsh + 13 batches + open + puts + close + unset = 18
        assert_eq!(cmds.len(), 18);
        assert_eq!(cmds[0], "tclsh");
        // First batch: set c "..."
        assert!(cmds[1].starts_with(r#"set c ""#));
        // Subsequent batches: append c "\n..."
        for i in 2..14 {
            assert!(cmds[i].starts_with(r#"append c "\n"#), "cmd[{}] = {}", i, cmds[i]);
        }
        // Then: open, puts, close, unset
        assert!(cmds[14].starts_with(r#"set fd [open"#));
        assert_eq!(cmds[15], r#"puts $fd $c"#);
        assert_eq!(cmds[16], "close $fd");
        assert_eq!(cmds[17], "unset c");
    }

    #[test]
    fn test_flash_temp_filename_format() {
        let hash = md5_hex("test config");
        let filename = format!("flash:_ayclic_{}.cfg", hash);
        assert!(filename.starts_with("flash:_ayclic_"));
        assert!(filename.ends_with(".cfg"));
        assert_eq!(filename.len(), "flash:_ayclic_.cfg".len() + 32); // 32 hex chars for MD5
    }

    // === Error type tests ===

    #[test]
    fn test_error_display_invalid_connection_type() {
        let err = CiscoIosError::InvalidConnectionType("test".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("test"));
        assert!(msg.contains("Invalid connection type"));
    }

    #[test]
    fn test_error_display_not_connected() {
        let err = CiscoIosError::NotConnected;
        let msg = format!("{}", err);
        assert!(msg.contains("Not connected"));
    }
}
