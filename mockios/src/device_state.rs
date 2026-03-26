//! Structured device state — the "data model" behind the CLI.

use std::collections::HashMap;
use std::net::Ipv4Addr;

use crate::InstallState;

/// Top-level device state model.
pub struct DeviceState {
    pub hostname: String,
    pub version: String,
    pub model: String,
    pub serial_number: String,
    pub config_register: String,
    pub uptime: String,
    pub interfaces: Vec<InterfaceState>, // ordered, like real IOS
    pub static_routes: Vec<StaticRoute>,
    pub flash_files: HashMap<String, Vec<u8>>,
    pub flash_total_size: u64,
    pub boot_variable: String,
    pub domain_name: String,
    pub name_servers: Vec<String>,
    pub enable_secret: Option<String>,
    pub banner_motd: String,
    pub install_state: Option<InstallState>,
    pub unmodeled_config: Vec<String>, // catch-all for unknown config lines
}

pub struct InterfaceState {
    pub name: String,        // "GigabitEthernet0/0"
    pub description: String,
    pub admin_up: bool,      // false = "shutdown"
    pub link_up: bool,       // simulated link state
    pub ip_address: Option<(Ipv4Addr, Ipv4Addr)>, // (addr, mask)
    pub speed: String,
    pub duplex: String,
    pub mtu: u16,
    pub switchport_mode: Option<String>,
    pub vlan: Option<u16>,
}

pub struct StaticRoute {
    pub prefix: Ipv4Addr,
    pub mask: Ipv4Addr,
    pub next_hop: Option<Ipv4Addr>,
    pub interface: Option<String>,
    pub admin_distance: u8,
}

impl InterfaceState {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            admin_up: true,
            link_up: true,
            ip_address: None,
            speed: "auto".to_string(),
            duplex: "auto".to_string(),
            mtu: 1500,
            switchport_mode: None,
            vlan: None,
        }
    }
}

impl DeviceState {
    /// Create a default state matching `default_running_config()` in lib.rs.
    pub fn new(hostname: &str) -> Self {
        let mut gi0 = InterfaceState::new("GigabitEthernet0/0");
        gi0.ip_address = Some((
            "10.0.0.1".parse().unwrap(),
            "255.255.255.0".parse().unwrap(),
        ));
        gi0.admin_up = true;

        let mut gi1 = InterfaceState::new("GigabitEthernet0/1");
        gi1.ip_address = Some((
            "10.0.1.1".parse().unwrap(),
            "255.255.255.0".parse().unwrap(),
        ));
        gi1.admin_up = false; // shutdown

        let default_route = StaticRoute {
            prefix: "0.0.0.0".parse().unwrap(),
            mask: "0.0.0.0".parse().unwrap(),
            next_hop: Some("10.0.0.254".parse().unwrap()),
            interface: None,
            admin_distance: 1,
        };

        Self {
            hostname: hostname.to_string(),
            version: "15.1(4)M".to_string(),
            model: "C2951".to_string(),
            serial_number: "FCZ123456789".to_string(),
            config_register: "0x2102".to_string(),
            uptime: "42 days, 3 hours, 17 minutes".to_string(),
            interfaces: vec![gi0, gi1],
            static_routes: vec![default_route],
            flash_files: HashMap::new(),
            flash_total_size: 8_000_000_000,
            boot_variable: String::new(),
            domain_name: String::new(),
            name_servers: Vec::new(),
            enable_secret: None,
            banner_motd: String::new(),
            install_state: None,
            unmodeled_config: vec![
                "line vty 0 4".to_string(),
                " transport input ssh".to_string(),
            ],
        }
    }

    /// Generate a running-config string from the structured state.
    pub fn generate_running_config(&self) -> String {
        let mut lines: Vec<String> = Vec::new();

        lines.push("!".to_string());
        lines.push(format!("hostname {}", self.hostname));
        lines.push("!".to_string());

        // Interfaces
        for iface in &self.interfaces {
            lines.push(format!("interface {}", iface.name));
            if !iface.description.is_empty() {
                lines.push(format!(" description {}", iface.description));
            }
            if let Some((addr, mask)) = &iface.ip_address {
                lines.push(format!(" ip address {} {}", addr, mask));
            }
            if !iface.admin_up {
                lines.push(" shutdown".to_string());
            } else {
                lines.push(" no shutdown".to_string());
            }
            lines.push("!".to_string());
        }

        // Static routes
        for route in &self.static_routes {
            if let Some(nh) = route.next_hop {
                lines.push(format!(
                    "ip route {} {} {}",
                    route.prefix, route.mask, nh
                ));
            } else if let Some(iface) = &route.interface {
                lines.push(format!(
                    "ip route {} {} {}",
                    route.prefix, route.mask, iface
                ));
            }
        }

        if !self.static_routes.is_empty() {
            lines.push("!".to_string());
        }

        // Unmodeled config lines (VTY, etc.)
        for line in &self.unmodeled_config {
            lines.push(line.clone());
        }

        if !self.unmodeled_config.is_empty() {
            lines.push("!".to_string());
        }

        lines.push("end".to_string());

        lines.join("\n")
    }

    /// Find an interface by name, returning a mutable reference.
    pub fn get_interface_mut(&mut self, name: &str) -> Option<&mut InterfaceState> {
        self.interfaces.iter_mut().find(|i| i.name == name)
    }

    /// Find an interface by name (immutable).
    pub fn get_interface(&self, name: &str) -> Option<&InterfaceState> {
        self.interfaces.iter().find(|i| i.name == name)
    }

    /// Get or create an interface by name.
    pub fn ensure_interface(&mut self, name: &str) -> &mut InterfaceState {
        if self.interfaces.iter().any(|i| i.name == name) {
            self.interfaces.iter_mut().find(|i| i.name == name).unwrap()
        } else {
            self.interfaces.push(InterfaceState::new(name));
            self.interfaces.last_mut().unwrap()
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interface_new_defaults() {
        let iface = InterfaceState::new("GigabitEthernet0/0");
        assert_eq!(iface.name, "GigabitEthernet0/0");
        assert!(iface.admin_up);
        assert!(iface.link_up);
        assert_eq!(iface.mtu, 1500);
        assert_eq!(iface.speed, "auto");
        assert_eq!(iface.duplex, "auto");
        assert!(iface.ip_address.is_none());
        assert!(iface.description.is_empty());
    }

    #[test]
    fn test_default_state_has_interfaces() {
        let state = DeviceState::new("Router1");
        assert_eq!(state.interfaces.len(), 2);
        assert_eq!(state.interfaces[0].name, "GigabitEthernet0/0");
        assert_eq!(state.interfaces[1].name, "GigabitEthernet0/1");
        // Gi0/0 is up, Gi0/1 is shutdown
        assert!(state.interfaces[0].admin_up);
        assert!(!state.interfaces[1].admin_up);
    }

    #[test]
    fn test_default_state_has_route() {
        let state = DeviceState::new("Router1");
        assert_eq!(state.static_routes.len(), 1);
        let route = &state.static_routes[0];
        assert_eq!(route.prefix.to_string(), "0.0.0.0");
        assert_eq!(route.mask.to_string(), "0.0.0.0");
        assert_eq!(route.next_hop.unwrap().to_string(), "10.0.0.254");
    }

    #[test]
    fn test_generate_running_config() {
        let state = DeviceState::new("Router1");
        let config = state.generate_running_config();

        assert!(config.contains("hostname Router1"));
        assert!(config.contains("interface GigabitEthernet0/0"));
        assert!(config.contains("interface GigabitEthernet0/1"));
        assert!(config.contains("ip address 10.0.0.1 255.255.255.0"));
        assert!(config.contains("ip address 10.0.1.1 255.255.255.0"));
        assert!(config.contains("ip route 0.0.0.0 0.0.0.0 10.0.0.254"));
        assert!(config.contains("end"));
    }

    #[test]
    fn test_generate_running_config_shutdown_interface() {
        let state = DeviceState::new("Router1");
        let config = state.generate_running_config();
        // Gi0/1 should have shutdown
        assert!(config.contains(" shutdown"));
    }

    #[test]
    fn test_generate_running_config_hostname() {
        let state = DeviceState::new("MyRouter");
        let config = state.generate_running_config();
        assert!(config.contains("hostname MyRouter"));
        assert!(!config.contains("hostname Router1"));
    }

    #[test]
    fn test_ensure_interface_creates_new() {
        let mut state = DeviceState::new("Router1");
        let initial_count = state.interfaces.len();
        state.ensure_interface("GigabitEthernet0/2");
        assert_eq!(state.interfaces.len(), initial_count + 1);
        assert!(state.get_interface("GigabitEthernet0/2").is_some());
    }

    #[test]
    fn test_ensure_interface_returns_existing() {
        let mut state = DeviceState::new("Router1");
        let initial_count = state.interfaces.len();
        state.ensure_interface("GigabitEthernet0/0");
        assert_eq!(state.interfaces.len(), initial_count); // no new iface added
    }

    #[test]
    fn test_get_interface_mut() {
        let mut state = DeviceState::new("Router1");
        {
            let iface = state.get_interface_mut("GigabitEthernet0/0").unwrap();
            iface.description = "WAN link".to_string();
        }
        assert_eq!(
            state.get_interface("GigabitEthernet0/0").unwrap().description,
            "WAN link"
        );
    }
}
