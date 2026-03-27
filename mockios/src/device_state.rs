//! Structured device state — the "data model" behind the CLI.

use std::collections::HashMap;
use std::net::Ipv4Addr;

use crate::InstallState;

pub struct AccessListEntry {
    pub action: String,      // "permit" or "deny"
    pub protocol: String,    // "ip", "tcp", "udp", "icmp"
    pub source: String,      // "any", "host 10.0.0.1", "10.0.0.0 0.0.0.255"
    pub destination: String, // same format as source
    pub extra: String,       // "eq 80", "gt 1024", etc.
}

pub struct AccessList {
    pub name: String,        // number or name like "100" or "BLOCK-MGMT"
    pub acl_type: String,    // "Standard" or "Extended"
    pub entries: Vec<AccessListEntry>,
}

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
    pub access_lists: Vec<AccessList>,
    // New fields for batch 1
    pub base_mac: String,
    pub sw_image: String,
    pub last_reload_reason: String,
    pub service_password_encryption: bool,
    pub aaa_new_model: bool,
    pub ip_routing: bool,
    pub spanning_tree_mode: String,
    pub vtp_mode: String,
    pub vtp_domain: String,
}

pub struct InterfaceState {
    pub name: String,        // "GigabitEthernet1/0/1"
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
    pub unsupported: bool,  // true → show "act/unsup" instead of "active"
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
    /// Create a default state matching a WS-C3560CX-12PD-S switch.
    pub fn new(hostname: &str) -> Self {
        // Vlan1 — management interface with IP
        let mut vlan1 = InterfaceState::new("Vlan1");
        vlan1.ip_address = Some((
            "10.0.0.1".parse().unwrap(),
            "255.255.255.0".parse().unwrap(),
        ));
        vlan1.admin_up = true;
        vlan1.link_up = true;

        // GigabitEthernet1/0/1 through 1/0/12
        let mut gi_interfaces: Vec<InterfaceState> = (1..=12).map(|n| {
            let mut iface = InterfaceState::new(&format!("GigabitEthernet1/0/{}", n));
            iface.admin_up = true;
            iface.link_up = n <= 4; // first 4 ports are "connected"
            iface
        }).collect();

        // GigabitEthernet1/0/13 through 1/0/16 (uplink capable ports, but not connected)
        let mut gi_uplink: Vec<InterfaceState> = (13..=16).map(|n| {
            let mut iface = InterfaceState::new(&format!("GigabitEthernet1/0/{}", n));
            iface.admin_up = true;
            iface.link_up = false;
            iface
        }).collect();

        // TenGigabitEthernet1/0/1 and 1/0/2
        let te_interfaces: Vec<InterfaceState> = (1..=2).map(|n| {
            let mut iface = InterfaceState::new(&format!("TenGigabitEthernet1/0/{}", n));
            iface.admin_up = true;
            iface.link_up = false;
            iface
        }).collect();

        let mut interfaces = vec![vlan1];
        interfaces.append(&mut gi_interfaces);
        interfaces.append(&mut gi_uplink);
        interfaces.extend(te_interfaces);

        let default_route = StaticRoute {
            prefix: "0.0.0.0".parse().unwrap(),
            mask: "0.0.0.0".parse().unwrap(),
            next_hop: Some("10.0.0.254".parse().unwrap()),
            interface: None,
            admin_distance: 1,
        };

        // Build VLAN 1 port list from all Gi and Te interfaces (short names)
        let vlan1_ports: Vec<String> = (1..=16)
            .map(|n| format!("Gi1/0/{}", n))
            .chain((1..=2).map(|n| format!("Te1/0/{}", n)))
            .collect();

        let vlans = vec![
            VlanState {
                id: 1,
                name: "default".to_string(),
                active: true,
                ports: vlan1_ports,
                unsupported: false,
            },
            VlanState {
                id: 1002,
                name: "fddi-default".to_string(),
                active: true,
                ports: vec![],
                unsupported: true,
            },
            VlanState {
                id: 1003,
                name: "trcrf-default".to_string(),
                active: true,
                ports: vec![],
                unsupported: true,
            },
            VlanState {
                id: 1004,
                name: "fddinet-default".to_string(),
                active: true,
                ports: vec![],
                unsupported: true,
            },
            VlanState {
                id: 1005,
                name: "trbrf-default".to_string(),
                active: true,
                ports: vec![],
                unsupported: true,
            },
        ];

        Self {
            hostname: hostname.to_string(),
            version: "15.2(7)E13".to_string(),
            model: "WS-C3560CX-12PD-S".to_string(),
            serial_number: "FCW2144L08G".to_string(),
            config_register: "0xF".to_string(),
            uptime: "42 days, 3 hours, 17 minutes".to_string(),
            interfaces,
            static_routes: vec![default_route],
            flash_files: HashMap::new(),
            flash_total_size: 8_000_000_000,
            boot_variable: String::new(),
            domain_name: String::new(),
            name_servers: Vec::new(),
            enable_secret: None,
            banner_motd: String::new(),
            install_state: None,
            unmodeled_config: Vec::new(),
            vlans,
            access_lists: Vec::new(),
            base_mac: "f4:cf:e2:aa:bb:cc".to_string(),
            sw_image: "C3560CX-UNIVERSALK9-M".to_string(),
            last_reload_reason: "power-on".to_string(),
            service_password_encryption: true,
            aaa_new_model: true,
            ip_routing: true,
            spanning_tree_mode: "rapid-pvst".to_string(),
            vtp_mode: "transparent".to_string(),
            vtp_domain: String::new(),
        }
    }

    /// Generate the `show vlan brief` table output.
    pub fn generate_show_vlan_brief(&self) -> String {
        let header = "VLAN Name                             Status    Ports\n\
---- -------------------------------- --------- -------------------------------";
        let mut lines = vec![header.to_string()];
        for vlan in &self.vlans {
            let status = if vlan.unsupported {
                "act/unsup"
            } else if vlan.active {
                "active"
            } else {
                "act/unsup"
            };
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

        // Version line — extract major.minor from version string (e.g., "15.2" from "15.2(7)E13")
        let ver_short = self.version.find('(')
            .map(|i| &self.version[..i])
            .unwrap_or(&self.version);
        body_lines.push(format!("version {}", ver_short));
        body_lines.push("service timestamps debug datetime msec".to_string());
        body_lines.push("service timestamps log datetime msec".to_string());
        if self.service_password_encryption {
            body_lines.push("service password-encryption".to_string());
        }
        body_lines.push("!".to_string());

        body_lines.push(format!("hostname {}", self.hostname));
        body_lines.push("!".to_string());
        body_lines.push("boot-start-marker".to_string());
        body_lines.push("boot-end-marker".to_string());
        body_lines.push("!".to_string());

        if self.aaa_new_model {
            body_lines.push("aaa new-model".to_string());
            body_lines.push("!".to_string());
        }

        if !self.banner_motd.is_empty() {
            body_lines.push(format!("banner motd ^{}^", self.banner_motd));
            body_lines.push("!".to_string());
        }

        if self.ip_routing {
            body_lines.push("ip routing".to_string());
            body_lines.push("!".to_string());
        }

        body_lines.push(format!("spanning-tree mode {}", self.spanning_tree_mode));
        body_lines.push("spanning-tree extend system-id".to_string());
        body_lines.push("!".to_string());

        body_lines.push(format!("vtp mode {}", self.vtp_mode));
        body_lines.push("!".to_string());

        // Interfaces
        for iface in &self.interfaces {
            body_lines.push(format!("interface {}", iface.name));
            if !iface.description.is_empty() {
                body_lines.push(format!(" description {}", iface.description));
            }
            // Add switchport mode access for Gi/Te interfaces with no IP
            let is_switchport = (iface.name.starts_with("GigabitEthernet") || iface.name.starts_with("TenGigabitEthernet"))
                && iface.ip_address.is_none();
            if is_switchport {
                let mode = iface.switchport_mode.as_deref().unwrap_or("access");
                body_lines.push(format!(" switchport mode {}", mode));
            }
            if let Some((addr, mask)) = &iface.ip_address {
                body_lines.push(format!(" ip address {} {}", addr, mask));
            }
            if !iface.admin_up {
                body_lines.push(" shutdown".to_string());
            }
            // Real IOS does NOT show "no shutdown" for admin-up interfaces
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

        // Access lists
        for acl in &self.access_lists {
            for entry in &acl.entries {
                let mut line = format!("access-list {} {} {}", acl.name, entry.action, entry.protocol);
                if !entry.source.is_empty() {
                    line.push_str(&format!(" {}", entry.source));
                }
                if !entry.destination.is_empty() {
                    line.push_str(&format!(" {}", entry.destination));
                }
                if !entry.extra.is_empty() {
                    line.push_str(&format!(" {}", entry.extra));
                }
                body_lines.push(line);
            }
        }
        if !self.access_lists.is_empty() {
            body_lines.push("!".to_string());
        }

        // Unmodeled config lines
        for line in &self.unmodeled_config {
            body_lines.push(line.clone());
        }

        if !self.unmodeled_config.is_empty() {
            body_lines.push("!".to_string());
        }

        // Line configuration (real IOS style)
        body_lines.push("!".to_string());
        body_lines.push("line con 0".to_string());
        body_lines.push(" stopbits 1".to_string());
        body_lines.push("line vty 0 4".to_string());
        body_lines.push(" login local".to_string());
        body_lines.push(" transport input ssh".to_string());
        body_lines.push("line vty 5 15".to_string());
        body_lines.push(" login local".to_string());
        body_lines.push(" transport input ssh".to_string());
        body_lines.push("!".to_string());

        body_lines.push("end".to_string());

        let body = body_lines.join("\n");
        let byte_count = body.len();

        format!(
            "Building configuration...\n\nCurrent configuration : {} bytes\n{}",
            byte_count, body
        )
    }

    /// Generate a startup-config string (same content as running config, different header).
    pub fn generate_startup_config(&self) -> String {
        // Build the config body (same as running config body)
        let mut body_lines: Vec<String> = Vec::new();

        body_lines.push("!".to_string());

        let ver_short = self.version.find('(')
            .map(|i| &self.version[..i])
            .unwrap_or(&self.version);
        body_lines.push(format!("version {}", ver_short));
        body_lines.push("service timestamps debug datetime msec".to_string());
        body_lines.push("service timestamps log datetime msec".to_string());
        if self.service_password_encryption {
            body_lines.push("service password-encryption".to_string());
        }
        body_lines.push("!".to_string());

        body_lines.push(format!("hostname {}", self.hostname));
        body_lines.push("!".to_string());
        body_lines.push("boot-start-marker".to_string());
        body_lines.push("boot-end-marker".to_string());
        body_lines.push("!".to_string());

        if self.aaa_new_model {
            body_lines.push("aaa new-model".to_string());
            body_lines.push("!".to_string());
        }

        if self.ip_routing {
            body_lines.push("ip routing".to_string());
            body_lines.push("!".to_string());
        }

        body_lines.push(format!("spanning-tree mode {}", self.spanning_tree_mode));
        body_lines.push("spanning-tree extend system-id".to_string());
        body_lines.push("!".to_string());

        body_lines.push(format!("vtp mode {}", self.vtp_mode));
        body_lines.push("!".to_string());

        for iface in &self.interfaces {
            body_lines.push(format!("interface {}", iface.name));
            if !iface.description.is_empty() {
                body_lines.push(format!(" description {}", iface.description));
            }
            let is_switchport = (iface.name.starts_with("GigabitEthernet") || iface.name.starts_with("TenGigabitEthernet"))
                && iface.ip_address.is_none();
            if is_switchport {
                let mode = iface.switchport_mode.as_deref().unwrap_or("access");
                body_lines.push(format!(" switchport mode {}", mode));
            }
            if let Some((addr, mask)) = &iface.ip_address {
                body_lines.push(format!(" ip address {} {}", addr, mask));
            }
            if !iface.admin_up {
                body_lines.push(" shutdown".to_string());
            }
            body_lines.push("!".to_string());
        }

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

        // Access lists
        for acl in &self.access_lists {
            for entry in &acl.entries {
                let mut line = format!("access-list {} {} {}", acl.name, entry.action, entry.protocol);
                if !entry.source.is_empty() {
                    line.push_str(&format!(" {}", entry.source));
                }
                if !entry.destination.is_empty() {
                    line.push_str(&format!(" {}", entry.destination));
                }
                if !entry.extra.is_empty() {
                    line.push_str(&format!(" {}", entry.extra));
                }
                body_lines.push(line);
            }
        }
        if !self.access_lists.is_empty() {
            body_lines.push("!".to_string());
        }

        for line in &self.unmodeled_config {
            body_lines.push(line.clone());
        }

        if !self.unmodeled_config.is_empty() {
            body_lines.push("!".to_string());
        }

        body_lines.push("!".to_string());
        body_lines.push("line con 0".to_string());
        body_lines.push(" stopbits 1".to_string());
        body_lines.push("line vty 0 4".to_string());
        body_lines.push(" login local".to_string());
        body_lines.push(" transport input ssh".to_string());
        body_lines.push("line vty 5 15".to_string());
        body_lines.push(" login local".to_string());
        body_lines.push(" transport input ssh".to_string());
        body_lines.push("!".to_string());

        body_lines.push("end".to_string());

        let body = body_lines.join("\n");
        let byte_count = body.len();

        format!("Using {} out of 524288 bytes\n{}", byte_count, body)
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
        let iface = InterfaceState::new("GigabitEthernet1/0/1");
        assert_eq!(iface.name, "GigabitEthernet1/0/1");
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
        let state = DeviceState::new("Switch1");
        // Should have Vlan1 + 16 Gi + 2 Te = 19 interfaces
        assert_eq!(state.interfaces.len(), 19);
        assert_eq!(state.interfaces[0].name, "Vlan1");
        assert_eq!(state.interfaces[1].name, "GigabitEthernet1/0/1");
        assert_eq!(state.interfaces[16].name, "GigabitEthernet1/0/16");
        assert_eq!(state.interfaces[17].name, "TenGigabitEthernet1/0/1");
        assert_eq!(state.interfaces[18].name, "TenGigabitEthernet1/0/2");
        // Vlan1 is up
        assert!(state.interfaces[0].admin_up);
        // All Gi/Te are admin up
        assert!(state.interfaces[1].admin_up);
    }

    #[test]
    fn test_default_state_has_route() {
        let state = DeviceState::new("Switch1");
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
    fn test_show_run_has_version_and_service() {
        let state = DeviceState::new("R1");
        let output = state.generate_running_config();
        assert!(output.contains("version 15.2"), "show run should have version line, got: {:?}", &output[..output.len().min(300)]);
        assert!(output.contains("service timestamps debug datetime msec"), "show run should have service timestamps debug");
        assert!(output.contains("service timestamps log datetime msec"), "show run should have service timestamps log");
    }

    #[test]
    fn test_generate_running_config() {
        let state = DeviceState::new("Switch1");
        let config = state.generate_running_config();

        assert!(config.contains("hostname Switch1"));
        assert!(config.contains("interface Vlan1"));
        assert!(config.contains("interface GigabitEthernet1/0/1"));
        assert!(config.contains("ip address 10.0.0.1 255.255.255.0"));
        assert!(config.contains("ip route 0.0.0.0 0.0.0.0 10.0.0.254"));
        assert!(config.contains("end"));
    }

    #[test]
    fn test_generate_running_config_shutdown_interface() {
        // No interfaces in the default config are shutdown; verify that works
        let mut state = DeviceState::new("Switch1");
        state.interfaces[1].admin_up = false; // shut down Gi1/0/1
        let config = state.generate_running_config();
        assert!(config.contains(" shutdown"));
    }

    #[test]
    fn test_generate_running_config_hostname() {
        let state = DeviceState::new("MySwitch");
        let config = state.generate_running_config();
        assert!(config.contains("hostname MySwitch"));
        assert!(!config.contains("hostname Switch1"));
    }

    #[test]
    fn test_ensure_interface_creates_new() {
        let mut state = DeviceState::new("Switch1");
        let initial_count = state.interfaces.len();
        state.ensure_interface("GigabitEthernet1/0/99");
        assert_eq!(state.interfaces.len(), initial_count + 1);
        assert!(state.get_interface("GigabitEthernet1/0/99").is_some());
    }

    #[test]
    fn test_ensure_interface_returns_existing() {
        let mut state = DeviceState::new("Switch1");
        let initial_count = state.interfaces.len();
        state.ensure_interface("GigabitEthernet1/0/1");
        assert_eq!(state.interfaces.len(), initial_count); // no new iface added
    }

    #[test]
    fn test_get_interface_mut() {
        let mut state = DeviceState::new("Switch1");
        {
            let iface = state.get_interface_mut("GigabitEthernet1/0/1").unwrap();
            iface.description = "WAN link".to_string();
        }
        assert_eq!(
            state.get_interface("GigabitEthernet1/0/1").unwrap().description,
            "WAN link"
        );
    }

    #[test]
    fn test_default_vlans_include_unsupported() {
        let state = DeviceState::new("Switch1");
        // Should have VLAN 1 + 1002-1005 = 5 VLANs
        assert_eq!(state.vlans.len(), 5);
        let vlan1002 = state.vlans.iter().find(|v| v.id == 1002).unwrap();
        assert!(vlan1002.unsupported);
        assert_eq!(vlan1002.name, "fddi-default");
    }

    #[test]
    fn test_show_vlan_brief_unsupported_status() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_vlan_brief();
        assert!(output.contains("act/unsup"), "VLANs 1002-1005 should show act/unsup");
        assert!(output.contains("fddi-default"));
    }
}
