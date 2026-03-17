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

/// Default prompt template — matches common CLI prompts.
/// Handles `#`, `>`, and `$` as done markers.
const DEFAULT_PROMPT_TEMPLATE: &str = r#"Start
  ^.*[#>$]\s*$$ -> Done
"#;

/// A vendor-neutral CLI connection to a network device.
///
/// Owns its transport and provides methods for sending commands and
/// running TextFSMPlus templates. Works with any device that has a
/// text-based CLI — Cisco, Juniper, Arista, MikroTik, Linux, etc.
///
/// The prompt template controls how `run_cmd()` detects command
/// completion and handles interactive prompts. Set it during
/// construction with `with_prompt_template()`.
#[derive(Debug)]
pub struct GenericCliConn {
    transport: Box<dyn RawTransport>,
    /// TextFSMPlus template content for prompt detection in run_cmd().
    /// A fresh engine is created from this for each command.
    prompt_template: String,
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
            prompt_template: DEFAULT_PROMPT_TEMPLATE.to_string(),
            cmd_timeout: Duration::from_secs(DEFAULT_CMD_TIMEOUT_SECS),
        })
    }

    /// Wrap an already-established connection.
    pub fn from_established(established: EstablishedPath) -> Self {
        Self {
            transport: established.into_transport(),
            prompt_template: DEFAULT_PROMPT_TEMPLATE.to_string(),
            cmd_timeout: Duration::from_secs(DEFAULT_CMD_TIMEOUT_SECS),
        }
    }

    /// Wrap a raw transport directly.
    pub fn from_transport(transport: Box<dyn RawTransport>) -> Self {
        Self {
            transport,
            prompt_template: DEFAULT_PROMPT_TEMPLATE.to_string(),
            cmd_timeout: Duration::from_secs(DEFAULT_CMD_TIMEOUT_SECS),
        }
    }

    /// Set the prompt template for `run_cmd()`.
    ///
    /// The template should have rules that match the device prompt
    /// (`-> Done`) and optionally handle interactive prompts (`-> Send`).
    /// A fresh TextFSMPlus engine is created from this template for
    /// each `run_cmd()` call.
    pub fn with_prompt_template(mut self, template: &str) -> Self {
        self.prompt_template = template.to_string();
        self
    }

    /// Set the default command timeout.
    pub fn with_cmd_timeout(mut self, timeout: Duration) -> Self {
        self.cmd_timeout = timeout;
        self
    }

    /// Get the current prompt template.
    pub fn prompt_template(&self) -> &str {
        &self.prompt_template
    }

    /// Send a command and collect output, using the stored prompt
    /// template to detect when the command is complete.
    ///
    /// A fresh TextFSMPlus engine is created from `self.prompt_template`
    /// for each call, ensuring clean state.
    pub async fn run_cmd(
        &mut self,
        cmd: &str,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<String, CiscoIosError> {
        let mut prompt_fsm = TextFSMPlus::from_str(&self.prompt_template);
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
            let result = prompt_fsm.feed(&buffer, vars, funcs);
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

    /// Send a command with a specific template (overrides stored prompt).
    ///
    /// Use this for commands that need special prompt handling different
    /// from the connection's default (e.g., interactive commands with
    /// confirmation prompts, or commands that produce non-standard output).
    pub async fn run_cmd_with_template(
        &mut self,
        cmd: &str,
        template: &str,
        vars: &(impl GetVar + Sync),
        funcs: &(impl CallFunc + Sync),
    ) -> Result<String, CiscoIosError> {
        let mut prompt_fsm = TextFSMPlus::from_str(template);

        self.transport.send(cmd.as_bytes()).await?;
        self.transport.send(b"\n").await?;
        debug!("GenericCliConn: sent command {:?} (custom template)", cmd);

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

            let result = prompt_fsm.feed(&buffer, vars, funcs);
            if result.consumed > 0 {
                buffer.drain(..result.consumed);
            }

            match result.action {
                InteractiveAction::Done => {
                    return String::from_utf8(buffer).map_err(|e| {
                        CiscoIosError::HttpUploadError(format!("Invalid UTF-8: {}", e))
                    });
                }
                InteractiveAction::Send(text) => {
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
            .with_cmd_timeout(Duration::from_secs(5))
            .with_prompt_template(r#"Start
  ^.*# -> Done
"#);

        let output = conn
            .run_cmd("show version", &NoVars, &NoFuncs)
            .await
            .unwrap();

        assert!(output.is_empty() || output.contains("#"));
    }

    #[tokio::test]
    async fn test_generic_conn_run_cmd_with_interactive_prompt() {
        let transport = MockTransport::new(vec![
            b"Destination filename [startup-config]?".to_vec(),
            b"Router1#".to_vec(),
        ]);

        let mut conn = GenericCliConn::from_transport(Box::new(transport))
            .with_cmd_timeout(Duration::from_secs(5))
            .with_prompt_template(r#"Start
  ^.*# -> Done
  ^.*\? -> Send ""
"#);

        let _output = conn
            .run_cmd("copy running-config startup-config", &NoVars, &NoFuncs)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_generic_conn_default_prompt_template() {
        let transport = MockTransport::new(vec![]);
        let conn = GenericCliConn::from_transport(Box::new(transport));
        // Default template should match common prompts
        assert!(conn.prompt_template().contains("Done"));
    }

    #[tokio::test]
    async fn test_generic_conn_custom_prompt_template() {
        let transport = MockTransport::new(vec![
            b"device$ ".to_vec(),
        ]);

        let mut conn = GenericCliConn::from_transport(Box::new(transport))
            .with_cmd_timeout(Duration::from_secs(5))
            .with_prompt_template(r#"Start
  ^.*\$\s -> Done
"#);

        let _output = conn.run_cmd("ls", &NoVars, &NoFuncs).await.unwrap();
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
