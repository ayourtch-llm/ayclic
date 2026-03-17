//! Generic CLI connection — vendor-neutral device interaction.
//!
//! `GenericCliConn` provides command execution over any established
//! transport, regardless of device vendor or protocol. All vendor-specific
//! behavior (login, prompts, enable mode) is handled by TextFSMPlus
//! templates, not by this module.
//!
//! # Example
//!
//! ```ignore
//! use ayclic::generic_conn::GenericCliConn;
//! use ayclic::path::*;
//!
//! // Connect via a multi-hop path
//! let path = ConnectionPath::new(vec![
//!     Hop::Transport(TransportSpec::Ssh { ... }),
//!     Hop::Interactive(login_template),
//! ]);
//! let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs).await?;
//!
//! // Run commands
//! let output = conn.run_cmd("show version", &prompt_template, &NoVars, &NoFuncs).await?;
//!
//! // Or get the transport back (e.g., return to pool)
//! let transport = conn.into_transport();
//! ```

use std::time::Duration;

use aycalc::{CallFunc, GetVar};
use tracing::{debug, info};

use aytextfsmplus::{InteractiveAction, TextFSMPlus};

use crate::error::CiscoIosError;
use crate::path::{drive_interactive, ConnectionPath, EstablishedPath, Hop};
use crate::raw_transport::RawTransport;

/// Default timeout for command execution (30 seconds).
const DEFAULT_CMD_TIMEOUT_SECS: u64 = 30;

/// A vendor-neutral CLI connection to a network device.
///
/// Owns its transport and provides methods for sending commands and
/// running TextFSMPlus templates. Works with any device that has a
/// text-based CLI — Cisco, Juniper, Arista, MikroTik, Linux, etc.
#[derive(Debug)]
pub struct GenericCliConn {
    transport: Box<dyn RawTransport>,
    /// Default timeout for commands.
    pub cmd_timeout: Duration,
}

impl GenericCliConn {
    /// Connect via a full `ConnectionPath` (from scratch).
    pub async fn connect(
        path: ConnectionPath,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<Self, CiscoIosError> {
        let established = path.connect(vars, funcs).await?;
        Ok(Self::from_established(established))
    }

    /// Connect by running additional hops on an existing transport.
    ///
    /// Use this when you have a pre-established connection (e.g., from
    /// a jumphost pool) and need to reach a specific device from there.
    pub async fn connect_over(
        established: EstablishedPath,
        hops: Vec<Hop>,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<Self, CiscoIosError> {
        let timeout = Duration::from_secs(DEFAULT_CMD_TIMEOUT_SECS);
        let mut transport = established.into_transport();

        for (i, hop) in hops.into_iter().enumerate() {
            match hop {
                Hop::Transport(_) => {
                    // Transport hops in connect_over are unusual — they'd
                    // replace the current transport. Log a warning but allow it.
                    info!(
                        "connect_over hop {}: Transport hop will replace \
                         the existing connection",
                        i
                    );
                    // For now, delegate to the path module's logic
                    // by wrapping in a ConnectionPath
                    return Err(CiscoIosError::InvalidConnectionType(
                        "Transport hops in connect_over are not yet supported. \
                         Use ConnectionPath::connect() instead."
                            .to_string(),
                    ));
                }
                Hop::Interactive(mut fsm) => {
                    info!("connect_over hop {}: running interactive template", i);
                    drive_interactive(&mut fsm, transport.as_mut(), timeout, vars, funcs).await?;
                }
            }
        }

        Ok(Self {
            transport,
            cmd_timeout: Duration::from_secs(DEFAULT_CMD_TIMEOUT_SECS),
        })
    }

    /// Wrap an already-established connection.
    ///
    /// The transport should already be authenticated and at a device
    /// prompt (or whatever state you need it in).
    pub fn from_established(established: EstablishedPath) -> Self {
        Self {
            transport: established.into_transport(),
            cmd_timeout: Duration::from_secs(DEFAULT_CMD_TIMEOUT_SECS),
        }
    }

    /// Wrap a raw transport directly.
    pub fn from_transport(transport: Box<dyn RawTransport>) -> Self {
        Self {
            transport,
            cmd_timeout: Duration::from_secs(DEFAULT_CMD_TIMEOUT_SECS),
        }
    }

    /// Set the default command timeout.
    pub fn with_cmd_timeout(mut self, timeout: Duration) -> Self {
        self.cmd_timeout = timeout;
        self
    }

    /// Send a command and collect output using a TextFSMPlus template
    /// to detect when the command is complete.
    ///
    /// The template should have rules that match the device prompt
    /// (-> Done) and optionally handle interactive prompts (-> Send).
    ///
    /// Returns the raw output bytes accumulated before the Done match.
    pub async fn run_cmd(
        &mut self,
        cmd: &str,
        prompt_template: &mut TextFSMPlus,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<String, CiscoIosError> {
        // Send the command
        self.transport.send(cmd.as_bytes()).await?;
        self.transport.send(b"\n").await?;
        debug!("GenericCliConn: sent command {:?}", cmd);

        // Collect output until the template signals Done
        let mut buffer = Vec::new();
        let deadline = tokio::time::Instant::now() + self.cmd_timeout;

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(CiscoIosError::Timeout {
                    accumulated: buffer,
                });
            }
            let remaining = deadline - now;

            // Try to match current buffer
            let result = prompt_template.feed(&buffer, vars, funcs);
            if result.consumed > 0 {
                buffer.drain(..result.consumed);
            }

            match result.action {
                InteractiveAction::Done => {
                    debug!("GenericCliConn: command complete");
                    return String::from_utf8(buffer).map_err(|e| {
                        CiscoIosError::HttpUploadError(format!("Invalid UTF-8: {}", e))
                    });
                }
                InteractiveAction::Send(text) => {
                    debug!("GenericCliConn: interactive send {:?}", &text);
                    self.transport.send(text.as_bytes()).await?;
                    self.transport.send(b"\n").await?;
                }
                InteractiveAction::Error(msg) => {
                    let msg_str = msg.as_deref().unwrap_or("unknown error");
                    return Err(CiscoIosError::HttpUploadError(format!(
                        "Command template error: {}",
                        msg_str
                    )));
                }
                InteractiveAction::None => {
                    // Read more data
                    let chunk = self
                        .transport
                        .receive(remaining.min(Duration::from_secs(5)))
                        .await?;
                    if !chunk.is_empty() {
                        buffer.extend_from_slice(&chunk);
                    }
                }
            }
        }
    }

    /// Run a TextFSMPlus template interactively on this connection.
    ///
    /// Unlike `run_cmd()`, this doesn't send an initial command — it
    /// just drives the template on whatever the device is currently
    /// sending. Useful for navigating menus, handling post-login
    /// banners, or any interaction that doesn't start with a command.
    pub async fn run_interactive(
        &mut self,
        fsm: &mut TextFSMPlus,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<(), CiscoIosError> {
        drive_interactive(fsm, self.transport.as_mut(), self.cmd_timeout, vars, funcs).await
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

    /// Extract the underlying transport (e.g., return to a pool).
    /// Consumes the connection.
    pub fn into_transport(self) -> Box<dyn RawTransport> {
        self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw_transport::MockTransport;
    use aytextfsmplus::{NoFuncs, NoVars, Value};

    #[tokio::test]
    async fn test_generic_conn_run_cmd() {
        let transport = MockTransport::new(vec![
            b"Router1#".to_vec(),
        ]);

        let mut conn = GenericCliConn::from_transport(Box::new(transport))
            .with_cmd_timeout(Duration::from_secs(5));

        // Template that matches # as done
        let mut prompt = TextFSMPlus::from_str(
            r#"Start
  ^.*# -> Done
"#,
        );

        let output = conn
            .run_cmd("show version", &mut prompt, &NoVars, &NoFuncs)
            .await
            .unwrap();

        // Output is whatever was in the buffer (may be empty since
        // the prompt match consumed everything)
        assert!(output.is_empty() || output.contains("#"));
    }

    #[tokio::test]
    async fn test_generic_conn_run_cmd_with_interactive_prompt() {
        let transport = MockTransport::new(vec![
            b"Destination filename [startup-config]?".to_vec(),
            b"Router1#".to_vec(),
        ]);

        let mut conn = GenericCliConn::from_transport(Box::new(transport))
            .with_cmd_timeout(Duration::from_secs(5));

        let mut prompt = TextFSMPlus::from_str(
            r#"Start
  ^.*# -> Done
  ^.*\? -> Send ""
"#,
        );

        let _output = conn
            .run_cmd(
                "copy running-config startup-config",
                &mut prompt,
                &NoVars,
                &NoFuncs,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_generic_conn_run_interactive() {
        let transport = MockTransport::new(vec![
            b"Router1>".to_vec(),
            b"Password: ".to_vec(),
            b"Router1#".to_vec(),
        ]);

        let mut conn = GenericCliConn::from_transport(Box::new(transport));

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
"#,
        )
        .with_preset("EnableSecret", "s3cret");

        conn.run_interactive(&mut fsm, &NoVars, &NoFuncs)
            .await
            .unwrap();

        assert_eq!(fsm.curr_state, "Done");
        assert_eq!(
            fsm.curr_record.get("Hostname"),
            Some(&Value::Single("Router1".to_string()))
        );
    }

    #[tokio::test]
    async fn test_generic_conn_connect_over() {
        let transport = MockTransport::new(vec![
            b"Username: ".to_vec(),
            b"Password: ".to_vec(),
            b"Switch1#".to_vec(),
        ]);

        let established = EstablishedPath::new(Box::new(transport));

        let login = TextFSMPlus::from_str(
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
        .with_preset("Password", "pass123");

        let conn = GenericCliConn::connect_over(
            established,
            vec![Hop::Interactive(login)],
            &NoVars,
            &NoFuncs,
        )
        .await
        .unwrap();

        // Connection established, can use conn for commands
        assert!(conn.cmd_timeout.as_secs() > 0);
    }

    #[tokio::test]
    async fn test_generic_conn_into_transport() {
        let transport = MockTransport::new(vec![]);
        let conn = GenericCliConn::from_transport(Box::new(transport));

        // Extract transport back (e.g., for pool return)
        let _transport = conn.into_transport();
        // conn is consumed — can't use it anymore
    }

    #[tokio::test]
    async fn test_generic_conn_with_timeout() {
        let transport = MockTransport::new(vec![]);
        let conn = GenericCliConn::from_transport(Box::new(transport))
            .with_cmd_timeout(Duration::from_secs(120));

        assert_eq!(conn.cmd_timeout, Duration::from_secs(120));
    }
}
