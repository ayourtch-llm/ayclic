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
    pub vlans: Vec<VlanState>,
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
    pub mac_address: String, // "xxxx.xxxx.xxxx" format
    // Counters (simplified)
    pub input_packets: u64,
    pub output_packets: u64,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub input_errors: u64,
    pub output_errors: u64,
}

/// A VLAN entry as shown in `show vlan brief`.
pub struct VlanState {
    pub id: u16,
    pub name: String,
    pub active: bool,
    pub ports: Vec<String>, // short interface names assigned to this vlan
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
        // Generate a deterministic MAC based on the interface name hash
        let mac = Self::generate_mac(name);
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
            mac_address: mac,
            input_packets: 0,
            output_packets: 0,
            input_bytes: 0,
            output_bytes: 0,
            input_errors: 0,
            output_errors: 0,
        }
    }

    /// Generate a deterministic MAC address in IOS format (xxxx.xxxx.xxxx)
    /// from an interface name using a simple hash.
    fn generate_mac(name: &str) -> String {
        // Simple deterministic hash — not cryptographic, just unique per name
        let mut hash: u64 = 0x1234_5678_9abc;
        for b in name.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(b as u64);
        }
        let b0 = ((hash >> 40) & 0xfe) as u8; // clear multicast bit
        let b1 = ((hash >> 32) & 0xff) as u8;
        let b2 = ((hash >> 24) & 0xff) as u8;
        let b3 = ((hash >> 16) & 0xff) as u8;
        let b4 = ((hash >> 8) & 0xff) as u8;
        let b5 = (hash & 0xff) as u8;
        format!(
            "{:02x}{:02x}.{:02x}{:02x}.{:02x}{:02x}",
            b0, b1, b2, b3, b4, b5
        )
    }

    /// Generate detailed `show interfaces` output for this interface (IOS format).
    pub fn generate_show_interface(&self) -> String {
        let (status, protocol) = if !self.admin_up {
            ("administratively down", "down")
        } else if self.link_up {
            ("up", "up")
        } else {
            ("down", "down (notconnect)")
        };

        let duplex_str = match self.duplex.as_str() {
            "full" => "Full-duplex",
            "half" => "Half-duplex",
            _ => "Auto-duplex",
        };
        let speed_str = match self.speed.as_str() {
            "10" => "10Mb/s",
            "100" => "100Mb/s",
            "1000" => "1Gb/s",
            "10000" => "10Gb/s",
            _ => "Auto-speed",
        };
        let bw_kbit = match self.speed.as_str() {
            "10" => 10_000u32,
            "100" => 100_000,
            "1000" => 1_000_000,
            "10000" => 10_000_000,
            _ => 1_000_000, // default 1G for auto
        };

        let ip_line = if let Some((addr, mask)) = self.ip_address {
            format!("  Internet address is {}/{}\n", addr, ipv4_mask_to_prefix_len(mask))
        } else {
            String::new()
        };

        format!(
            "{name} is {status}, line protocol is {protocol}\n\
  Hardware is Gigabit Ethernet, address is {mac} (bia {mac})\n\
{ip_line}\
  MTU {mtu} bytes, BW {bw} Kbit/sec, DLY 1000 usec,\n\
     reliability 255/255, txload 1/255, rxload 1/255\n\
  Encapsulation ARPA, loopback not set\n\
  Keepalive set (10 sec)\n\
  {duplex}, {speed}, media type is 10/100/1000BaseTX\n\
  input flow-control is off, output flow-control is unsupported\n\
  ARP type: ARPA, ARP Timeout 04:00:00\n\
  Last input never, output never, output hang never\n\
  Last clearing of \"show interface\" counters never\n\
  Input queue: 0/75/0/0 (size/max/drops/flushes); Total output drops: 0\n\
  5 minute input rate 0 bits/sec, 0 packets/sec\n\
  5 minute output rate 0 bits/sec, 0 packets/sec\n\
     {in_pkts} packets input, {in_bytes} bytes, 0 no buffer\n\
     Received 0 broadcasts (0 IP multicasts)\n\
     0 runts, 0 giants, 0 throttles\n\
     {in_err} input errors, 0 CRC, 0 frame, 0 overrun, 0 ignored\n\
     0 watchdog, 0 multicast, 0 pause input\n\
     {out_pkts} packets output, {out_bytes} bytes, 0 underruns\n\
     {out_err} output errors, 0 collisions, 1 interface resets\n",
            name = self.name,
            status = status,
            protocol = protocol,
            mac = self.mac_address,
            ip_line = ip_line,
            mtu = self.mtu,
            bw = bw_kbit,
            duplex = duplex_str,
            speed = speed_str,
            in_pkts = self.input_packets,
            in_bytes = self.input_bytes,
            in_err = self.input_errors,
            out_pkts = self.output_packets,
            out_bytes = self.output_bytes,
            out_err = self.output_errors,
        )
    }
}

/// Convert an IPv4 netmask to prefix length.
fn ipv4_mask_to_prefix_len(mask: Ipv4Addr) -> u8 {
    u32::from(mask).count_ones() as u8
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
            vlans: vec![VlanState {
                id: 1,
                name: "default".to_string(),
                active: true,
                ports: vec!["Gi0/0".to_string(), "Gi0/1".to_string()],
            }],
        }
    }

    /// Generate the `show vlan brief` table output.
    pub fn generate_show_vlan_brief(&self) -> String {
        let header = "VLAN Name                             Status    Ports\n\
---- -------------------------------- --------- -------------------------------";
        let mut lines = vec![header.to_string()];
        for vlan in &self.vlans {
            let status = if vlan.active { "active" } else { "act/unsup" };
            let ports_str = vlan.ports.join(", ");
            lines.push(format!(
                "{:<5}{:<33}{:<10}{}",
                vlan.id, vlan.name, status, ports_str
            ));
        }
        lines.join("\n")
    }

    /// Generate a running-config string from the structured state.
    pub fn generate_running_config(&self) -> String {
        // Build config body first so we can compute its byte count for the header.
        let mut body_lines: Vec<String> = Vec::new();

        body_lines.push("!".to_string());
        body_lines.push(format!("hostname {}", self.hostname));
        body_lines.push("!".to_string());

        // Interfaces
        for iface in &self.interfaces {
            body_lines.push(format!("interface {}", iface.name));
            if !iface.description.is_empty() {
                body_lines.push(format!(" description {}", iface.description));
            }
            if let Some((addr, mask)) = &iface.ip_address {
                body_lines.push(format!(" ip address {} {}", addr, mask));
            }
            if !iface.admin_up {
                body_lines.push(" shutdown".to_string());
            } else {
                body_lines.push(" no shutdown".to_string());
            }
            body_lines.push("!".to_string());
        }

        // Static routes
        for route in &self.static_routes {
            if let Some(nh) = route.next_hop {
                body_lines.push(format!(
                    "ip route {} {} {}",
                    route.prefix, route.mask, nh
                ));
            } else if let Some(iface) = &route.interface {
                body_lines.push(format!(
                    "ip route {} {} {}",
                    route.prefix, route.mask, iface
                ));
            }
        }

        if !self.static_routes.is_empty() {
            body_lines.push("!".to_string());
        }

        // Unmodeled config lines (VTY, etc.)
        for line in &self.unmodeled_config {
            body_lines.push(line.clone());
        }

        if !self.unmodeled_config.is_empty() {
            body_lines.push("!".to_string());
        }

        body_lines.push("end".to_string());

        let body = body_lines.join("\n");
        let byte_count = body.len();

        format!(
            "Building configuration...\n\nCurrent configuration : {} bytes\n{}",
            byte_count, body
        )
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
    fn test_show_run_has_building_header() {
        let state = DeviceState::new("R1");
        let output = state.generate_running_config();
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
