use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use aho_corasick::AhoCorasick;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use digest::Digest;
use md5::Md5;
use tokio::net::TcpListener;
use tracing::{debug, info};

use crate::error::CiscoIosError;
use crate::generic_conn::GenericCliConn;
use crate::path::{ConnectionPath, Hop, TransportSpec};
use crate::raw_transport::SshAuth;
use crate::transport::{
    ios_prompt_actions, receive_until_match, run_interactive, CiscoTransport, PromptAction,
    SshTransport, TelnetTransport,
};

/// Adapter that wraps a `Box<dyn RawTransport>` as a `CiscoTransport`.
/// This bridges the new vendor-neutral transport layer with the existing
/// Cisco-specific code that expects `CiscoTransport`.
struct RawTransportAdapter(Box<dyn crate::raw_transport::RawTransport>);

impl std::fmt::Debug for RawTransportAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawTransportAdapter")
            .field("inner", &self.0)
            .finish()
    }
}

#[async_trait::async_trait]
impl CiscoTransport for RawTransportAdapter {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.0.send(data).await
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        self.0.receive(timeout).await
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.0.close().await
    }
}

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

/// Start an HTTP server that serves `content` at `/<filename>`, and shuts
/// down when a GET to `/<filename>/done` is received. The file-specific
/// `/done` path prevents easy guessing. The IOS device can make multiple
/// HTTP requests (probe + download), and the caller signals completion by
/// having the device `copy http://.../<filename>/done null:`.
///
/// Returns `(ip, port, join_handle)`. The server runs until the `/done`
/// endpoint is hit or the JoinHandle is aborted.
pub async fn start_config_http(
    content: Vec<u8>,
    bind_ip: &str,
    filename: &str,
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

    info!("Config HTTP server listening on {}:{}", ip, port);

    let content = Arc::new(content);
    let content_len = content.len();

    // Shutdown signal: fires when /<filename>/done is requested
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown_tx = Arc::new(tokio::sync::Mutex::new(Some(shutdown_tx)));

    let done_route = format!("/{}/done", filename);
    let serve_route = format!("/{}", filename);

    let app = axum::Router::new()
        .route(&done_route, get({
            let shutdown_tx = shutdown_tx.clone();
            move || {
                let shutdown_tx = shutdown_tx.clone();
                async move {
                    info!("HTTP: /done requested, shutting down server");
                    if let Some(tx) = shutdown_tx.lock().await.take() {
                        let _ = tx.send(());
                    }
                    (
                        [(header::CONTENT_TYPE, "text/plain")],
                        "done\n",
                    ).into_response()
                }
            }
        }))
        .route(&serve_route, get({
            move |State(data): State<Arc<Vec<u8>>>| {
                async move {
                    info!("HTTP: serving {} bytes", data.len());
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
            })
            .await
            .ok();
        debug!("Config HTTP server shut down after serving {} bytes", content_len);
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

/// Safety mechanism for config changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeSafety {
    /// No safety mechanism — apply config directly.
    None,
    /// Schedule a reload before applying config. If the device becomes
    /// unreachable after the change (bad config), it will reload after
    /// the specified minutes and revert to the saved startup-config.
    /// After a successful apply, the reload is automatically cancelled.
    DelayedReload { minutes: u32 },
}

/// Initialize a Cisco IOS session: send `term len 0` and wait for prompt.
/// Used after transport-level connection + auth is complete.
#[allow(dead_code)]
async fn ios_init(
    transport: &mut dyn CiscoTransport,
    read_timeout: Duration,
) -> Result<(), CiscoIosError> {
    let prompt = AhoCorasick::new(&["#"]).unwrap();
    transport.send(b"term len 0\n").await?;
    // Wait for prompt (ignore the output)
    let _ = receive_until_match(transport, &prompt, read_timeout, vec![]).await;
    Ok(())
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

/// Unified Cisco IOS connection supporting both Telnet and SSH
///
/// Provides a single type that can connect to Cisco IOS devices via
/// telnet, SSH password, SSH public key, or SSH keyboard-interactive
/// authentication. All methods share the same `run_cmd` / `disconnect` API.
///
/// Internally uses the `CiscoTransport` trait for a unified send/receive
/// codepath across all protocols. Pattern matching uses aho-corasick
/// for efficient multi-pattern detection.
pub struct CiscoIosConn {
    config: CiscoIosConfig,
    transport: Box<dyn CiscoTransport>,
}

impl std::fmt::Debug for CiscoIosConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CiscoIosConn")
            .field("config", &self.config)
            .field("transport", &self.transport)
            .finish()
    }
}

impl CiscoIosConn {
    /// Create a new connection with password authentication and default timeouts (30s).
    ///
    /// Uses the template-driven ConnectionPath architecture internally.
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

    /// Create a new connection with password authentication and custom timeouts.
    ///
    /// Uses the template-driven ConnectionPath architecture internally.
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

        info!("Connecting to {} via {:?} (template-driven)", target, conntype);

        // Parse target into SocketAddr
        let addr: std::net::SocketAddr = Self::parse_target(target, &conntype)?;

        // Build the ConnectionPath based on connection type
        let mut hops: Vec<Hop> = Vec::new();

        match conntype {
            ConnectionType::Telnet => {
                hops.push(Hop::Transport(TransportSpec::Telnet { target: addr }));
                hops.push(Hop::Interactive(
                    aytextfsmplus::TextFSMPlus::from_str(crate::templates::CISCO_IOS_TELNET_LOGIN)
                        .with_preset("Username", username)
                        .with_preset("Password", password),
                ));
            }
            ConnectionType::Ssh => {
                hops.push(Hop::Transport(TransportSpec::Ssh {
                    target: addr,
                    auth: SshAuth::Password {
                        username: username.to_string(),
                        password: password.to_string(),
                    },
                }));
                hops.push(Hop::Interactive(
                    aytextfsmplus::TextFSMPlus::from_str(
                        crate::templates::CISCO_IOS_SSH_POST_LOGIN,
                    ),
                ));
            }
            ConnectionType::SshKbdInteractive => {
                hops.push(Hop::Transport(TransportSpec::Ssh {
                    target: addr,
                    auth: SshAuth::KbdInteractive {
                        username: username.to_string(),
                        password: password.to_string(),
                    },
                }));
                hops.push(Hop::Interactive(
                    aytextfsmplus::TextFSMPlus::from_str(
                        crate::templates::CISCO_IOS_SSH_POST_LOGIN,
                    ),
                ));
            }
            ConnectionType::SshKey => unreachable!(),
        }

        let path = ConnectionPath::new(hops).with_timeout(timeout);
        let conn = GenericCliConn::connect(
            path,
            &aytextfsmplus::NoVars,
            &aytextfsmplus::NoFuncs,
        )
        .await?
        .with_prompt_template(crate::templates::CISCO_IOS_PROMPT)
        .with_cmd_timeout(read_timeout);

        debug!("Connected to {} successfully (template-driven)", target);

        Ok(Self::from_generic(conn, target))
    }

    /// Legacy: create a new connection using the old Cisco-specific
    /// transport layers (CiscoTelnet, ayssh::CiscoConn) directly.
    ///
    /// This bypasses the template-driven ConnectionPath architecture.
    /// Use `new()` instead for the template-driven path.
    pub async fn new_legacy(
        target: &str,
        conntype: ConnectionType,
        username: &str,
        password: &str,
    ) -> Result<Self, CiscoIosError> {
        Self::with_timeouts_legacy(
            target,
            conntype,
            username,
            password,
            Duration::from_secs(30),
            Duration::from_secs(30),
        )
        .await
    }

    /// Legacy: create with custom timeouts using the old transport layers.
    pub async fn with_timeouts_legacy(
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

        info!("Connecting to {} via {:?} (legacy)", target, conntype);

        let transport: Box<dyn CiscoTransport> = match conntype {
            ConnectionType::Telnet => {
                let mut client = aytelnet::CiscoTelnet::new(target, username, password)
                    .with_timeout(timeout)
                    .with_read_timeout(read_timeout)
                    .with_prompt("Router#")
                    .with_prompt("Switch#")
                    .with_prompt("config#")
                    .with_prompt("cli#");
                client.connect().await.map_err(CiscoIosError::Telnet)?;
                Box::new(TelnetTransport(client))
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
                Box::new(SshTransport(conn))
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
                Box::new(SshTransport(conn))
            }
            ConnectionType::SshKey => unreachable!(),
        };

        debug!("Connected to {} successfully (legacy)", target);

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
            transport,
        })
    }

    /// Parse a target string into a SocketAddr, adding default port
    /// based on connection type.
    fn parse_target(
        target: &str,
        conntype: &ConnectionType,
    ) -> Result<std::net::SocketAddr, CiscoIosError> {
        let default_port = match conntype {
            ConnectionType::Telnet => 23,
            _ => 22,
        };

        // Try parsing as-is first (might already have port)
        if let Ok(addr) = target.parse::<std::net::SocketAddr>() {
            return Ok(addr);
        }

        // Try as [IPv6]:port
        if let Ok(addr) = target.parse::<std::net::SocketAddr>() {
            return Ok(addr);
        }

        // Try adding default port
        let with_port = if target.contains(':') && !target.contains('[') {
            // Looks like IPv6 without port — wrap in brackets
            format!("[{}]:{}", target, default_port)
        } else if target.contains(':') {
            // Already has port or is bracketed IPv6
            target.to_string()
        } else {
            // Plain hostname or IPv4
            format!("{}:{}", target, default_port)
        };

        with_port.parse::<std::net::SocketAddr>().map_err(|e| {
            CiscoIosError::InvalidConnectionType(format!(
                "Cannot parse target '{}' as address: {}",
                target, e
            ))
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
            transport: Box::new(SshTransport(conn)),
        })
    }

    /// Create from a `GenericCliConn`.
    ///
    /// The connection should already be authenticated and at a device
    /// prompt. This constructor bridges the vendor-neutral `GenericCliConn`
    /// with the Cisco-specific `CiscoIosConn` functionality.
    pub fn from_generic(conn: GenericCliConn, target: &str) -> Self {
        let transport = conn.into_transport();
        Self {
            config: CiscoIosConfig {
                target: target.to_string(),
                ..Default::default()
            },
            transport: Box::new(RawTransportAdapter(transport)),
        }
    }

    /// Connect via a `ConnectionPath` (multi-hop, template-driven).
    ///
    /// The path should include all necessary Transport and Interactive
    /// hops to reach the device and authenticate. After the path
    /// completes, the device should be at a privileged prompt (`#`).
    ///
    /// Example:
    /// ```ignore
    /// let path = ConnectionPath::new(vec![
    ///     Hop::Transport(TransportSpec::Ssh { target, auth }),
    ///     Hop::Interactive(cisco_login_template),
    /// ]);
    /// let conn = CiscoIosConn::from_path(path, "10.1.1.1", &NoVars, &NoFuncs).await?;
    /// ```
    pub async fn from_path(
        path: ConnectionPath,
        target: &str,
        vars: &(impl aycalc::GetVar + Sync),
        funcs: &(impl aycalc::CallFunc + Sync),
    ) -> Result<Self, CiscoIosError> {
        let conn = GenericCliConn::connect(path, vars, funcs).await?;
        Ok(Self::from_generic(conn, target))
    }

    /// Execute a command on the connected device and return its output.
    ///
    /// Sends the command, then waits for the `#` prompt using aho-corasick
    /// pattern matching over the transport. This is the single unified
    /// codepath for both telnet and SSH.
    pub async fn run_cmd(&mut self, cmd: &str) -> Result<String, CiscoIosError> {
        debug!("run_cmd on {}: {}", self.config.target, cmd);
        let prompt = AhoCorasick::new(&["#"]).unwrap();
        self.transport
            .send(format!("{}\n", cmd).as_bytes())
            .await?;
        let (data, _) = receive_until_match(
            self.transport.as_mut(),
            &prompt,
            self.config.read_timeout,
            vec![],
        )
        .await?;
        String::from_utf8(data)
            .map_err(|e| CiscoIosError::HttpUploadError(format!("Invalid UTF-8: {}", e)))
    }

    /// Execute an interactive command, auto-responding to IOS prompts.
    ///
    /// Like `run_cmd`, but also handles intermediate prompts such as
    /// `]?`, `[confirm]`, `(yes/no)` by automatically sending the
    /// appropriate response. Useful for `copy`, `delete`, etc.
    ///
    /// Custom prompt/response pairs can be provided; if `None`, uses
    /// the standard IOS confirmation prompts.
    pub async fn run_cmd_chat(
        &mut self,
        cmd: &str,
        extra_prompts: Option<&[(&str, PromptAction)]>,
    ) -> Result<String, CiscoIosError> {
        debug!("run_cmd_chat on {}: {}", self.config.target, cmd);
        let prompts = match extra_prompts {
            Some(custom) => custom.to_vec(),
            None => ios_prompt_actions(),
        };
        run_interactive(
            self.transport.as_mut(),
            cmd,
            &prompts,
            self.config.read_timeout,
        )
        .await
    }

    /// Atomically apply a configuration snippet to the device.
    ///
    /// This method:
    /// 1. Optionally schedules a safety reload (`ChangeSafety::DelayedReload`)
    /// 2. Computes an MD5 hash of the config for a unique temp filename
    /// 3. Spins up a one-shot HTTP server to serve the file content
    /// 4. Tells the device to `copy http://our_ip:port/file flash:<tempfile>`
    /// 5. Runs `verify /md5` to confirm file integrity
    /// 6. Only if the MD5 matches, applies with `copy flash:<tempfile> running-config`
    /// 7. Cleans up the temp file
    /// 8. On success with `DelayedReload`, cancels the scheduled reload
    ///
    /// Returns the output of the copy command.
    pub async fn config_atomic(
        &mut self,
        config: &str,
        safety: ChangeSafety,
    ) -> Result<String, CiscoIosError> {
        // Schedule a safety reload if requested
        if let ChangeSafety::DelayedReload { minutes } = &safety {
            info!(
                "config_atomic: scheduling safety reload in {} minutes on {}",
                minutes, self.config.target
            );
            // reload in N — IOS may prompt "Save? [yes/no]:" and "[confirm]"
            // Answer "no" to save (we want to revert on reload) and confirm
            let reload_prompts = vec![
                ("#", PromptAction::Done),
                ("[yes/no]", PromptAction::Respond(b"no\n".to_vec())),
                ("[confirm]", PromptAction::Respond(b"\n".to_vec())),
                ("]?", PromptAction::Respond(b"\n".to_vec())),
            ];
            let reload_output = run_interactive(
                self.transport.as_mut(),
                &format!("reload in {}", minutes),
                &reload_prompts,
                self.config.read_timeout,
            )
            .await?;
            info!("config_atomic: reload scheduled: {}", reload_output.trim());
        }

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

        // Start HTTP server (stays alive until /<filename>/done is requested)
        let (ip, port, http_handle) =
            start_config_http(file_content, &local_ip, &flash_file).await?;
        let http_url = format!("http://{}:{}/{}", ip, port, flash_file);
        let done_url = format!("http://{}:{}/{}/done", ip, port, flash_file);
        info!("config_atomic: serving config at {}", http_url);

        // Download file from our HTTP server to flash, with retry
        const MAX_COPY_ATTEMPTS: u32 = 3;
        let mut _copy_to_flash = String::new();
        let mut last_err: Option<CiscoIosError> = None;

        for attempt in 1..=MAX_COPY_ATTEMPTS {
            let copy_cmd = format!("copy {} {}", http_url, flash_path);
            match self.run_cmd_chat(&copy_cmd, None).await {
                Ok(output) => {
                    info!("copy to flash output (attempt {}): {}", attempt, output);
                    if output.contains("%Error") {
                        info!(
                            "config_atomic: copy attempt {}/{} failed: device reported error",
                            attempt, MAX_COPY_ATTEMPTS
                        );
                        last_err = Some(CiscoIosError::HttpUploadError(format!(
                            "Device copy error: {}",
                            output.trim()
                        )));
                        if attempt < MAX_COPY_ATTEMPTS {
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                    } else {
                        _copy_to_flash = output;
                        last_err = None;
                        break;
                    }
                }
                Err(e) => {
                    info!(
                        "config_atomic: copy attempt {}/{} failed: {}",
                        attempt, MAX_COPY_ATTEMPTS, e
                    );
                    last_err = Some(e);
                    if attempt < MAX_COPY_ATTEMPTS {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }

        if let Some(err) = last_err {
            info!("config_atomic: all copy attempts failed, shutting down HTTP server");
            http_handle.abort();
            return Err(err);
        }

        // Verify MD5 of the file on flash
        let verify_cmd = format!("verify /md5 {}", flash_path);
        let verify_output = self.run_cmd(&verify_cmd).await?;
        debug!("verify output: {}", verify_output);

        let actual_md5 = match parse_verify_md5(&verify_output) {
            Some(md5) => md5,
            None => {
                http_handle.abort();
                return Err(CiscoIosError::Md5ParseError(verify_output));
            }
        };

        if actual_md5 != expected_md5 {
            // Keep the file on flash for investigation
            info!(
                "config_atomic: MD5 mismatch! Retaining {} on flash for debugging",
                flash_path
            );
            http_handle.abort();
            return Err(CiscoIosError::Md5Mismatch {
                expected: expected_md5,
                actual: actual_md5,
            });
        }

        info!("config_atomic: MD5 verified ({}), applying config", expected_md5);

        // Shut down HTTP server cleanly via /done endpoint
        let done_cmd = format!("copy {} null:", done_url);
        let _ = self.run_cmd_chat(&done_cmd, None).await;
        let _ = tokio::time::timeout(Duration::from_secs(5), http_handle).await;

        // Apply config: copy from flash to running-config
        let copy_output = self
            .run_cmd_chat(&format!("copy {} running-config", flash_path), None)
            .await?;

        // Clean up temp file from flash
        let delete_cmd = format!("delete /force {}", flash_path);
        self.run_cmd(&delete_cmd).await?;

        // Cancel the safety reload if one was scheduled
        if let ChangeSafety::DelayedReload { .. } = &safety {
            info!("config_atomic: cancelling safety reload on {}", self.config.target);
            let cancel_output = self.run_cmd("reload cancel").await?;
            info!("config_atomic: reload cancelled: {}", cancel_output.trim());
        }

        info!(
            "config_atomic: applied successfully on {}",
            self.config.target
        );
        Ok(copy_output)
    }

    /// Disconnect from the device
    pub async fn disconnect(&mut self) -> Result<(), CiscoIosError> {
        info!("Disconnecting from {}", self.config.target);
        self.transport.close().await
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
