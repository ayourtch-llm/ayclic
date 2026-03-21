//! Connection path: multi-hop, template-driven device access.
//!
//! A `ConnectionPath` is an ordered list of `Hop`s that describe how to
//! reach a device. Each hop is either a `Transport` (opens a new byte
//! stream) or an `Interactive` (drives text-based interaction on the
//! current stream using a TextFSMPlus template).
//!
//! # Example
//!
//! ```ignore
//! use ayclic::path::*;
//! use aytextfsmplus::TextFSMPlus;
//!
//! let path = ConnectionPath::new(vec![
//!     // SSH to bastion with pubkey
//!     Hop::Transport(TransportSpec::Ssh {
//!         target: "10.1.1.1:22".parse()?,
//!         auth: SshAuth::PubKey { .. },
//!     }),
//!     // In bastion shell, login to device via template
//!     Hop::Interactive(
//!         TextFSMPlus::from_file("ssh_jump.textfsm")
//!             .with_preset("TargetHost", "10.200.0.5")
//!     ),
//! ]);
//!
//! let established = path.connect(&NoVars, &NoFuncs).await?;
//! // established.transport is ready for commands
//! ```

use std::net::SocketAddr;
use std::time::Duration;

use aycalc::{CallFunc, GetVar};
use tracing::{debug, info, error};

use crate::error::CiscoIosError;
use crate::raw_transport::{RawTelnetTransport, RawSshTransport, RawTransport, SshAuth};

use aytextfsmplus::{InteractiveAction, TextFSMPlus};

/// Default timeout for interactive feed() loops (30 seconds).
const DEFAULT_INTERACTIVE_TIMEOUT_SECS: u64 = 30;

/// Specification for opening a new byte stream.
#[derive(Debug, Clone)]
pub enum TransportSpec {
    /// Connect via Telnet (no protocol-level auth).
    Telnet { target: SocketAddr },
    /// Connect via SSH (protocol-level auth included).
    Ssh { target: SocketAddr, auth: SshAuth },
}

/// A single step in reaching a device.
#[derive(Debug)]
pub enum Hop {
    /// Open a new byte stream (TCP + protocol handshake).
    Transport(TransportSpec),
    /// Drive text-based interaction on the current stream.
    /// The TextFSMPlus template handles login, prompts, enable, etc.
    Interactive(TextFSMPlus),
}

/// An ordered list of hops describing how to reach a device.
#[derive(Debug)]
pub struct ConnectionPath {
    pub hops: Vec<Hop>,
    /// Timeout for each interactive feed() loop.
    pub interactive_timeout: Duration,
}

impl ConnectionPath {
    pub fn new(hops: Vec<Hop>) -> Self {
        Self {
            hops,
            interactive_timeout: Duration::from_secs(DEFAULT_INTERACTIVE_TIMEOUT_SECS),
        }
    }

    /// Set the timeout for interactive hops.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.interactive_timeout = timeout;
        self
    }

    /// Execute the connection path: process each hop sequentially.
    ///
    /// Returns an `EstablishedPath` with the active transport ready
    /// for command execution.
    pub async fn connect(
        self,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<EstablishedPath, CiscoIosError> {
        let mut transport: Option<Box<dyn RawTransport>> = None;
        let timeout = self.interactive_timeout;

        for (i, hop) in self.hops.into_iter().enumerate() {
            match hop {
                Hop::Transport(spec) => {
                    info!("Hop {}: opening transport {:?}", i, &spec);
                    let new_transport: Box<dyn RawTransport> = match spec {
                        TransportSpec::Telnet { target } => {
                            Box::new(RawTelnetTransport::connect(target).await?)
                        }
                        TransportSpec::Ssh { target, auth } => {
                            Box::new(RawSshTransport::connect(target, auth).await?)
                        }
                    };
                    transport = Some(new_transport);
                }
                Hop::Interactive(mut fsm) => {
                    info!("Hop {}: running interactive template", i);
                    let t = transport.as_mut().ok_or_else(|| {
                        CiscoIosError::NotConnected
                    })?;
                    drive_interactive(&mut fsm, t.as_mut(), timeout, vars, funcs).await?;
                    info!("Hop {}: interactive completed (state: {})", i, &fsm.curr_state);
                }
            }
        }

        let transport = transport.ok_or(CiscoIosError::NotConnected)?;
        Ok(EstablishedPath { transport })
    }
}

/// A fully established connection path — the transport is authenticated
/// and ready for command execution.
#[derive(Debug)]
pub struct EstablishedPath {
    /// The active transport to the final device.
    pub transport: Box<dyn RawTransport>,
}

impl EstablishedPath {
    /// Create from a raw transport.
    pub fn new(transport: Box<dyn RawTransport>) -> Self {
        Self { transport }
    }

    /// Extract the underlying transport (e.g., to return to a pool).
    pub fn into_transport(self) -> Box<dyn RawTransport> {
        self.transport
    }

    /// Send raw bytes to the device.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.transport.send(data).await
    }

    /// Receive raw bytes from the device.
    pub async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        self.transport.receive(timeout).await
    }

    /// Close the connection.
    pub async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.transport.close().await
    }

    /// Run a TextFSMPlus template interactively on this connection.
    ///
    /// Useful for post-connection interactions (e.g., running commands
    /// and parsing output, or navigating menus).
    pub async fn run_interactive(
        &mut self,
        fsm: &mut TextFSMPlus,
        timeout: Duration,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<(), CiscoIosError> {
        drive_interactive(fsm, self.transport.as_mut(), timeout, vars, funcs).await
    }
}

/// Drive a TextFSMPlus template to completion on a transport.
///
/// This is the core interactive loop: read data, feed to the state machine,
/// send responses, until Done or Error.
pub async fn drive_interactive(
    fsm: &mut TextFSMPlus,
    transport: &mut dyn RawTransport,
    timeout: Duration,
    vars: &(impl GetVar + Sync),
    funcs: &(impl CallFunc + Sync),
) -> Result<(), CiscoIosError> {
    let mut buffer = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        // Check overall timeout
        let now = tokio::time::Instant::now();
        if now >= deadline {
            error!(
                "Interactive timeout after {}s (state: {}, buffer: {} bytes)",
                timeout.as_secs(),
                &fsm.curr_state,
                buffer.len()
            );
            return Err(CiscoIosError::Timeout {
                accumulated: buffer,
            });
        }
        let remaining = deadline - now;

        // Try to match current buffer first (before reading more)
        let result = fsm.feed(&buffer, vars, funcs);
        if result.consumed > 0 {
            buffer.drain(..result.consumed);
        }

        match result.action {
            InteractiveAction::Send(text) => {
                debug!(
                    "Interactive send: {:?} (state: {})",
                    &text, &fsm.curr_state
                );
                transport.send(text.as_bytes()).await?;
                transport.send(b"\n").await?;
                // After sending, continue to read the response
            }
            InteractiveAction::Done => {
                debug!("Interactive done");
                return Ok(());
            }
            InteractiveAction::Error(msg) => {
                let msg_str = msg
                    .as_deref()
                    .unwrap_or("unknown error");
                error!("Interactive error: {}", msg_str);
                return Err(CiscoIosError::HttpUploadError(format!(
                    "Interactive template error: {}",
                    msg_str
                )));
            }
            InteractiveAction::None => {
                // No match yet — read more data
                let chunk = transport
                    .receive(remaining.min(Duration::from_secs(5)))
                    .await?;
                if chunk.is_empty() {
                    // Transport timeout but overall deadline not reached — retry
                    debug!(
                        "Interactive: no data (state: {}, buffer: {} bytes)",
                        &fsm.curr_state,
                        buffer.len()
                    );
                    continue;
                }
                debug!(
                    "Interactive: received {} bytes (total buffer: {})",
                    chunk.len(),
                    buffer.len() + chunk.len()
                );
                buffer.extend_from_slice(&chunk);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw_transport::MockTransport;
    use aytextfsmplus::{NoVars, NoFuncs, Value};

    #[tokio::test]
    async fn test_drive_interactive_login() {
        let mut transport = MockTransport::new(vec![
            b"Username: ".to_vec(),
            b"Password: ".to_vec(),
            b"Router1#".to_vec(),
        ]);

        let mut fsm = TextFSMPlus::from_str(
            r#"Value Preset Username ()
Value Preset Password ()
Value Hostname (\S+)

Start
  ^Username:\s* -> Send ${Username} WaitPassword

WaitPassword
  ^Password:\s* -> Send ${Password} WaitPrompt

WaitPrompt
  ^${Hostname}# -> Done
"#,
        )
        .with_preset("Username", "admin")
        .with_preset("Password", "secret");

        drive_interactive(
            &mut fsm,
            &mut transport,
            Duration::from_secs(5),
            &NoVars,
            &NoFuncs,
        )
        .await
        .unwrap();

        assert_eq!(fsm.curr_state, "Done");
        assert_eq!(
            fsm.curr_record.get("Hostname"),
            Some(&Value::Single("Router1".to_string()))
        );
        // Sent username + \n, password + \n
        assert_eq!(transport.sent[0], b"admin");
        assert_eq!(transport.sent[1], b"\n");
        assert_eq!(transport.sent[2], b"secret");
        assert_eq!(transport.sent[3], b"\n");
    }

    #[tokio::test]
    async fn test_drive_interactive_error() {
        let mut transport = MockTransport::new(vec![
            b"% Login invalid".to_vec(),
        ]);

        let mut fsm = TextFSMPlus::from_str(
            r#"Start
  ^% -> Error "auth failed"
  ^# -> Done
"#,
        );

        let result = drive_interactive(
            &mut fsm,
            &mut transport,
            Duration::from_secs(5),
            &NoVars,
            &NoFuncs,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_drive_interactive_timeout() {
        let mut transport = MockTransport::new(vec![
            // No matching data — will timeout
            b"some banner text".to_vec(),
        ]);

        let mut fsm = TextFSMPlus::from_str(
            r#"Start
  ^# -> Done
"#,
        );

        let result = drive_interactive(
            &mut fsm,
            &mut transport,
            Duration::from_millis(100), // very short timeout
            &NoVars,
            &NoFuncs,
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::Timeout { accumulated } => {
                assert!(!accumulated.is_empty());
            }
            other => panic!("Expected Timeout, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_drive_interactive_enable_mode() {
        let mut transport = MockTransport::new(vec![
            b"Router1>".to_vec(),
            b"Password: ".to_vec(),
            b"Router1#".to_vec(),
        ]);

        let mut fsm = TextFSMPlus::from_str(
            r#"Value Preset EnableSecret ()
Value Hostname (\S+)

Start
  ^${Hostname}# -> Done
  ^${Hostname}> -> Send "enable" Enable

Enable
  ^Password:\s* -> Send ${EnableSecret} WaitEnable

WaitEnable
  ^${Hostname}# -> Done
  ^% -> Error "enable failed"
"#,
        )
        .with_preset("EnableSecret", "s3cret");

        drive_interactive(
            &mut fsm,
            &mut transport,
            Duration::from_secs(5),
            &NoVars,
            &NoFuncs,
        )
        .await
        .unwrap();

        assert_eq!(fsm.curr_state, "Done");
        assert_eq!(
            fsm.curr_record.get("Hostname"),
            Some(&Value::Single("Router1".to_string()))
        );
        // Sent: "enable\n", "s3cret\n"
        assert_eq!(transport.sent[0], b"enable");
        assert_eq!(transport.sent[2], b"s3cret");
    }

    #[tokio::test]
    async fn test_connection_path_single_transport_interactive() {
        // Can't test real transport connections without network,
        // but we can test the ConnectionPath structure creation
        let path = ConnectionPath::new(vec![
            Hop::Transport(TransportSpec::Telnet {
                target: "10.1.1.1:23".parse().unwrap(),
            }),
            Hop::Interactive(
                TextFSMPlus::from_str(
                    r#"Value Preset Username ()
Start
  ^Username:\s* -> Send ${Username} Done
"#,
                )
                .with_preset("Username", "admin"),
            ),
        ])
        .with_timeout(Duration::from_secs(60));

        assert_eq!(path.hops.len(), 2);
        assert_eq!(path.interactive_timeout, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_established_path_new_and_into_transport() {
        let transport = MockTransport::new(vec![
            b"data chunk".to_vec(),
        ]);

        let established = EstablishedPath::new(Box::new(transport));

        // Round-trip: new -> into_transport -> use transport
        let mut transport = established.into_transport();
        let chunk = transport.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(chunk, b"data chunk");
    }

    #[tokio::test]
    async fn test_established_path_send_receive_close() {
        let transport = MockTransport::new(vec![
            b"response".to_vec(),
        ]);

        let mut established = EstablishedPath::new(Box::new(transport));

        established.send(b"command").await.unwrap();
        let data = established.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(data, b"response");
        established.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_connection_path_with_timeout() {
        let path = ConnectionPath::new(vec![])
            .with_timeout(Duration::from_secs(120));

        assert_eq!(path.interactive_timeout, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_connection_path_default_timeout() {
        let path = ConnectionPath::new(vec![]);
        assert_eq!(path.interactive_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_connection_path_connect_no_transport_returns_error() {
        // A path with only interactive hops but no transport should fail
        let path = ConnectionPath::new(vec![
            Hop::Interactive(TextFSMPlus::from_str("Start\n  ^# -> Done\n")),
        ]);

        let result = path.connect(&NoVars, &NoFuncs).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::NotConnected => {}
            other => panic!("Expected NotConnected, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_connection_path_connect_empty_hops_returns_error() {
        let path = ConnectionPath::new(vec![]);
        let result = path.connect(&NoVars, &NoFuncs).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            CiscoIosError::NotConnected => {}
            other => panic!("Expected NotConnected, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_established_path_run_interactive() {
        let transport = MockTransport::new(vec![
            b"Router1#".to_vec(),
        ]);

        let mut established = EstablishedPath {
            transport: Box::new(transport),
        };

        let mut fsm = TextFSMPlus::from_str(
            r#"Value Hostname (\S+)

Start
  ^${Hostname}# -> Done
"#,
        );

        established
            .run_interactive(
                &mut fsm,
                Duration::from_secs(5),
                &NoVars,
                &NoFuncs,
            )
            .await
            .unwrap();

        assert_eq!(fsm.curr_state, "Done");
        assert_eq!(
            fsm.curr_record.get("Hostname"),
            Some(&Value::Single("Router1".to_string()))
        );
    }
}
