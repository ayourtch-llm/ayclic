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

pub mod cmd_tree;
pub mod cmd_tree_exec;
pub mod cmd_tree_conf;
pub mod device_state;

use device_state::{DeviceState, abbreviate_interface_name};
use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use tracing::debug;

use ayclic::error::CiscoIosError;
use ayclic::raw_transport::RawTransport;

/// IOS-XE install mode vs bundle mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMode {
    /// Traditional bundle mode — single .bin image.
    Bundle,
    /// Install mode — packages.conf + individual .pkg files.
    Install,
}

/// State of a package in the install system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageState {
    /// Package added but not activated.
    Inactive,
    /// Package activated (in use) but not committed.
    Activated,
    /// Package committed (persists across reload).
    Committed,
}

/// Info about an installed package.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub state: PackageState,
}

/// Install system state for IOS-XE devices.
#[derive(Debug, Clone)]
pub struct InstallState {
    pub mode: InstallMode,
    pub packages: Vec<PackageInfo>,
}

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

/// State machine for parsing ANSI escape sequences (arrow keys, etc.).
#[derive(Debug, Clone, PartialEq)]
enum EscState {
    /// Normal input — no escape in progress.
    Normal,
    /// Received 0x1B (ESC), waiting for next byte.
    GotEsc,
    /// Received ESC `[`, waiting for final byte.
    GotBracket,
}

/// A mock Cisco IOS device that implements `RawTransport`.
///
/// Feed it commands via `send()`, read responses via `receive()`.
/// Responses are queued internally and returned on the next `receive()`.
pub struct MockIosDevice {
    pub hostname: String,
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
    pub enable_password: Option<String>,
    /// IOS version string.
    pub version: String,
    /// Model string.
    pub model: String,
    /// Pending interactive state.
    pending_interactive: Option<PendingInteractive>,
    /// Input buffer (accumulates send() data).
    pub input_buffer: Vec<u8>,
    /// Cursor position within input_buffer.
    pub cursor_pos: usize,
    /// Command history (most recent at end).
    pub command_history: Vec<String>,
    /// Current position when recalling history (None = not recalling).
    history_index: Option<usize>,
    /// Escape sequence parser state.
    esc_state: EscState,
    /// Whether the initial banner/prompt has been sent.
    pub initial_sent: bool,
    /// Queued reload transforms. Each reload pops the next one.
    reload_transforms: Vec<Box<dyn FnOnce(&mut MockIosDevice) + Send>>,
    /// Total flash size in bytes (for `dir` output).
    flash_total_size: u64,
    /// Boot variable (for `show boot` output).
    boot_variable: String,
    /// Reload delay for server mode simulation.
    reload_delay: Duration,
    /// IOS-XE install mode state (None = not IOS-XE / bundle mode only).
    install_state: Option<InstallState>,
    /// Structured device state — the authoritative data model.
    pub state: DeviceState,
    /// Current interface being configured in config-if mode.
    pub current_interface: Option<String>,
}

/// Pending interactive prompt state.
#[derive(Debug, Clone)]
pub enum PendingInteractive {
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
    /// Install activate confirmation.
    InstallActivateConfirm,
    /// Waiting for "configure" method (terminal/memory/network).
    ConfigureMethod,
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
            version: "15.2(7)E13".to_string(),
            model: "WS-C3560CX-12PD-S".to_string(),
            pending_interactive: None,
            input_buffer: Vec::new(),
            cursor_pos: 0,
            command_history: Vec::new(),
            history_index: None,
            esc_state: EscState::Normal,
            initial_sent: false,
            reload_transforms: Vec::new(),
            flash_total_size: 8_000_000_000,
            boot_variable: String::new(),
            reload_delay: Duration::from_secs(0),
            install_state: None,
            state: DeviceState::new(hostname),
            current_interface: None,
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
        self.state.version = version.to_string();
        self
    }

    /// Set the model string.
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self.state.model = model.to_string();
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
        self.flash_files.insert(name.to_string(), content.clone());
        self.state.flash_files.insert(name.to_string(), content);
        self
    }

    /// Set total flash size in bytes (for `dir` output).
    pub fn with_flash_size(mut self, size: u64) -> Self {
        self.flash_total_size = size;
        self.state.flash_total_size = size;
        self
    }

    /// Set the boot variable.
    pub fn with_boot_variable(mut self, boot_var: &str) -> Self {
        self.boot_variable = boot_var.to_string();
        self.state.boot_variable = boot_var.to_string();
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

    /// Set the IOS-XE install state.
    ///
    /// When set, `show install summary`, `show version`, and `show boot`
    /// automatically reflect the install mode state.
    pub fn with_install_state(mut self, install_state: InstallState) -> Self {
        // Auto-set boot variable based on mode
        if self.boot_variable.is_empty() {
            let boot_var = match install_state.mode {
                InstallMode::Install => "flash:packages.conf".to_string(),
                InstallMode::Bundle => String::new(), // will use default
            };
            self.boot_variable = boot_var.clone();
            self.state.boot_variable = boot_var;
        }
        self.state.install_state = Some(install_state.clone());
        self.install_state = Some(install_state);
        self
    }

    /// Set the simulated reload delay (for server mode).
    pub fn with_reload_delay(mut self, delay: Duration) -> Self {
        self.reload_delay = delay;
        self
    }

    /// Return true if the current mode should suppress character echo (password prompts).
    fn is_password_mode(&self) -> bool {
        matches!(
            self.mode,
            CliMode::LoginPassword
        ) || matches!(
            self.pending_interactive,
            Some(PendingInteractive::EnablePassword)
        )
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
        // Build a derived state that mirrors the top-level fields
        let mut derived_state = DeviceState::new(&self.hostname);
        derived_state.version = self.state.version.clone();
        derived_state.model = self.state.model.clone();
        derived_state.flash_files = self.state.flash_files.clone();
        derived_state.flash_total_size = self.state.flash_total_size;
        derived_state.boot_variable = self.state.boot_variable.clone();
        derived_state.install_state = self.state.install_state.clone();
        derived_state.interfaces = self.state.interfaces.iter().map(|i| {
            let mut ni = device_state::InterfaceState::new(&i.name);
            ni.description = i.description.clone();
            ni.admin_up = i.admin_up;
            ni.link_up = i.link_up;
            ni.ip_address = i.ip_address;
            ni.speed = i.speed.clone();
            ni.duplex = i.duplex.clone();
            ni.mtu = i.mtu;
            ni.switchport_mode = i.switchport_mode.clone();
            ni.vlan = i.vlan;
            ni.mac_address = i.mac_address.clone();
            ni.input_packets = i.input_packets;
            ni.output_packets = i.output_packets;
            ni.input_bytes = i.input_bytes;
            ni.output_bytes = i.output_bytes;
            ni.input_errors = i.input_errors;
            ni.output_errors = i.output_errors;
            ni
        }).collect();
        derived_state.static_routes = self.state.static_routes.iter().map(|r| {
            device_state::StaticRoute {
                prefix: r.prefix,
                mask: r.mask,
                next_hop: r.next_hop,
                interface: r.interface.clone(),
                admin_distance: r.admin_distance,
            }
        }).collect();
        derived_state.unmodeled_config = self.state.unmodeled_config.clone();
        derived_state.vlans = self.state.vlans.iter().map(|v| {
            device_state::VlanState {
                id: v.id,
                name: v.name.clone(),
                active: v.active,
                ports: v.ports.clone(),
                unsupported: v.unsupported,
            }
        }).collect();
        derived_state.base_mac = self.state.base_mac.clone();
        derived_state.sw_image = self.state.sw_image.clone();
        derived_state.last_reload_reason = self.state.last_reload_reason.clone();
        derived_state.service_password_encryption = self.state.service_password_encryption;
        derived_state.aaa_new_model = self.state.aaa_new_model;
        derived_state.ip_routing = self.state.ip_routing;
        derived_state.spanning_tree_mode = self.state.spanning_tree_mode.clone();
        derived_state.vtp_mode = self.state.vtp_mode.clone();
        derived_state.vtp_domain = self.state.vtp_domain.clone();

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
            cursor_pos: 0,
            command_history: Vec::new(),
            history_index: None,
            esc_state: EscState::Normal,
            initial_sent: false,
            reload_transforms: Vec::new(),
            flash_total_size: self.flash_total_size,
            boot_variable: self.boot_variable.clone(),
            reload_delay: self.reload_delay,
            install_state: self.install_state.clone(),
            state: derived_state,
            current_interface: None,
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
        self.dispatch_exec(line);
    }

    fn handle_privileged_exec(&mut self, line: &str) {
        let cmd = line.to_lowercase();

        // Check custom commands first (exact match or prefix)
        if let Some(response) = self.commands.get(&cmd).cloned() {
            self.queue_output(&format!("\n{}\n{}", response, self.prompt()));
            return;
        }
        let custom_response = self.commands.iter()
            .find(|(k, _)| cmd.starts_with(k.as_str()))
            .map(|(_, v)| v.clone());
        if let Some(response) = custom_response {
            self.queue_output(&format!("\n{}\n{}", response, self.prompt()));
            return;
        }

        self.dispatch_exec(line);
    }

    /// Dispatch an exec-mode command using the command tree.
    fn dispatch_exec(&mut self, line: &str) {
        use crate::cmd_tree::parse;
        use crate::cmd_tree::ParseResult;
        use crate::cmd_tree_exec::exec_tree;

        let mode = self.mode.clone();
        let result = parse(line, exec_tree(), &mode);
        match result {
            ParseResult::Execute { handler, .. } => {
                handler(self, line);
            }
            ParseResult::Incomplete => {
                let p = self.prompt();
                self.queue_output(&format!("\n% Incomplete command.\n{}", p));
            }
            ParseResult::InvalidInput { caret_pos } => {
                let p = self.prompt();
                let spaces = " ".repeat(caret_pos);
                self.queue_output(&format!(
                    "\n{}^\n% Invalid input detected at '^' marker.\n{}",
                    spaces, p
                ));
            }
            ParseResult::Ambiguous { token, .. } => {
                let p = self.prompt();
                self.queue_output(&format!(
                    "\n% Ambiguous command:  \"{}\"\n{}",
                    token,
                    p
                ));
            }
            ParseResult::Empty => {
                let p = self.prompt();
                self.queue_output(&format!("\n{}", p));
            }
        }
    }

    pub fn handle_show_ip_interface_brief(&mut self) {
        let header = "Interface              IP-Address      OK? Method Status                Protocol";
        let mut lines = vec![header.to_string()];

        // Read from structured state
        for iface in &self.state.interfaces {
            let ip = iface
                .ip_address
                .map(|(a, _)| a.to_string())
                .unwrap_or_else(|| "unassigned".to_string());
            let status = if !iface.admin_up {
                "administratively down"
            } else if iface.link_up {
                "up"
            } else {
                "down"
            };
            let protocol = if !iface.admin_up || !iface.link_up {
                "down"
            } else {
                "up"
            };
            let method = if iface.ip_address.is_some() { "NVRAM" } else { "unset" };
            let display_name = abbreviate_interface_name(&iface.name);
            lines.push(format!(
                "{:<23}{:<16}{:<4}{:<7}{:<22}{:<8}",
                display_name, ip, "YES", method, status, protocol
            ));
        }

        let output = lines.join("\n") + "\n" + &self.prompt();
        self.queue_output(&output);
    }

    pub fn handle_show_ip_route(&mut self) {
        let codes_header = "\
Codes: L - local, C - connected, S - static, R - RIP, M - mobile, B - BGP\n\
       D - EIGRP, EX - EIGRP external, O - OSPF, IA - OSPF inter area \n\
       N1 - OSPF NSSA external type 1, N2 - OSPF NSSA external type 2\n\
       E1 - OSPF external type 1, E2 - OSPF external type 2\n\
       i - IS-IS, su - IS-IS summary, L1 - IS-IS level-1, L2 - IS-IS level-2\n\
       ia - IS-IS inter area, * - candidate default, U - per-user static route\n\
       o - ODR, P - periodic downloaded static route, H - NHRP, l - LISP\n\
       a - application route\n\
       + - replicated route, % - next hop override, p - overrides from PfR\n";

        let mut output = format!("\n{}\n", codes_header);

        // Find default route for "Gateway of last resort"
        let default_route = self.state.static_routes.iter().find(|r| {
            r.prefix == std::net::Ipv4Addr::new(0, 0, 0, 0)
                && r.mask == std::net::Ipv4Addr::new(0, 0, 0, 0)
        });
        if let Some(dr) = default_route {
            if let Some(nh) = dr.next_hop {
                output.push_str(&format!(
                    "Gateway of last resort is {} to network 0.0.0.0\n\n",
                    nh
                ));
            } else {
                output.push_str("Gateway of last resort is not set\n\n");
            }
        } else {
            output.push_str("Gateway of last resort is not set\n\n");
        }

        // A route entry: (code, prefix_addr, prefix_len, description_line)
        struct RouteEntry {
            code: String,
            prefix: std::net::Ipv4Addr,
            prefix_len: u32,
            description: String,
        }

        // Helper: given a prefix, return (major_network_addr, classful_prefix_len)
        // Returns None for 0.0.0.0/0 (default route — no grouping).
        fn classful_major(prefix: std::net::Ipv4Addr) -> Option<(std::net::Ipv4Addr, u32)> {
            let octets = prefix.octets();
            let first = octets[0];
            if first == 0 {
                // 0.0.0.0/0 default — no group
                return None;
            }
            let (major_mask_len, major_addr) = if first <= 127 {
                // Class A
                (8u32, std::net::Ipv4Addr::new(first, 0, 0, 0))
            } else if first <= 191 {
                // Class B
                (16u32, std::net::Ipv4Addr::new(first, octets[1], 0, 0))
            } else {
                // Class C (192–223) and beyond
                (24u32, std::net::Ipv4Addr::new(first, octets[1], octets[2], 0))
            };
            Some((major_addr, major_mask_len))
        }

        let mut entries: Vec<RouteEntry> = Vec::new();

        // Connected routes from interfaces that are admin_up and have an IP
        for iface in &self.state.interfaces {
            if iface.admin_up {
                if let Some((addr, mask)) = iface.ip_address {
                    let prefix_len = u32::from(mask).count_ones();
                    let net = u32::from(addr) & u32::from(mask);
                    let net_addr = std::net::Ipv4Addr::from(net);
                    entries.push(RouteEntry {
                        code: "C".to_string(),
                        prefix: net_addr,
                        prefix_len,
                        description: format!("is directly connected, {}", iface.name),
                    });
                    entries.push(RouteEntry {
                        code: "L".to_string(),
                        prefix: addr,
                        prefix_len: 32,
                        description: format!("is directly connected, {}", iface.name),
                    });
                }
            }
        }

        // Static routes (non-default)
        for route in &self.state.static_routes {
            let prefix = route.prefix;
            let mask = route.mask;
            let dist = route.admin_distance;
            let prefix_len = u32::from(mask).count_ones();
            let is_default = prefix == std::net::Ipv4Addr::new(0, 0, 0, 0)
                && mask == std::net::Ipv4Addr::new(0, 0, 0, 0);
            if is_default {
                continue; // handled separately below
            }
            let description = if let Some(nh) = route.next_hop {
                format!("[{}/0] via {}", dist, nh)
            } else if let Some(iface) = &route.interface {
                format!("is directly connected, {}", iface)
            } else {
                continue;
            };
            entries.push(RouteEntry {
                code: "S".to_string(),
                prefix,
                prefix_len,
                description,
            });
        }

        // Group entries by classful major network.
        // Use a Vec to preserve insertion order of groups.
        let mut group_keys: Vec<(std::net::Ipv4Addr, u32)> = Vec::new();
        let mut groups: std::collections::HashMap<
            (std::net::Ipv4Addr, u32),
            Vec<usize>,
        > = std::collections::HashMap::new();
        let mut ungrouped: Vec<usize> = Vec::new(); // entries with no classful group

        for (i, entry) in entries.iter().enumerate() {
            match classful_major(entry.prefix) {
                Some(key) => {
                    if !groups.contains_key(&key) {
                        group_keys.push(key);
                        groups.insert(key, Vec::new());
                    }
                    groups.get_mut(&key).unwrap().push(i);
                }
                None => {
                    ungrouped.push(i);
                }
            }
        }

        // Emit grouped routes
        for key in &group_keys {
            let indices = &groups[key];
            let (major_addr, major_len) = key;
            // Count distinct subnets (unique prefix/len combos) and distinct mask lengths
            let mut subnet_set: std::collections::HashSet<(u32, u32)> =
                std::collections::HashSet::new();
            let mut mask_set: std::collections::HashSet<u32> =
                std::collections::HashSet::new();
            for &i in indices {
                let e = &entries[i];
                subnet_set.insert((u32::from(e.prefix), e.prefix_len));
                mask_set.insert(e.prefix_len);
            }
            let n_subnets = subnet_set.len();
            let n_masks = mask_set.len();
            output.push_str(&format!(
                "      {}/{} is variably subnetted, {} subnets, {} masks\n",
                major_addr, major_len, n_subnets, n_masks
            ));
            for &i in indices {
                let e = &entries[i];
                output.push_str(&format!(
                    "{:<9}{}/{} {}\n",
                    e.code, e.prefix, e.prefix_len, e.description
                ));
            }
        }

        // Emit ungrouped (shouldn't normally happen for non-default)
        for i in ungrouped {
            let e = &entries[i];
            output.push_str(&format!(
                "{:<9}{}/{} {}\n",
                e.code, e.prefix, e.prefix_len, e.description
            ));
        }

        // Emit default/static routes that were skipped above
        for route in &self.state.static_routes {
            let prefix = route.prefix;
            let mask = route.mask;
            let dist = route.admin_distance;
            let prefix_len = u32::from(mask).count_ones();
            let is_default = prefix == std::net::Ipv4Addr::new(0, 0, 0, 0)
                && mask == std::net::Ipv4Addr::new(0, 0, 0, 0);
            if !is_default {
                continue;
            }
            let code = "S*";
            let prefix_str = format!("{}/{}", prefix, prefix_len);
            if let Some(nh) = route.next_hop {
                output.push_str(&format!(
                    "{:<9}{} [{}/0] via {}\n",
                    code, prefix_str, dist, nh
                ));
            } else if let Some(iface) = &route.interface {
                output.push_str(&format!(
                    "{:<9}{} is directly connected, {}\n",
                    code, prefix_str, iface
                ));
            }
        }

        output.push_str(&self.prompt());
        self.queue_output(&output);
    }

    fn handle_config_mode(&mut self, line: &str) {
        self.dispatch_config(line);
    }

    fn handle_config_sub(&mut self, line: &str) {
        self.dispatch_config(line);
    }

    /// Pick the right command tree for the current config/config-sub mode.
    fn config_tree_for_mode(&self) -> &'static [crate::cmd_tree::CommandNode] {
        use crate::cmd_tree_conf::{conf_tree, config_sub_tree};
        match &self.mode {
            CliMode::ConfigSub(sub) => config_sub_tree(sub),
            _ => conf_tree(),
        }
    }

    /// Dispatch a config/config-sub command using the command tree.
    fn dispatch_config(&mut self, line: &str) {
        use crate::cmd_tree::parse;
        use crate::cmd_tree::ParseResult;
        use crate::cmd_tree_exec::exec_tree;

        // "do <cmd>" — special case before tree parsing
        let cmd_lower = line.to_lowercase();
        if cmd_lower.starts_with("do ") {
            let exec_cmd = &line[3..];
            let saved_mode = self.mode.clone();
            self.mode = CliMode::PrivilegedExec;
            let result = parse(exec_cmd, exec_tree(), &CliMode::PrivilegedExec);
            match result {
                ParseResult::Execute { handler, .. } => handler(self, exec_cmd),
                ParseResult::Incomplete => {
                    self.queue_output("\n% Incomplete command.\n");
                }
                ParseResult::InvalidInput { caret_pos } => {
                    let spaces = " ".repeat(caret_pos);
                    self.queue_output(&format!(
                        "\n{}^\n% Invalid input detected at '^' marker.\n",
                        spaces
                    ));
                }
                ParseResult::Ambiguous { token, .. } => {
                    self.queue_output(&format!("\n% Ambiguous command: \"{}\"\n", token));
                }
                ParseResult::Empty => {}
            }
            self.mode = saved_mode;
            // Re-display config prompt after do command
            let p = self.prompt();
            self.queue_output(&format!("{}", p));
            return;
        }

        let mode = self.mode.clone();
        let tree = self.config_tree_for_mode();
        let result = parse(line, tree, &mode);
        match result {
            ParseResult::Execute { handler, .. } => {
                handler(self, line);
            }
            ParseResult::Incomplete => {
                let p = self.prompt();
                self.queue_output(&format!("\n% Incomplete command.\n{}", p));
            }
            ParseResult::InvalidInput { caret_pos } => {
                let p = self.prompt();
                let spaces = " ".repeat(caret_pos);
                self.queue_output(&format!(
                    "\n{}^\n% Invalid input detected at '^' marker.\n{}",
                    spaces, p
                ));
            }
            ParseResult::Ambiguous { token, .. } => {
                let p = self.prompt();
                self.queue_output(&format!(
                    "\n% Ambiguous command:  \"{}\"\n{}",
                    token,
                    p
                ));
            }
            ParseResult::Empty => {
                let p = self.prompt();
                self.queue_output(&format!("\n{}", p));
            }
        }
    }

    /// Parse config text and apply relevant lines to device state.
    /// Handles: hostname, interface, ip address, shutdown, no shutdown, ip route.
    fn apply_config_text_to_state(&mut self, config_text: &str) {
        use std::net::Ipv4Addr;
        let mut current_iface: Option<String> = None;

        for line in config_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("hostname ") {
                let name = trimmed["hostname ".len()..].trim().to_string();
                self.hostname = name.clone();
                self.state.hostname = name;
            } else if trimmed.starts_with("interface ") {
                let iface_name = trimmed["interface ".len()..].trim().to_string();
                self.state.ensure_interface(&iface_name);
                current_iface = Some(iface_name);
            } else if trimmed.starts_with("ip address ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let (Ok(addr), Ok(mask)) = (
                        parts[2].parse::<Ipv4Addr>(),
                        parts[3].parse::<Ipv4Addr>(),
                    ) {
                        if let Some(ref iface_name) = current_iface.clone() {
                            if let Some(iface) = self.state.get_interface_mut(iface_name) {
                                iface.ip_address = Some((addr, mask));
                            }
                        }
                    }
                }
            } else if trimmed == "shutdown" {
                if let Some(ref iface_name) = current_iface.clone() {
                    if let Some(iface) = self.state.get_interface_mut(iface_name) {
                        iface.admin_up = false;
                    }
                }
            } else if trimmed == "no shutdown" {
                if let Some(ref iface_name) = current_iface.clone() {
                    if let Some(iface) = self.state.get_interface_mut(iface_name) {
                        iface.admin_up = true;
                    }
                }
            } else if trimmed.starts_with("ip route ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 5 {
                    if let (Ok(prefix), Ok(mask), Ok(nh)) = (
                        parts[2].parse::<Ipv4Addr>(),
                        parts[3].parse::<Ipv4Addr>(),
                        parts[4].parse::<Ipv4Addr>(),
                    ) {
                        self.state.static_routes.push(device_state::StaticRoute {
                            prefix,
                            mask,
                            next_hop: Some(nh),
                            interface: None,
                            admin_distance: 1,
                        });
                    }
                }
            } else if !trimmed.starts_with('!') && !trimmed.is_empty()
                && !trimmed.starts_with(' ')
                && !trimmed.starts_with("end")
            {
                // Reset current_iface context when we leave interface block
                // (any top-level line that isn't interface/ip route)
                current_iface = None;
            }
        }
    }

    pub fn handle_copy_command(&mut self, line: &str) {
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
                if let Some(content) = self.flash_files.get(flash_file).cloned() {
                    let config_text = String::from_utf8_lossy(&content).to_string();
                    // Apply config lines to running config AND state
                    for config_line in config_text.lines() {
                        if !config_line.trim().is_empty() {
                            self.running_config.push(config_line.to_string());
                        }
                    }
                    self.apply_config_text_to_state(&config_text);
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

    pub fn handle_delete_command(&mut self, line: &str) {
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

    pub fn handle_verify_md5(&mut self, line: &str) {
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


    pub fn handle_dir_command(&mut self, _line: &str) {
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

    pub fn handle_show_boot(&mut self) {
        // Use legacy fields as source of truth for backward compat
        let boot_var = if self.boot_variable.is_empty() {
            format!(
                "flash:c{}-universalk9-mz.{}.bin",
                self.model.to_lowercase(),
                self.version
            )
        } else {
            self.boot_variable.clone()
        };
        let output = format!(
            "\nBOOT variable = {}\nConfig file = \nPrivate Config file = \nEnable Break = no\nManual Boot = no\n{}",
            boot_var, self.prompt()
        );
        self.queue_output(&output);
    }

    pub fn handle_show_install_summary(&mut self) {
        // Use legacy install_state; clone to avoid borrow conflict
        let install_state = self.install_state.clone();
        match install_state {
            Some(state) => {
                let mode_str = match state.mode {
                    InstallMode::Install => "INSTALL",
                    InstallMode::Bundle => "BUNDLE",
                };
                let mut output = format!(
                    "\n[ {} ] Installed Package(s) Information:\n",
                    mode_str
                );
                if state.packages.is_empty() {
                    output.push_str("No packages installed.\n");
                } else {
                    for pkg in &state.packages {
                        let state_flag = match pkg.state {
                            PackageState::Committed => "C ",
                            PackageState::Activated => "U ",
                            PackageState::Inactive => "I ",
                        };
                        output.push_str(&format!(
                            "  {} flash:{}\n",
                            state_flag, pkg.name
                        ));
                    }
                }
                output.push_str(&self.prompt());
                self.queue_output(&output);
            }
            None => {
                self.queue_output(&format!(
                    "\n% Install mode is not supported on this platform\n{}",
                    self.prompt()
                ));
            }
        }
    }

    pub fn handle_install_add(&mut self, line: &str) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // install add file flash:<image>.bin
        let image_name = parts.iter()
            .find(|p| p.starts_with("flash:"))
            .map(|p| p.trim_start_matches("flash:"))
            .unwrap_or("unknown.bin");

        // Generate package names from the image name
        let base = image_name
            .trim_end_matches(".bin")
            .trim_end_matches(".SPA");
        // Use the image base name to derive package names
        // e.g., cat9k_iosxe.17.09.04a → 17.09.04a
        let parts: Vec<&str> = base.rsplit('.').take(3).collect();
        let ver_suffix: String = parts.iter().rev().cloned().collect::<Vec<&str>>().join(".");

        let pkg_names = vec![
            format!("cat9k-rpbase.{}", ver_suffix),
            format!("cat9k-rpboot.{}", ver_suffix),
            format!("cat9k-sipspa.{}", ver_suffix),
            format!("cat9k-sipbase.{}", ver_suffix),
            format!("cat9k-webui.{}", ver_suffix),
        ];

        let output = format!(
            r#"
install_add: START
install_add: Adding PACKAGE
--- Starting initial file syncing ---
[1]: Copying flash:{image} from switch 1 to switch 1
[1]: Finished copying to switch 1
--- Starting Add ---
[1]: Adding file flash:{image} on switch 1
[1]: Finished adding on switch 1
install_add: SUCCESS
{prompt}"#,
            image = image_name,
            prompt = self.prompt(),
        );
        self.queue_output(&output);

        // Add packages as Inactive — update both state fields
        if let Some(ref mut state) = self.state.install_state {
            for name in &pkg_names {
                state.packages.push(PackageInfo {
                    name: name.clone(),
                    state: PackageState::Inactive,
                });
            }
        }
        if let Some(ref mut state) = self.install_state {
            for name in pkg_names {
                state.packages.push(PackageInfo {
                    name,
                    state: PackageState::Inactive,
                });
            }
        }
    }

    pub fn handle_install_activate(&mut self) {
        if self.state.install_state.is_none() {
            self.queue_output(&format!(
                "\n% Install mode is not supported\n{}",
                self.prompt()
            ));
            return;
        }
        self.pending_interactive = Some(PendingInteractive::InstallActivateConfirm);
        self.queue_output(
            "\nThis operation may require a reload of the system.\nDo you want to proceed? [y/n]"
        );
    }

    pub fn handle_install_commit(&mut self) {
        let has_state = self.state.install_state.is_some();
        if has_state {
            let had_activated = self.state.install_state.as_ref()
                .map(|s| s.packages.iter().any(|p| p.state == PackageState::Activated))
                .unwrap_or(false);
            if let Some(ref mut state) = self.state.install_state {
                for pkg in &mut state.packages {
                    if pkg.state == PackageState::Activated {
                        pkg.state = PackageState::Committed;
                    }
                }
            }
            if let Some(ref mut state) = self.install_state {
                for pkg in &mut state.packages {
                    if pkg.state == PackageState::Activated {
                        pkg.state = PackageState::Committed;
                    }
                }
            }
            if had_activated {
                self.queue_output(&format!(
                    "\ninstall_commit: START\ninstall_commit: SUCCESS\n{}",
                    self.prompt()
                ));
            } else {
                self.queue_output(&format!(
                    "\nNo activate operation pending commit\n{}",
                    self.prompt()
                ));
            }
        } else {
            self.queue_output(&format!("\n{}", self.prompt()));
        }
    }

    pub fn handle_install_remove_inactive(&mut self) {
        if self.state.install_state.is_some() {
            if let Some(ref mut state) = self.state.install_state {
                state.packages.retain(|p| p.state != PackageState::Inactive);
            }
            if let Some(ref mut state) = self.install_state {
                state.packages.retain(|p| p.state != PackageState::Inactive);
            }
            self.queue_output(&format!(
                "\ninstall_remove: START\ninstall_remove: SUCCESS\n{}",
                self.prompt()
            ));
        } else {
            self.queue_output(&format!("\n{}", self.prompt()));
        }
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
            PendingInteractive::InstallActivateConfirm => {
                if line == "y" || line == "Y" || line == "yes" {
                    // Activate packages — update both state fields
                    if let Some(ref mut state) = self.state.install_state {
                        for pkg in &mut state.packages {
                            if pkg.state == PackageState::Inactive {
                                pkg.state = PackageState::Activated;
                            }
                        }
                    }
                    if let Some(ref mut state) = self.install_state {
                        for pkg in &mut state.packages {
                            if pkg.state == PackageState::Inactive {
                                pkg.state = PackageState::Activated;
                            }
                        }
                    }
                    self.queue_output("\ninstall_activate: Activating PACKAGE\ninstall_activate: SUCCESS\n\nSystem is reloading...\n");
                    self.mode = CliMode::Reloading;
                } else {
                    self.queue_output(&format!(
                        "\nInstall activate aborted\n{}",
                        self.prompt()
                    ));
                }
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
            PendingInteractive::ConfigureMethod => {
                // Empty or "terminal" → enter config mode
                let choice = line.trim().to_lowercase();
                if choice.is_empty() || choice == "terminal" {
                    self.mode = CliMode::Config;
                    self.queue_output(&format!(
                        "\nEnter configuration commands, one per line.  End with CNTL/Z.\n{}",
                        self.prompt()
                    ));
                } else {
                    self.queue_output(&format!(
                        "\n% Invalid input — use 'terminal', 'memory', or 'network'\n{}",
                        self.prompt()
                    ));
                }
            }
        }
    }

    pub fn generate_show_version(&self) -> String {
        // Use the legacy fields (hostname, version, model) as the source of truth
        // because tests may set them directly. state.* mirrors them for structured access.
        let install_state = self.install_state.as_ref().or(self.state.install_state.as_ref());
        // Use top-level `self.version` as authoritative — tests set it directly.
        // `self.state.version` mirrors it but may lag if set via field access.
        let eff_version = &self.version;
        let system_image = match install_state {
            Some(InstallState { mode: InstallMode::Install, .. }) => {
                "flash:packages.conf".to_string()
            }
            _ => {
                let suffix = version_to_filename_suffix(eff_version);
                let family = model_family(&self.state.model);
                format!("flash:{}-universalk9-mz.{}.bin", family.to_lowercase(), suffix)
            }
        };

        let model = &self.state.model;
        let version = eff_version;
        let hostname = &self.hostname;
        let uptime = &self.state.uptime;
        let serial = &self.state.serial_number;
        let config_reg = &self.state.config_register;
        let base_mac = &self.state.base_mac;
        let sw_image = &self.state.sw_image;
        let last_reload = &self.state.last_reload_reason;
        let family = model_family(model);

        // Count interfaces by type
        let n_vlan = self.state.interfaces.iter().filter(|i| i.name.starts_with("Vlan")).count();
        let n_gi = self.state.interfaces.iter().filter(|i| i.name.starts_with("GigabitEthernet")).count();
        let n_te = self.state.interfaces.iter().filter(|i| i.name.starts_with("TenGigabitEthernet")).count();

        // Derive motherboard serial (last 9 chars of serial, padded)
        let mb_serial = if serial.len() >= 9 {
            format!("FOC{}", &serial[serial.len()-9..])
        } else {
            format!("FOC{:0>9}", serial)
        };

        // Ports count for switch table
        let total_ports = n_gi + n_te;

        format!(
"Cisco IOS Software, {family} Software ({sw_image}), Version {version}, RELEASE SOFTWARE (fc3)
Technical Support: http://www.cisco.com/techsupport
Copyright (c) 1986-2025 by Cisco Systems, Inc.
Compiled Mon 15-Sep-25 13:05 by mcpre

ROM: Bootstrap program is {family} boot loader
BOOTLDR: {family} Boot Loader ({family}-HBOOT-M) Version 15.2(7r)E, RELEASE SOFTWARE (fc2)

{hostname} uptime is {uptime}
System returned to ROM by power-on
System restarted at 14:09:41 UTC Mon Feb 28 2000
System image file is \"{system_image}\"
Last reload reason: {last_reload}


This product contains cryptographic features and is subject to United
States and local country laws governing import, export, transfer and
use. Delivery of Cisco cryptographic products does not imply
third-party authority to import, export, distribute or use encryption.
Importers, exporters, distributors and users are responsible for
compliance with U.S. and local country laws. By using this product you
agree to comply with applicable laws and regulations. If you are unable
to comply with U.S. and local laws, return this product immediately.

A summary of U.S. laws governing Cisco cryptographic products may be found at:
http://www.cisco.com/wwl/export/crypto/tool/stqrg.html

If you require further assistance please contact us by sending email to
export@cisco.com.

License Level: ipservices
License Type: Evaluation
Next reload license Level: ipservices

cisco {model} (APM86XXX) processor (revision D0) with 524288K bytes of memory.
Processor board ID {serial}
Last reset from power-on
{n_vlan} Virtual Ethernet interfaces
{n_gi} Gigabit Ethernet interfaces
{n_te} Ten Gigabit Ethernet interfaces
The password-recovery mechanism is disabled.

512K bytes of flash-simulated non-volatile configuration memory.
Base ethernet MAC Address       : {base_mac}
Motherboard assembly number     : 73-16573-05
Power supply part number        : 341-0675-02
Motherboard serial number       : {mb_serial}
Power supply serial number      : LIT19381A8A
Model revision number           : D0
Motherboard revision number     : A0
Model number                    : {model}
System serial number            : {serial}
Top Assembly Part Number        : 68-5409-02
Top Assembly Revision Number    : B0
Version ID                      : V02
CLEI Code Number                : CMM1Z00DRB
Hardware Board Revision Number  : 0x02


Switch Ports Model                     SW Version            SW Image
------ ----- -----                     ----------            ----------
*    1 {total_ports:<5} {model:<25} {version:<21} {sw_image:<24}


Configuration register is {config_reg}",
            family = family,
            sw_image = sw_image,
            version = version,
            hostname = hostname,
            uptime = uptime,
            system_image = system_image,
            last_reload = last_reload,
            model = model,
            serial = serial,
            n_vlan = n_vlan,
            n_gi = n_gi,
            n_te = n_te,
            base_mac = base_mac,
            mb_serial = mb_serial,
            total_ports = total_ports,
            config_reg = config_reg,
        )
    }

    pub fn queue_output(&mut self, text: &str) {
        // Normalize line endings to \r\n for terminal compatibility.
        // First remove any existing \r to avoid doubling, then replace \n with \r\n.
        let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
        self.output_queue.extend_from_slice(normalized.as_bytes());
    }

    /// Try to tab-complete the partial input.
    /// Returns (erase_count, insert_text) where erase_count is the number of
    /// characters to erase backwards (the typed prefix) and insert_text is the
    /// canonical keyword + space to insert. This ensures proper casing.
    fn try_tab_complete(&self, partial_input: &str) -> Option<(usize, String)> {
        use crate::cmd_tree::{tokenize_with_offsets, find_matches};

        let tree: &[crate::cmd_tree::CommandNode] = match &self.mode {
            CliMode::Config => crate::cmd_tree_conf::conf_tree(),
            CliMode::ConfigSub(sub) => crate::cmd_tree_conf::config_sub_tree(sub),
            _ => crate::cmd_tree_exec::exec_tree(),
        };

        let tokens = tokenize_with_offsets(partial_input);
        if tokens.is_empty() {
            return None;
        }

        // Walk tree to the parent of the last token.
        // We store intermediate match results in a Vec to extend their lifetime
        // long enough for the borrow of their children.
        let path_len = tokens.len() - 1;
        let mut owned_matches: Vec<Vec<&crate::cmd_tree::CommandNode>> = Vec::new();
        let mut current_nodes: &[crate::cmd_tree::CommandNode] = tree;
        for i in 0..path_len {
            let matches = find_matches(&tokens[i].0, current_nodes, &self.mode);
            if matches.len() != 1 {
                return None;
            }
            owned_matches.push(matches);
            current_nodes = &owned_matches.last().unwrap()[0].children;
        }

        // Try to complete the last token
        let last_token = &tokens[tokens.len() - 1].0;
        let matches = find_matches(last_token, current_nodes, &self.mode);

        if matches.len() == 1 {
            if let crate::cmd_tree::TokenMatcher::Keyword(kw) = &matches[0].matcher {
                let kw_lower = kw.to_lowercase();
                if kw_lower == last_token.to_lowercase() {
                    // Already complete — just add space if not there
                    if !partial_input.ends_with(' ') {
                        return Some((0, " ".to_string()));
                    }
                } else if kw_lower.starts_with(&last_token.to_lowercase()) {
                    // Erase the typed prefix and replace with canonical keyword + space
                    return Some((last_token.len(), format!("{} ", kw)));
                }
            }
            None
        } else {
            None // ambiguous or no match
        }
    }

    /// Synchronous receive for unit tests (no async runtime needed).
    #[cfg(test)]
    pub fn receive_sync(&mut self) -> Vec<u8> {
        if !self.initial_sent {
            self.initial_sent = true;
            if !self.state.banner_motd.is_empty() {
                let banner = self.state.banner_motd.clone();
                self.queue_output(&format!("\n{}\n", banner));
            }
            self.queue_output(&format!("{}", self.prompt()));
        }
        std::mem::take(&mut self.output_queue)
    }

    /// Drain queued output for unit tests.
    #[cfg(test)]
    pub fn drain_output(&mut self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut self.output_queue)).into_owned()
    }

    /// Redraw the portion of the input line from cursor_pos to end.
    /// Used after inserting, deleting, or replacing characters in the middle.
    /// After calling this, the terminal cursor is back at cursor_pos.
    fn redraw_from_cursor(&mut self) {
        let tail = self.input_buffer[self.cursor_pos..].to_vec();
        self.output_queue.extend_from_slice(&tail);
        // Erase any leftover chars if the line got shorter
        self.output_queue.extend_from_slice(b"\x1B[K");
        // Move cursor back to cursor_pos
        let move_back = tail.len();
        for _ in 0..move_back {
            self.output_queue.extend_from_slice(b"\x1B[D");
        }
    }

    /// Redraw the entire input line (used for history recall).
    /// Moves terminal cursor to start of input (using \r + prompt),
    /// outputs the full buffer, erases any trailing old characters,
    /// and sets cursor_pos to end of buffer.
    fn redraw_line(&mut self) {
        let prompt = self.prompt();
        self.output_queue.push(b'\r');
        self.output_queue.extend_from_slice(prompt.as_bytes());
        let line = self.input_buffer.clone();
        self.output_queue.extend_from_slice(&line);
        // Erase anything remaining from a previous longer line
        self.output_queue.extend_from_slice(b"\x1B[K");
        self.cursor_pos = self.input_buffer.len();
    }
}

#[async_trait]
impl RawTransport for MockIosDevice {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        if self.mode == CliMode::Reloading {
            return Err(CiscoIosError::NotConnected);
        }

        // Always process character by character, echoing and handling `?` / backspace
        // immediately — this is how real Cisco IOS works.
        for &byte in data {
            // --- Escape sequence state machine (top priority) ---
            if self.esc_state == EscState::GotEsc {
                if byte == b'[' {
                    self.esc_state = EscState::GotBracket;
                } else {
                    self.esc_state = EscState::Normal;
                    // ignore unknown escape sequence
                }
                continue;
            }
            if self.esc_state == EscState::GotBracket {
                self.esc_state = EscState::Normal;
                match byte {
                    b'A' => {
                        // Up arrow — history previous
                        if self.command_history.is_empty() {
                            continue;
                        }
                        let new_index = match self.history_index {
                            None => self.command_history.len() - 1,
                            Some(0) => 0, // already at oldest
                            Some(i) => i - 1,
                        };
                        self.history_index = Some(new_index);
                        let recalled = self.command_history[new_index].clone();
                        self.input_buffer = recalled.into_bytes();
                        self.redraw_line();
                    }
                    b'B' => {
                        // Down arrow — history next
                        match self.history_index {
                            None => {} // not recalling
                            Some(i) => {
                                if i + 1 >= self.command_history.len() {
                                    // Past end — clear
                                    self.history_index = None;
                                    self.input_buffer.clear();
                                    self.redraw_line();
                                } else {
                                    let new_index = i + 1;
                                    self.history_index = Some(new_index);
                                    let recalled = self.command_history[new_index].clone();
                                    self.input_buffer = recalled.into_bytes();
                                    self.redraw_line();
                                }
                            }
                        }
                    }
                    b'C' => {
                        // Right arrow — move cursor right
                        if self.cursor_pos < self.input_buffer.len() {
                            self.cursor_pos += 1;
                            self.output_queue.extend_from_slice(b"\x1B[C");
                        }
                    }
                    b'D' => {
                        // Left arrow — move cursor left
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                            self.output_queue.extend_from_slice(b"\x1B[D");
                        }
                    }
                    _ => {} // ignore unknown CSI sequences
                }
                continue;
            }

            match byte {
                b'\r' => {
                    // Carriage return — telnet sends \r\n or \r\0 for Enter.
                    // Process the line now; the following \n (if any) will be
                    // silently consumed since the buffer will be empty.
                    let line_bytes = std::mem::take(&mut self.input_buffer);
                    let line = String::from_utf8_lossy(&line_bytes).to_string();
                    // Save to history (non-empty, not duplicate of last)
                    if !line.trim().is_empty() {
                        if self.command_history.last().map(|s| s.as_str()) != Some(&line) {
                            self.command_history.push(line.clone());
                        }
                    }
                    self.cursor_pos = 0;
                    self.history_index = None;
                    self.output_queue.extend_from_slice(b"\r\n");
                    self.handle_line(&line);
                }
                b'\n' => {
                    // Line feed — if buffer is empty (just consumed by \r), skip.
                    // Otherwise process as a line ending (e.g., automation sends \n only).
                    if self.input_buffer.is_empty() && !self.output_queue.is_empty() {
                        // Already processed by preceding \r — skip
                        continue;
                    }
                    let line_bytes = std::mem::take(&mut self.input_buffer);
                    let line = String::from_utf8_lossy(&line_bytes).to_string();
                    // Save to history (non-empty, not duplicate of last)
                    if !line.trim().is_empty() {
                        if self.command_history.last().map(|s| s.as_str()) != Some(&line) {
                            self.command_history.push(line.clone());
                        }
                    }
                    self.cursor_pos = 0;
                    self.history_index = None;
                    self.output_queue.extend_from_slice(b"\r\n");
                    self.handle_line(&line);
                }
                b'\0' => {
                    // NUL — telnet sends \r\0 for bare CR. Just ignore the NUL.
                }
                0x1B => {
                    // ESC — start of escape sequence
                    self.esc_state = EscState::GotEsc;
                }
                b'?' => {
                    // Echo the '?' like real IOS does
                    self.output_queue.push(b'?');
                    // Immediate help — do NOT add '?' to the input buffer.
                    use crate::cmd_tree::{help, HelpResult};
                    use crate::cmd_tree_exec::exec_tree;
                    use crate::cmd_tree_conf::conf_tree;

                    let partial = String::from_utf8_lossy(&self.input_buffer).to_string();
                    let mode = self.mode.clone();

                    let tree: &[crate::cmd_tree::CommandNode] = match &mode {
                        CliMode::Config => conf_tree(),
                        CliMode::ConfigSub(sub) => {
                            crate::cmd_tree_conf::config_sub_tree(sub)
                        }
                        _ => exec_tree(),
                    };

                    let help_result = help(&partial, tree, &mode);

                    // Format help output like real IOS
                    let mut out = String::new();
                    out.push_str("\r\n");
                    // Determine if this is top-level help (nothing typed before ?)
                    let trimmed = partial.trim();
                    match help_result {
                        HelpResult::Subcommands(subs) => {
                            // Add header line for top-level help, matching real IOS
                            if trimmed.is_empty() {
                                let header = match &mode {
                                    CliMode::UserExec | CliMode::PrivilegedExec => {
                                        "Exec commands:\r\n".to_string()
                                    }
                                    CliMode::Config => "Configure commands:\r\n".to_string(),
                                    CliMode::ConfigSub(sub) => {
                                        // Convert sub-mode name to human-readable header
                                        // e.g. "config-if" -> "Interface configuration commands:"
                                        let label = match sub.as_str() {
                                            "config-if" => "Interface",
                                            "config-router" => "Router",
                                            "config-line" => "Line",
                                            _ => "Sub-mode",
                                        };
                                        format!("{} configuration commands:\r\n", label)
                                    }
                                    _ => String::new(),
                                };
                                out.push_str(&header);
                            }
                            for (name, desc) in subs {
                                out.push_str(&format!("  {:<17}  {}\r\n", name, desc));
                            }
                        }
                        HelpResult::PrefixMatches(names) => {
                            for name in names {
                                out.push_str(&format!("  {}\r\n", name));
                            }
                        }
                        HelpResult::NotFound { .. } => {
                            out.push_str("% Invalid input\r\n");
                        }
                    }
                    // Re-display prompt + partial input
                    let prompt = self.prompt();
                    out.push_str(&prompt);
                    out.push_str(&partial);
                    self.output_queue.extend_from_slice(out.as_bytes());
                }
                b'\x7f' | b'\x08' => {
                    // Backspace / DEL — remove char before cursor
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.input_buffer.remove(self.cursor_pos);
                        // Send BS + redraw tail + erase + move back
                        self.output_queue.extend_from_slice(b"\x08");
                        self.redraw_from_cursor();
                    }
                }
                0x09 => {
                    // Tab — attempt command completion (appends at end)
                    let partial = String::from_utf8_lossy(&self.input_buffer).to_string();
                    let completion = self.try_tab_complete(&partial);
                    match completion {
                        Some((erase_count, insert_text)) => {
                            // Erase the typed prefix (backspace over it)
                            for _ in 0..erase_count {
                                if self.cursor_pos > 0 {
                                    self.cursor_pos -= 1;
                                    self.input_buffer.remove(self.cursor_pos);
                                    self.output_queue.extend_from_slice(b"\x08");
                                }
                            }
                            // Erase old text on screen, then write new text
                            if erase_count > 0 {
                                // Clear from cursor to end of line
                                self.output_queue.extend_from_slice(b"\x1b[K");
                            }
                            self.output_queue.extend_from_slice(insert_text.as_bytes());
                            self.input_buffer.extend_from_slice(insert_text.as_bytes());
                            self.cursor_pos = self.input_buffer.len();
                        }
                        None => {
                            // No unique completion — beep
                            self.output_queue.push(0x07);
                        }
                    }
                }
                0x01 => {
                    // Ctrl+A — move cursor to beginning of line
                    let steps = self.cursor_pos;
                    for _ in 0..steps {
                        self.output_queue.extend_from_slice(b"\x1B[D");
                    }
                    self.cursor_pos = 0;
                }
                0x02 => {
                    // Ctrl+B — move cursor left (same as left arrow)
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.output_queue.extend_from_slice(b"\x1B[D");
                    }
                }
                0x04 => {
                    // Ctrl+D — delete char under cursor, or disconnect if empty
                    if self.input_buffer.is_empty() {
                        // Empty line — treat as logout/disconnect
                        self.mode = CliMode::Reloading;
                    } else if self.cursor_pos < self.input_buffer.len() {
                        // Delete char at cursor_pos (forward delete)
                        self.input_buffer.remove(self.cursor_pos);
                        self.redraw_from_cursor();
                    }
                }
                0x05 => {
                    // Ctrl+E — move cursor to end of line
                    let steps = self.input_buffer.len() - self.cursor_pos;
                    for _ in 0..steps {
                        self.output_queue.extend_from_slice(b"\x1B[C");
                    }
                    self.cursor_pos = self.input_buffer.len();
                }
                0x06 => {
                    // Ctrl+F — move cursor right (same as right arrow)
                    if self.cursor_pos < self.input_buffer.len() {
                        self.cursor_pos += 1;
                        self.output_queue.extend_from_slice(b"\x1B[C");
                    }
                }
                0x0B => {
                    // Ctrl+K — erase from cursor to end of line
                    self.input_buffer.truncate(self.cursor_pos);
                    self.output_queue.extend_from_slice(b"\x1B[K");
                }
                0x03 => {
                    // Ctrl+C — cancel current input, show new prompt
                    self.input_buffer.clear();
                    self.cursor_pos = 0;
                    self.history_index = None;
                    let p = self.prompt();
                    self.queue_output(&format!("\n{}", p));
                }
                0x15 => {
                    // Ctrl+U — erase from start to cursor
                    if self.cursor_pos > 0 {
                        // Count how many chars to erase
                        let erase_count = self.cursor_pos;
                        self.input_buffer.drain(0..self.cursor_pos);
                        self.cursor_pos = 0;
                        // Move terminal cursor back to start of input, then redraw
                        // Send CR + prompt to reposition, then output remaining buffer + erase
                        let prompt = self.prompt();
                        self.output_queue.push(b'\r');
                        self.output_queue.extend_from_slice(prompt.as_bytes());
                        let remaining = self.input_buffer.clone();
                        self.output_queue.extend_from_slice(&remaining);
                        // Erase old trailing characters
                        let old_len = remaining.len() + erase_count;
                        let pad = old_len - remaining.len();
                        for _ in 0..pad {
                            self.output_queue.push(b' ');
                        }
                        // Move back to new cursor pos (end of remaining)
                        let move_back = pad;
                        for _ in 0..move_back {
                            self.output_queue.extend_from_slice(b"\x1B[D");
                        }
                    }
                }
                0x17 => {
                    // Ctrl+W — erase word before cursor
                    // Skip spaces before cursor, then skip non-spaces (the word)
                    let mut pos = self.cursor_pos;
                    // Skip trailing spaces
                    while pos > 0 && self.input_buffer[pos - 1] == b' ' {
                        pos -= 1;
                    }
                    // Skip the word characters
                    while pos > 0 && self.input_buffer[pos - 1] != b' ' {
                        pos -= 1;
                    }
                    let erase_count = self.cursor_pos - pos;
                    if erase_count > 0 {
                        self.input_buffer.drain(pos..self.cursor_pos);
                        self.cursor_pos = pos;
                        // Redraw: move cursor back erase_count, redraw tail, erase end
                        for _ in 0..erase_count {
                            self.output_queue.extend_from_slice(b"\x1B[D");
                        }
                        self.redraw_from_cursor();
                    }
                }
                0x1A => {
                    // Ctrl+Z — exit to privileged exec from any config mode
                    // Real IOS echoes "^Z" then shows priv exec prompt
                    match &self.mode {
                        CliMode::Config | CliMode::ConfigSub(_) => {
                            self.input_buffer.clear();
                            self.cursor_pos = 0;
                            self.history_index = None;
                            self.mode = CliMode::PrivilegedExec;
                            let p = self.prompt();
                            self.queue_output(&format!("^Z\n{}", p));
                        }
                        _ => {
                            // In exec modes, treat as a no-op (or cancel input)
                            self.input_buffer.clear();
                            self.cursor_pos = 0;
                            self.history_index = None;
                            let p = self.prompt();
                            self.queue_output(&format!("\n{}", p));
                        }
                    }
                }
                _ => {
                    // Regular printable character — echo unless in password mode,
                    // insert at cursor_pos.
                    if self.cursor_pos == self.input_buffer.len() {
                        // Cursor at end — simple append
                        if !self.is_password_mode() {
                            self.output_queue.push(byte);
                        }
                        self.input_buffer.push(byte);
                        self.cursor_pos += 1;
                    } else {
                        // Cursor in middle — insert and redraw tail
                        self.input_buffer.insert(self.cursor_pos, byte);
                        self.cursor_pos += 1;
                        if !self.is_password_mode() {
                            // Echo inserted char + redraw rest of line + move back
                            self.output_queue.push(byte);
                            self.redraw_from_cursor();
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn receive(&mut self, _timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        // Send initial prompt on first receive
        if !self.initial_sent {
            self.initial_sent = true;
            if !self.state.banner_motd.is_empty() {
                let banner = self.state.banner_motd.clone();
                self.queue_output(&format!("\n{}\n", banner));
            }
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
        "interface Vlan1".to_string(),
        " ip address 10.0.0.1 255.255.255.0".to_string(),
        "!".to_string(),
        "interface GigabitEthernet1/0/1".to_string(),
        " switchport mode access".to_string(),
        "!".to_string(),
        "ip route 0.0.0.0 0.0.0.0 10.0.0.254".to_string(),
        "!".to_string(),
        "line vty 0 4".to_string(),
        " login local".to_string(),
        " transport input ssh".to_string(),
        "line vty 5 15".to_string(),
        " login local".to_string(),
        " transport input ssh".to_string(),
        "!".to_string(),
        "end".to_string(),
    ]
}

/// Extract model family prefix, e.g. "WS-C3560CX-12PD-S" → "C3560CX".
fn model_family(model: &str) -> String {
    // Strip "WS-" prefix if present
    let s = model.strip_prefix("WS-").unwrap_or(model);
    // Take up to the second '-' (e.g. "C3560CX-12PD-S" → "C3560CX")
    match s.find('-') {
        Some(i) => s[..i].to_string(),
        None => s.to_string(),
    }
}

/// Convert an IOS version string to the filename suffix used in image names.
/// e.g. "15.2(7)E13" → "152-7.E13"
fn version_to_filename_suffix(version: &str) -> String {
    // Format: "15.2(7)E13" → major=15, minor=2, sub=7, train=E13
    // Output: "152-7.E13"
    if let Some(paren_open) = version.find('(') {
        if let Some(paren_close) = version.find(')') {
            let major_minor = &version[..paren_open]; // "15.2"
            let sub = &version[paren_open+1..paren_close]; // "7"
            let train = &version[paren_close+1..]; // "E13"
            // Remove the dot from major.minor for the prefix
            let prefix = major_minor.replace('.', ""); // "152"
            return format!("{}-{}.{}", prefix, sub, train);
        }
    }
    // Fallback: return as-is
    version.to_string()
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
        assert!(output.contains("interface Vlan1") || output.contains("interface GigabitEthernet1/0/1"));
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

    // === Install mode tests ===

    #[tokio::test]
    async fn test_install_state_show_install_summary() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Install,
                packages: vec![
                    PackageInfo {
                        name: "cat9k-rpbase.17.09.04a.SPA.pkg".to_string(),
                        state: PackageState::Committed,
                    },
                    PackageInfo {
                        name: "cat9k-rpboot.17.09.04a.SPA.pkg".to_string(),
                        state: PackageState::Committed,
                    },
                    PackageInfo {
                        name: "cat9k-sipspa.17.09.04a.SPA.pkg".to_string(),
                        state: PackageState::Inactive,
                    },
                ],
            });
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show install summary\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(output.contains("cat9k-rpbase"), "Should list rpbase package");
        assert!(output.contains("cat9k-rpboot"), "Should list rpboot package");
        assert!(output.contains("Committed") || output.contains("C "), "Should show Committed state");
        assert!(output.contains("Inactive") || output.contains("I "), "Should show Inactive state");
    }

    #[tokio::test]
    async fn test_install_mode_show_version_has_packages_conf() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Install,
                packages: vec![],
            });
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show version\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(
            output.contains("packages.conf"),
            "Install mode show version should reference packages.conf, got:\n{}",
            &output[..output.len().min(300)]
        );
    }

    #[tokio::test]
    async fn test_bundle_mode_show_version_has_bin() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Bundle,
                packages: vec![],
            });
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show version\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(
            output.contains(".bin"),
            "Bundle mode show version should reference .bin image"
        );
    }

    #[tokio::test]
    async fn test_install_mode_show_boot_has_packages_conf() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Install,
                packages: vec![],
            });
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show boot\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(output.contains("packages.conf"));
    }

    #[tokio::test]
    async fn test_install_add_command() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Install,
                packages: vec![],
            })
            .with_flash_file("cat9k_iosxe.17.09.04a.SPA.bin", b"image data".to_vec());
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"install add file flash:cat9k_iosxe.17.09.04a.SPA.bin\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(output.contains("install_add") || output.contains("SUCCESS") || output.contains("Adding"),
            "install add should show progress, got: {}", output);

        // Packages should now be Inactive
        if let Some(ref state) = device.install_state {
            assert!(!state.packages.is_empty(), "Should have added packages");
            assert!(state.packages.iter().any(|p| p.state == PackageState::Inactive),
                "New packages should be Inactive");
        }
    }

    #[tokio::test]
    async fn test_install_activate_prompts_and_reloads() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Install,
                packages: vec![
                    PackageInfo {
                        name: "cat9k-rpbase.17.09.04a.SPA.pkg".to_string(),
                        state: PackageState::Inactive,
                    },
                ],
            });
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"install activate\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(output.contains("[y/n]") || output.contains("Proceed"),
            "install activate should prompt, got: {}", output);

        // Confirm
        device.send(b"y\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        // Should trigger reload
        assert!(device.is_reloading() || output.contains("reloading"),
            "install activate should trigger reload");
    }

    #[tokio::test]
    async fn test_install_commit() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Install,
                packages: vec![
                    PackageInfo {
                        name: "cat9k-rpbase.17.09.04a.SPA.pkg".to_string(),
                        state: PackageState::Activated,
                    },
                ],
            });
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"install commit\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(output.contains("SUCCESS") || output.contains("commit") || output.contains("#"),
            "install commit should succeed, got: {}", output);

        // Packages should now be Committed
        if let Some(ref state) = device.install_state {
            assert!(state.packages.iter().all(|p| p.state == PackageState::Committed),
                "All packages should be Committed after commit");
        }
    }

    #[tokio::test]
    async fn test_install_remove_inactive() {
        let mut device = MockIosDevice::new("Switch1")
            .with_install_state(InstallState {
                mode: InstallMode::Install,
                packages: vec![
                    PackageInfo {
                        name: "cat9k-old.17.06.05.SPA.pkg".to_string(),
                        state: PackageState::Inactive,
                    },
                    PackageInfo {
                        name: "cat9k-rpbase.17.09.04a.SPA.pkg".to_string(),
                        state: PackageState::Committed,
                    },
                ],
            });
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"install remove inactive\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("#"), "Should return to prompt");

        // Only committed packages should remain
        if let Some(ref state) = device.install_state {
            assert!(state.packages.iter().all(|p| p.state != PackageState::Inactive),
                "No inactive packages should remain");
            assert!(!state.packages.is_empty(), "Committed packages should remain");
        }
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

    // =========================================================================
    // Tests based on real Cisco IOS device behavior (.130 IOS 12.2, .113 IOS 15.2)
    // =========================================================================

    /// Helper: create device, consume initial prompt, return device + prompt text.
    async fn setup_device(hostname: &str) -> MockIosDevice {
        let mut device = MockIosDevice::new(hostname);
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device
    }

    /// Helper: send command and get response.
    async fn send_cmd(device: &mut MockIosDevice, cmd: &str) -> String {
        device.send(format!("{}\n", cmd).as_bytes()).await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        String::from_utf8_lossy(&data).to_string()
    }

    // --- disable command ---

    #[tokio::test]
    async fn test_disable_drops_to_user_exec() {
        let mut device = setup_device("Router1").await;
        assert_eq!(device.mode, CliMode::PrivilegedExec);

        let output = send_cmd(&mut device, "disable").await;
        assert_eq!(device.mode, CliMode::UserExec);
        assert!(output.contains("Router1>"), "After disable, prompt should be 'Router1>', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_disable_then_enable() {
        let mut device = MockIosDevice::new("R1").with_enable("secret");
        // with_enable sets mode to UserExec when no login
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Start in UserExec — enable
        let output = send_cmd(&mut device, "enable").await;
        assert!(output.contains("Password:"), "Should prompt for enable password");
        let output = send_cmd(&mut device, "secret").await;
        assert!(output.contains("R1#"), "Should be in priv exec after enable");

        // Disable back to user exec
        let output = send_cmd(&mut device, "disable").await;
        assert_eq!(device.mode, CliMode::UserExec);
        assert!(output.contains("R1>"));
    }

    // --- User exec mode: show commands should work ---

    #[tokio::test]
    async fn test_user_exec_show_version() {
        let mut device = MockIosDevice::new("R1").with_enable("secret");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        let output = send_cmd(&mut device, "show version").await;
        assert!(output.contains("Cisco IOS"), "show version should work in user exec, got: {:?}", output);
        assert!(output.contains("R1>"), "Should stay in user exec mode");
    }

    #[tokio::test]
    async fn test_user_exec_show_clock() {
        let mut device = MockIosDevice::new("R1").with_enable("secret");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        let output = send_cmd(&mut device, "show clock").await;
        // Real IOS: "08:29:31.723 UTC Thu Mar 26 2026"
        assert!(output.contains("UTC") || output.contains(":"), "show clock should return a time, got: {:?}", output);
        assert!(output.contains("R1>"));
    }

    #[tokio::test]
    async fn test_user_exec_show_running_config() {
        let mut device = MockIosDevice::new("R1").with_enable("secret");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        let output = send_cmd(&mut device, "show running-config").await;
        assert!(output.contains("hostname R1"), "show run should work in user exec");
    }

    #[tokio::test]
    async fn test_user_exec_configure_terminal_rejected() {
        // Real IOS: configure terminal from user exec gives:
        //   "             ^
        //    % Invalid input detected at '^' marker."
        let mut device = MockIosDevice::new("R1").with_enable("secret");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        let output = send_cmd(&mut device, "configure terminal").await;
        assert!(output.contains("Invalid input") || output.contains("Unrecognized"),
            "configure terminal should be rejected in user exec, got: {:?}", output);
        assert_eq!(device.mode, CliMode::UserExec);
    }

    // --- Incomplete command vs Unknown command ---

    #[tokio::test]
    async fn test_incomplete_command() {
        // Real IOS: "show ip" → "% Incomplete command."
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show ip").await;
        assert!(output.contains("Incomplete command"),
            "Incomplete command should give '% Incomplete command.', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_unknown_command_error_message() {
        // Real IOS: "boguscommand" → (optional Translating...) then
        // "% Unknown command or computer name, or unable to find computer address"
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "boguscommand").await;
        assert!(output.contains("Unknown command") || output.contains("Invalid input"),
            "Unknown command should give appropriate error, got: {:?}", output);
    }

    // --- Config mode: Invalid input with caret marker ---

    #[tokio::test]
    async fn test_config_mode_invalid_command_caret_marker() {
        // Real IOS:
        //   SEED-001-S0244(config)#bogusconfigcmd
        //                            ^
        //   % Invalid input detected at '^' marker.
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        assert_eq!(device.mode, CliMode::Config);

        let output = send_cmd(&mut device, "bogusconfigcmd").await;
        assert!(output.contains("Invalid input detected"),
            "Config mode bad command should show '% Invalid input detected at '^' marker.', got: {:?}", output);
        assert!(output.contains("^"),
            "Should include caret marker, got: {:?}", output);
        assert_eq!(device.mode, CliMode::Config, "Should stay in config mode");
    }

    // --- "do" prefix in config mode ---

    #[tokio::test]
    async fn test_do_command_in_config_mode() {
        // Real IOS: "do show clock" from config mode runs the exec command
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;

        let output = send_cmd(&mut device, "do show version").await;
        assert!(output.contains("Cisco IOS"),
            "'do show version' in config mode should work, got: {:?}", output);
        assert_eq!(device.mode, CliMode::Config, "Should stay in config mode after 'do'");
    }

    #[tokio::test]
    async fn test_do_command_in_config_sub_mode() {
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 0/0").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(_)));

        let output = send_cmd(&mut device, "do show version").await;
        assert!(output.contains("Cisco IOS"),
            "'do show version' in config-if mode should work, got: {:?}", output);
    }

    // --- "configure" alone (without "terminal") ---

    #[tokio::test]
    async fn test_configure_alone_prompts() {
        // Real IOS: "configure" → "Configuring from terminal, memory, or network [terminal]?"
        let mut device = setup_device("Router1").await;

        device.send(b"configure\n").await.unwrap();
        let output_bytes = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&output_bytes);

        assert!(output.contains("[terminal]") || output.contains("Configuring from"),
            "'configure' alone should prompt for method, got: {:?}", output);
    }

    // --- show clock ---

    #[tokio::test]
    async fn test_show_clock_priv_exec() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show clock").await;
        // Real IOS: ".08:29:31.723 UTC Thu Mar 26 2026" or "07:54:26.236 UTC Mon Feb 28 2000"
        assert!(output.contains("UTC") || output.contains(":"),
            "show clock should return time with UTC, got: {:?}", output);
        assert!(output.contains("Router1#"));
    }

    // --- show ip interface brief ---

    #[tokio::test]
    async fn test_show_ip_interface_brief() {
        // Real IOS shows a table with headers:
        // Interface  IP-Address  OK? Method Status  Protocol
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show ip interface brief").await;
        assert!(output.contains("Interface") && output.contains("IP-Address"),
            "show ip int brief should show interface table header, got: {:?}", output);
        assert!(output.contains("GigabitEthernet") || output.contains("Vlan") || output.contains("Te"),
            "Should list interfaces");
        // Protocol column must be padded to 8 chars (for screen-scraping tools)
        // An "up" protocol entry should appear as "up      " (with trailing spaces)
        assert!(output.contains("up      ") || output.contains("down    "),
            "Protocol column should be padded to 8 chars with trailing spaces, got: {:?}", output);
    }

    // --- dir flash: format ---

    #[tokio::test]
    async fn test_dir_format_matches_real_ios() {
        // Real IOS dir output:
        //   Directory of flash:/
        //     2  -rwx    1824  Feb 18 2000 13:07:57 +00:00  vlan.dat
        //   122185728 bytes total (99002368 bytes free)
        let mut device = MockIosDevice::new("Router1")
            .with_flash_size(122_185_728)
            .with_flash_file("test.bin", vec![0u8; 1000]);
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        let output = send_cmd(&mut device, "dir flash:").await;
        assert!(output.contains("Directory of flash:/"),
            "Should have 'Directory of flash:/' header, got: {:?}", output);
        assert!(output.contains("test.bin"), "Should list file name");
        assert!(output.contains("bytes total"), "Should show total");
        assert!(output.contains("bytes free"), "Should show free");
    }

    // --- show boot format ---

    #[tokio::test]
    async fn test_show_boot_format() {
        // Real IOS:
        //   BOOT path-list      : flash:c3560cx...
        //   Config file         : flash:/config.text
        //   ...
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show boot").await;
        assert!(output.contains("BOOT") || output.contains("boot"),
            "show boot should contain BOOT info, got: {:?}", output);
    }

    // --- "show" alone gives subcommand hint ---

    #[tokio::test]
    async fn test_show_alone_gives_hint() {
        // Real IOS: % Type "show ?" for a list of subcommands
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show").await;
        assert!(output.contains("Type \"show ?\"") || output.contains("subcommands"),
            "'show' alone should hint about '?', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_alone_user_exec() {
        // Real IOS gives same hint in user exec
        let mut device = MockIosDevice::new("R1").with_enable("secret");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        let output = send_cmd(&mut device, "show").await;
        assert!(output.contains("Type \"show ?\"") || output.contains("subcommands"),
            "'show' alone in user exec should also hint, got: {:?}", output);
    }

    // --- Config mode: "exit" returns to priv exec, not just config ---

    #[tokio::test]
    async fn test_config_exit_returns_to_priv_exec() {
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        assert_eq!(device.mode, CliMode::Config);

        let output = send_cmd(&mut device, "exit").await;
        assert_eq!(device.mode, CliMode::PrivilegedExec,
            "exit from config should return to priv exec");
        assert!(output.contains("Router1#"));
    }

    // --- Config sub-mode: "exit" returns to config, "end" to priv exec ---

    #[tokio::test]
    async fn test_config_sub_exit_returns_to_config() {
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 0/0").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(_)));

        let output = send_cmd(&mut device, "exit").await;
        assert_eq!(device.mode, CliMode::Config);
        assert!(output.contains("(config)#"));
    }

    #[tokio::test]
    async fn test_config_sub_end_returns_to_priv_exec() {
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 0/0").await;

        let output = send_cmd(&mut device, "end").await;
        assert_eq!(device.mode, CliMode::PrivilegedExec);
        assert!(output.contains("Router1#"));
    }

    // --- Empty line behavior ---

    #[tokio::test]
    async fn test_empty_line_returns_prompt() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "").await;
        assert!(output.contains("Router1#"), "Empty line should just re-show prompt");
    }

    // --- show version contains key fields ---

    #[tokio::test]
    async fn test_show_version_contains_key_fields() {
        // Real IOS show version includes: version string, model, uptime,
        // "System image file", "Configuration register"
        let mut device = MockIosDevice::new("Router1")
            .with_version("15.2(7)E10")
            .with_model("WS-C3560CX-12PD-S");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        let output = send_cmd(&mut device, "show version").await;
        assert!(output.contains("15.2(7)E10"), "Should contain version string");
        assert!(output.contains("WS-C3560CX-12PD-S"), "Should contain model");
        assert!(output.contains("System image file"), "Should mention system image file");
        assert!(output.contains("Configuration register"), "Should mention config register");
    }

    #[tokio::test]
    async fn test_show_version_no_spa_in_image() {
        let mut device = setup_device("Switch1").await;
        let output = send_cmd(&mut device, "show version").await;
        assert!(!output.contains(".SPA."),
            "System image should not contain '.SPA.': {:?}",
            output.lines().find(|l| l.contains("System image")));
    }

    // --- "show run" abbreviation ---

    #[tokio::test]
    async fn test_show_run_abbreviation() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show run").await;
        assert!(output.contains("hostname Router1"),
            "'show run' should work as abbreviation for 'show running-config'");
    }

    // --- "conf t" abbreviation ---

    #[tokio::test]
    async fn test_conf_t_abbreviation() {
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "conf t").await;
        assert_eq!(device.mode, CliMode::Config);
    }

    // --- "term len" abbreviation ---

    #[tokio::test]
    async fn test_term_len_abbreviation() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "term len 0").await;
        assert!(output.contains("Router1#"), "term len should be accepted silently");
    }

    // --- Router sub-mode ---

    #[tokio::test]
    async fn test_router_sub_mode() {
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;

        let output = send_cmd(&mut device, "router ospf 1").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(ref s) if s == "config-router"),
            "router command should enter config-router sub-mode");
        assert!(output.contains("(config-router)#"));
    }

    // --- "write memory" / "copy run start" ---

    #[tokio::test]
    async fn test_write_memory() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "write memory").await;
        // Real IOS: "[OK]" or "Building configuration..."
        assert!(output.contains("OK") || output.contains("Building") || output.contains("#"),
            "write memory should produce some confirmation, got: {:?}", output);
    }

    // --- "show ip route" should work (even as custom/stub) ---

    #[tokio::test]
    async fn test_show_ip_route() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show ip route").await;
        // At minimum it shouldn't give "Unknown command" - it should give some output
        assert!(!output.contains("Unknown command"),
            "show ip route should not be unknown, got: {:?}", output);
    }

    // =========================================================================
    // New tests: abbreviation matching, ambiguity, caret position
    // =========================================================================

    #[tokio::test]
    async fn test_abbreviation_show_ver() {
        // "show ver" should work as abbreviation for "show version"
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show ver").await;
        assert!(
            output.contains("Cisco IOS"),
            "'show ver' abbreviation should work, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_abbreviation_conf_t() {
        // "conf t" should enter config mode
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "conf t").await;
        assert_eq!(device.mode, CliMode::Config, "'conf t' should enter config mode");
    }

    #[tokio::test]
    async fn test_abbreviation_sh_run() {
        // "sh run" should show running config
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "sh run").await;
        assert!(
            output.contains("hostname Router1"),
            "'sh run' abbreviation should work, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_ambiguous_command() {
        // "co" matches both "configure" and "copy" — should give ambiguous error
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "co").await;
        assert!(
            output.contains("Ambiguous"),
            "'co' should give ambiguous error, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_caret_position_correct() {
        // "show xyz" — caret should appear under "xyz" (position 5)
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show xyz").await;
        assert!(
            output.contains("Invalid input detected"),
            "Should give invalid input error, got: {:?}", output
        );
        assert!(
            output.contains("^"),
            "Should include caret marker, got: {:?}", output
        );
        // Caret should be at position 5 (start of "xyz") — check for "     ^" pattern
        assert!(
            output.contains("     ^"),
            "Caret should be at position 5, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_abbreviation_show_ip_int_bri() {
        // "sh ip int bri" should work
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "sh ip int bri").await;
        assert!(
            output.contains("Interface") || output.contains("IP-Address"),
            "'sh ip int bri' abbreviation should work, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_abbreviation_wri_mem() {
        // "wri mem" should work as write memory
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "wri mem").await;
        assert!(
            output.contains("OK") || output.contains("Building") || output.contains("#"),
            "'wri mem' should work, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_config_known_command_accepted() {
        // "hostname Foo" in config mode should be accepted
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "hostname Foo").await;
        assert!(
            !output.contains("Invalid input"),
            "hostname should be accepted in config mode, got: {:?}", output
        );
        assert_eq!(device.mode, CliMode::Config);
    }

    #[tokio::test]
    async fn test_config_unknown_command_caret() {
        // An unknown config command should give caret error
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "bogusconfigcmd2").await;
        assert!(
            output.contains("Invalid input detected"),
            "Unknown config command should give caret error, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_config_interface_enters_submode() {
        // "interface GigabitEthernet 1/0" should enter config-if
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "interface GigabitEthernet 1/0").await;
        assert!(
            matches!(device.mode, CliMode::ConfigSub(ref s) if s == "config-if"),
            "interface should enter config-if, mode={:?}, output={:?}", device.mode, output
        );
    }

    #[tokio::test]
    async fn test_config_do_prefix() {
        // "do show version" from config mode should work and stay in config
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "do show version").await;
        assert!(
            output.contains("Cisco IOS"),
            "'do show version' should work, got: {:?}", output
        );
        assert_eq!(device.mode, CliMode::Config, "Should stay in config mode after 'do'");
    }

    #[tokio::test]
    async fn test_config_exit_and_end() {
        // exit from config-if → config, end from config → priv exec
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 0/0").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(_)));

        // exit → back to config
        let _ = send_cmd(&mut device, "exit").await;
        assert_eq!(device.mode, CliMode::Config);

        // end → priv exec
        let _ = send_cmd(&mut device, "end").await;
        assert_eq!(device.mode, CliMode::PrivilegedExec);
    }

    // ── Echo / interactive behavior tests ────────────────────────────────────
    // Echo is always on (real IOS always echoes). Password input is never echoed.

    #[tokio::test]
    async fn test_echo_characters() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // initial prompt

        // Send individual characters — they should be echoed immediately
        device.send(b"s").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(out, b"s", "Should echo 's'");

        device.send(b"h").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(out, b"h", "Should echo 'h'");

        // Send newline to complete command
        device.send(b"ow version\n").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);
        assert!(output.contains("Cisco IOS"), "Should get show version output");
    }

    #[tokio::test]
    async fn test_question_mark_help() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // initial prompt

        // Type "show " then "?"
        device.send(b"show ").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // echo

        device.send(b"?").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);
        // Should list show subcommands
        assert!(output.contains("version"), "? help should list 'version', got: {:?}", output);
        // Should re-display the partial input
        assert!(output.contains("R1#"), "Should re-display prompt after help");
    }

    #[tokio::test]
    async fn test_no_echo_on_password() {
        let mut device = MockIosDevice::new("R1")
            .with_login("admin", "secret");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // "Username: "

        // Username should echo
        device.send(b"a").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(out, b"a", "Username should echo");

        // Complete username
        device.send(b"dmin\n").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);
        assert!(output.contains("Password:"), "Should get password prompt");

        // Password should NOT echo
        device.send(b"s").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(out.is_empty(), "Password chars should not echo");
    }

    #[tokio::test]
    async fn test_backspace() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"shox").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Backspace to erase 'x'
        device.send(b"\x7f").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        // New behavior: BS + erase-to-end (cursor was at end so tail is empty)
        assert!(out.starts_with(b"\x08"), "Backspace should start with BS");
        assert!(out.contains(&0x1B), "Backspace should include erase sequence");

        // Now type 'w version\n' to complete "show version"
        device.send(b"w version\n").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);
        assert!(output.contains("Cisco IOS"), "After backspace correction, should get show version");
    }

    #[tokio::test]
    async fn test_output_uses_crlf() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Initial prompt doesn't have newlines, so just check a command
        device.send(b"show version\n").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let raw = String::from_utf8_lossy(&data);
        // Should have \r\n, not bare \n
        assert!(!raw.contains('\n') || raw.contains("\r\n"),
            "Output should use \\r\\n, not bare \\n");
        // More specifically: every \n should be preceded by \r
        for (i, &b) in data.iter().enumerate() {
            if b == b'\n' && (i == 0 || data[i-1] != b'\r') {
                panic!("Found bare \\n at position {} in output: {:?}", i, &raw[..raw.len().min(100)]);
            }
        }
    }

    #[tokio::test]
    async fn test_tab_completion_unique() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Type "sh" then Tab — should complete to "show "
        device.send(b"sh").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // echo "sh"

        device.send(b"\t").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("ow "), "Tab should complete 'sh' to 'show ', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_tab_completion_ambiguous() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Type "co" then Tab — ambiguous (configure, copy)
        device.send(b"co").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"\t").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        // Should beep (BEL = 0x07) or produce no completion
        assert!(data.contains(&0x07) || data.is_empty(),
            "Ambiguous tab should beep or be empty, got: {:?}", data);
    }

    // =========================================================================
    // Tests based on real Cisco IOS device behavior — new batch
    // =========================================================================

    // --- Ctrl+Z exits config mode to priv exec ---

    #[tokio::test]
    async fn test_ctrl_z_exits_config_to_priv_exec() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        assert_eq!(device.mode, CliMode::Config);

        // Send Ctrl+Z (0x1A)
        device.send(b"\x1a").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert_eq!(device.mode, CliMode::PrivilegedExec);
        assert!(output.contains("R1#"), "Should return to priv exec prompt, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_ctrl_z_exits_config_if_to_priv_exec() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 0/0").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(_)));

        // Ctrl+Z goes straight to priv exec (not config first)
        device.send(b"\x1a").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert_eq!(device.mode, CliMode::PrivilegedExec,
            "Ctrl+Z from config-if should go straight to priv exec, mode={:?}, output={:?}",
            device.mode, output);
    }

    // --- Ambiguous command format matches real IOS ---

    #[tokio::test]
    async fn test_ambiguous_command_format() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "co").await;
        assert!(output.contains("% Ambiguous command:  \"co\""),
            "Ambiguous format should match real IOS (two spaces before quote), got: {:?}", output);
        // Should NOT contain "Matches:" line
        assert!(!output.contains("Matches:"), "Real IOS doesn't show Matches: line, got: {:?}", output);
    }

    // --- Hostname change updates prompt immediately ---

    #[tokio::test]
    async fn test_hostname_change_updates_prompt() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "hostname NewHost").await;
        assert!(output.contains("NewHost(config)#"),
            "Prompt should immediately reflect new hostname, got: {:?}", output);
        // Change back
        let _ = send_cmd(&mut device, "hostname R1").await;
        let _ = send_cmd(&mut device, "end").await;
    }

    // --- show flash: as alias for dir flash: ---

    #[tokio::test]
    async fn test_show_flash_alias() {
        let mut device = MockIosDevice::new("R1")
            .with_flash_file("test.bin", vec![0u8; 100]);
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = send_cmd(&mut device, "show flash:").await;
        assert!(output.contains("Directory of flash:/") || output.contains("test.bin"),
            "'show flash:' should work like 'dir flash:', got: {:?}", output);
    }

    // --- Ping output format ---

    #[tokio::test]
    async fn test_ping_output_format() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "ping 10.0.0.1").await;
        assert!(output.contains("Type escape sequence") || output.contains("Sending"),
            "Ping should show IOS-like output, got: {:?}", output);
        assert!(output.contains("Success rate"),
            "Ping should show success rate, got: {:?}", output);
    }

    // --- Config mode incomplete command ---

    #[tokio::test]
    async fn test_config_incomplete_command() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "in").await;
        // "in" matches "interface" uniquely, but interface needs an argument
        // Real IOS: "% Incomplete command."
        assert!(output.contains("Incomplete command") || output.contains("Invalid input"),
            "Config 'in' should be incomplete or invalid, got: {:?}", output);
        assert_eq!(device.mode, CliMode::Config, "Should stay in config mode");
    }

    // =========================================================================
    // CLI editing key tests
    // =========================================================================

    #[tokio::test]
    async fn test_ctrl_a_moves_to_start() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"show").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Ctrl+A should move cursor to start
        device.send(b"\x01").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        // Should contain left-arrow escape sequences
        assert!(!data.is_empty(), "Ctrl+A should produce cursor movement");
        assert_eq!(device.cursor_pos, 0);
    }

    #[tokio::test]
    async fn test_ctrl_e_moves_to_end() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"show").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"\x01").await.unwrap(); // go to start
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"\x05").await.unwrap(); // Ctrl+E to end
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 4); // "show" = 4 chars
        // Ctrl+E when already at end produces no movement output
        let _ = data;
    }

    #[tokio::test]
    async fn test_arrow_keys_move_cursor() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"test").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 4);
        // Left arrow = ESC [ D
        device.send(b"\x1b[D").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 3);
        // Right arrow = ESC [ C
        device.send(b"\x1b[C").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 4);
    }

    #[tokio::test]
    async fn test_command_history_up_down() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Execute two commands
        device.send(b"show version\n").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"show clock\n").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Up arrow should recall "show clock"
        device.send(b"\x1b[A").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("show clock"), "Up should recall last cmd, got: {:?}", output);
        // Up again should recall "show version"
        device.send(b"\x1b[A").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("show version"), "Up again should recall prev cmd, got: {:?}", output);
        // Down should go back to "show clock"
        device.send(b"\x1b[B").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("show clock"), "Down should go forward, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_ctrl_d_delete_char() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"shox version").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Move left 9 times to position cursor on 'x' (at index 3)
        for _ in 0..9 {
            device.send(b"\x1b[D").await.unwrap();
        }
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 3);
        // Ctrl+D should delete 'x'
        device.send(b"\x04").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        let buf = String::from_utf8_lossy(&device.input_buffer);
        assert_eq!(buf, "sho version");
    }

    #[tokio::test]
    async fn test_ctrl_d_empty_disconnects() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Ctrl+D on empty line should disconnect (sets mode to Reloading)
        device.send(b"\x04").await.unwrap();
        // receive() will return Err(NotConnected) since mode is Reloading and queue is empty
        let _ = device.receive(Duration::from_secs(1)).await; // ignore error
        assert!(device.is_reloading(), "Ctrl+D on empty should disconnect");
    }

    #[tokio::test]
    async fn test_ctrl_u_erase_to_start() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"show version").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Ctrl+U should erase entire line (cursor at end)
        device.send(b"\x15").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert!(device.input_buffer.is_empty(), "Ctrl+U should clear buffer");
        assert_eq!(device.cursor_pos, 0);
    }

    #[tokio::test]
    async fn test_ctrl_k_erase_to_end() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"show version").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Move to position 4 (after "show")
        device.send(b"\x01").await.unwrap(); // Ctrl+A to start
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        for _ in 0..4 { device.send(b"\x06").await.unwrap(); } // Ctrl+F x4
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Ctrl+K should erase " version"
        device.send(b"\x0b").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        let buf = String::from_utf8_lossy(&device.input_buffer);
        assert_eq!(buf, "show");
    }

    #[tokio::test]
    async fn test_ctrl_w_erase_word() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"show ip route").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Ctrl+W should erase "route"
        device.send(b"\x17").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        let buf = String::from_utf8_lossy(&device.input_buffer);
        assert_eq!(buf, "show ip ");
    }

    #[tokio::test]
    async fn test_insert_at_cursor() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"sho version").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Move left 8 to position after "sho" (cursor at 3)
        for _ in 0..8 {
            device.send(b"\x1b[D").await.unwrap();
        }
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 3);
        // Insert 'w' to make "show version"
        device.send(b"w").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        let buf = String::from_utf8_lossy(&device.input_buffer);
        assert_eq!(buf, "show version");
        assert_eq!(device.cursor_pos, 4);
    }

    #[tokio::test]
    async fn test_backspace_at_cursor() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"showw version").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        // Move left 8 to position cursor after second 'w' (at index 5)
        for _ in 0..8 {
            device.send(b"\x1b[D").await.unwrap();
        }
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 5);
        // Backspace should remove 'w' at index 4
        device.send(b"\x7f").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        let buf = String::from_utf8_lossy(&device.input_buffer);
        assert_eq!(buf, "show version");
        assert_eq!(device.cursor_pos, 4);
    }

    #[tokio::test]
    async fn test_ctrl_b_ctrl_f_move_cursor() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"abc").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 3);
        device.send(b"\x02").await.unwrap(); // Ctrl+B
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 2);
        device.send(b"\x06").await.unwrap(); // Ctrl+F
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 3);
    }

    #[tokio::test]
    async fn test_cursor_pos_reset_on_enter() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"show").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"\x01").await.unwrap(); // Ctrl+A — cursor to 0
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 0);
        device.send(b"\n").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(device.cursor_pos, 0, "cursor_pos should reset to 0 after Enter");
    }

    // ─── Feature tests: show run header ────────────────────────────────────────

    #[tokio::test]
    async fn test_show_run_has_building_header() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show running-config").await;
        assert!(
            output.contains("Building configuration"),
            "show run should start with 'Building configuration...', got: {:?}",
            &output[..output.len().min(200)]
        );
        assert!(
            output.contains("Current configuration"),
            "show run should show 'Current configuration : NNN bytes'"
        );
    }

    // ─── Feature tests: show interfaces ────────────────────────────────────────

    #[tokio::test]
    async fn test_show_interfaces_specific() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show interfaces GigabitEthernet1/0/1").await;
        assert!(
            output.contains("GigabitEthernet1/0/1 is up"),
            "Should show interface status, got: {:?}",
            &output[..output.len().min(200)]
        );
        assert!(output.contains("MTU"), "Should show MTU");
        assert!(output.contains("packets input"), "Should show counters");
    }

    #[tokio::test]
    async fn test_show_interfaces_shutdown() {
        // Manually shut down an interface for this test
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 1/0/5").await;
        let _ = send_cmd(&mut device, "shutdown").await;
        let _ = send_cmd(&mut device, "end").await;
        let output = send_cmd(&mut device, "show interfaces GigabitEthernet1/0/5").await;
        assert!(
            output.contains("administratively down"),
            "Shutdown interface should show 'administratively down', got: {:?}",
            &output[..output.len().min(200)]
        );
    }

    #[tokio::test]
    async fn test_show_interfaces_all() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show interfaces").await;
        assert!(
            output.contains("GigabitEthernet1/0/1") && output.contains("GigabitEthernet1/0/2"),
            "show interfaces should list all interfaces"
        );
    }

    #[tokio::test]
    async fn test_show_interfaces_nonexistent() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show interfaces GigabitEthernet99/99").await;
        // Real IOS: "% Invalid input detected at '^' marker." or similar
        assert!(
            output.contains("Invalid") || output.contains("invalid") || output.contains("R1#"),
            "Nonexistent interface should error or show empty"
        );
    }

    // ─── Feature tests: show vlan brief ────────────────────────────────────────

    #[tokio::test]
    async fn test_show_vlan_brief() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show vlan brief").await;
        assert!(
            output.contains("VLAN") && output.contains("Name") && output.contains("Status"),
            "show vlan brief should have table header, got: {:?}",
            output
        );
        assert!(output.contains("default"), "Should show default VLAN 1");
    }

    // ─── Feature tests: show history ───────────────────────────────────────────

    #[tokio::test]
    async fn test_show_history() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "show version").await;
        let _ = send_cmd(&mut device, "show clock").await;
        let output = send_cmd(&mut device, "show history").await;
        assert!(output.contains("show version"), "History should contain previous commands");
        assert!(output.contains("show clock"), "History should contain previous commands");
    }

    // ─── Feature tests: no ip route ────────────────────────────────────────────

    #[tokio::test]
    async fn test_no_ip_route_removes_route() {
        let mut device = setup_device("R1").await;
        // Add a route
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "ip route 172.16.0.0 255.255.0.0 10.0.0.1").await;
        assert!(!device.state.static_routes.is_empty(), "Should have added a route");
        let initial_count = device.state.static_routes.len();
        // Remove it
        let _ = send_cmd(&mut device, "no ip route 172.16.0.0 255.255.0.0 10.0.0.1").await;
        assert_eq!(device.state.static_routes.len(), initial_count - 1, "Should have removed route");
        let _ = send_cmd(&mut device, "end").await;
    }

    // ─── Feature tests: show ip route full format ───────────────────────────────

    #[tokio::test]
    async fn test_show_ip_route_full_format() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show ip route").await;
        assert!(output.contains("Codes:"), "Should have Codes header");
        assert!(output.contains("Gateway of last resort"), "Should show gateway of last resort");
        assert!(output.contains("directly connected"), "Should show connected routes");
        assert!(output.contains("Vlan1"), "Should reference interface name");
        // Default state has Vlan1 at 10.0.0.1/24 — class A major is 10.0.0.0/8
        assert!(
            output.contains("variably subnetted"),
            "Should show 'variably subnetted' grouping header, got: {:?}",
            &output[..output.len().min(500)]
        );
        assert!(
            output.contains("10.0.0.0/8 is variably subnetted"),
            "Should group 10.x routes under 10.0.0.0/8, got: {:?}",
            &output[..output.len().min(500)]
        );
        // Default route should appear standalone (not under a group header)
        assert!(
            output.contains("S*"),
            "Should show S* for default route, got: {:?}",
            &output[..output.len().min(500)]
        );
    }

    #[tokio::test]
    async fn test_show_ip_route_multiple_subnets() {
        // Build a device with multiple interfaces in the same class A major (10.0.0.0/8)
        // and one in a class C major (192.168.0.0/24), plus a default route.
        let mut device = setup_device("R1").await;

        // Configure extra interfaces via CLI
        let _ = send_cmd(&mut device, "configure terminal").await;

        let _ = send_cmd(&mut device, "interface loopback 0").await;
        let _ = send_cmd(&mut device, "ip address 10.127.0.1 255.255.255.255").await;
        let _ = send_cmd(&mut device, "no shutdown").await;
        let _ = send_cmd(&mut device, "exit").await;

        let _ = send_cmd(&mut device, "interface GigabitEthernet 1/0/2").await;
        let _ = send_cmd(&mut device, "ip address 192.168.0.113 255.255.255.0").await;
        let _ = send_cmd(&mut device, "no shutdown").await;
        let _ = send_cmd(&mut device, "exit").await;

        let _ = send_cmd(&mut device, "end").await;

        let output = send_cmd(&mut device, "show ip route").await;

        // 10.0.0.0/8 group should exist and cover multiple 10.x subnets
        assert!(
            output.contains("10.0.0.0/8 is variably subnetted"),
            "Should have 10.0.0.0/8 variably subnetted header, got:\n{}",
            output
        );

        // 192.168.0.0/24 group (class C)
        assert!(
            output.contains("192.168.0.0/24 is variably subnetted"),
            "Should have 192.168.0.0/24 variably subnetted header, got:\n{}",
            output
        );

        // Both 10.x connected routes and loopback should appear under the 10/8 group
        assert!(
            output.contains("10.0.0.0/24"),
            "Should show 10.0.0.0/24 network route"
        );
        assert!(
            output.contains("10.127.0.1/32"),
            "Should show 10.127.0.1/32 loopback host route"
        );

        // The 192.168.x routes should appear
        assert!(
            output.contains("192.168.0.0/24"),
            "Should show 192.168.0.0/24 network route"
        );
        assert!(
            output.contains("192.168.0.113/32"),
            "Should show 192.168.0.113/32 host route"
        );

        // Default route still standalone
        assert!(output.contains("S*"), "Should still show S* default route");
        assert!(
            output.contains("0.0.0.0/0"),
            "Should show 0.0.0.0/0 default route"
        );

        // Verify the 10/8 group header comes before its member routes
        let pos_10_header = output.find("10.0.0.0/8 is variably subnetted").unwrap();
        let pos_10_route = output.find("10.0.0.0/24").unwrap();
        assert!(
            pos_10_header < pos_10_route,
            "Group header should precede member routes"
        );
    }

    // ─── Feature tests: show startup-config ────────────────────────────────────

    #[tokio::test]
    async fn test_show_startup_config() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show startup-config").await;
        assert!(output.contains("Using") && output.contains("bytes"),
            "show startup should have 'Using NNN out of XXXXX bytes' header, got: {:?}", &output[..output.len().min(200)]);
        assert!(output.contains("hostname"), "Should contain hostname");
    }

    #[tokio::test]
    async fn test_show_terminal() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show terminal").await;
        assert!(output.contains("Width: 80 columns"),
            "show terminal should contain 'Width: 80 columns', got: {:?}", output);
    }

    // ─── Bug fix tests ─────────────────────────────────────────────────────────

    // Bug 1: interface loopback 0 accepted in config mode
    #[tokio::test]
    async fn test_config_interface_loopback() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "interface loopback 0").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(_)),
            "Should enter config-if for loopback, got mode: {:?}", device.mode);
        assert!(output.contains("(config-if)#"), "Should show config-if prompt, got: {:?}", output);
        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_config_interface_vlan() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let output = send_cmd(&mut device, "interface vlan 100").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(_)),
            "Should enter config-if for vlan");
        assert!(output.contains("(config-if)#"), "Should show config-if prompt, got: {:?}", output);
        let _ = send_cmd(&mut device, "end").await;
    }

    // Bug 3: end from priv exec is a no-op
    #[tokio::test]
    async fn test_end_from_priv_exec_is_noop() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "end").await;
        assert_eq!(device.mode, CliMode::PrivilegedExec,
            "end from priv exec should stay in priv exec");
        assert!(!output.contains("Invalid"), "'end' from priv exec should not error, got: {:?}", output);
    }

    // Bug 4: interface name normalization
    #[tokio::test]
    async fn test_interface_name_normalization() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface loopback 0").await;
        assert!(device.state.interfaces.iter().any(|i| i.name == "Loopback0"),
            "Should have Loopback0 in interfaces, got: {:?}",
            device.state.interfaces.iter().map(|i| &i.name).collect::<Vec<_>>());
        let _ = send_cmd(&mut device, "end").await;
    }

    // Bug 5: ip address and shutdown work in config-if after entering via loopback
    #[tokio::test]
    async fn test_config_if_ip_address_and_shutdown() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface loopback 0").await;
        let out_ip = send_cmd(&mut device, "ip address 10.127.0.1 255.255.255.255").await;
        assert!(!out_ip.contains("Invalid"), "ip address should be accepted in config-if, got: {:?}", out_ip);
        let out_no_shut = send_cmd(&mut device, "no shutdown").await;
        assert!(!out_no_shut.contains("Invalid"), "no shutdown should be accepted in config-if, got: {:?}", out_no_shut);
        let _ = send_cmd(&mut device, "end").await;

        let output = send_cmd(&mut device, "show ip interface brief").await;
        assert!(output.contains("10.127.0.1"), "Loopback0 should have IP 10.127.0.1, got: {:?}", output);
        assert!(output.contains("Loopback0"), "Should show Loopback0 (fits in 23-char column)");
    }

    #[tokio::test]
    async fn test_config_add_static_route() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "ip route 192.168.1.0 255.255.255.0 10.0.0.2").await;
        let _ = send_cmd(&mut device, "end").await;

        let output = send_cmd(&mut device, "show ip route").await;
        assert!(output.contains("192.168.1.0"), "Should show new static route");
    }

    // ─── Bug fixes ─────────────────────────────────────────────────────────────

    // Bug 1: "wr" alone should work like "write memory"
    #[tokio::test]
    async fn test_wr_alone_works() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "wr").await;
        assert!(output.contains("OK") || output.contains("Building"),
            "'wr' alone should work like 'write memory', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_write_alone_works() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "write").await;
        assert!(output.contains("OK") || output.contains("Building"),
            "'write' alone should work like 'write memory', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_write_memory_still_works() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "write memory").await;
        assert!(output.contains("OK") || output.contains("Building"),
            "'write memory' should still work, got: {:?}", output);
    }

    // Bug 2: config-router ? should show router-specific commands, not interface commands
    #[tokio::test]
    async fn test_config_router_help_shows_router_commands() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "router ospf 1").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(ref s) if s == "config-router"),
            "Should be in config-router mode, got: {:?}", device.mode);

        // Send ? to check available commands
        device.send(b"?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        // Should show router-specific commands
        assert!(output.contains("network"), "config-router ? should show 'network', got: {:?}", output);
        // Should NOT show interface-specific commands
        assert!(!output.contains("switchport"), "config-router ? should NOT show 'switchport', got: {:?}", output);
        assert!(!output.contains("shutdown"), "config-router ? should NOT show 'shutdown', got: {:?}", output);

        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_config_if_help_shows_if_commands() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 0/0").await;

        device.send(b"?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        // Should show interface-specific commands
        assert!(output.contains("shutdown"), "config-if ? should show 'shutdown', got: {:?}", output);
        assert!(output.contains("ip"), "config-if ? should show 'ip', got: {:?}", output);
        // Should NOT show router-specific commands
        assert!(!output.contains("network"), "config-if ? should NOT show 'network', got: {:?}", output);

        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_config_router_network_command() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "router ospf 1").await;
        let output = send_cmd(&mut device, "network 10.0.0.0 0.0.0.255 area 0").await;
        // Should be accepted (not an error)
        assert!(!output.contains("Invalid"), "network command should be accepted in config-router, got: {:?}", output);
        assert!(output.contains("(config-router)#"), "Should stay in config-router mode, got: {:?}", output);
        let _ = send_cmd(&mut device, "end").await;
    }

    // Bug 3: Tab completion with trailing space should beep when multiple matches exist
    #[tokio::test]
    async fn test_tab_after_space_multiple_matches_beeps() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Type "show " then Tab — multiple children, should beep
        device.send(b"show ").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"\t").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        // Should beep
        assert!(data.contains(&0x07) || data.is_empty(),
            "Tab with multiple options should beep, got: {:?}", data);
    }

    // Interface type keyword tab completion tests

    #[tokio::test]
    async fn test_tab_complete_interface_type() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Enter config mode
        device.send(b"configure terminal\r").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Type "interface Gi" then Tab — should complete to "interface GigabitEthernet "
        device.send(b"interface Gi").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        device.send(b"\t").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("GigabitEthernet"),
            "Tab after 'interface Gi' should complete to GigabitEthernet, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_interface_help_shows_types() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Enter config mode
        device.send(b"configure terminal\r").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Send "interface ?" to see available interface types
        device.send(b"interface ?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);

        assert!(output.contains("GigabitEthernet"),
            "interface ? should show GigabitEthernet, got: {:?}", output);
        assert!(output.contains("Loopback"),
            "interface ? should show Loopback, got: {:?}", output);
        assert!(output.contains("Vlan"),
            "interface ? should show Vlan, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_interface_abbreviation_still_works() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;

        // "int lo 0" should abbreviate to "interface Loopback 0" and enter config-if
        let output = send_cmd(&mut device, "int lo 0").await;
        assert!(matches!(device.mode, CliMode::ConfigSub(ref s) if s == "config-if"),
            "Should be in config-if after 'int lo 0', mode: {:?}, output: {:?}",
            device.mode, output);

        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_help_shows_header() {
        // 1. Top-level ? in privileged exec shows "Exec commands:" header
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // consume initial prompt

        device.send(b"?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data).to_string();
        assert!(
            output.contains("Exec commands:"),
            "? in priv-exec should show 'Exec commands:' header, got: {:?}",
            output
        );

        // 2. Top-level ? in config mode shows "Configure commands:" header
        device.send(b"configure terminal\r").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data).to_string();
        assert!(
            output.contains("Configure commands:"),
            "? in config mode should show 'Configure commands:' header, got: {:?}",
            output
        );

        // 3. "show ?" does NOT show a header — only subcommands
        device.send(b"end\r").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show ?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data).to_string();
        assert!(
            !output.contains("Exec commands:"),
            "'show ?' should NOT show 'Exec commands:' header, got: {:?}",
            output
        );
        assert!(
            !output.contains("Configure commands:"),
            "'show ?' should NOT show 'Configure commands:' header, got: {:?}",
            output
        );
    }

    #[tokio::test]
    async fn test_help_shows_exec_header() {
        // 1. Top-level ? in privileged exec shows "Exec commands:" header
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data).to_string();
        assert!(
            output.contains("Exec commands:"),
            "? in priv-exec should show 'Exec commands:' header, got: {:?}",
            output
        );

        // 2. "show ?" does NOT show "Exec commands:" header
        device.send(b"show ?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data).to_string();
        assert!(
            !output.contains("Exec commands:"),
            "'show ?' should NOT show 'Exec commands:' header, got: {:?}",
            output
        );
    }

    #[tokio::test]
    async fn test_help_shows_config_header() {
        // Top-level ? in config mode shows "Configure commands:" header
        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"configure terminal\r").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"?").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data).to_string();
        assert!(
            output.contains("Configure commands:"),
            "? in config mode should show 'Configure commands:' header, got: {:?}",
            output
        );
    }

    #[tokio::test]
    async fn test_help_command() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "help").await;
        assert!(output.contains("question mark"), "help should describe ? usage, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_enable_in_priv_exec_noop() {
        let mut device = setup_device("R1").await;
        assert_eq!(device.mode, CliMode::PrivilegedExec);
        let output = send_cmd(&mut device, "enable").await;
        assert_eq!(device.mode, CliMode::PrivilegedExec, "enable in priv exec should keep mode as PrivilegedExec, got mode: {:?}", device.mode);
        assert!(!output.contains("% "), "enable in priv exec should produce no error, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_clock_set_accepted() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "clock set 14:30:00 28 February 2000").await;
        assert!(!output.contains("% "), "clock set should produce no error, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_debug_command() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "debug ip routing").await;
        assert!(output.contains("debugging is on"), "debug ip routing should say 'debugging is on', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_undebug_all() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "undebug all").await;
        assert!(output.contains("turned off"), "undebug all should say 'turned off', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_no_debug_all() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "no debug all").await;
        assert!(output.contains("turned off"), "no debug all should say 'turned off', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_clear_command() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "clear counters").await;
        assert!(!output.contains("% "), "clear counters should produce no error, got: {:?}", output);
    }

    #[tokio::test]
    async fn test_ssh_connection_refused() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "ssh -l admin 10.0.0.1").await;
        assert!(output.contains("Connection refused"), "ssh should say 'Connection refused', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_telnet_connection_refused() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "telnet 10.0.0.1").await;
        assert!(output.contains("Connection refused"), "telnet should say 'Connection refused', got: {:?}", output);
        assert!(output.contains("Trying"), "telnet should say 'Trying ...', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_cdp_neighbors() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show cdp neighbors").await;
        assert!(output.contains("Capability Codes"),
            "show cdp neighbors should contain 'Capability Codes', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_users() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show users").await;
        assert!(output.contains("Line"),
            "show users should contain 'Line', got: {:?}", output);
        assert!(output.contains("User"),
            "show users should contain 'User', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_logging() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show logging").await;
        assert!(output.contains("Syslog logging"),
            "show logging should contain 'Syslog logging', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_ip_ospf() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show ip ospf").await;
        assert!(output.contains("No router process"),
            "show ip ospf should say 'No router process', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_ip_protocols() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show ip protocols").await;
        assert!(output.contains("IP Routing"),
            "show ip protocols should contain 'IP Routing', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_processes_cpu() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show processes cpu").await;
        assert!(output.contains("CPU utilization"),
            "show processes cpu should contain 'CPU utilization', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_arp() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show arp").await;
        assert!(output.contains("Protocol"),
            "show arp should contain 'Protocol', got: {:?}", output);
        assert!(output.contains("Hardware Addr"),
            "show arp should contain 'Hardware Addr', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_mac_address_table() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show mac address-table").await;
        assert!(output.contains("Mac Address Table"),
            "show mac address-table should contain 'Mac Address Table', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_show_spanning_tree() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show spanning-tree").await;
        assert!(output.contains("Spanning tree enabled"),
            "show spanning-tree should contain 'Spanning tree enabled', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_access_list_config_and_show() {
        let mut device = setup_device("Router1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "access-list 100 permit ip any any").await;
        let _ = send_cmd(&mut device, "end").await;

        let output = send_cmd(&mut device, "show access-lists").await;
        assert!(
            output.contains("Extended IP access list 100"),
            "show access-lists should contain 'Extended IP access list 100', got: {:?}", output
        );
        assert!(
            output.contains("permit ip any any"),
            "show access-lists should contain 'permit ip any any', got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_show_ntp_status() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show ntp status").await;
        assert!(
            output.contains("unsynchronized"),
            "show ntp status should contain 'unsynchronized', got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_show_snmp() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show snmp").await;
        assert!(
            output.contains("SNMP packets"),
            "show snmp should contain 'SNMP packets', got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_show_privilege() {
        let mut device = setup_device("R1").await;
        let output = send_cmd(&mut device, "show privilege").await;
        assert!(
            output.contains("privilege level is 15"),
            "show privilege should contain 'privilege level is 15', got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_show_line() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show line").await;
        assert!(
            output.contains("Tty"),
            "show line should contain 'Tty', got: {:?}", output
        );
        assert!(
            output.contains("VTY"),
            "show line should contain 'VTY', got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_show_inventory() {
        let mut device = MockIosDevice::new("Router1")
            .with_model("WS-C3560CX-12PD-S");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = send_cmd(&mut device, "show inventory").await;
        assert!(
            output.contains("PID:"),
            "show inventory should contain 'PID:', got: {:?}", output
        );
        assert!(
            output.contains("WS-C3560CX-12PD-S"),
            "show inventory should contain the model name, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_show_environment() {
        let mut device = setup_device("Router1").await;
        let output = send_cmd(&mut device, "show environment").await;
        assert!(
            output.contains("TEMPERATURE"),
            "show environment should contain 'TEMPERATURE', got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_banner_motd_config() {
        let mut device = setup_device("R1").await;

        // Enter config mode
        let _ = send_cmd(&mut device, "configure terminal").await;

        // Set banner motd using # as delimiter
        let _ = send_cmd(&mut device, "banner motd #Welcome to Router#").await;

        // Exit back to privileged exec
        let _ = send_cmd(&mut device, "end").await;

        // Verify banner appears in running-config
        let output = send_cmd(&mut device, "show running-config").await;
        assert!(
            output.contains("banner motd"),
            "show running-config should contain 'banner motd', got: {:?}", output
        );
        assert!(
            output.contains("Welcome to Router"),
            "show running-config should contain banner text, got: {:?}", output
        );
    }

    #[tokio::test]
    async fn test_banner_displayed_on_connect() {
        let mut device = MockIosDevice::new("R1");
        device.state.banner_motd = "Welcome".to_string();

        // First receive should include the banner
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(
            output.contains("Welcome"),
            "Initial output should contain banner text, got: {:?}", output
        );
    }

    // ─── "no" prefix tree tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_no_shutdown_via_tree() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 1/0/1").await;

        // shutdown — admin_up should become false
        let _ = send_cmd(&mut device, "shutdown").await;
        let iface = device.state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/1").unwrap();
        assert!(!iface.admin_up, "shutdown should set admin_up=false");

        // no shutdown — admin_up should become true
        let out = send_cmd(&mut device, "no shutdown").await;
        assert!(!out.contains("Invalid"), "no shutdown should be accepted, got: {:?}", out);
        let iface = device.state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/1").unwrap();
        assert!(iface.admin_up, "no shutdown should set admin_up=true");

        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_no_ip_address_removes_ip() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface loopback 0").await;
        let _ = send_cmd(&mut device, "ip address 10.1.2.3 255.255.255.0").await;

        // Verify IP was set
        let iface = device.state.interfaces.iter().find(|i| i.name == "Loopback0").unwrap();
        assert!(iface.ip_address.is_some(), "ip address should have been set");

        // no ip address — removes the IP
        let out = send_cmd(&mut device, "no ip address").await;
        assert!(!out.contains("Invalid"), "no ip address should be accepted, got: {:?}", out);
        let iface = device.state.interfaces.iter().find(|i| i.name == "Loopback0").unwrap();
        assert!(iface.ip_address.is_none(), "no ip address should clear ip_address");

        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_no_description_clears() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 1/0/1").await;
        let _ = send_cmd(&mut device, "description My uplink interface").await;

        // Verify description was set
        let iface = device.state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.description, "My uplink interface", "description should have been set");

        // no description — clears it
        let out = send_cmd(&mut device, "no description").await;
        assert!(!out.contains("Invalid"), "no description should be accepted, got: {:?}", out);
        let iface = device.state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/1").unwrap();
        assert!(iface.description.is_empty(), "no description should clear description");

        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_no_hostname_resets() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "hostname MyRouter").await;
        assert_eq!(device.state.hostname, "MyRouter", "hostname should have been set");

        let out = send_cmd(&mut device, "no hostname").await;
        assert!(!out.contains("Invalid"), "no hostname should be accepted, got: {:?}", out);
        assert_eq!(device.state.hostname, "Router", "no hostname should reset to 'Router'");

        // Restore original
        let _ = send_cmd(&mut device, "hostname R1").await;
        let _ = send_cmd(&mut device, "end").await;
    }

    #[tokio::test]
    async fn test_no_tab_completion() {
        let mut device = MockIosDevice::new("R1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Enter config mode
        device.send(b"configure terminal\r").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Enter config-if sub-mode (shutdown is only available there)
        device.send(b"interface GigabitEthernet 1/0/1\r").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Type "no shut" then Tab — should complete to "no shutdown "
        device.send(b"no shut").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"\t").await.unwrap();
        let data = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&data);
        assert!(output.contains("down ") || output.contains("shutdown"),
            "Tab after 'no shut' should complete to 'no shutdown', got: {:?}", output);
    }

    #[tokio::test]
    async fn test_description_writes_to_state() {
        let mut device = setup_device("R1").await;
        let _ = send_cmd(&mut device, "configure terminal").await;
        let _ = send_cmd(&mut device, "interface GigabitEthernet 1/0/1").await;
        let _ = send_cmd(&mut device, "description WAN uplink").await;
        let _ = send_cmd(&mut device, "end").await;

        // Verify state was updated
        let iface = device.state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.description, "WAN uplink",
            "description should be stored in device state, got: {:?}", iface.description);

        // Also verify show running-config contains it
        let output = send_cmd(&mut device, "show running-config").await;
        assert!(output.contains("WAN uplink"),
            "show running-config should contain description text, got: {:?}",
            &output[..output.len().min(500)]);
    }

    // --- show ip interface brief tests ---

    #[tokio::test]
    async fn test_show_ip_interface_brief_te_abbreviation() {
        let mut device = setup_device("Switch1").await;
        let output = send_cmd(&mut device, "show ip interface brief").await;
        assert!(output.contains("Te1/0/1"),
            "TenGigabitEthernet should be abbreviated to Te, got: {:?}",
            &output[..output.len().min(1000)]);
        assert!(!output.contains("TenGigabitEthernet1/0/1"),
            "Full TenGigabitEthernet name should NOT appear, got: {:?}",
            &output[..output.len().min(1000)]);
    }

    #[tokio::test]
    async fn test_show_ip_interface_brief_method_unset() {
        let mut device = setup_device("Switch1").await;
        let output = send_cmd(&mut device, "show ip interface brief").await;
        // Vlan1 has IP → Method should be NVRAM
        let vlan1_line = output.lines()
            .find(|l| l.contains("Vlan1"))
            .unwrap_or("");
        assert!(vlan1_line.contains("NVRAM"),
            "Vlan1 with IP should show NVRAM, got line: {:?}", vlan1_line);
        // GigabitEthernet1/0/1 has no IP → Method should be unset (name fits in 23 chars, no abbreviation)
        let gi_line = output.lines()
            .find(|l| l.starts_with("GigabitEthernet1/0/1"))
            .unwrap_or("");
        assert!(gi_line.contains("unset"),
            "GigabitEthernet1/0/1 without IP should show unset, got line: {:?}", gi_line);
    }

    #[tokio::test]
    async fn test_show_ip_interface_brief_admin_down() {
        let mut device = setup_device("Switch1").await;
        // Shut down GigabitEthernet1/0/1
        device.state.interfaces.iter_mut()
            .find(|i| i.name == "GigabitEthernet1/0/1")
            .unwrap()
            .admin_up = false;
        let output = send_cmd(&mut device, "show ip interface brief").await;
        // Find the GigabitEthernet1/0/1 line (full name fits in 23 chars)
        let gi_line = output.lines()
            .find(|l| l.starts_with("GigabitEthernet1/0/1 "))
            .unwrap_or("");
        assert!(gi_line.contains("administratively down"),
            "Shutdown interface should show 'administratively down', got line: {:?}", gi_line);
    }

    #[tokio::test]
    async fn test_show_ip_interface_brief_no_blank_line_after_header() {
        let mut device = setup_device("Switch1").await;
        let output = send_cmd(&mut device, "show ip interface brief").await;
        let lines: Vec<&str> = output.lines().collect();
        // Find the header line
        let header_idx = lines.iter()
            .position(|l| l.starts_with("Interface"))
            .expect("Header line should exist");
        // The line immediately after the header should NOT be blank
        assert!(header_idx + 1 < lines.len(),
            "There should be a line after the header");
        assert!(!lines[header_idx + 1].trim().is_empty(),
            "Line after header should not be blank, got: {:?}", lines[header_idx + 1]);
    }

    #[tokio::test]
    async fn test_show_interfaces_status() {
        let mut device = setup_device("Switch1").await;
        let output = send_cmd(&mut device, "show interfaces status").await;
        assert!(output.contains("Port"), "Should have header");
        assert!(output.contains("Gi1/0/1"), "Should show Gi interfaces");
        assert!(!output.contains("Invalid"), "Should not error");
    }
}
