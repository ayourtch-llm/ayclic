# MockIOS Device State Model

## Status: IN PROGRESS

## Goal

Replace the flat `running_config: Vec<String>` with a structured device
state model. Command handlers read from and write to this model, so the
mock device behaves like a real device — changes to config are reflected
in show commands, interface state affects routing, etc.

## Current State (before this change)

MockIosDevice stores state as:
- `hostname: String`
- `running_config: Vec<String>` — flat list of config lines
- `flash_files: HashMap<String, Vec<u8>>`
- `version: String`, `model: String`
- `boot_variable: String`
- `install_state: Option<InstallState>`

Handlers parse `running_config` on-the-fly to generate show output
(e.g., `handle_show_ip_interface_brief` scans for `interface` and
`ip address` lines). This is fragile and doesn't support state changes
like shutdown toggling link state, or routes being derived from
interface addresses.

## Proposed Device State Model

```rust
/// Structured device state — the "data model" behind the CLI.
pub struct DeviceState {
    pub hostname: String,
    pub version: String,
    pub model: String,
    pub serial_number: String,
    pub config_register: String,
    pub uptime: String,

    /// Network interfaces, keyed by canonical name.
    pub interfaces: IndexMap<String, InterfaceState>,

    /// Static routes.
    pub static_routes: Vec<StaticRoute>,

    /// Flash filesystem.
    pub flash_files: HashMap<String, Vec<u8>>,
    pub flash_total_size: u64,

    /// Boot configuration.
    pub boot_variable: String,

    /// Domain name and DNS.
    pub domain_name: String,
    pub name_servers: Vec<String>,

    /// Users.
    pub usernames: Vec<Username>,

    /// Enable secret (hashed in real IOS, plaintext in mock).
    pub enable_secret: Option<String>,

    /// VTY/console line config.
    pub lines: Vec<LineConfig>,

    /// Banners.
    pub banner_motd: String,
    pub banner_login: String,

    /// NTP.
    pub ntp_servers: Vec<String>,

    /// Logging.
    pub logging_hosts: Vec<String>,

    /// IOS-XE install state.
    pub install_state: Option<InstallState>,

    /// Arbitrary extra config lines (for commands we don't model yet).
    pub unmodeled_config: Vec<String>,
}

pub struct InterfaceState {
    pub name: String,           // e.g., "GigabitEthernet0/0"
    pub short_name: String,     // e.g., "Gi0/0"
    pub description: String,
    pub admin_up: bool,         // false = "shutdown"
    pub link_up: bool,          // simulated link state
    pub ip_address: Option<(Ipv4Addr, Ipv4Addr)>, // (addr, mask)
    pub speed: String,          // "auto", "100", "1000"
    pub duplex: String,         // "auto", "full", "half"
    pub mtu: u16,
    pub vlan: Option<u16>,      // for switchport access vlan
    pub switchport_mode: Option<String>, // "access", "trunk"
    pub mac_address: String,
    // Counters (for show interfaces)
    pub input_packets: u64,
    pub output_packets: u64,
    pub input_errors: u64,
    pub output_errors: u64,
    pub crc_errors: u64,
}

pub struct StaticRoute {
    pub prefix: Ipv4Addr,
    pub mask: Ipv4Addr,
    pub next_hop: Option<Ipv4Addr>,
    pub interface: Option<String>,
    pub admin_distance: u8,     // default 1
    pub name: Option<String>,
}

pub struct Username {
    pub name: String,
    pub privilege: u8,
    pub secret: String,
}

pub struct LineConfig {
    pub line_type: String,      // "vty", "console"
    pub first: u16,
    pub last: u16,
    pub transport_input: Vec<String>,
    pub login: String,          // "local", etc.
}
```

## How handlers change

### Before (parsing running_config):
```rust
fn handle_show_ip_interface_brief(d: &mut MockIosDevice) {
    for line in &d.running_config {
        if line.starts_with("interface ") { ... }
        if line.starts_with("ip address ") { ... }
    }
}
```

### After (reading structured state):
```rust
fn handle_show_ip_interface_brief(d: &mut MockIosDevice) {
    for (name, iface) in &d.state.interfaces {
        let ip = iface.ip_address
            .map(|(a, _)| a.to_string())
            .unwrap_or("unassigned".into());
        let status = if !iface.admin_up { "administratively down" }
                     else if iface.link_up { "up" }
                     else { "down" };
        // ... format table row
    }
}
```

### Config handlers write to state:
```rust
fn handle_config_interface(d: &mut MockIosDevice, args: &str) {
    let name = parse_interface_name(args);
    d.state.interfaces.entry(name.clone())
        .or_insert_with(|| InterfaceState::new(&name));
    d.mode = CliMode::ConfigSub("config-if".into());
    d.current_interface = Some(name);
}

fn handle_config_shutdown(d: &mut MockIosDevice, _args: &str) {
    if let Some(ref iface_name) = d.current_interface {
        if let Some(iface) = d.state.interfaces.get_mut(iface_name) {
            iface.admin_up = false;
        }
    }
}
```

### Show ip route reads from state:
```rust
fn handle_show_ip_route(d: &mut MockIosDevice, _args: &str) {
    // Connected routes: auto-generated from interfaces with IP + admin_up
    for iface in d.state.interfaces.values() {
        if iface.admin_up && iface.ip_address.is_some() {
            // Add "C    x.x.x.x/y is directly connected, InterfaceName"
        }
    }
    // Static routes from state
    for route in &d.state.static_routes {
        // Add "S    x.x.x.x/y [1/0] via z.z.z.z"
    }
}
```

### show running-config generates from state:
```rust
fn handle_show_running_config(d: &mut MockIosDevice, _args: &str) {
    // Generate running-config from structured state
    let mut lines = vec![];
    lines.push(format!("hostname {}", d.state.hostname));
    for (name, iface) in &d.state.interfaces {
        lines.push(format!("interface {}", name));
        if let Some((ip, mask)) = &iface.ip_address {
            lines.push(format!(" ip address {} {}", ip, mask));
        }
        if !iface.admin_up {
            lines.push(" shutdown".into());
        }
        lines.push("!".into());
    }
    // ... static routes, lines, etc.
}
```

## Implementation Plan

### Step 1: Define DeviceState struct
- Create `mockios/src/device_state.rs` with all types
- Add `InterfaceState::new()`, `DeviceState::default_for(hostname)`
- Default state should match current `default_running_config()` output
- Add `DeviceState::generate_running_config() -> Vec<String>` method
- Tests: verify default state generates same running config as before

### Step 2: Add DeviceState to MockIosDevice
- Add `pub state: DeviceState` field
- Keep `running_config: Vec<String>` temporarily for backward compat
- Update `new()` to initialize both
- Update `derive()` to copy state

### Step 3: Migrate show handlers to use DeviceState
- `show running-config` → generate from state
- `show ip interface brief` → read from state.interfaces
- `show ip route` → read from state.interfaces + state.static_routes
- `show version` → read from state.version, state.model, etc.
- `show boot` → read from state.boot_variable
- `show clock` → (keep static for now, could add clock state later)
- `ping` → check if target is in routing table
- Tests: existing tests must still pass

### Step 4: Migrate config handlers to write DeviceState
- `hostname` → update state.hostname (already works but should go through state)
- `interface X` → create/select interface in state.interfaces
- `ip address` → set on current interface
- `shutdown` / `no shutdown` → toggle admin_up on current interface
- `ip route` → add to state.static_routes
- `no ip route` → remove from state.static_routes
- Config handlers should track `current_interface` for config-if context
- Tests: configure interface, verify show commands reflect it

### Step 5: Remove running_config Vec<String>
- All handlers use DeviceState
- `show running-config` generates from state
- Remove `running_config` field
- `with_running_config()` builder parses config into state (or deprecated)

### Step 6: Enhanced behaviors
- Connected routes auto-generated from interfaces with IP + admin_up
- `ping` checks routing table for reachability
- `show interfaces` shows detailed info from state
- Interface counters (can be incremented for testing)

## Dependencies
- `indexmap` crate for ordered interface map (preserves insertion order
  like real IOS show commands)
- Standard library `Ipv4Addr` for IP addresses

## Notes for future sessions
- The command tree (cmd_tree.rs, cmd_tree_exec.rs, cmd_tree_conf.rs) is
  already in place and working with 151 unit tests
- Handlers are `fn(&mut MockIosDevice, &str)` — they have full access
  to device state
- `current_interface` tracking is needed for config-if sub-mode commands
  (shutdown, ip address, description, etc.)
- The `with_command()` custom command mechanism should still work for
  test-specific overrides
