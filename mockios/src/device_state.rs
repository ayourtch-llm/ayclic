//! Structured device state — the "data model" behind the CLI.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

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
    // IPv6 state
    pub ipv6_unicast_routing: bool,
    pub ipv6_static_routes: Vec<Ipv6StaticRoute>,
    pub ospfv3_processes: Vec<OspfV3Process>,
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
    // IPv6 state
    pub ipv6_enabled: bool,
    pub ipv6_addresses: Vec<Ipv6AddrConfig>,
    pub ospfv3_config: Option<InterfaceOspfV3Config>,
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

// ─── IPv6 Types ──────────────────────────────────────────────────────────────

/// An IPv6 address configured on an interface, with prefix length and type.
#[derive(Clone, Debug)]
pub struct Ipv6AddrConfig {
    pub address: Ipv6Addr,
    pub prefix_len: u8,
    pub addr_type: Ipv6AddrType,
    pub eui64: bool, // was this auto-generated via EUI-64?
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ipv6AddrType {
    LinkLocal,
    Global,
}

/// An entry in the IPv6 routing table (forwarding-engine-ready).
#[derive(Clone, Debug)]
pub struct Ipv6Route {
    pub prefix: Ipv6Addr,
    pub prefix_len: u8,
    pub route_type: Ipv6RouteType,
    pub admin_distance: u16,
    pub metric: u32,
    pub next_hop: Option<Ipv6Addr>,
    pub interface: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ipv6RouteType {
    Connected,      // C
    Local,          // L
    LocalConnected, // LC (loopback connected)
    Static,         // S
    OspfIntra,      // O
    OspfInter,      // OI
    OspfExt1,       // OE1
    OspfExt2,       // OE2
    NdDefault,      // ND
    NdPrefix,       // NDp
}

impl Ipv6RouteType {
    /// Short code as displayed in `show ipv6 route`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Connected => "C",
            Self::Local => "L",
            Self::LocalConnected => "LC",
            Self::Static => "S",
            Self::OspfIntra => "O",
            Self::OspfInter => "OI",
            Self::OspfExt1 => "OE1",
            Self::OspfExt2 => "OE2",
            Self::NdDefault => "ND",
            Self::NdPrefix => "NDp",
        }
    }
}

/// IPv6 static route (config: `ipv6 route <prefix>/<len> <next-hop|interface>`).
#[derive(Clone, Debug)]
pub struct Ipv6StaticRoute {
    pub prefix: Ipv6Addr,
    pub prefix_len: u8,
    pub next_hop: Option<Ipv6Addr>,
    pub interface: Option<String>,
    pub admin_distance: u8,
}

/// OSPFv3 process state (forwarding-engine-ready structure).
#[derive(Clone, Debug)]
pub struct OspfV3Process {
    pub process_id: u16,
    pub router_id: Option<Ipv4Addr>, // OSPFv3 still uses IPv4 router-ID
    pub areas: Vec<OspfV3Area>,
    pub reference_bandwidth: u32, // Mbps, default 100
    pub spf_delay: u32,           // initial delay ms, default 5000
    pub spf_hold: u32,            // min hold ms, default 10000
    pub spf_max_wait: u32,        // max wait ms, default 10000
}

impl OspfV3Process {
    pub fn new(process_id: u16) -> Self {
        Self {
            process_id,
            router_id: None,
            areas: Vec::new(),
            reference_bandwidth: 100,
            spf_delay: 5000,
            spf_hold: 10000,
            spf_max_wait: 10000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OspfV3Area {
    pub area_id: u32,
    pub area_type: OspfV3AreaType,
    pub spf_executions: u32,
    pub lsa_count: u32,
    pub lsa_checksum: u32,
}

impl OspfV3Area {
    pub fn new(area_id: u32) -> Self {
        Self {
            area_id,
            area_type: OspfV3AreaType::Normal,
            spf_executions: 0,
            lsa_count: 0,
            lsa_checksum: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OspfV3AreaType {
    Normal,
    Stub,
    Nssa,
}

/// Per-interface OSPFv3 config.
#[derive(Clone, Debug)]
pub struct InterfaceOspfV3Config {
    pub process_id: u16,
    pub area_id: u32,
    pub network_type: Option<String>, // "point-to-point", "broadcast", etc.
    pub cost: Option<u32>,
}

impl InterfaceState {
    pub fn new(name: &str) -> Self {
        // Generate a deterministic MAC based on the interface name hash
        let mac = Self::generate_mac(name);
        // TenGigabitEthernet ports default to full/10G; others default to auto/auto
        let is_te = name.starts_with("TenGigabitEthernet");
        let (speed, duplex) = if is_te {
            ("10000".to_string(), "full".to_string())
        } else {
            ("auto".to_string(), "auto".to_string())
        };
        // Virtual interfaces (Vlan, Loopback) are always link-up when admin-up.
        // Physical interfaces have no cable by default.
        let is_virtual = name.starts_with("Vlan") || name.starts_with("Loopback");
        Self {
            name: name.to_string(),
            description: String::new(),
            admin_up: true,
            link_up: is_virtual,
            ip_address: None,
            speed,
            duplex,
            mtu: 1500,
            switchport_mode: None,
            vlan: None,
            mac_address: mac,
            ipv6_enabled: false,
            ipv6_addresses: Vec::new(),
            ospfv3_config: None,
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

    /// Generate an EUI-64 link-local IPv6 address from this interface's MAC.
    /// MAC format: "xxxx.xxxx.xxxx" (Cisco dotted) → FE80::EUI64
    pub fn generate_eui64_link_local(&self) -> Ipv6Addr {
        mac_to_eui64_link_local(&self.mac_address)
    }

    /// Get the link-local address for this interface.
    /// Returns the explicitly configured one, or auto-generated EUI-64 if ipv6 is enabled.
    pub fn ipv6_link_local(&self) -> Option<Ipv6Addr> {
        // Check for explicitly configured link-local first
        for addr in &self.ipv6_addresses {
            if addr.addr_type == Ipv6AddrType::LinkLocal {
                return Some(addr.address);
            }
        }
        // Auto-generate from MAC if IPv6 is enabled or any global address exists
        if self.ipv6_enabled || self.ipv6_addresses.iter().any(|a| a.addr_type == Ipv6AddrType::Global) {
            return Some(self.generate_eui64_link_local());
        }
        None
    }

    /// Get all global IPv6 addresses configured on this interface.
    pub fn ipv6_global_addrs(&self) -> Vec<&Ipv6AddrConfig> {
        self.ipv6_addresses.iter().filter(|a| a.addr_type == Ipv6AddrType::Global).collect()
    }

    /// Whether this interface has any IPv6 configuration (enabled or addresses).
    pub fn has_ipv6(&self) -> bool {
        self.ipv6_enabled || !self.ipv6_addresses.is_empty()
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

        let desc_line = if !self.description.is_empty() {
            format!("  Description: {}\n", self.description)
        } else {
            String::new()
        };

        // Hardware type and media type vary by interface type
        let (hw_type, media_type, dly) = if self.name.starts_with("TenGigabitEthernet") {
            ("Ten Gigabit Ethernet", "SFP+ 10GBase", 10)
        } else if self.name.starts_with("Vlan") {
            ("EtherSVI", "unknown", 10)
        } else if self.name.starts_with("Loopback") {
            ("Loopback", "unknown", 5000)
        } else {
            ("Gigabit Ethernet", "10/100/1000BaseTX", 10)
        };

        format!(
            "{name} is {status}, line protocol is {protocol}\n\
  Hardware is {hw_type}, address is {mac} (bia {mac})\n\
{desc_line}\
{ip_line}\
  MTU {mtu} bytes, BW {bw} Kbit/sec, DLY {dly} usec, \n\
     reliability 255/255, txload 1/255, rxload 1/255\n\
  Encapsulation ARPA, loopback not set\n\
  Keepalive set (10 sec)\n\
  {duplex}, {speed}, media type is {media_type}\n\
  input flow-control is off, output flow-control is unsupported \n\
  ARP type: ARPA, ARP Timeout 04:00:00\n\
  Last input never, output never, output hang never\n\
  Last clearing of \"show interface\" counters never\n\
  Input queue: 0/75/0/0 (size/max/drops/flushes); Total output drops: 0\n\
  Queueing strategy: fifo\n\
  Output queue: 0/40 (size/max)\n\
  5 minute input rate 0 bits/sec, 0 packets/sec\n\
  5 minute output rate 0 bits/sec, 0 packets/sec\n\
     {in_pkts} packets input, {in_bytes} bytes, 0 no buffer\n\
     Received 0 broadcasts (0 multicasts)\n\
     0 runts, 0 giants, 0 throttles \n\
     {in_err} input errors, 0 CRC, 0 frame, 0 overrun, 0 ignored\n\
     0 watchdog, 0 multicast, 0 pause input\n\
     0 input packets with dribble condition detected\n\
     {out_pkts} packets output, {out_bytes} bytes, 0 underruns\n\
     {out_err} output errors, 0 collisions, 1 interface resets\n\
     0 unknown protocol drops\n\
     0 babbles, 0 late collision, 0 deferred\n\
     0 lost carrier, 0 no carrier, 0 pause output\n\
     0 output buffer failures, 0 output buffers swapped out\n",
            name = self.name,
            status = status,
            protocol = protocol,
            hw_type = hw_type,
            mac = self.mac_address,
            desc_line = desc_line,
            ip_line = ip_line,
            mtu = self.mtu,
            bw = bw_kbit,
            dly = dly,
            duplex = duplex_str,
            speed = speed_str,
            media_type = media_type,
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

/// Generate an EUI-64 link-local address from a Cisco dotted MAC (xxxx.xxxx.xxxx).
pub fn mac_to_eui64_link_local(mac: &str) -> Ipv6Addr {
    // Parse MAC bytes from Cisco dotted format or colon format
    let hex: String = mac.replace('.', "").replace(':', "").to_lowercase();
    if hex.len() < 12 {
        return Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1);
    }
    let bytes: Vec<u8> = (0..6)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0))
        .collect();

    // EUI-64: insert FF:FE in the middle, flip bit 7 (universal/local)
    let b0 = bytes[0] ^ 0x02; // flip U/L bit
    let eui64: [u8; 8] = [b0, bytes[1], bytes[2], 0xff, 0xfe, bytes[3], bytes[4], bytes[5]];

    Ipv6Addr::new(
        0xfe80, 0, 0, 0,
        u16::from_be_bytes([eui64[0], eui64[1]]),
        u16::from_be_bytes([eui64[2], eui64[3]]),
        u16::from_be_bytes([eui64[4], eui64[5]]),
        u16::from_be_bytes([eui64[6], eui64[7]]),
    )
}

/// Apply a prefix length mask to an IPv6 address (zero out host bits).
pub fn ipv6_apply_prefix(addr: Ipv6Addr, prefix_len: u8) -> Ipv6Addr {
    let bits = u128::from(addr);
    if prefix_len >= 128 {
        return addr;
    }
    let mask = !((1u128 << (128 - prefix_len)) - 1);
    Ipv6Addr::from(bits & mask)
}

/// Convert a colon-separated MAC address (e.g., "00:A3:D1:4F:22:80") to
/// Cisco dotted format ("00a3.d14f.2280").
pub fn mac_to_cisco_format(mac: &str) -> String {
    let hex: String = mac.replace(':', "").to_lowercase();
    format!("{}.{}.{}", &hex[0..4], &hex[4..8], &hex[8..12])
}

/// Extract the port number from an interface name like "GigabitEthernet1/0/13" → 13.
/// Parses the last number after the final `/`.
fn extract_port_number(name: &str) -> u16 {
    name.rsplit('/')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Always-abbreviated short interface name for spanning-tree tables.
/// GigabitEthernet→Gi, TenGigabitEthernet→Te, FastEthernet→Fa, etc.
pub fn short_interface_name(name: &str) -> String {
    let prefixes: &[(&str, &str)] = &[
        ("TenGigabitEthernet", "Te"),
        ("GigabitEthernet",    "Gi"),
        ("FastEthernet",       "Fa"),
        ("Loopback",           "Lo"),
        ("Vlan",               "Vl"),
    ];
    for (long, short) in prefixes {
        if let Some(suffix) = name.strip_prefix(long) {
            return format!("{}{}", short, suffix);
        }
    }
    name.to_string()
}

/// Abbreviate an interface name for use in `show ip interface brief` (max 23 chars).
/// Real IOS only abbreviates when the full name exceeds the 23-char column width.
/// - `TenGigabitEthernet1/0/1` (25 chars) → `Te1/0/1` (abbreviated)
/// - `GigabitEthernet1/0/1` (22 chars) → kept as-is (fits)
/// - `GigabitEthernet1/0/10` (23 chars) → kept as-is (fits)
/// Standard Cisco interface name abbreviation prefixes.
const INTERFACE_PREFIXES: &[(&str, &str)] = &[
    ("TenGigabitEthernet", "Te"),
    ("GigabitEthernet",    "Gi"),
    ("FastEthernet",       "Fa"),
    ("Loopback",           "Lo"),
    ("Vlan",               "Vl"),
];

/// Abbreviate an interface name to fit a fixed-width column (e.g., `show interfaces status`).
/// Only abbreviates if the name exceeds `max_len` characters.
pub fn abbreviate_interface_name(name: &str) -> String {
    abbreviate_interface_name_max(name, 23)
}

/// Abbreviate an interface name only if it exceeds `max_len` characters.
fn abbreviate_interface_name_max(name: &str, max_len: usize) -> String {
    if name.len() < max_len {
        return name.to_string();
    }

    for (long, short) in INTERFACE_PREFIXES {
        if let Some(suffix) = name.strip_prefix(long) {
            let abbreviated = format!("{}{}", short, suffix);
            if abbreviated.len() > max_len {
                return abbreviated[..max_len].to_string();
            }
            return abbreviated;
        }
    }

    name[..max_len].to_string()
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
        // Ports 1-4: admin up, no link (notconnect — no cable in simulation)
        // Ports 5-12: shutdown (disabled)
        let mut gi_interfaces: Vec<InterfaceState> = (1..=12).map(|n| {
            let mut iface = InterfaceState::new(&format!("GigabitEthernet1/0/{}", n));
            iface.admin_up = n <= 4;
            // link_up remains false (no cable); Vlan1 is the only link_up=true interface
            iface
        }).collect();

        // GigabitEthernet1/0/13 through 1/0/16 (uplink capable ports, shutdown — unconnected)
        let mut gi_uplink: Vec<InterfaceState> = (13..=16).map(|n| {
            let mut iface = InterfaceState::new(&format!("GigabitEthernet1/0/{}", n));
            iface.admin_up = false;
            iface
        }).collect();

        // TenGigabitEthernet1/0/1 and 1/0/2: admin up, no link (no SFP inserted)
        // Real IOS shows "notconnect" for Te ports without SFP, not "disabled".
        // Speed/duplex are already set to 10000/full by InterfaceState::new for Te ports.
        let te_interfaces: Vec<InterfaceState> = (1..=2).map(|n| {
            InterfaceState::new(&format!("TenGigabitEthernet1/0/{}", n))
            // admin_up=true (default), link_up=false (no SFP) → notconnect
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
            serial_number: "FOC2231X1YZ".to_string(),
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
            base_mac: "00:A3:D1:4F:22:80".to_string(),
            sw_image: "C3560CX-UNIVERSALK9-M".to_string(),
            last_reload_reason: "power-on".to_string(),
            service_password_encryption: true,
            aaa_new_model: true,
            ip_routing: true,
            spanning_tree_mode: "rapid-pvst".to_string(),
            vtp_mode: "transparent".to_string(),
            vtp_domain: String::new(),
            ipv6_unicast_routing: false,
            ipv6_static_routes: Vec::new(),
            ospfv3_processes: Vec::new(),
        }
    }

    /// Compute VLAN access port membership dynamically from interface state.
    /// Real IOS only shows access-mode ports in show vlan brief, not trunks.
    fn vlan_access_ports(&self, vlan_id: u16) -> Vec<String> {
        self.interfaces
            .iter()
            .filter(|iface| {
                let is_physical = iface.name.contains("Ethernet");
                let is_trunk = iface.switchport_mode.as_deref() == Some("trunk");
                let iface_vlan = iface.vlan.unwrap_or(1);
                is_physical && !is_trunk && iface_vlan == vlan_id
            })
            .map(|iface| short_interface_name(&iface.name))
            .collect()
    }

    /// Wrap a list of items into lines of at most `max_width` characters, comma-separated.
    fn wrap_comma_list(items: &[String], max_width: usize) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        let mut current = String::new();
        for item in items {
            if current.is_empty() {
                current.push_str(item);
            } else {
                let candidate = format!("{}, {}", current, item);
                if candidate.len() <= max_width {
                    current = candidate;
                } else {
                    result.push(current);
                    current = item.clone();
                }
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
        result
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

            // Dynamically compute port list from interface state
            let ports = self.vlan_access_ports(vlan.id);

            // Ports column starts at position 48; real IOS wraps at ~31 chars
            const PORTS_COL: usize = 48;
            const MAX_PORTS_WIDTH: usize = 31;

            let port_lines = Self::wrap_comma_list(&ports, MAX_PORTS_WIDTH);

            let first_ports = port_lines.first().map(|s| s.as_str()).unwrap_or("");
            lines.push(format!(
                "{:<5}{:<33}{:<10}{}",
                vlan.id, vlan.name, status, first_ports
            ));
            let indent = " ".repeat(PORTS_COL);
            for continuation in port_lines.iter().skip(1) {
                lines.push(format!("{}{}", indent, continuation));
            }
        }
        lines.join("\n")
    }

    /// Generate the secondary SAID/MTU table section for `show vlan`.
    fn generate_vlan_said_table(&self) -> String {
        let header = "\nVLAN Type  SAID       MTU   Parent RingNo BridgeNo Stp  BrdgMode Trans1 Trans2\n\
---- ----- ---------- ----- ------ ------ -------- ---- -------- ------ ------";
        let mut lines = vec![header.to_string()];
        for vlan in &self.vlans {
            let status = if vlan.unsupported || !vlan.active {
                "act/--"
            } else {
                "act/--"
            };
            let said = 100000u32 + vlan.id as u32;
            lines.push(format!(
                "{:<5}{:<6}{:<11}{:<6}{:<7}{:<7}{:<9}{:<5}{:<9}{:<7}{}",
                vlan.id, status, said, 1500, " ", " ", " ", " ", " ", " ", " "
            ));
        }
        lines.join("\n")
    }

    /// Generate the `show vlan` output (brief table + SAID/MTU section).
    pub fn generate_show_vlan(&self) -> String {
        format!("{}{}", self.generate_show_vlan_brief(), self.generate_vlan_said_table())
    }

    /// Generate `show vlan id <N>` output for a specific VLAN.
    pub fn generate_show_vlan_id(&self, vlan_id: u16) -> String {
        let vlan = self.vlans.iter().find(|v| v.id == vlan_id);
        match vlan {
            None => format!("VLAN id {} not found in current VLAN database", vlan_id),
            Some(vlan) => {
                let status = if vlan.unsupported || !vlan.active {
                    "act/unsup"
                } else {
                    "active"
                };
                let ports = self.vlan_access_ports(vlan_id);
                const MAX_PORTS_WIDTH: usize = 31;
                let port_lines = Self::wrap_comma_list(&ports, MAX_PORTS_WIDTH);
                let first_ports = port_lines.first().map(|s| s.as_str()).unwrap_or("");

                let mut lines = vec![
                    "VLAN Name                             Status    Ports".to_string(),
                    "---- -------------------------------- --------- -------------------------------".to_string(),
                    format!("{:<5}{:<33}{:<10}{}", vlan.id, vlan.name, status, first_ports),
                ];
                let indent = " ".repeat(48);
                for continuation in port_lines.iter().skip(1) {
                    lines.push(format!("{}{}", indent, continuation));
                }

                let said = 100000u32 + vlan.id as u32;
                lines.push(String::new());
                lines.push("VLAN Type  SAID       MTU   Parent RingNo BridgeNo Stp  BrdgMode Trans1 Trans2".to_string());
                lines.push("---- ----- ---------- ----- ------ ------ -------- ---- -------- ------ ------".to_string());
                lines.push(format!(
                    "{:<5}{:<6}{:<11}{:<6}{:<7}{:<7}{:<9}{:<5}{:<9}{:<7}{}",
                    vlan.id, "act/--", said, 1500, " ", " ", " ", " ", " ", " ", " "
                ));
                lines.join("\n")
            }
        }
    }

    /// Build the shared config body lines (used by both running-config and startup-config).
    fn build_config_body(&self) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();

        lines.push("!".to_string());

        // Version line — extract major.minor from version string (e.g., "15.2" from "15.2(7)E13")
        let ver_short = self.version.find('(')
            .map(|i| &self.version[..i])
            .unwrap_or(&self.version);
        lines.push(format!("version {}", ver_short));

        // 1. Before service timestamps
        lines.push("no service pad".to_string());
        lines.push("service unsupported-transceiver".to_string());

        lines.push("service timestamps debug datetime msec".to_string());
        lines.push("service timestamps log datetime msec".to_string());
        if self.service_password_encryption {
            lines.push("service password-encryption".to_string());
        }
        lines.push("!".to_string());

        lines.push(format!("hostname {}", self.hostname));
        lines.push("!".to_string());
        lines.push("boot-start-marker".to_string());
        lines.push("boot-end-marker".to_string());
        lines.push("!".to_string());

        // 2. enable secret after boot-end-marker block
        if self.enable_secret.is_some() {
            lines.push("enable secret 9 $9$d3ETqPBJcIJKIT$2FVhFPr0jwSkhfF7ShDEOF7Ns9YhPn6mFgOnTsGt5gQ".to_string());
            lines.push("!".to_string());
        }

        if self.aaa_new_model {
            lines.push("aaa new-model".to_string());
            lines.push("!".to_string());

            // 3. AAA method lists after aaa new-model section
            lines.push("aaa authentication login default local".to_string());
            lines.push("aaa authentication enable default enable".to_string());
            lines.push("aaa authorization exec default local".to_string());
            lines.push("!".to_string());
        }

        // 4. After AAA block: switch provision and system mtu
        lines.push(format!("switch 1 provision {}", self.model.to_lowercase()));
        lines.push("system mtu routing 1500".to_string());
        lines.push("!".to_string());

        if !self.banner_motd.is_empty() {
            lines.push(format!("banner motd ^{}^", self.banner_motd));
            lines.push("!".to_string());
        }

        if self.ip_routing {
            lines.push("ip routing".to_string());
            lines.push("!".to_string());
        }

        if self.ipv6_unicast_routing {
            lines.push("ipv6 unicast-routing".to_string());
            lines.push("!".to_string());
        }

        // 5. Before spanning-tree

        lines.push(format!("spanning-tree mode {}", self.spanning_tree_mode));
        lines.push("spanning-tree extend system-id".to_string());
        lines.push("!".to_string());

        lines.push(format!("vtp mode {}", self.vtp_mode));
        lines.push("!".to_string());

        // 6. After VTP config, before interfaces: lldp run
        lines.push("lldp run".to_string());
        lines.push("!".to_string());

        // Interfaces
        for iface in &self.interfaces {
            lines.push(format!("interface {}", iface.name));
            if !iface.description.is_empty() {
                lines.push(format!(" description {}", iface.description));
            }
            // Add switchport mode access for Gi/Te interfaces with no IP
            let is_switchport = (iface.name.starts_with("GigabitEthernet") || iface.name.starts_with("TenGigabitEthernet"))
                && iface.ip_address.is_none();
            if is_switchport {
                let mode = iface.switchport_mode.as_deref().unwrap_or("access");
                lines.push(format!(" switchport mode {}", mode));
                if let Some(vlan_id) = iface.vlan {
                    lines.push(format!(" switchport access vlan {}", vlan_id));
                }
            }
            if let Some((addr, mask)) = &iface.ip_address {
                lines.push(format!(" ip address {} {}", addr, mask));
            }
            // IPv6 addresses
            for v6addr in &iface.ipv6_addresses {
                match v6addr.addr_type {
                    Ipv6AddrType::LinkLocal => {
                        lines.push(format!(" ipv6 address {} link-local", v6addr.address));
                    }
                    Ipv6AddrType::Global => {
                        lines.push(format!(" ipv6 address {}/{}", v6addr.address, v6addr.prefix_len));
                    }
                }
            }
            if iface.ipv6_enabled && iface.ipv6_addresses.is_empty() {
                lines.push(" ipv6 enable".to_string());
            }
            // OSPFv3 interface config
            if let Some(ref ospf_cfg) = iface.ospfv3_config {
                if let Some(ref net_type) = ospf_cfg.network_type {
                    lines.push(format!(" ip ospf network {}", net_type));
                }
                lines.push(format!(" ipv6 ospf {} area {}", ospf_cfg.process_id, ospf_cfg.area_id));
            }
            if !iface.admin_up {
                lines.push(" shutdown".to_string());
            }
            // Real IOS does NOT show "no shutdown" for admin-up interfaces
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

        // IPv6 static routes
        for route in &self.ipv6_static_routes {
            if let Some(nh) = route.next_hop {
                lines.push(format!("ipv6 route {}/{} {}", route.prefix, route.prefix_len, nh));
            } else if let Some(ref iface) = route.interface {
                lines.push(format!("ipv6 route {}/{} {}", route.prefix, route.prefix_len, iface));
            }
        }

        if !self.ipv6_static_routes.is_empty() {
            lines.push("!".to_string());
        }

        // OSPFv3 processes
        for proc in &self.ospfv3_processes {
            lines.push(format!("ipv6 router ospf {}", proc.process_id));
            if let Some(rid) = proc.router_id {
                lines.push(format!(" router-id {}", rid));
            }
            lines.push("!".to_string());
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
                lines.push(line);
            }
        }
        if !self.access_lists.is_empty() {
            lines.push("!".to_string());
        }

        // Unmodeled config lines
        for line in &self.unmodeled_config {
            lines.push(line.clone());
        }

        if !self.unmodeled_config.is_empty() {
            lines.push("!".to_string());
        }

        // 7. After ip route section, before line con
        lines.push("ip forward-protocol nd".to_string());
        lines.push("ip http server".to_string());
        lines.push("ip http secure-server".to_string());
        lines.push("ip ssh version 2".to_string());
        lines.push("!".to_string());

        // Line configuration (real IOS 15.2 style)
        lines.push("!".to_string());
        lines.push("line con 0".to_string());
        lines.push(" privilege level 15".to_string());
        lines.push("line vty 0 4".to_string());
        lines.push(" exec-timeout 0 0".to_string());
        lines.push(" privilege level 15".to_string());
        lines.push(" length 0".to_string());
        lines.push(" transport input telnet ssh".to_string());
        lines.push("line vty 5 15".to_string());
        lines.push(" privilege level 15".to_string());
        lines.push(" transport input telnet ssh".to_string());
        lines.push("!".to_string());

        lines.push("end".to_string());

        lines
    }

    /// Generate a running-config string from the structured state.
    pub fn generate_running_config(&self) -> String {
        let body_lines = self.build_config_body();
        let body = body_lines.join("\n");
        let byte_count = body.len();

        format!(
            "Building configuration...\n\nCurrent configuration : {} bytes\n{}",
            byte_count, body
        )
    }

    /// Generate a startup-config string (same content as running config, different header).
    pub fn generate_startup_config(&self) -> String {
        let body_lines = self.build_config_body();
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

    /// Generate `show spanning-tree` output matching real IOS format.
    pub fn generate_show_spanning_tree(&self) -> String {
        let protocol = match self.spanning_tree_mode.as_str() {
            "rapid-pvst" => "rstp",
            "pvst"       => "ieee",
            other        => other,
        };

        let bridge_mac = mac_to_cisco_format(&self.base_mac);

        let mut blocks: Vec<String> = Vec::new();

        for vlan in &self.vlans {
            if !vlan.active || vlan.unsupported {
                continue;
            }

            let priority = 32768u32 + vlan.id as u32;

            // Collect interfaces that are: physical (not Vlan*), admin_up, link_up,
            // and assigned to this VLAN (access mode, not trunk).
            let mut iface_lines: Vec<String> = Vec::new();
            for iface in &self.interfaces {
                if iface.name.starts_with("Vlan") || iface.name.starts_with("Loopback") {
                    continue;
                }
                if !iface.admin_up || !iface.link_up {
                    continue;
                }
                // Dynamic VLAN membership: access ports in this VLAN
                let is_trunk = iface.switchport_mode.as_deref() == Some("trunk");
                let iface_vlan = iface.vlan.unwrap_or(1);
                if is_trunk || iface_vlan != vlan.id {
                    continue;
                }
                let short = short_interface_name(&iface.name);

                let port_num = extract_port_number(&iface.name);
                let cost = if iface.name.starts_with("TenGigabitEthernet") {
                    2u32
                } else {
                    4u32
                };
                let prio_nbr = format!("128.{}", port_num);

                iface_lines.push(format!(
                    "{:<20} {:<4} {:<3} {:<9} {:<8} {}",
                    short, "Desg", "FWD", cost, prio_nbr, "P2p"
                ));
            }

            let iface_section = if iface_lines.is_empty() {
                String::new()
            } else {
                format!(
                    "\n\nInterface           Role Sts Cost      Prio.Nbr Type\n\
                     ------------------- ---- --- --------- -------- --------------------------------\n\
                     {}",
                    iface_lines.join("\n")
                )
            };

            blocks.push(format!(
                "VLAN{vlan_id:04}\n\
                 \x20 Spanning tree enabled protocol {protocol}\n\
                 \x20 Root ID    Priority    {priority}\n\
                 \x20            Address     {mac}\n\
                 \x20            This bridge is the root\n\
                 \x20            Hello Time   2 sec  Max Age 20 sec  Forward Delay 15 sec\n\
                 \n\
                 \x20 Bridge ID  Priority    {priority}  (priority 32768 sys-id-ext {vlan_id})\n\
                 \x20            Address     {mac}\n\
                 \x20            Hello Time   2 sec  Max Age 20 sec  Forward Delay 15 sec\n\
                 \x20            Aging Time  300 sec\
                 {iface_section}",
                vlan_id       = vlan.id,
                protocol      = protocol,
                priority      = priority,
                mac           = bridge_mac,
                iface_section = iface_section,
            ));
        }

        blocks.join("\n\n")
    }

    /// Generate `show interfaces status` output matching real IOS format.
    pub fn generate_show_interfaces_status(&self) -> String {
        let header = "Port      Name               Status       Vlan       Duplex  Speed Type";
        let mut lines = vec![header.to_string()];

        for iface in &self.interfaces {
            // Skip Vlan (SVI) interfaces — they don't appear in this output
            if iface.name.starts_with("Vlan") {
                continue;
            }
            // Only physical interfaces (Gi / Te / Fa)
            if !iface.name.starts_with("GigabitEthernet")
                && !iface.name.starts_with("TenGigabitEthernet")
                && !iface.name.starts_with("FastEthernet")
            {
                continue;
            }

            let port = short_interface_name(&iface.name);

            let name_field: String = if iface.description.is_empty() {
                String::new()
            } else {
                iface.description.chars().take(18).collect()
            };

            let status = if !iface.admin_up {
                "disabled"
            } else if iface.link_up {
                "connected"
            } else {
                "notconnect"
            };

            let is_trunk = iface.switchport_mode.as_deref() == Some("trunk");
            let vlan_field: String = if is_trunk {
                "trunk".to_string()
            } else {
                iface.vlan.unwrap_or(1).to_string()
            };

            let is_connected = iface.admin_up && iface.link_up;
            let is_te = iface.name.starts_with("TenGigabitEthernet");

            // Duplex: use interface configured value for non-auto; show "auto" for unresolved Gi
            let duplex = if is_te {
                // Te ports always display their configured duplex (full)
                match iface.duplex.as_str() {
                    "full" => "full",
                    "half" => "half",
                    _ => "auto",
                }
            } else if is_connected {
                "a-full"
            } else {
                "auto"
            };
            // Speed: use interface configured value for Te; show "auto" for unresolved Gi
            let speed: String = if is_te {
                // Te ports always show their configured speed
                match iface.speed.as_str() {
                    "10000" => "10G".to_string(),
                    "1000" => "1G".to_string(),
                    s => s.to_string(),
                }
            } else if is_connected {
                "a-1000".to_string()
            } else {
                "auto".to_string()
            };
            let itype = if is_te { "Not Present" } else { "10/100/1000BaseTX" };

            lines.push(format!(
                "{:<10}{:<19}{:<13}{:<11}{:>6}  {:>5} {}",
                port, name_field, status, vlan_field, duplex, speed, itype
            ));
        }

        lines.join("\n")
    }

    /// Generate `show interfaces trunk` output.
    /// Real IOS shows four sections: Mode/Encapsulation/Status, VLANs allowed,
    /// VLANs active, and VLANs in STP forwarding.
    pub fn generate_show_interfaces_trunk(&self) -> String {
        let trunk_ifaces: Vec<&InterfaceState> = self.interfaces.iter()
            .filter(|i| i.switchport_mode.as_deref() == Some("trunk") && i.admin_up)
            .collect();

        if trunk_ifaces.is_empty() {
            return String::new();
        }

        let mut output = String::new();

        // Section 1: Port Mode Encapsulation Status Native vlan
        output.push_str("\nPort        Mode             Encapsulation  Status        Native vlan\n");
        for iface in &trunk_ifaces {
            let short = short_interface_name(&iface.name);
            let status = if iface.link_up { "trunking" } else { "not-trunking" };
            output.push_str(&format!(
                "{:<12}{:<17}{:<15}{:<14}{}\n",
                short, "on", "802.1q", status, iface.vlan.unwrap_or(1)
            ));
        }

        // Section 2: VLANs allowed on trunk
        output.push_str("\nPort        Vlans allowed on trunk\n");
        for iface in &trunk_ifaces {
            let short = short_interface_name(&iface.name);
            output.push_str(&format!("{:<12}1-4094\n", short));
        }

        // Section 3: VLANs allowed and active in management domain
        let active_vlans: Vec<String> = self.vlans.iter()
            .filter(|v| v.active && !v.unsupported)
            .map(|v| v.id.to_string())
            .collect();
        let active_str = Self::compress_vlan_ranges(&active_vlans);
        output.push_str("\nPort        Vlans allowed and active in management domain\n");
        for iface in &trunk_ifaces {
            let short = short_interface_name(&iface.name);
            output.push_str(&format!("{:<12}{}\n", short, active_str));
        }

        // Section 4: VLANs in spanning tree forwarding state and not pruned
        output.push_str("\nPort        Vlans in spanning tree forwarding state and not pruned\n");
        for iface in &trunk_ifaces {
            let short = short_interface_name(&iface.name);
            if iface.link_up {
                output.push_str(&format!("{:<12}{}\n", short, active_str));
            } else {
                output.push_str(&format!("{:<12}none\n", short));
            }
        }

        output
    }

    /// Generate `show interfaces switchport` output matching real IOS 15.2 format.
    ///
    /// Iterates all physical Ethernet interfaces (Gi, Te, Fa), skipping VLANs and
    /// Loopbacks.  For each port emits a block showing administrative/operational
    /// switchport mode, encapsulation, VLANs, etc.
    pub fn generate_show_interfaces_switchport(&self) -> String {
        let mut output = String::new();

        for iface in &self.interfaces {
            // Only physical Ethernet interfaces
            if iface.name.starts_with("Vlan") || iface.name.starts_with("Loopback") {
                continue;
            }
            if !iface.name.starts_with("GigabitEthernet")
                && !iface.name.starts_with("TenGigabitEthernet")
                && !iface.name.starts_with("FastEthernet")
            {
                continue;
            }

            let short = short_interface_name(&iface.name);

            let is_trunk = iface.switchport_mode.as_deref() == Some("trunk");

            // Operational mode: trunk when configured as trunk AND link is up,
            // otherwise "down" (port not connected / negotiating).
            let oper_mode = if is_trunk && iface.link_up {
                "trunk"
            } else {
                "down"
            };

            let admin_mode = if is_trunk {
                "trunk"
            } else {
                "dynamic auto"
            };

            let access_vlan = iface.vlan.unwrap_or(1);
            let native_vlan = iface.vlan.unwrap_or(1);

            // Look up VLAN name for the access VLAN.
            let access_vlan_name: String = if access_vlan == 1 {
                "default".to_string()
            } else {
                self.vlans.iter()
                    .find(|v| v.id == access_vlan)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| format!("VLAN{:04}", access_vlan))
            };

            let native_vlan_name: String = if native_vlan == 1 {
                "default".to_string()
            } else {
                self.vlans.iter()
                    .find(|v| v.id == native_vlan)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| format!("VLAN{:04}", native_vlan))
            };

            output.push_str(&format!(
                "Name: {short}\n\
Switchport: Enabled\n\
Administrative Mode: {admin_mode}\n\
Operational Mode: {oper_mode}\n\
Administrative Trunking Encapsulation: dot1q\n\
Negotiation of Trunking: On\n\
Access Mode VLAN: {access_vlan} ({access_vlan_name})\n\
Trunking Native Mode VLAN: {native_vlan} ({native_vlan_name})\n\
Administrative Native VLAN tagging: disabled\n\
Voice VLAN: none\n\
Administrative private-vlan host-association: none\n\
Administrative private-vlan mapping: none\n\
Administrative private-vlan trunk native VLAN: none\n\
Administrative private-vlan trunk Native VLAN tagging: enabled\n\
Administrative private-vlan trunk encapsulation: dot1q\n\
Administrative private-vlan trunk normal VLANs: none\n\
Administrative private-vlan trunk associations: none\n\
Administrative private-vlan trunk mappings: none\n\
Operational private-vlan: none\n\
Trunking VLANs Enabled: ALL\n\
Pruning VLANs Enabled: 2-1001\n\
Capture Mode Disabled\n\
Capture VLANs Allowed: ALL\n\
\n\
Protected: false\n\
Unknown unicast blocked: disabled\n\
Unknown multicast blocked: disabled\n\
Appliance trust: none\n",
                short = short,
                admin_mode = admin_mode,
                oper_mode = oper_mode,
                access_vlan = access_vlan,
                access_vlan_name = access_vlan_name,
                native_vlan = native_vlan,
                native_vlan_name = native_vlan_name,
            ));
        }

        output
    }

    /// Compress a list of VLAN ID strings into range notation (e.g., "1-15,100-101,127")
    fn compress_vlan_ranges(vlan_strs: &[String]) -> String {
        let mut ids: Vec<u16> = vlan_strs.iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        ids.sort();
        ids.dedup();

        if ids.is_empty() {
            return "none".to_string();
        }

        let mut ranges: Vec<String> = Vec::new();
        let mut start = ids[0];
        let mut end = ids[0];

        for &id in &ids[1..] {
            if id == end + 1 {
                end = id;
            } else {
                if start == end {
                    ranges.push(start.to_string());
                } else {
                    ranges.push(format!("{}-{}", start, end));
                }
                start = id;
                end = id;
            }
        }
        if start == end {
            ranges.push(start.to_string());
        } else {
            ranges.push(format!("{}-{}", start, end));
        }

        ranges.join(",")
    }

    /// Generate `show interfaces counters` output matching real IOS format.
    ///
    /// Produces two tables: input counters and output counters.
    /// Skips Vlan (SVI) interfaces — only physical ports are shown.
    pub fn generate_show_interfaces_counters(&self) -> String {
        let mut lines = Vec::new();

        // Input counters table
        lines.push(format!(
            "{:<16}{:>14}{:>14}{:>14}{:>14}",
            "Port", "InOctets", "InUcastPkts", "InMcastPkts", "InBcastPkts"
        ));
        for iface in &self.interfaces {
            if iface.name.starts_with("Vlan") {
                continue;
            }
            let port = short_interface_name(&iface.name);
            lines.push(format!(
                "{:<16}{:>14}{:>14}{:>14}{:>14}",
                port,
                iface.input_bytes,
                iface.input_packets,
                0u64,
                0u64,
            ));
        }

        lines.push(String::new());

        // Output counters table
        lines.push(format!(
            "{:<16}{:>14}{:>14}{:>14}{:>14}",
            "Port", "OutOctets", "OutUcastPkts", "OutMcastPkts", "OutBcastPkts"
        ));
        for iface in &self.interfaces {
            if iface.name.starts_with("Vlan") {
                continue;
            }
            let port = short_interface_name(&iface.name);
            lines.push(format!(
                "{:<16}{:>14}{:>14}{:>14}{:>14}",
                port,
                iface.output_bytes,
                iface.output_packets,
                0u64,
                0u64,
            ));
        }

        lines.join("\n")
    }

    /// Generate `show interfaces description` output matching real IOS format.
    ///
    /// Format:
    /// ```text
    /// Interface                      Status         Protocol Description
    /// Vl1                            up             up       Tool Provisioning VLAN
    /// Gi1/0/1                        down           down
    /// Gi1/0/5                        admin down     down     DISABLED-PORT
    /// ```
    pub fn generate_show_interfaces_description(&self) -> String {
        let header = "Interface                      Status         Protocol Description";
        let mut lines = vec![header.to_string()];

        for iface in &self.interfaces {
            let short = short_interface_name(&iface.name);

            let status = if !iface.admin_up {
                "admin down".to_string()
            } else if iface.link_up {
                "up".to_string()
            } else {
                "down".to_string()
            };

            let protocol = if iface.admin_up && iface.link_up {
                "up"
            } else {
                "down"
            };

            let desc = &iface.description;

            lines.push(format!(
                "{:<31}{:<15}{:<9}{}",
                short, status, protocol, desc
            ));
        }

        lines.join("\n")
    }

    /// Generate `show arp` output with self-entries for each interface that has an IP address.
    pub fn generate_show_arp(&self) -> String {
        let header = "Protocol  Address          Age (min)  Hardware Addr   Type   Interface";
        let mut lines = vec![header.to_string()];

        for iface in &self.interfaces {
            if let Some((addr, _mask)) = iface.ip_address {
                let ip = addr.to_string();
                let mac = &iface.mac_address;
                let name = &iface.name;
                lines.push(format!(
                    "Internet  {:<16}{:>8}   {}  ARPA   {}",
                    ip, "-", mac, name
                ));
            }
        }

        lines.join("\n")
    }

    pub fn generate_show_mac_address_table(&self) -> String {
        let header = "          Mac Address Table\n\
            -------------------------------------------\n\
            \n\
            Vlan    Mac Address       Type        Ports\n\
            ----    -----------       --------    -----";

        let mut entries: Vec<String> = Vec::new();

        for iface in &self.interfaces {
            if !iface.admin_up {
                continue;
            }

            let (vlan, port_name) = if iface.name.starts_with("Vlan") {
                // SVI: extract VLAN number from name like "Vlan1"
                let vlan_num: u16 = iface.name
                    .strip_prefix("Vlan")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                let short = short_interface_name(&iface.name);
                (vlan_num, short)
            } else {
                // Physical interface: use access VLAN (default 1)
                let vlan_num = iface.vlan.unwrap_or(1);
                let short = short_interface_name(&iface.name);
                (vlan_num, short)
            };

            let mac = &iface.mac_address;
            entries.push(format!(
                "{:>4}    {}    {:<12}{}",
                vlan, mac, "STATIC", port_name
            ));
        }

        let count = entries.len();
        let mut output = header.to_string();
        if !entries.is_empty() {
            output.push('\n');
            output.push_str(&entries.join("\n"));
        }
        output.push('\n');
        output.push_str(&format!("Total Mac Addresses for this criterion: {}", count));
        output
    }

    // ─── IPv6 Show Commands ──────────────────────────────────────────────────

    /// Generate `show ipv6 interface brief` output matching real IOS format.
    pub fn generate_show_ipv6_interface_brief(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for iface in &self.interfaces {
            let (status, protocol) = if !iface.admin_up {
                ("administratively down", "down")
            } else if iface.link_up {
                ("up", "up")
            } else {
                ("down", "down")
            };

            // Interface name — abbreviated if > 23 chars (same as show ip interface brief)
            let iface_display = abbreviate_interface_name(&iface.name);
            lines.push(format!("{:<23}[{}/{}]", iface_display, status, protocol));

            // IPv6 addresses: link-local first, then globals, or "unassigned"
            if let Some(ll) = iface.ipv6_link_local() {
                lines.push(format!("    {}", ll));
                for global in iface.ipv6_global_addrs() {
                    // Format: address without prefix for brief display
                    lines.push(format!("    {}", global.address));
                }
            } else {
                lines.push("    unassigned".to_string());
            }
        }
        lines.join("\n")
    }

    /// Compute the IPv6 routing table from interface addresses and static routes.
    pub fn compute_ipv6_routes(&self) -> Vec<Ipv6Route> {
        let mut routes: Vec<Ipv6Route> = Vec::new();

        for iface in &self.interfaces {
            if !iface.admin_up || !iface.link_up {
                continue;
            }

            let is_loopback = iface.name.starts_with("Loopback");

            for addr_cfg in &iface.ipv6_addresses {
                if addr_cfg.addr_type == Ipv6AddrType::LinkLocal {
                    continue; // link-locals don't go in routing table
                }

                let prefix = ipv6_apply_prefix(addr_cfg.address, addr_cfg.prefix_len);

                if is_loopback && addr_cfg.prefix_len == 128 {
                    // Loopback /128: LC (Local Connected)
                    routes.push(Ipv6Route {
                        prefix: addr_cfg.address,
                        prefix_len: 128,
                        route_type: Ipv6RouteType::LocalConnected,
                        admin_distance: 0,
                        metric: 0,
                        next_hop: None,
                        interface: Some(iface.name.clone()),
                    });
                } else {
                    // Connected route (network)
                    routes.push(Ipv6Route {
                        prefix,
                        prefix_len: addr_cfg.prefix_len,
                        route_type: Ipv6RouteType::Connected,
                        admin_distance: 0,
                        metric: 0,
                        next_hop: None,
                        interface: Some(iface.name.clone()),
                    });

                    // Local route (host /128)
                    routes.push(Ipv6Route {
                        prefix: addr_cfg.address,
                        prefix_len: 128,
                        route_type: Ipv6RouteType::Local,
                        admin_distance: 0,
                        metric: 0,
                        next_hop: None,
                        interface: Some(iface.name.clone()),
                    });
                }
            }
        }

        // Static routes
        for sr in &self.ipv6_static_routes {
            routes.push(Ipv6Route {
                prefix: sr.prefix,
                prefix_len: sr.prefix_len,
                route_type: Ipv6RouteType::Static,
                admin_distance: sr.admin_distance as u16,
                metric: 0,
                next_hop: sr.next_hop,
                interface: sr.interface.clone(),
            });
        }

        // Multicast catch-all: FF00::/8 via Null0
        routes.push(Ipv6Route {
            prefix: "ff00::".parse().unwrap(),
            prefix_len: 8,
            route_type: Ipv6RouteType::Local,
            admin_distance: 0,
            metric: 0,
            next_hop: None,
            interface: Some("Null0".to_string()),
        });

        routes
    }

    /// Generate `show ipv6 route` output matching real IOS format.
    pub fn generate_show_ipv6_route(&self) -> String {
        let routes = self.compute_ipv6_routes();
        let entry_count = routes.len();

        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("IPv6 Routing Table - default - {} entries", entry_count));
        lines.push("Codes: C - Connected, L - Local, S - Static, U - Per-user Static route".to_string());
        lines.push("       B - BGP, R - RIP, I1 - ISIS L1, I2 - ISIS L2".to_string());
        lines.push("       IA - ISIS interarea, IS - ISIS summary, D - EIGRP, EX - EIGRP external".to_string());
        lines.push("       ND - ND Default, NDp - ND Prefix, DCE - Destination, NDr - Redirect".to_string());
        lines.push("       O - OSPF Intra, OI - OSPF Inter, OE1 - OSPF ext 1, OE2 - OSPF ext 2".to_string());
        lines.push("       ON1 - OSPF NSSA ext 1, ON2 - OSPF NSSA ext 2, a - Application".to_string());

        for route in &routes {
            let code = route.route_type.code();
            let prefix_str = format!("{}/{}", route.prefix, route.prefix_len);
            lines.push(format!("{:<4}{} [{}/{}]",
                code, prefix_str, route.admin_distance, route.metric));

            // Next-hop line
            if let Some(nh) = route.next_hop {
                if let Some(ref iface) = route.interface {
                    lines.push(format!("     via {}, {}", nh, iface));
                } else {
                    lines.push(format!("     via {}", nh));
                }
            } else if let Some(ref iface) = route.interface {
                let via_type = match route.route_type {
                    Ipv6RouteType::Connected => "directly connected",
                    Ipv6RouteType::Local | Ipv6RouteType::LocalConnected => "receive",
                    _ => "directly connected",
                };
                lines.push(format!("     via {}, {}", iface, via_type));
            }
        }

        lines.join("\n")
    }

    /// Generate `show ipv6 ospf` output matching real IOS format.
    pub fn generate_show_ipv6_ospf(&self) -> String {
        let mut lines: Vec<String> = Vec::new();

        for proc in &self.ospfv3_processes {
            let rid = proc.router_id
                .map(|r| r.to_string())
                .unwrap_or_else(|| "0.0.0.0".to_string());

            lines.push(format!(" Routing Process \"ospfv3 {}\" with ID {}", proc.process_id, rid));
            lines.push(" Supports NSSA (compatible with RFC 3101)".to_string());
            lines.push(" Does not support Database Exchange Summary List Optimization (RFC 5243)".to_string());
            lines.push(" Event-log enabled, Maximum number of events: 1000, Mode: cyclic".to_string());
            lines.push(" Router is not originating router-LSAs with maximum metric".to_string());
            lines.push(format!(" Initial SPF schedule delay {} msecs", proc.spf_delay));
            lines.push(format!(" Minimum hold time between two consecutive SPFs {} msecs", proc.spf_hold));
            lines.push(format!(" Maximum wait time between two consecutive SPFs {} msecs", proc.spf_max_wait));
            lines.push(" Minimum LSA interval 5 secs".to_string());
            lines.push(" Minimum LSA arrival 1000 msecs".to_string());
            lines.push(" LSA group pacing timer 240 secs".to_string());
            lines.push(" Interface flood pacing timer 33 msecs".to_string());
            lines.push(" Retransmission pacing timer 66 msecs".to_string());
            lines.push(" Retransmission limit dc 24 non-dc 24".to_string());
            lines.push(" Number of external LSA 0. Checksum Sum 0x000000".to_string());

            let normal_count = proc.areas.iter().filter(|a| a.area_type == OspfV3AreaType::Normal).count();
            let stub_count = proc.areas.iter().filter(|a| a.area_type == OspfV3AreaType::Stub).count();
            let nssa_count = proc.areas.iter().filter(|a| a.area_type == OspfV3AreaType::Nssa).count();
            lines.push(format!(" Number of areas in this router is {}. {} normal {} stub {} nssa",
                proc.areas.len(), normal_count, stub_count, nssa_count));
            lines.push(" Graceful restart helper support enabled".to_string());
            lines.push(format!(" Reference bandwidth unit is {} mbps", proc.reference_bandwidth));
            lines.push(" RFC1583 compatibility enabled".to_string());

            for area in &proc.areas {
                let area_name = if area.area_id == 0 {
                    "BACKBONE(0)".to_string()
                } else {
                    format!("{}", area.area_id)
                };

                // Count interfaces in this area
                let iface_count = self.interfaces.iter()
                    .filter(|i| i.ospfv3_config.as_ref()
                        .map(|c| c.process_id == proc.process_id && c.area_id == area.area_id)
                        .unwrap_or(false))
                    .count();

                let active = if iface_count > 0 && self.interfaces.iter()
                    .any(|i| i.ospfv3_config.as_ref()
                        .map(|c| c.process_id == proc.process_id && c.area_id == area.area_id)
                        .unwrap_or(false) && i.admin_up && i.link_up)
                {
                    ""
                } else {
                    " (Inactive)"
                };

                lines.push(format!("    Area {}{}", area_name, active));
                lines.push(format!("        Number of interfaces in this area is {}", iface_count));
                lines.push(format!("        SPF algorithm executed {} times", area.spf_executions));
                lines.push(format!("        Number of LSA {}. Checksum Sum 0x{:06X}", area.lsa_count, area.lsa_checksum));
                lines.push("        Number of DCbitless LSA 0".to_string());
                lines.push("        Number of indication LSA 0".to_string());
                lines.push("        Number of DoNotAge LSA 0".to_string());
                lines.push("        Flood list length 0".to_string());
            }
        }

        if lines.is_empty() {
            return String::new();
        }

        lines.join("\n")
    }

    /// Generate `show ipv6 ospf interface brief` output.
    pub fn generate_show_ipv6_ospf_interface_brief(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push("Interface    PID   Area            Intf ID    Cost  State Nbrs F/C".to_string());

        let mut intf_id = 1u32;
        for iface in &self.interfaces {
            if let Some(ref ospf_cfg) = iface.ospfv3_config {
                let short_name = short_interface_name(&iface.name);

                let cost = ospf_cfg.cost.unwrap_or_else(|| {
                    // Auto-calculate: reference_bandwidth / interface_bandwidth
                    let ref_bw = self.ospfv3_processes.iter()
                        .find(|p| p.process_id == ospf_cfg.process_id)
                        .map(|p| p.reference_bandwidth)
                        .unwrap_or(100);
                    let iface_bw_mbps = match iface.speed.as_str() {
                        "10" => 10u32,
                        "100" => 100,
                        "1000" => 1000,
                        "10000" => 10000,
                        _ => 1000, // default 1G for auto
                    };
                    std::cmp::max(1, ref_bw / iface_bw_mbps)
                });

                let state = if iface.name.starts_with("Loopback") {
                    "LOOP"
                } else if !iface.admin_up || !iface.link_up {
                    "DOWN"
                } else {
                    "DR" // simplified — real IOS shows DR/BDR/DROTHER/P2P
                };

                lines.push(format!("{:<13}{:<6}{:<16}{:<11}{:<6}{:<6}0/0",
                    short_name, ospf_cfg.process_id, ospf_cfg.area_id,
                    intf_id, cost, state));
                intf_id += 1;
            }
        }

        lines.join("\n")
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
        // Physical interfaces default to link_up=false (no cable connected)
        assert!(!iface.link_up);
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
        // Gi1/0/1 (first connected port) is admin up
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
        // Verify that shutdown interfaces appear with a shutdown line in running-config.
        let state = DeviceState::new("Switch1");
        let config = state.generate_running_config();
        // Gi1/0/5 is shutdown by default, so running-config should contain " shutdown"
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

    #[test]
    fn test_show_vlan_brief_port_wrapping() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_vlan_brief();
        // VLAN 1 has 18 ports - should wrap across multiple lines
        let lines: Vec<&str> = output.lines().collect();
        // Find lines for VLAN 1 (after header, before VLAN 1002)
        let vlan1_start = lines.iter().position(|l| l.starts_with("1 ") || l.starts_with("1    ")).unwrap();
        let vlan1002_start = lines.iter().position(|l| l.starts_with("1002")).unwrap();
        let vlan1_lines = &lines[vlan1_start..vlan1002_start];
        assert!(vlan1_lines.len() > 1,
            "VLAN 1 with 18 ports should wrap, got {} lines: {:?}", vlan1_lines.len(), vlan1_lines);
        // Continuation lines should be indented to column 48
        for line in &vlan1_lines[1..] {
            assert!(line.starts_with(&" ".repeat(48)),
                "Continuation should be indented 48 spaces: {:?}", line);
        }
        // Real IOS wraps at ~3 Gi ports per line (31 char port column width)
        // First data line should have 3 ports, not 6
        let first_port_section = vlan1_lines[0].split("active").last().unwrap().trim();
        let port_count = first_port_section.split(',').count();
        assert!(port_count <= 4,
            "First line should have ~3 ports (real IOS wraps at 31 chars), got {}: {:?}",
            port_count, first_port_section);
    }

    /// Real IOS excludes trunk ports from show vlan brief port lists.
    #[test]
    fn test_show_vlan_brief_excludes_trunk_ports() {
        let mut state = DeviceState::new("Switch1");
        // Set Gi1/0/1 to trunk mode
        if let Some(iface) = state.get_interface_mut("GigabitEthernet1/0/1") {
            iface.switchport_mode = Some("trunk".to_string());
        }
        let output = state.generate_show_vlan_brief();
        // Gi1/0/1 should NOT appear in the VLAN 1 port list
        let vlan1_line = output.lines().find(|l| l.starts_with("1 ") || l.starts_with("1    ")).unwrap();
        assert!(!vlan1_line.contains("Gi1/0/1"),
            "Trunk port Gi1/0/1 should not appear in VLAN port list, got: {:?}", vlan1_line);
        // But Gi1/0/2 (still access) should appear
        assert!(output.contains("Gi1/0/2"),
            "Access port Gi1/0/2 should still appear in VLAN port list");
    }

    #[test]
    fn test_abbreviate_interface_name() {
        // TenGigabitEthernet1/0/1 = 25 chars > 23, must abbreviate to Te
        assert_eq!(abbreviate_interface_name("TenGigabitEthernet1/0/1"), "Te1/0/1");
        // GigabitEthernet1/0/1 = 22 chars <= 23, keep as-is
        assert_eq!(abbreviate_interface_name("GigabitEthernet1/0/1"), "GigabitEthernet1/0/1");
        // GigabitEthernet1/0/10 = 21 chars < 23, keep as-is
        assert_eq!(abbreviate_interface_name("GigabitEthernet1/0/10"), "GigabitEthernet1/0/10");
        // FastEthernet0/1 = 16 chars <= 23, keep as-is
        assert_eq!(abbreviate_interface_name("FastEthernet0/1"), "FastEthernet0/1");
        // Loopback0 = 9 chars <= 23, keep as-is
        assert_eq!(abbreviate_interface_name("Loopback0"), "Loopback0");
        // Vlan1 = 5 chars <= 23, keep as-is
        assert_eq!(abbreviate_interface_name("Vlan1"), "Vlan1");
        // Short names are returned as-is
        assert_eq!(abbreviate_interface_name("Gi1/0/1"), "Gi1/0/1");
    }

    #[test]
    fn test_default_state_shutdown_interfaces() {
        let state = DeviceState::new("Switch1");
        // Gi1/0/1 through 1/0/4 should be admin up but link down (no cable)
        let gi1 = state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/1").unwrap();
        assert!(gi1.admin_up, "Gi1/0/1 should be admin up");
        assert!(!gi1.link_up, "Gi1/0/1 should be link down (no cable by default)");
        let gi4 = state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/4").unwrap();
        assert!(gi4.admin_up, "Gi1/0/4 should be admin up");
        assert!(!gi4.link_up, "Gi1/0/4 should be link down (no cable by default)");
        // Gi1/0/5 through 1/0/12 should be shutdown
        let gi5 = state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/5").unwrap();
        assert!(!gi5.admin_up, "Gi1/0/5 should be admin down (shutdown)");
        assert!(!gi5.link_up, "Gi1/0/5 should be link down");
        let gi12 = state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/12").unwrap();
        assert!(!gi12.admin_up, "Gi1/0/12 should be admin down (shutdown)");
        // Gi1/0/13 through 1/0/16 should be shutdown
        let gi13 = state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/13").unwrap();
        assert!(!gi13.admin_up, "Gi1/0/13 should be admin down (shutdown)");
        let gi16 = state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/16").unwrap();
        assert!(!gi16.admin_up, "Gi1/0/16 should be admin down (shutdown)");
        // Te1/0/1 and Te1/0/2 should be admin up (no SFP → notconnect, not disabled)
        let te1 = state.interfaces.iter().find(|i| i.name == "TenGigabitEthernet1/0/1").unwrap();
        assert!(te1.admin_up, "Te1/0/1 should be admin up (notconnect, no SFP)");
        assert!(!te1.link_up, "Te1/0/1 should be link down (no SFP)");
        let te2 = state.interfaces.iter().find(|i| i.name == "TenGigabitEthernet1/0/2").unwrap();
        assert!(te2.admin_up, "Te1/0/2 should be admin up (notconnect, no SFP)");
        assert!(!te2.link_up, "Te1/0/2 should be link down (no SFP)");
        // Vlan1 should be admin up
        let vlan1 = state.interfaces.iter().find(|i| i.name == "Vlan1").unwrap();
        assert!(vlan1.admin_up, "Vlan1 should be admin up");
        assert!(vlan1.link_up, "Vlan1 should be link up");
    }

    #[test]
    fn test_default_serial_and_mac() {
        let state = DeviceState::new("Switch1");
        assert_eq!(state.serial_number, "FOC2231X1YZ",
            "serial_number should be FOC2231X1YZ, got: {:?}", state.serial_number);
        assert_eq!(state.base_mac, "00:A3:D1:4F:22:80",
            "base_mac should be 00:A3:D1:4F:22:80, got: {:?}", state.base_mac);
    }

    #[test]
    fn test_running_config_enriched() {
        let state = DeviceState::new("Switch1");
        let config = state.generate_running_config();
        assert!(config.contains("no service pad"));
        assert!(config.contains("service unsupported-transceiver"));
        assert!(config.contains("aaa authentication login default local"));
        assert!(config.contains("switch 1 provision ws-c3560cx-12pd-s"));
        assert!(config.contains("system mtu routing 1500"));
        assert!(!config.contains("no ip source-route"), "no ip source-route should not appear");
        assert!(config.contains("ip forward-protocol nd"));
        assert!(config.contains("lldp run"));
        assert!(config.contains("ip http server"));
        assert!(config.contains("ip ssh version 2"));
    }

    #[test]
    fn test_running_config_line_section() {
        let state = DeviceState::new("Switch1");
        let config = state.generate_running_config();
        // line con 0 should have privilege level 15 (no stopbits)
        assert!(config.contains("line con 0\n privilege level 15"), "line con 0 should have privilege level 15");
        assert!(!config.contains("stopbits"), "stopbits should not appear in line section");
        // line vty 0 4 should have exec-timeout, privilege level, length, and transport input telnet ssh
        assert!(config.contains("line vty 0 4\n exec-timeout 0 0\n privilege level 15\n length 0\n transport input telnet ssh"),
            "line vty 0 4 should have correct real IOS format");
        // line vty 5 15 should have privilege level and transport input telnet ssh (no exec-timeout)
        assert!(config.contains("line vty 5 15\n privilege level 15\n transport input telnet ssh"),
            "line vty 5 15 should have correct real IOS format");
        // login local should not appear in line sections
        assert!(!config.contains(" login local"), "login local should not appear in line sections");
    }

    #[test]
    fn test_running_config_enable_secret() {
        let mut state = DeviceState::new("Switch1");
        state.enable_secret = Some("cisco123".to_string());
        let config = state.generate_running_config();
        assert!(config.contains("enable secret 9"), "Should show enable secret hash");
    }

    #[test]
    fn test_mac_to_cisco_format() {
        assert_eq!(mac_to_cisco_format("00:A3:D1:4F:22:80"), "00a3.d14f.2280");
        assert_eq!(mac_to_cisco_format("18:8B:45:17:F7:80"), "188b.4517.f780");
    }

    #[test]
    fn test_generate_show_spanning_tree() {
        let mut state = DeviceState::new("Switch1");
        // Simulate a cable connected to Gi1/0/1
        if let Some(iface) = state.get_interface_mut("GigabitEthernet1/0/1") {
            iface.link_up = true;
        }
        let output = state.generate_show_spanning_tree();
        assert!(output.contains("VLAN0001"), "Should show VLAN 1");
        assert!(output.contains("protocol rstp"), "Should show rstp protocol");
        assert!(output.contains("32769"), "Priority should be 32768 + 1 = 32769");
        assert!(output.contains("00a3.d14f.2280"), "Should show bridge MAC in Cisco format");
        assert!(output.contains("This bridge is the root"));
        assert!(output.contains("Gi1/0/1"), "Should show connected interfaces");
        assert!(!output.contains("Gi1/0/5"), "Should not show shutdown interfaces");
        assert!(!output.contains("VLAN1002"), "Should not show unsupported VLANs");
    }

    #[test]
    fn test_short_interface_name() {
        assert_eq!(short_interface_name("GigabitEthernet1/0/1"), "Gi1/0/1");
        assert_eq!(short_interface_name("TenGigabitEthernet1/0/1"), "Te1/0/1");
        assert_eq!(short_interface_name("Vlan1"), "Vl1");
        assert_eq!(short_interface_name("Loopback0"), "Lo0");
    }

    #[test]
    fn test_generate_show_arp() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_arp();
        assert!(output.contains("Protocol"), "Should have header");
        // Vlan1 has IP 10.0.0.1, should appear as self-entry
        assert!(output.contains("10.0.0.1"), "Should show Vlan1 IP");
        assert!(output.contains("ARPA"), "Should show ARPA type");
        assert!(output.contains("Vlan1"), "Should show interface name");
        // Self entries use "-" for age
        let vlan1_line = output.lines().find(|l| l.contains("10.0.0.1")).unwrap();
        assert!(
            vlan1_line.contains("  -  ") || vlan1_line.contains("  -   "),
            "Self entry should have '-' for age: {:?}", vlan1_line
        );
        // The line should have "Internet" protocol
        assert!(vlan1_line.starts_with("Internet"), "ARP entry should start with 'Internet': {:?}", vlan1_line);
    }

    #[test]
    fn test_generate_show_interfaces_status() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_status();
        // Should have header
        assert!(output.contains("Port"), "Output should contain 'Port' header: {:?}", &output[..output.len().min(200)]);
        assert!(output.contains("Status"), "Output should contain 'Status' header");
        // Gi1/0/1 is admin_up + link_down → "notconnect" (no cable by default)
        let gi1_line = output.lines().find(|l| l.starts_with("Gi1/0/1")).unwrap();
        assert!(gi1_line.contains("notconnect"), "Gi1/0/1 should be notconnect (admin up, no link): {:?}", gi1_line);
        // Gi1/0/5 is admin_up=false → "disabled"
        let gi5_line = output.lines().find(|l| l.starts_with("Gi1/0/5")).unwrap();
        assert!(gi5_line.contains("disabled"), "Gi1/0/5 should be disabled: {:?}", gi5_line);
        // Vlan1 should NOT appear
        assert!(!output.contains("Vl1"), "Vlan interfaces should not appear");
    }

    #[test]
    fn test_show_interfaces_status_alignment() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_status();

        // Header must match real IOS exactly
        let header = output.lines().next().unwrap();
        assert_eq!(
            header,
            "Port      Name               Status       Vlan       Duplex  Speed Type",
            "Header alignment mismatch"
        );

        // Gi1/0/1 is connected, vlan 1 — check Vlan column is left-aligned in 11-char field
        // Expected: "Gi1/0/1   " (10) + "                   " (19) + "connected    " (13) + "1          " (11) + ...
        let gi1_line = output.lines().find(|l| l.starts_with("Gi1/0/1")).unwrap();
        // Column offsets: Port(0-9), Name(10-28), Status(29-41), Vlan(42-52)
        assert_eq!(&gi1_line[42..53], "1          ",
            "Vlan '1' should be left-justified in 11-char field: {:?}", gi1_line);

        // Gi1/0/5 is disabled, vlan 1 — same Vlan column check
        let gi5_line = output.lines().find(|l| l.starts_with("Gi1/0/5")).unwrap();
        assert_eq!(&gi5_line[42..53], "1          ",
            "Vlan '1' on disabled port should be left-justified in 11-char field: {:?}", gi5_line);

        // Duplex column at offset 53, right-aligned in 6-char field: "  auto"
        assert_eq!(&gi5_line[53..59], "  auto",
            "Duplex should be right-aligned in 6-char field: {:?}", gi5_line);

        // Two literal spaces at 59..61, then Speed right-aligned in 5-char field at 61..66
        assert_eq!(&gi5_line[59..61], "  ",
            "Two spaces between Duplex and Speed: {:?}", gi5_line);
        assert_eq!(&gi5_line[61..66], " auto",
            "Speed should be right-aligned in 5-char field: {:?}", gi5_line);
    }

    #[test]
    fn test_generate_show_interfaces_description_header() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_description();
        let header = output.lines().next().unwrap();
        assert_eq!(
            header,
            "Interface                      Status         Protocol Description",
            "Header must match real IOS format exactly"
        );
    }

    #[test]
    fn test_generate_show_interfaces_description_all_interfaces_present() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_description();
        // All interfaces should be listed (VLANs, physical, loopbacks)
        assert!(output.contains("Vl1"), "Vlan1 SVI should appear as Vl1");
        assert!(output.contains("Gi1/0/1"), "GigabitEthernet should appear as Gi");
        assert!(output.contains("Te1/0/1"), "TenGigabitEthernet should appear as Te");
    }

    #[test]
    fn test_generate_show_interfaces_description_status_up() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_description();
        // Vlan1 is admin_up=true, link_up=true → "up" / "up"
        let vl1_line = output.lines().find(|l| l.starts_with("Vl1")).unwrap();
        // Status column at offset 31, 15 chars wide
        assert_eq!(&vl1_line[31..46], "up             ",
            "Vlan1 status should be 'up': {:?}", vl1_line);
        // Protocol column at offset 46, 9 chars wide
        assert_eq!(&vl1_line[46..55], "up       ",
            "Vlan1 protocol should be 'up': {:?}", vl1_line);
    }

    #[test]
    fn test_generate_show_interfaces_description_status_down() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_description();
        // Gi1/0/1 is admin_up=true, link_up=false → "down" / "down"
        let gi1_line = output.lines().find(|l| l.starts_with("Gi1/0/1")).unwrap();
        assert_eq!(&gi1_line[31..46], "down           ",
            "Gi1/0/1 status should be 'down': {:?}", gi1_line);
        assert_eq!(&gi1_line[46..55], "down     ",
            "Gi1/0/1 protocol should be 'down': {:?}", gi1_line);
    }

    #[test]
    fn test_generate_show_interfaces_description_admin_down() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_description();
        // Gi1/0/5 is admin_up=false → "admin down" / "down"
        let gi5_line = output.lines().find(|l| l.starts_with("Gi1/0/5")).unwrap();
        assert_eq!(&gi5_line[31..46], "admin down     ",
            "Gi1/0/5 status should be 'admin down': {:?}", gi5_line);
        assert_eq!(&gi5_line[46..55], "down     ",
            "Gi1/0/5 protocol should be 'down': {:?}", gi5_line);
    }

    #[test]
    fn test_generate_show_interfaces_description_with_description() {
        let mut state = DeviceState::new("Switch1");
        if let Some(iface) = state.get_interface_mut("GigabitEthernet1/0/1") {
            iface.description = "uplink to core".to_string();
        }
        let output = state.generate_show_interfaces_description();
        let gi1_line = output.lines().find(|l| l.starts_with("Gi1/0/1")).unwrap();
        assert!(gi1_line.ends_with("uplink to core"),
            "Description should appear at end of line: {:?}", gi1_line);
    }

    #[test]
    fn test_generate_show_interfaces_description_empty_description() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_description();
        // Gi1/0/1 has no description — line should end after protocol column
        let gi1_line = output.lines().find(|l| l.starts_with("Gi1/0/1")).unwrap();
        // After the 55-char prefix (31+15+9), there should be nothing extra
        assert_eq!(gi1_line.len(), 55,
            "Line with empty description should be exactly 55 chars: {:?}", gi1_line);
    }

    #[test]
    fn test_generate_show_interfaces_description_interface_col_width() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_description();
        // Interface column is 31 chars wide
        // "Vl1" is 3 chars, should be padded to 31
        let vl1_line = output.lines().find(|l| l.starts_with("Vl1")).unwrap();
        assert_eq!(&vl1_line[0..31], "Vl1                            ",
            "Interface column should be 31 chars: {:?}", vl1_line);
    }

    #[test]
    fn test_generate_show_mac_address_table() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_mac_address_table();
        assert!(output.contains("Mac Address Table"), "Should have header");
        assert!(output.contains("Vlan    Mac Address"), "Should have column headers");
        assert!(output.contains("Total Mac Addresses"), "Should have footer");
        // Should have entries for interfaces that are up
        assert!(output.contains("STATIC"), "Should have static entries for own MACs");
    }

    #[test]
    fn test_show_mac_address_table_vlan_and_ports() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_mac_address_table();

        // Vlan1 SVI should appear as Vl1 in Ports column
        assert!(output.contains("Vl1"), "Should have Vl1 for the SVI");

        // Physical interfaces that are admin_up (Gi1/0/1 through Gi1/0/4) should appear as Gi shortnames
        assert!(output.contains("Gi1/0/1"), "Should have Gi1/0/1 entry");
        assert!(output.contains("Gi1/0/4"), "Should have Gi1/0/4 entry");

        // Shutdown interfaces should NOT appear
        assert!(!output.contains("Gi1/0/5"), "Gi1/0/5 is shutdown, should not appear");
        // Te1/0/1 is admin-up (no SFP → notconnect), so it SHOULD appear
        assert!(output.contains("Te1/0/1"), "Te1/0/1 is admin-up, should appear");

        // VLAN column for physical interfaces should be 1
        // Find a Gi1/0/1 line and check it starts with "   1"
        let gi1_line = output.lines().find(|l| l.contains("Gi1/0/1")).unwrap();
        assert!(gi1_line.trim_start().starts_with("1 "), "VLAN should be 1 for Gi1/0/1: {:?}", gi1_line);
    }

    #[test]
    fn test_show_mac_address_table_count() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_mac_address_table();

        // admin_up interfaces: Vlan1 (1) + Gi1/0/1..4 (4) + Te1/0/1..2 (2) = 7 total
        assert!(
            output.contains("Total Mac Addresses for this criterion: 7"),
            "Should count 7 admin-up interfaces, got output: {:?}",
            output
        );
    }

    // ─── IPv6 Tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_eui64_link_local_from_cisco_mac() {
        // Real device: MAC 18:8B:45:17:F7:80 → FE80::1A8B:45FF:FE17:F780
        let ll = mac_to_eui64_link_local("188b.4517.f780");
        // bit flip: 18 → 1A (bit 1 of first byte flipped)
        assert_eq!(ll, "fe80::1a8b:45ff:fe17:f780".parse::<Ipv6Addr>().unwrap(),
            "EUI-64 should match real IOS: got {}", ll);
    }

    #[test]
    fn test_eui64_link_local_from_colon_mac() {
        let ll = mac_to_eui64_link_local("00:A3:D1:4F:22:80");
        // 00 → 02 (flip U/L bit), insert FF:FE
        assert_eq!(ll, "fe80::2a3:d1ff:fe4f:2280".parse::<Ipv6Addr>().unwrap(),
            "EUI-64 from colon MAC: got {}", ll);
    }

    #[test]
    fn test_interface_ipv6_link_local_not_enabled() {
        let iface = InterfaceState::new("GigabitEthernet1/0/1");
        assert!(!iface.has_ipv6());
        assert!(iface.ipv6_link_local().is_none());
    }

    #[test]
    fn test_interface_ipv6_link_local_enabled() {
        let mut iface = InterfaceState::new("GigabitEthernet1/0/1");
        iface.ipv6_enabled = true;
        assert!(iface.has_ipv6());
        assert!(iface.ipv6_link_local().is_some());
    }

    #[test]
    fn test_interface_ipv6_explicit_link_local() {
        let mut iface = InterfaceState::new("Loopback0");
        let explicit_ll: Ipv6Addr = "fe80::10:127:0:0".parse().unwrap();
        iface.ipv6_addresses.push(Ipv6AddrConfig {
            address: explicit_ll,
            prefix_len: 128,
            addr_type: Ipv6AddrType::LinkLocal,
            eui64: false,
        });
        // Explicit link-local should be returned instead of EUI-64
        assert_eq!(iface.ipv6_link_local().unwrap(), explicit_ll);
    }

    #[test]
    fn test_interface_ipv6_global_address_implies_link_local() {
        let mut iface = InterfaceState::new("Vlan1");
        iface.ipv6_addresses.push(Ipv6AddrConfig {
            address: "2001:db8::1".parse().unwrap(),
            prefix_len: 64,
            addr_type: Ipv6AddrType::Global,
            eui64: false,
        });
        // Having a global address should auto-generate link-local
        assert!(iface.ipv6_link_local().is_some());
        assert!(iface.has_ipv6());
    }

    #[test]
    fn test_ipv6_apply_prefix() {
        let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let prefix = ipv6_apply_prefix(addr, 64);
        assert_eq!(prefix, "2001:db8::".parse::<Ipv6Addr>().unwrap());

        let addr2: Ipv6Addr = "2a11:d940:2:7f00::".parse().unwrap();
        let prefix2 = ipv6_apply_prefix(addr2, 128);
        assert_eq!(prefix2, addr2);
    }

    #[test]
    fn test_show_ipv6_interface_brief_no_ipv6() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_ipv6_interface_brief();
        // All interfaces should show "unassigned" since no IPv6 configured
        for line in output.lines() {
            if line.starts_with("    ") {
                assert_eq!(line.trim(), "unassigned",
                    "Without IPv6 config, should show 'unassigned': {:?}", line);
            }
        }
    }

    #[test]
    fn test_show_ipv6_interface_brief_with_addresses() {
        let mut state = DeviceState::new("Switch1");
        // Add IPv6 to Vlan1 like the real device
        let vlan1 = state.get_interface_mut("Vlan1").unwrap();
        vlan1.ipv6_addresses.push(Ipv6AddrConfig {
            address: "2001:db8::1".parse().unwrap(),
            prefix_len: 64,
            addr_type: Ipv6AddrType::Global,
            eui64: false,
        });

        let output = state.generate_show_ipv6_interface_brief();
        // Vlan1 should show link-local (auto EUI-64) + global
        assert!(output.contains("[up/up]"), "Should show status");
        let vlan1_section: Vec<&str> = output.lines()
            .skip_while(|l| !l.starts_with("Vlan1"))
            .take_while(|l| l.starts_with("Vlan1") || l.starts_with("    "))
            .collect();
        assert!(vlan1_section.len() >= 3,
            "Vlan1 should have name + link-local + global lines, got: {:?}", vlan1_section);
        assert!(vlan1_section[1].trim().starts_with("FE80::") || vlan1_section[1].trim().starts_with("fe80::"),
            "Second line should be link-local: {:?}", vlan1_section[1]);
        assert!(vlan1_section[2].trim().contains("2001:db8::1"),
            "Third line should be global address: {:?}", vlan1_section[2]);
    }

    #[test]
    fn test_show_ipv6_route_with_connected() {
        let mut state = DeviceState::new("Switch1");
        // Add IPv6 to Vlan1
        let vlan1 = state.get_interface_mut("Vlan1").unwrap();
        vlan1.ipv6_addresses.push(Ipv6AddrConfig {
            address: "2001:db8::1".parse().unwrap(),
            prefix_len: 64,
            addr_type: Ipv6AddrType::Global,
            eui64: false,
        });

        let output = state.generate_show_ipv6_route();
        assert!(output.contains("IPv6 Routing Table"), "Should have header");
        assert!(output.contains("Codes: C - Connected"), "Should have codes");
        // Connected route for 2001:db8::/64
        assert!(output.contains("2001:db8::/64"), "Should have connected route");
        // Local route for 2001:db8::1/128
        assert!(output.contains("2001:db8::1/128"), "Should have local route");
        // Multicast FF00::/8
        assert!(output.contains("FF00::/8") || output.contains("ff00::/8"),
            "Should have multicast route");
    }

    #[test]
    fn test_show_ipv6_route_loopback_lc() {
        let mut state = DeviceState::new("Switch1");
        // Add a Loopback0 with /128
        let mut lo0 = InterfaceState::new("Loopback0");
        lo0.ipv6_addresses.push(Ipv6AddrConfig {
            address: "2a11:d940:2:7f00::".parse().unwrap(),
            prefix_len: 128,
            addr_type: Ipv6AddrType::Global,
            eui64: false,
        });
        state.interfaces.push(lo0);

        let output = state.generate_show_ipv6_route();
        // Loopback /128 should appear as LC (Local Connected)
        assert!(output.contains("LC"), "Loopback /128 should show as LC route");
    }

    #[test]
    fn test_show_ipv6_ospf_empty() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_ipv6_ospf();
        assert!(output.is_empty(), "No OSPFv3 processes configured should give empty output");
    }

    #[test]
    fn test_show_ipv6_ospf_with_process() {
        let mut state = DeviceState::new("Switch1");
        let mut proc = OspfV3Process::new(1);
        proc.router_id = Some("10.127.0.0".parse().unwrap());
        proc.areas.push(OspfV3Area::new(0));
        state.ospfv3_processes.push(proc);

        let output = state.generate_show_ipv6_ospf();
        assert!(output.contains("ospfv3 1"), "Should show process ID");
        assert!(output.contains("10.127.0.0"), "Should show router ID");
        assert!(output.contains("BACKBONE(0)"), "Should show area 0 as BACKBONE");
        assert!(output.contains("(Inactive)"), "Area with no active interfaces should be Inactive");
        assert!(output.contains("Reference bandwidth unit is 100 mbps"), "Should show reference bandwidth");
    }

    #[test]
    fn test_ipv6_route_type_codes() {
        assert_eq!(Ipv6RouteType::Connected.code(), "C");
        assert_eq!(Ipv6RouteType::Local.code(), "L");
        assert_eq!(Ipv6RouteType::LocalConnected.code(), "LC");
        assert_eq!(Ipv6RouteType::Static.code(), "S");
        assert_eq!(Ipv6RouteType::OspfIntra.code(), "O");
        assert_eq!(Ipv6RouteType::NdDefault.code(), "ND");
    }

    #[test]
    fn test_default_state_no_ipv6() {
        let state = DeviceState::new("Switch1");
        assert!(!state.ipv6_unicast_routing);
        assert!(state.ipv6_static_routes.is_empty());
        assert!(state.ospfv3_processes.is_empty());
    }

    // ─── Interface Status State Tests ─────────────────────────────────────────

    /// Default (non-shutdown, no cable) ports must show `notconnect` in
    /// `show interfaces status`.  Only ports that are explicitly shutdown
    /// should show `disabled`.
    #[test]
    fn test_show_interfaces_status_notconnect_default() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_status();

        // All Gi ports (1-16) that are admin_up should be notconnect by default
        // (no physical cable connected in simulation).
        // In the NEW model Gi1/0/1..16 are all admin_up, link_up=false → notconnect.
        let gi1_line = output.lines().find(|l| l.starts_with("Gi1/0/1 ")).unwrap();
        assert!(gi1_line.contains("notconnect"),
            "Gi1/0/1 (admin up, no link) should be notconnect: {:?}", gi1_line);
    }

    /// Explicitly shutdown ports must show `disabled`.
    #[test]
    fn test_show_interfaces_status_disabled_for_shutdown() {
        let mut state = DeviceState::new("Switch1");
        // Manually shut down Gi1/0/3
        state.interfaces.iter_mut()
            .find(|i| i.name == "GigabitEthernet1/0/3")
            .unwrap()
            .admin_up = false;
        let output = state.generate_show_interfaces_status();
        let gi3_line = output.lines().find(|l| l.starts_with("Gi1/0/3")).unwrap();
        assert!(gi3_line.contains("disabled"),
            "Shutdown Gi1/0/3 should show disabled: {:?}", gi3_line);
    }

    /// Te ports must show `full` duplex and `10G` speed in
    /// `show interfaces status` regardless of link state.
    #[test]
    fn test_show_interfaces_status_te_port_defaults() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_status();
        let te1_line = output.lines().find(|l| l.starts_with("Te1/0/1")).unwrap();
        assert!(te1_line.contains("10G"),
            "Te1/0/1 should show 10G speed: {:?}", te1_line);
        assert!(te1_line.contains("full"),
            "Te1/0/1 should show full duplex: {:?}", te1_line);
    }

    /// `InterfaceState::new()` for a TenGigabitEthernet port should default to
    /// `speed = "10000"` and `duplex = "full"`.
    #[test]
    fn test_te_interface_new_defaults_speed_duplex() {
        let iface = InterfaceState::new("TenGigabitEthernet1/0/1");
        assert_eq!(iface.speed, "10000",
            "Te interface should default to speed 10000, got: {:?}", iface.speed);
        assert_eq!(iface.duplex, "full",
            "Te interface should default to duplex full, got: {:?}", iface.duplex);
    }

    /// `InterfaceState::new()` should default `link_up` to `false` (no cable
    /// connected in the simulation).
    #[test]
    fn test_interface_new_defaults_link_up_false() {
        let iface = InterfaceState::new("GigabitEthernet1/0/1");
        assert!(!iface.link_up,
            "New interface should default to link_up=false (no cable): {:?}", iface.link_up);
    }

    /// `show ip interface brief` must show `down`/`down` for a port that is
    /// admin-up but has no physical link (notconnect state).
    #[test]
    fn test_show_ip_interface_brief_notconnect_shows_down() {
        let state = DeviceState::new("Switch1");
        // Gi1/0/1 should be admin_up=true, link_up=false in new model
        let gi1 = state.interfaces.iter().find(|i| i.name == "GigabitEthernet1/0/1").unwrap();
        assert!(gi1.admin_up, "Gi1/0/1 should be admin up");
        assert!(!gi1.link_up, "Gi1/0/1 should have no link in default state");

        // The generate_show_interface method should reflect down/down for notconnect
        let detail = gi1.generate_show_interface();
        assert!(detail.contains("down"), "notconnect port should show 'down' in show interface detail");
    }

    #[test]
    fn test_generate_show_interfaces_switchport_basic() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_switchport();

        // Physical Ethernet interfaces should appear
        assert!(output.contains("Name: Gi1/0/1"),
            "Gi1/0/1 should appear in switchport output: {:?}", &output[..output.len().min(400)]);
        assert!(output.contains("Name: Te1/0/1"),
            "Te1/0/1 should appear in switchport output");

        // Vlan and Loopback interfaces should NOT appear
        assert!(!output.contains("Name: Vl"),
            "Vlan SVIs should not appear in switchport output");
        assert!(!output.contains("Name: Lo"),
            "Loopback interfaces should not appear in switchport output");
    }

    #[test]
    fn test_generate_show_interfaces_switchport_access_port_fields() {
        let state = DeviceState::new("Switch1");
        let output = state.generate_show_interfaces_switchport();

        // Find the Gi1/0/1 block
        let gi1_block_start = output.find("Name: Gi1/0/1").expect("Gi1/0/1 block missing");
        let gi1_block_end = output[gi1_block_start..]
            .find("\nName: ")
            .map(|off| gi1_block_start + off)
            .unwrap_or(output.len());
        let gi1_block = &output[gi1_block_start..gi1_block_end];

        assert!(gi1_block.contains("Switchport: Enabled"),
            "Should show Switchport: Enabled: {:?}", gi1_block);
        assert!(gi1_block.contains("Administrative Mode: dynamic auto"),
            "Access port admin mode should be 'dynamic auto': {:?}", gi1_block);
        assert!(gi1_block.contains("Operational Mode: down"),
            "Unconnected access port operational mode should be 'down': {:?}", gi1_block);
        assert!(gi1_block.contains("Administrative Trunking Encapsulation: dot1q"),
            "Should show dot1q encapsulation: {:?}", gi1_block);
        assert!(gi1_block.contains("Access Mode VLAN: 1 (default)"),
            "Default VLAN should be 1 (default): {:?}", gi1_block);
        assert!(gi1_block.contains("Trunking Native Mode VLAN: 1 (default)"),
            "Native VLAN should be 1 (default): {:?}", gi1_block);
        assert!(gi1_block.contains("Voice VLAN: none"),
            "Voice VLAN should be none: {:?}", gi1_block);
    }

    #[test]
    fn test_generate_show_interfaces_switchport_trunk_port() {
        let mut state = DeviceState::new("Switch1");
        // Configure Gi1/0/1 as a trunk with link up
        if let Some(iface) = state.get_interface_mut("GigabitEthernet1/0/1") {
            iface.switchport_mode = Some("trunk".to_string());
            iface.link_up = true;
        }
        let output = state.generate_show_interfaces_switchport();

        let gi1_block_start = output.find("Name: Gi1/0/1").expect("Gi1/0/1 block missing");
        let gi1_block_end = output[gi1_block_start..]
            .find("\nName: ")
            .map(|off| gi1_block_start + off)
            .unwrap_or(output.len());
        let gi1_block = &output[gi1_block_start..gi1_block_end];

        assert!(gi1_block.contains("Administrative Mode: trunk"),
            "Trunk port admin mode should be 'trunk': {:?}", gi1_block);
        assert!(gi1_block.contains("Operational Mode: trunk"),
            "Connected trunk port operational mode should be 'trunk': {:?}", gi1_block);
    }

    #[test]
    fn test_generate_show_interfaces_switchport_trunk_not_connected() {
        let mut state = DeviceState::new("Switch1");
        // Configure Gi1/0/1 as a trunk but link is down (no cable)
        if let Some(iface) = state.get_interface_mut("GigabitEthernet1/0/1") {
            iface.switchport_mode = Some("trunk".to_string());
            iface.link_up = false;
        }
        let output = state.generate_show_interfaces_switchport();

        let gi1_block_start = output.find("Name: Gi1/0/1").expect("Gi1/0/1 block missing");
        let gi1_block_end = output[gi1_block_start..]
            .find("\nName: ")
            .map(|off| gi1_block_start + off)
            .unwrap_or(output.len());
        let gi1_block = &output[gi1_block_start..gi1_block_end];

        assert!(gi1_block.contains("Administrative Mode: trunk"),
            "Trunk port admin mode should be 'trunk': {:?}", gi1_block);
        assert!(gi1_block.contains("Operational Mode: down"),
            "Disconnected trunk port operational mode should be 'down': {:?}", gi1_block);
    }

    #[test]
    fn test_generate_show_interfaces_switchport_custom_vlan() {
        let mut state = DeviceState::new("Switch1");
        // Add a custom VLAN and assign Gi1/0/2 to it
        state.vlans.push(VlanState {
            id: 10,
            name: "DATA".to_string(),
            active: true,
            ports: vec!["Gi1/0/2".to_string()],
            unsupported: false,
        });
        if let Some(iface) = state.get_interface_mut("GigabitEthernet1/0/2") {
            iface.vlan = Some(10);
        }
        let output = state.generate_show_interfaces_switchport();

        let gi2_block_start = output.find("Name: Gi1/0/2").expect("Gi1/0/2 block missing");
        let gi2_block_end = output[gi2_block_start..]
            .find("\nName: ")
            .map(|off| gi2_block_start + off)
            .unwrap_or(output.len());
        let gi2_block = &output[gi2_block_start..gi2_block_end];

        assert!(gi2_block.contains("Access Mode VLAN: 10 (DATA)"),
            "VLAN 10 with name DATA should be shown: {:?}", gi2_block);
    }

    // ── show interfaces counters ──────────────────────────────────────────────

    fn make_counters_state() -> DeviceState {
        DeviceState::new("Switch")
    }

    #[test]
    fn test_show_interfaces_counters_has_two_tables() {
        let state = make_counters_state();
        let output = state.generate_show_interfaces_counters();
        assert!(output.contains("InOctets"), "missing input header");
        assert!(output.contains("OutOctets"), "missing output header");
    }

    #[test]
    fn test_show_interfaces_counters_input_header_columns() {
        let state = make_counters_state();
        let output = state.generate_show_interfaces_counters();
        let first_line = output.lines().next().unwrap();
        assert!(first_line.contains("InOctets"), "InOctets missing: {first_line}");
        assert!(first_line.contains("InUcastPkts"), "InUcastPkts missing: {first_line}");
        assert!(first_line.contains("InMcastPkts"), "InMcastPkts missing: {first_line}");
        assert!(first_line.contains("InBcastPkts"), "InBcastPkts missing: {first_line}");
    }

    #[test]
    fn test_show_interfaces_counters_output_header_columns() {
        let state = make_counters_state();
        let output = state.generate_show_interfaces_counters();
        let out_header = output.lines().find(|l| l.contains("OutOctets")).expect("OutOctets line missing");
        assert!(out_header.contains("OutUcastPkts"), "OutUcastPkts missing");
        assert!(out_header.contains("OutMcastPkts"), "OutMcastPkts missing");
        assert!(out_header.contains("OutBcastPkts"), "OutBcastPkts missing");
    }

    #[test]
    fn test_show_interfaces_counters_no_vlan_interfaces() {
        let state = make_counters_state();
        let output = state.generate_show_interfaces_counters();
        // Vlan SVI interfaces should not appear
        assert!(!output.contains("Vl1"), "SVI Vlan1 should not appear: {output}");
    }

    #[test]
    fn test_show_interfaces_counters_physical_interfaces_present() {
        let state = make_counters_state();
        let output = state.generate_show_interfaces_counters();
        // GigabitEthernet1/0/1 should appear as Gi1/0/1
        assert!(output.contains("Gi1/0/1"), "Gi1/0/1 missing: {output}");
    }

    #[test]
    fn test_show_interfaces_counters_zero_values() {
        let state = make_counters_state();
        let output = state.generate_show_interfaces_counters();
        // All counters default to 0
        for line in output.lines() {
            if line.starts_with("Gi") || line.starts_with("Te") || line.starts_with("Fa") {
                assert!(line.contains("0"), "expected zeros in: {line}");
            }
        }
    }

    #[test]
    fn test_show_interfaces_counters_nonzero_values() {
        let mut state = make_counters_state();
        state.interfaces[1].input_packets = 100;
        state.interfaces[1].input_bytes = 12000;
        state.interfaces[1].output_packets = 50;
        state.interfaces[1].output_bytes = 6000;
        let output = state.generate_show_interfaces_counters();
        assert!(output.contains("12000"), "input_bytes not shown: {output}");
        assert!(output.contains("100"), "input_packets not shown: {output}");
        assert!(output.contains("6000"), "output_bytes not shown: {output}");
        assert!(output.contains("50"), "output_packets not shown: {output}");
    }

    #[test]
    fn test_show_interfaces_counters_blank_line_separates_tables() {
        let state = make_counters_state();
        let output = state.generate_show_interfaces_counters();
        // There should be a blank line between the two tables
        assert!(output.contains("\n\n"), "no blank line separator between tables: {output}");
    }
}
