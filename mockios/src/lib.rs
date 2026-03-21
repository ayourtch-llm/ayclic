//! Mock Cisco IOS device for testing network automation tools.
//!
//! `MockIosDevice` implements `RawTransport` and simulates a Cisco IOS
//! CLI session. It handles login, command execution, interactive prompts,
//! and config_atomic flows — enough to test the full ayclic stack without
//! a real device.
//!
//! # Example
//!
//! ```rust
//! use mockios::MockIosDevice;
//! use ayclic::{GenericCliConn, RawTransport};
//! use aytextfsmplus::NoVars;
//! use aytextfsmplus::NoFuncs;
//! use std::time::Duration;
//!
//! # tokio_test::block_on(async {
//! let device = MockIosDevice::new("Router1");
//! let mut conn = GenericCliConn::from_transport(Box::new(device))
//!     .with_prompt_template(ayclic::templates::CISCO_IOS_PROMPT)
//!     .with_cmd_timeout(Duration::from_secs(5));
//!
//! let output = conn.run_cmd("show version", &NoVars, &NoFuncs).await.unwrap();
//! assert!(output.contains("Cisco IOS"));
//! # });
//! ```

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use tracing::debug;

use ayclic::error::CiscoIosError;
use ayclic::raw_transport::RawTransport;

/// The CLI mode the mock device is currently in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliMode {
    /// Waiting for username.
    Login,
    /// Username received, waiting for password.
    LoginPassword,
    /// User EXEC mode (hostname>).
    UserExec,
    /// Privileged EXEC mode (hostname#).
    PrivilegedExec,
    /// Global configuration mode (hostname(config)#).
    Config,
    /// Sub-configuration mode (hostname(config-if)#, etc.).
    ConfigSub(String),
    /// Device is reloading — connection should be closed.
    Reloading,
}

/// A mock Cisco IOS device that implements `RawTransport`.
///
/// Feed it commands via `send()`, read responses via `receive()`.
/// Responses are queued internally and returned on the next `receive()`.
pub struct MockIosDevice {
    hostname: String,
    pub mode: CliMode,
    /// Pending output to be returned on next receive().
    output_queue: Vec<u8>,
    /// Registered command handlers: command prefix → response.
    commands: HashMap<String, String>,
    /// Running configuration lines.
    running_config: Vec<String>,
    /// Flash files: filename → content.
    pub flash_files: HashMap<String, Vec<u8>>,
    /// Username for login (None = skip login).
    username: Option<String>,
    /// Password for login.
    password: Option<String>,
    /// Enable password (None = already privileged).
    enable_password: Option<String>,
    /// IOS version string.
    version: String,
    /// Model string.
    model: String,
    /// Pending interactive state.
    pending_interactive: Option<PendingInteractive>,
    /// Input buffer (accumulates send() data).
    input_buffer: Vec<u8>,
    /// Whether the initial banner/prompt has been sent.
    initial_sent: bool,
    /// Queued reload transforms. Each reload pops the next one.
    reload_transforms: Vec<Box<dyn FnOnce(&mut MockIosDevice) + Send>>,
    /// Total flash size in bytes (for `dir` output).
    flash_total_size: u64,
    /// Boot variable (for `show boot` output).
    boot_variable: String,
    /// Reload delay for server mode simulation.
    reload_delay: Duration,
}

/// Pending interactive prompt state.
#[derive(Debug, Clone)]
enum PendingInteractive {
    /// Waiting for confirmation of a copy command.
    CopyConfirm {
        source: String,
        dest: String,
    },
    /// Waiting for destination filename confirmation.
    CopyFilename {
        source: String,
        dest: String,
        default_filename: String,
    },
    /// Reload confirmation.
    ReloadConfirm {
        _minutes: Option<u32>,
    },
    /// Reload save confirmation (yes/no).
    ReloadSave,
    /// Enable password prompt.
    EnablePassword,
}

impl std::fmt::Debug for MockIosDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockIosDevice")
            .field("hostname", &self.hostname)
            .field("mode", &self.mode)
            .field("flash_files", &self.flash_files.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl MockIosDevice {
    /// Create a new mock device with the given hostname.
    /// Starts in PrivilegedExec mode (already logged in).
    pub fn new(hostname: &str) -> Self {
        let mut device = Self {
            hostname: hostname.to_string(),
            mode: CliMode::PrivilegedExec,
            output_queue: Vec::new(),
            commands: HashMap::new(),
            running_config: default_running_config(hostname),
            flash_files: HashMap::new(),
            username: None,
            password: None,
            enable_password: None,
            version: "15.1(4)M".to_string(),
            model: "C2951".to_string(),
            pending_interactive: None,
            input_buffer: Vec::new(),
            initial_sent: false,
            reload_transforms: Vec::new(),
            flash_total_size: 8_000_000_000,
            boot_variable: String::new(),
            reload_delay: Duration::from_secs(0),
        };
        device.register_default_commands();
        device
    }

    /// Create a device that requires login.
    pub fn with_login(mut self, username: &str, password: &str) -> Self {
        self.username = Some(username.to_string());
        self.password = Some(password.to_string());
        self.mode = CliMode::Login;
        self
    }

    /// Set the enable password.
    pub fn with_enable(mut self, password: &str) -> Self {
        self.enable_password = Some(password.to_string());
        self.mode = if self.username.is_some() {
            CliMode::Login
        } else {
            CliMode::UserExec
        };
        self
    }

    /// Set the IOS version string.
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    /// Set the model string.
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Set the running configuration.
    pub fn with_running_config(mut self, config: Vec<String>) -> Self {
        self.running_config = config;
        self
    }

    /// Register a custom command response.
    /// The command prefix is matched case-insensitively.
    pub fn with_command(mut self, command: &str, response: &str) -> Self {
        self.commands
            .insert(command.to_lowercase(), response.to_string());
        self
    }

    /// Add a file to flash storage.
    pub fn with_flash_file(mut self, name: &str, content: Vec<u8>) -> Self {
        self.flash_files.insert(name.to_string(), content);
        self
    }

    /// Set total flash size in bytes (for `dir` output).
    pub fn with_flash_size(mut self, size: u64) -> Self {
        self.flash_total_size = size;
        self
    }

    /// Set the boot variable.
    pub fn with_boot_variable(mut self, boot_var: &str) -> Self {
        self.boot_variable = boot_var.to_string();
        self
    }

    /// Add a reload transform. Each reload pops the next transform.
    /// Transforms are applied in order: first reload uses first transform, etc.
    pub fn with_reload_transform<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut MockIosDevice) + Send + 'static,
    {
        self.reload_transforms.push(Box::new(f));
        self
    }

    /// Set the simulated reload delay (for server mode).
    pub fn with_reload_delay(mut self, delay: Duration) -> Self {
        self.reload_delay = delay;
        self
    }

    /// Create a new device derived from this one, carrying over flash
    /// files, hostname, model, commands, and config. The new device
    /// starts in PrivilegedExec mode (as if freshly booted).
    ///
    /// Use this to create the post-reload device in tests:
    /// ```ignore
    /// let post = pre.derive().with_version("17.09.04a");
    /// ```
    pub fn derive(&self) -> Self {
        Self {
            hostname: self.hostname.clone(),
            mode: CliMode::PrivilegedExec,
            output_queue: Vec::new(),
            commands: self.commands.clone(),
            running_config: self.running_config.clone(),
            flash_files: self.flash_files.clone(),
            username: None,
            password: None,
            enable_password: None,
            version: self.version.clone(),
            model: self.model.clone(),
            pending_interactive: None,
            input_buffer: Vec::new(),
            initial_sent: false,
            reload_transforms: Vec::new(),
            flash_total_size: self.flash_total_size,
            boot_variable: self.boot_variable.clone(),
            reload_delay: self.reload_delay,
        }
    }

    /// Check if the device is in the Reloading state.
    pub fn is_reloading(&self) -> bool {
        self.mode == CliMode::Reloading
    }

    /// Get the reload delay.
    pub fn reload_delay(&self) -> Duration {
        self.reload_delay
    }

    /// Apply the next queued reload transform and reset to booted state.
    /// Call this to simulate the device coming back after a reload.
    pub fn power_on(&mut self) {
        if !self.reload_transforms.is_empty() {
            let transform = self.reload_transforms.remove(0);
            transform(self);
        }
        self.mode = CliMode::PrivilegedExec;
        self.output_queue.clear();
        self.input_buffer.clear();
        self.initial_sent = false;
        self.pending_interactive = None;
    }

    fn register_default_commands(&mut self) {
        // These are overridable via with_command()
    }

    fn prompt(&self) -> String {
        match &self.mode {
            CliMode::Reloading => String::new(),
            CliMode::Login | CliMode::LoginPassword => String::new(),
            CliMode::UserExec => format!("{}>", self.hostname),
            CliMode::PrivilegedExec => format!("{}#", self.hostname),
            CliMode::Config => format!("{}(config)#", self.hostname),
            CliMode::ConfigSub(sub) => format!("{}({})#", self.hostname, sub),
        }
    }

    fn handle_line(&mut self, line: &str) {
        let line = line.trim();

        // Handle pending interactive state first (even for empty lines —
        // empty line is a valid confirm response)
        if let Some(pending) = self.pending_interactive.take() {
            self.handle_interactive_response(line, pending);
            return;
        }

        if line.is_empty() {
            self.queue_output(&format!("\n{}", self.prompt()));
            return;
        }

        debug!("MockIOS [{}] cmd: {:?}", self.hostname, line);

        // Handle based on current mode
        match &self.mode {
            CliMode::Login | CliMode::LoginPassword => self.handle_login(line),
            CliMode::UserExec => self.handle_user_exec(line),
            CliMode::PrivilegedExec => self.handle_privileged_exec(line),
            CliMode::Config => self.handle_config_mode(line),
            CliMode::ConfigSub(_) => self.handle_config_sub(line),
            CliMode::Reloading => {} // ignore input while reloading
        }
    }

    fn handle_login(&mut self, line: &str) {
        match &self.mode {
            CliMode::Login => {
                // Waiting for username
                if let Some(ref expected_user) = self.username {
                    if line == expected_user {
                        self.mode = CliMode::LoginPassword;
                        self.queue_output("\nPassword: ");
                        return;
                    }
                }
                self.queue_output("\n% Login invalid\n\nUsername: ");
            }
            CliMode::LoginPassword => {
                // Waiting for password
                if let Some(ref expected_pass) = self.password {
                    if line == expected_pass {
                        if self.enable_password.is_some() {
                            self.mode = CliMode::UserExec;
                        } else {
                            self.mode = CliMode::PrivilegedExec;
                        }
                        self.queue_output(&format!("\n{}", self.prompt()));
                        return;
                    }
                }
                self.queue_output("\n% Login invalid\n\nUsername: ");
                self.mode = CliMode::Login;
            }
            _ => unreachable!(),
        }
    }

    fn handle_user_exec(&mut self, line: &str) {
        let cmd = line.to_lowercase();
        if cmd == "enable" {
            if self.enable_password.is_some() {
                self.pending_interactive = Some(PendingInteractive::EnablePassword);
                self.queue_output("\nPassword: ");
            } else {
                self.mode = CliMode::PrivilegedExec;
                self.queue_output(&format!("\n{}", self.prompt()));
            }
        } else if cmd.starts_with("terminal length")
            || cmd.starts_with("term len")
            || cmd.starts_with("terminal width")
        {
            self.queue_output(&format!("\n{}", self.prompt()));
        } else {
            self.queue_output(&format!(
                "\n% Unrecognized command\n{}",
                self.prompt()
            ));
        }
    }

    fn handle_privileged_exec(&mut self, line: &str) {
        let cmd = line.to_lowercase();

        // Check custom commands first
        if let Some(response) = self.commands.get(&cmd) {
            self.queue_output(&format!("\n{}\n{}", response, self.prompt()));
            return;
        }

        // Built-in commands
        if cmd.starts_with("show running-config") || cmd.starts_with("show run") {
            let config = self.running_config.join("\n");
            self.queue_output(&format!("\n{}\n{}", config, self.prompt()));
        } else if cmd.starts_with("show version") || cmd.starts_with("show ver") {
            let version_output = self.generate_show_version();
            self.queue_output(&format!("\n{}\n{}", version_output, self.prompt()));
        } else if cmd.starts_with("terminal length")
            || cmd.starts_with("term len")
            || cmd.starts_with("terminal width")
        {
            self.queue_output(&format!("\n{}", self.prompt()));
        } else if cmd.starts_with("configure terminal") || cmd.starts_with("conf t") {
            self.mode = CliMode::Config;
            self.queue_output(&format!(
                "\nEnter configuration commands, one per line.  End with CNTL/Z.\n{}",
                self.prompt()
            ));
        } else if cmd.starts_with("copy ") {
            self.handle_copy_command(line);
        } else if cmd.starts_with("delete ") {
            self.handle_delete_command(line);
        } else if cmd.starts_with("verify /md5 ") {
            self.handle_verify_md5(line);
        } else if cmd.starts_with("reload") {
            self.handle_reload_command(line);
        } else if cmd.starts_with("show boot") || cmd == "show boot" {
            self.handle_show_boot();
        } else if cmd.starts_with("dir ") || cmd == "dir" {
            self.handle_dir_command(line);
        } else {
            // Try prefix matching on custom commands
            let mut found = false;
            for (key, response) in &self.commands {
                if cmd.starts_with(key) {
                    self.queue_output(&format!("\n{}\n{}", response, self.prompt()));
                    found = true;
                    break;
                }
            }
            if !found {
                self.queue_output(&format!(
                    "\n% Unknown command or computer name, or unable to find computer address\n{}",
                    self.prompt()
                ));
            }
        }
    }

    fn handle_config_mode(&mut self, line: &str) {
        let cmd = line.to_lowercase();
        if cmd == "end" || cmd == "exit" {
            self.mode = CliMode::PrivilegedExec;
            self.queue_output(&format!("\n{}", self.prompt()));
        } else if cmd.starts_with("interface ") {
            let sub = line.trim_start_matches("interface ").trim_start_matches("Interface ");
            self.mode = CliMode::ConfigSub(format!("config-if"));
            self.running_config
                .push(format!("interface {}", sub));
            self.queue_output(&format!("\n{}", self.prompt()));
        } else if cmd.starts_with("router ") {
            self.mode = CliMode::ConfigSub(format!("config-router"));
            self.running_config.push(line.to_string());
            self.queue_output(&format!("\n{}", self.prompt()));
        } else {
            // Accept any config line
            self.running_config.push(line.to_string());
            self.queue_output(&format!("\n{}", self.prompt()));
        }
    }

    fn handle_config_sub(&mut self, line: &str) {
        let cmd = line.to_lowercase();
        if cmd == "exit" {
            self.mode = CliMode::Config;
            self.queue_output(&format!("\n{}", self.prompt()));
        } else if cmd == "end" {
            self.mode = CliMode::PrivilegedExec;
            self.queue_output(&format!("\n{}", self.prompt()));
        } else {
            self.running_config.push(format!(" {}", line));
            self.queue_output(&format!("\n{}", self.prompt()));
        }
    }

    fn handle_copy_command(&mut self, line: &str) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            self.queue_output(&format!(
                "\n% Incomplete command.\n{}",
                self.prompt()
            ));
            return;
        }

        let source = parts[1].to_string();
        let dest = parts[2].to_string();

        if dest == "null:" {
            // copy X null: — used for /done endpoint, discard silently
            self.queue_output(&format!("\n{}", self.prompt()));
            return;
        }

        if source.starts_with("http://") {
            // HTTP copy — simulate download
            // Extract filename from URL
            let filename = source.rsplit('/').next().unwrap_or("file");
            let default_dest = if dest.starts_with("flash:") {
                dest.trim_start_matches("flash:").to_string()
            } else {
                filename.to_string()
            };
            self.pending_interactive = Some(PendingInteractive::CopyFilename {
                source: source.clone(),
                dest: dest.clone(),
                default_filename: default_dest,
            });
            self.queue_output(&format!(
                "\nDestination filename [{}]?",
                filename
            ));
        } else if source.contains("running-config") || dest.contains("running-config") {
            // copy X running-config — apply config from flash
            if source.starts_with("flash:") {
                let flash_file = source.trim_start_matches("flash:");
                if let Some(content) = self.flash_files.get(flash_file) {
                    let config_text = String::from_utf8_lossy(content);
                    // Apply config lines to running config
                    for config_line in config_text.lines() {
                        if !config_line.trim().is_empty() {
                            self.running_config.push(config_line.to_string());
                        }
                    }
                }
                self.pending_interactive = Some(PendingInteractive::CopyFilename {
                    source,
                    dest,
                    default_filename: "running-config".to_string(),
                });
                self.queue_output("\nDestination filename [running-config]?");
            } else {
                self.queue_output(&format!(
                    "\n[OK]\n{}", self.prompt()
                ));
            }
        } else {
            self.pending_interactive = Some(PendingInteractive::CopyConfirm {
                source,
                dest,
            });
            self.queue_output("\n[confirm]");
        }
    }

    fn handle_delete_command(&mut self, line: &str) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let filename = parts
            .iter()
            .filter(|p| !p.starts_with('/'))
            .skip(1)
            .next()
            .unwrap_or(&"");
        let filename = filename.trim_start_matches("flash:");
        self.flash_files.remove(filename);
        self.queue_output(&format!("\n{}", self.prompt()));
    }

    fn handle_verify_md5(&mut self, line: &str) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // "verify /md5 flash:filename"
        let flash_path = parts.last().unwrap_or(&"");
        let filename = flash_path.trim_start_matches("flash:");

        if let Some(content) = self.flash_files.get(filename) {
            let md5 = compute_md5(content);
            self.queue_output(&format!(
                "\nverify /md5 ({}) = {}\n{}",
                flash_path, md5, self.prompt()
            ));
        } else {
            self.queue_output(&format!(
                "\n%Error verifying flash:{}\n{}",
                filename,
                self.prompt()
            ));
        }
    }

    fn handle_reload_command(&mut self, line: &str) {
        let cmd = line.to_lowercase();
        if cmd == "reload cancel" {
            self.queue_output(&format!(
                "\n***\n*** --- SHUTDOWN ABORTED ---\n***\n{}",
                self.prompt()
            ));
        } else if cmd.starts_with("reload in ") {
            let _minutes = cmd
                .trim_start_matches("reload in ")
                .parse::<u32>()
                .ok();
            self.pending_interactive = Some(PendingInteractive::ReloadSave);
            self.queue_output("\nSystem configuration has been modified. Save? [yes/no]: ");
        } else {
            self.pending_interactive = Some(PendingInteractive::ReloadConfirm { _minutes: None });
            self.queue_output("\nProceed with reload? [confirm]");
        }
    }

    fn handle_dir_command(&mut self, _line: &str) {
        let mut output = String::from("\nDirectory of flash:/\n\n");
        let mut used: u64 = 0;
        for (name, content) in &self.flash_files {
            output.push_str(&format!(
                "  {:>10} bytes  {}\n",
                content.len(),
                name
            ));
            used += content.len() as u64;
        }
        let free = self.flash_total_size.saturating_sub(used);
        output.push_str(&format!(
            "\n{} bytes total ({} bytes free)\n{}",
            self.flash_total_size, free, self.prompt()
        ));
        self.queue_output(&output);
    }

    fn handle_show_boot(&mut self) {
        let boot_var = if self.boot_variable.is_empty() {
            format!("flash:c{}-universalk9-mz.SPA.{}.bin", self.model.to_lowercase(), self.version)
        } else {
            self.boot_variable.clone()
        };
        let output = format!(
            "\nBOOT variable = {}\nConfig file = \nPrivate Config file = \nEnable Break = no\nManual Boot = no\n{}",
            boot_var, self.prompt()
        );
        self.queue_output(&output);
    }

    fn handle_interactive_response(&mut self, line: &str, pending: PendingInteractive) {
        match pending {
            PendingInteractive::CopyConfirm { source: _, dest: _ } => {
                // Any response confirms
                self.queue_output(&format!(
                    "\n[OK - 0 bytes]\n\n{}", self.prompt()
                ));
            }
            PendingInteractive::CopyFilename {
                source,
                dest,
                default_filename,
            } => {
                // Accept default or custom filename
                let _filename = if line.is_empty() {
                    &default_filename
                } else {
                    line
                };

                if source.starts_with("http://") {
                    // Simulate HTTP download — create a flash file
                    // Use a dummy content for testing
                    let flash_name = if dest.starts_with("flash:") {
                        dest.trim_start_matches("flash:").to_string()
                    } else {
                        default_filename.clone()
                    };

                    // For HTTP copies, simulate downloading content
                    // In real tests, the content should be pre-loaded via with_flash_file
                    // or the test should check that the file appears
                    if !self.flash_files.contains_key(&flash_name) {
                        // Create a placeholder — real tests pre-populate via with_flash_file
                        self.flash_files
                            .insert(flash_name, b"mock content\n".to_vec());
                    }

                    self.queue_output(&format!(
                        "\nAccessing {}...\n[OK - 100 bytes]\n\n{}",
                        source,
                        self.prompt()
                    ));
                } else {
                    // copy flash: running-config
                    self.queue_output(&format!(
                        "\n[OK]\n\n{}",
                        self.prompt()
                    ));
                }
            }
            PendingInteractive::EnablePassword => {
                if let Some(ref expected) = self.enable_password {
                    if line == expected {
                        self.mode = CliMode::PrivilegedExec;
                        self.queue_output(&format!("\n{}", self.prompt()));
                        return;
                    }
                }
                self.queue_output(&format!("\n% Access denied\n{}", self.prompt()));
            }
            PendingInteractive::ReloadConfirm { .. } => {
                // Enter reloading state — subsequent send/receive will error
                self.queue_output("\n\nSystem is reloading...\n");
                self.mode = CliMode::Reloading;
            }
            PendingInteractive::ReloadSave => {
                // yes/no to save before reload
                self.pending_interactive =
                    Some(PendingInteractive::ReloadConfirm { _minutes: None });
                self.queue_output("\nProceed with reload? [confirm]");
            }
        }
    }

    fn generate_show_version(&self) -> String {
        format!(
            r#"Cisco IOS Software, {} Software ({}), Version {}, RELEASE SOFTWARE (fc1)
Technical Support: http://www.cisco.com/techsupport
Copyright (c) 1986-2024 by Cisco Systems, Inc.

ROM: System Bootstrap, Version 15.0(1r)M16

{} uptime is 42 days, 3 hours, 17 minutes
System returned to ROM by reload
System image file is "flash:c2951-universalk9-mz.SPA.{}.bin"

Cisco {} ({}) processor with 524288K/262144K bytes of memory.
Processor board ID FCZ123456789

Configuration register is 0x2102"#,
            self.model,
            self.model,
            self.version,
            self.hostname,
            self.version,
            self.model,
            self.model,
        )
    }

    fn queue_output(&mut self, text: &str) {
        self.output_queue.extend_from_slice(text.as_bytes());
    }
}

#[async_trait]
impl RawTransport for MockIosDevice {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        if self.mode == CliMode::Reloading {
            return Err(CiscoIosError::NotConnected);
        }
        self.input_buffer.extend_from_slice(data);

        // Process complete lines from the input buffer
        while let Some(newline_pos) = self.input_buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.input_buffer.drain(..=newline_pos).collect();
            let line = String::from_utf8_lossy(&line_bytes)
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string();
            self.handle_line(&line);
        }

        // NOTE: We do NOT eagerly process input_buffer for pending
        // interactive prompts. Wait for the \n to arrive (via the next
        // send() call) so we process exactly one response per line.
        // This prevents double-prompt issues when the caller sends
        // text and \n as separate send() calls.

        Ok(())
    }

    async fn receive(&mut self, _timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        // Send initial prompt on first receive
        if !self.initial_sent {
            self.initial_sent = true;
            match &self.mode {
                CliMode::Login => {
                    self.queue_output("Username: ");
                }
                _ => {
                    self.queue_output(&format!("{}", self.prompt()));
                }
            }
        }

        if self.mode == CliMode::Reloading && self.output_queue.is_empty() {
            return Err(CiscoIosError::NotConnected);
        }

        if self.output_queue.is_empty() {
            return Ok(vec![]);
        }

        let data = std::mem::take(&mut self.output_queue);
        Ok(data)
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        Ok(())
    }
}

fn default_running_config(hostname: &str) -> Vec<String> {
    vec![
        "!".to_string(),
        format!("hostname {}", hostname),
        "!".to_string(),
        "interface GigabitEthernet0/0".to_string(),
        " ip address 10.0.0.1 255.255.255.0".to_string(),
        " no shutdown".to_string(),
        "!".to_string(),
        "interface GigabitEthernet0/1".to_string(),
        " ip address 10.0.1.1 255.255.255.0".to_string(),
        " shutdown".to_string(),
        "!".to_string(),
        "ip route 0.0.0.0 0.0.0.0 10.0.0.254".to_string(),
        "!".to_string(),
        "line vty 0 4".to_string(),
        " transport input ssh".to_string(),
        "!".to_string(),
        "end".to_string(),
    ]
}

fn compute_md5(data: &[u8]) -> String {
    // Simple MD5 using the same approach as ayclic
    let mut hasher = Md5Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

/// Minimal MD5 implementation for the mock (avoids adding md-5 dependency).
/// Uses the same output format as ayclic::md5_hex_bytes.
struct Md5Hasher {
    data: Vec<u8>,
}

impl Md5Hasher {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn update(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data);
    }

    fn finalize(&self) -> String {
        // For testing purposes, we compute a simple hash that's deterministic
        // and matches the format (32 hex chars). We use a basic approach.
        // In real usage, the test sets up flash files with known content
        // and known MD5 hashes.
        //
        // For proper MD5, we'd need the md-5 crate. Instead, we compute
        // a fake but deterministic "hash" that's sufficient for mock testing.
        // Tests that need real MD5 verification should pre-compute the hash.
        let mut hash: u128 = 0;
        for (_i, &byte) in self.data.iter().enumerate() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u128);
            hash = hash.rotate_left(7);
        }
        format!("{:032x}", hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ayclic::raw_transport::RawTransport;

    #[tokio::test]
    async fn test_mock_device_prompt() {
        let mut device = MockIosDevice::new("Router1");
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&data), "Router1#");
    }

    #[tokio::test]
    async fn test_mock_device_show_version() {
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // initial prompt

        device.send(b"show version\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("Cisco IOS"));
        assert!(output.contains("Router1"));
        assert!(output.contains("Router1#"));
    }

    #[tokio::test]
    async fn test_mock_device_show_running_config() {
        let mut device = MockIosDevice::new("Switch1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show running-config\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("hostname Switch1"));
        assert!(output.contains("interface GigabitEthernet0/0"));
    }

    #[tokio::test]
    async fn test_mock_device_term_len() {
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"terminal length 0\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("Router1#"));
    }

    #[tokio::test]
    async fn test_mock_device_verify_md5() {
        let content = b"test config content\n";
        let mut device = MockIosDevice::new("Router1")
            .with_flash_file("test.cfg", content.to_vec());
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device
            .send(b"verify /md5 flash:test.cfg\n")
            .await
            .unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("verify /md5 (flash:test.cfg) ="));
        // Should contain a 32-char hex hash
        assert!(output.contains("Router1#"));
    }

    #[tokio::test]
    async fn test_mock_device_delete() {
        let mut device = MockIosDevice::new("Router1")
            .with_flash_file("temp.cfg", b"data".to_vec());
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        assert!(device.flash_files.contains_key("temp.cfg"));
        device
            .send(b"delete /force flash:temp.cfg\n")
            .await
            .unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(!device.flash_files.contains_key("temp.cfg"));
    }

    #[tokio::test]
    async fn test_mock_device_with_login() {
        let mut device = MockIosDevice::new("Router1")
            .with_login("admin", "secret");

        // First receive should show Username prompt
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&data), "Username: ");

        // Send username
        device.send(b"admin\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(String::from_utf8_lossy(&data).contains("Password:"));

        // Send password
        device.send(b"secret\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(String::from_utf8_lossy(&data).contains("Router1#"));
    }

    #[tokio::test]
    async fn test_mock_device_custom_command() {
        let mut device = MockIosDevice::new("Router1")
            .with_command("show clock", "14:32:00.123 UTC Fri Mar 21 2026");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show clock\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("14:32:00"));
    }

    #[tokio::test]
    async fn test_mock_device_reload_cancel() {
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"reload cancel\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("SHUTDOWN ABORTED"));
    }

    #[tokio::test]
    async fn test_mock_device_with_generic_cli_conn() {
        let device = MockIosDevice::new("Router1");
        // Use a hostname-specific prompt template for precise matching
        let prompt_template = r#"Start
  ^Router1# -> Done
  ^.*\]\?\s* -> Send ""
  ^\[confirm\] -> Send ""
"#;
        let mut conn = ayclic::GenericCliConn::from_transport(Box::new(device))
            .with_prompt_template(prompt_template)
            .with_cmd_timeout(Duration::from_secs(5));

        // First run_cmd consumes the initial prompt as "output" (empty or prompt text),
        // subsequent commands work normally. This mirrors real device behavior where
        // the initial prompt is consumed during login/init.
        let _ = conn
            .run_cmd(
                "terminal length 0",
                &aytextfsmplus::NoVars,
                &aytextfsmplus::NoFuncs,
            )
            .await
            .unwrap();

        let output = conn
            .run_cmd(
                "show version",
                &aytextfsmplus::NoVars,
                &aytextfsmplus::NoFuncs,
            )
            .await
            .unwrap();

        assert!(output.contains("Cisco IOS"));
    }

    #[tokio::test]
    async fn test_mock_device_config_mode() {
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"configure terminal\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("Router1(config)#"));

        device.send(b"hostname NewRouter\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(String::from_utf8_lossy(&data).contains("(config)#"));

        device.send(b"end\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(String::from_utf8_lossy(&data).contains("#"));
    }

    // === Reload simulation tests ===

    #[tokio::test]
    async fn test_reload_enters_reloading_state() {
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Trigger reload
        device.send(b"reload\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("[confirm]"));

        // Confirm reload
        device.send(b"\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("reloading"));

        // Device is now reloading
        assert!(device.is_reloading());
    }

    #[tokio::test]
    async fn test_reload_errors_on_send_receive() {
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"reload\n").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"\n").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // "reloading..."

        // send should error
        assert!(device.send(b"test\n").await.is_err());
        // receive should error
        assert!(device.receive(Duration::from_secs(1)).await.is_err());
    }

    #[tokio::test]
    async fn test_derive_creates_fresh_device() {
        let pre = MockIosDevice::new("Switch1")
            .with_version("17.06.05")
            .with_flash_file("image.bin", b"firmware".to_vec());

        let post = pre.derive().with_version("17.09.04a");

        assert_eq!(post.hostname, "Switch1");
        assert_eq!(post.version, "17.09.04a");
        assert!(post.flash_files.contains_key("image.bin"));
        assert_eq!(post.mode, CliMode::PrivilegedExec);
        assert!(!post.initial_sent);
    }

    #[tokio::test]
    async fn test_derive_preserves_flash_files() {
        let pre = MockIosDevice::new("Switch1")
            .with_flash_file("file1.bin", b"data1".to_vec())
            .with_flash_file("file2.cfg", b"data2".to_vec());

        let post = pre.derive();
        assert_eq!(post.flash_files.len(), 2);
        assert_eq!(post.flash_files.get("file1.bin").unwrap(), b"data1");
    }

    #[tokio::test]
    async fn test_power_on_applies_transform() {
        let mut device = MockIosDevice::new("Switch1")
            .with_version("17.06.05")
            .with_reload_transform(|d| {
                d.version = "17.09.04a".to_string();
            });

        // Simulate reload
        device.mode = CliMode::Reloading;

        // Power on applies the transform
        device.power_on();
        assert_eq!(device.version, "17.09.04a");
        assert_eq!(device.mode, CliMode::PrivilegedExec);
        assert!(!device.initial_sent);
    }

    #[tokio::test]
    async fn test_power_on_multiple_transforms() {
        let mut device = MockIosDevice::new("Switch1")
            .with_version("17.06.05")
            .with_reload_transform(|d| {
                d.version = "17.09.04a".to_string();
            })
            .with_reload_transform(|d| {
                d.version = "17.09.04a-final".to_string();
            });

        // First reload
        device.mode = CliMode::Reloading;
        device.power_on();
        assert_eq!(device.version, "17.09.04a");

        // Second reload
        device.mode = CliMode::Reloading;
        device.power_on();
        assert_eq!(device.version, "17.09.04a-final");
    }

    #[tokio::test]
    async fn test_reload_in_with_save_prompt() {
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"reload in 5\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("Save?"));

        // Answer no to save
        device.send(b"no\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("[confirm]"));

        // Confirm
        device.send(b"\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(String::from_utf8_lossy(&data).contains("reloading"));
        assert!(device.is_reloading());
    }

    #[tokio::test]
    async fn test_flash_size() {
        let device = MockIosDevice::new("Router1")
            .with_flash_size(4_000_000_000);
        assert_eq!(device.flash_total_size, 4_000_000_000);
    }

    #[tokio::test]
    async fn test_show_boot() {
        let mut device = MockIosDevice::new("Router1")
            .with_boot_variable("flash:packages.conf");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show boot\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("flash:packages.conf"));
    }

    #[tokio::test]
    async fn test_dir_shows_flash_space() {
        let mut device = MockIosDevice::new("Router1")
            .with_flash_size(8_000_000_000)
            .with_flash_file("test.bin", vec![0u8; 1000]);
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"dir flash:\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("test.bin"));
        assert!(output.contains("1000 bytes"));
        assert!(output.contains("bytes total"));
        assert!(output.contains("bytes free"));
    }

    #[tokio::test]
    async fn test_full_reload_workflow() {
        // Pre-reload device
        let mut device = MockIosDevice::new("Switch1")
            .with_version("17.06.05")
            .with_flash_file("new_image.bin", b"firmware".to_vec())
            .with_reload_transform(|d| {
                d.version = "17.09.04a".to_string();
            });

        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Run show version — old version
        device.send(b"show version\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(String::from_utf8_lossy(&data).contains("17.06.05"));

        // Trigger reload
        device.send(b"reload\n").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"\n").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(device.is_reloading());

        // Create post-reload device using derive (alternative to power_on)
        let mut post = device.derive();
        // Apply the transform manually (derive doesn't auto-apply transforms)
        post.version = "17.09.04a".to_string();

        let _ = post.receive(Duration::from_secs(1)).await.unwrap();
        post.send(b"show version\n").await.unwrap();
        let data = post.receive(Duration::from_secs(1)).await.unwrap();
        assert!(String::from_utf8_lossy(&data).contains("17.09.04a"));

        // Flash files persist across reload
        assert!(post.flash_files.contains_key("new_image.bin"));
    }

    #[tokio::test]
    async fn test_trace_enable_flow_step_by_step() {
        // Trace every send/receive to find stale prompts
        let mut device = MockIosDevice::new("R1")
            .with_enable("secret");
        device.mode = CliMode::UserExec;

        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("STEP 1 initial: {:?}", String::from_utf8_lossy(&data));
        assert_eq!(String::from_utf8_lossy(&data), "R1>");

        // Send "enable\n"
        device.send(b"enable\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("STEP 2 after enable: {:?}", String::from_utf8_lossy(&data));
        assert!(String::from_utf8_lossy(&data).contains("Password:"));

        // Send password
        device.send(b"secret\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("STEP 3 after password: {:?}", String::from_utf8_lossy(&data));
        assert!(String::from_utf8_lossy(&data).contains("R1#"));
        assert_eq!(device.mode, CliMode::PrivilegedExec);

        // Now send "terminal length 0\n"
        device.send(b"terminal length 0\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("STEP 4 after term len: {:?}", String::from_utf8_lossy(&data));
        assert!(String::from_utf8_lossy(&data).contains("R1#"));

        // Check: is there anything leftover?
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("STEP 5 leftover: {:?} (len={})", String::from_utf8_lossy(&data), data.len());
        assert!(data.is_empty(), "Should have no leftover data");

        // Now send "show version\n"
        device.send(b"show version\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        eprintln!("STEP 6 show version (first 100): {:?}", &output[..output.len().min(100)]);
        assert!(output.contains("Cisco IOS"), "show version should contain Cisco IOS");
    }

    #[tokio::test]
    async fn test_trace_drive_interactive_style() {
        // Mimic how drive_interactive sends: text then \n separately
        let mut device = MockIosDevice::new("R1")
            .with_enable("secret");
        device.mode = CliMode::UserExec;

        // Step 1: receive initial prompt
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("DI-1 initial: {:?}", String::from_utf8_lossy(&data));

        // Step 2: drive_interactive matches ">" and sends "enable" + "\n"
        device.send(b"enable").await.unwrap();
        device.send(b"\n").await.unwrap();

        // Step 3: receive response
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("DI-2 after enable: {:?}", String::from_utf8_lossy(&data));

        // Step 4: drive_interactive matches "Password:" and sends "secret" + "\n"
        device.send(b"secret").await.unwrap();
        device.send(b"\n").await.unwrap();

        // Step 5: receive — should get the prompt
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("DI-3 after password: {:?}", String::from_utf8_lossy(&data));

        // Step 6: drive_interactive matches "#" and sends "terminal length 0" + "\n"
        device.send(b"terminal length 0").await.unwrap();
        device.send(b"\n").await.unwrap();

        // Step 7: receive — should get prompt after term len
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("DI-4 after term len: {:?}", String::from_utf8_lossy(&data));

        // Step 8: drive_interactive matches "#" → Done. Interaction complete.

        // Step 9: now run_cmd("show version") — sends text + \n
        device.send(b"show version").await.unwrap();
        device.send(b"\n").await.unwrap();

        // Step 10: receive — should get version output
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        eprintln!("DI-5 show version (first 100): {:?}", &output[..output.len().min(100)]);

        // Check for leftover
        let data2 = device.receive(Duration::from_secs(1)).await.unwrap();
        eprintln!("DI-6 leftover: {:?} (len={})", String::from_utf8_lossy(&data2), data2.len());

        assert!(output.contains("Cisco IOS"), "show version should contain Cisco IOS");
    }
}
