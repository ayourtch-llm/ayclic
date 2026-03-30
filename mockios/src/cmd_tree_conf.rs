//! Config-mode command tree definitions and handlers for MockIOS.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::OnceLock;

use crate::cmd_tree::{keyword, param, CliModeClass, CmdHandler, CommandNode, ModeFilter, ParamType};
use crate::device_state::{AccessList, AccessListEntry, StaticRoute,
    Ipv6AddrConfig, Ipv6AddrType, Ipv6StaticRoute,
    OspfV3Process, OspfV3Area, InterfaceOspfV3Config};
use crate::{CliMode, MockIosDevice};

// ─── Mode helpers ─────────────────────────────────────────────────────────────

fn config_only() -> ModeFilter {
    ModeFilter::Only(vec![CliModeClass::Config])
}

fn config_if_only() -> ModeFilter {
    ModeFilter::Only(vec![CliModeClass::ConfigSub])
}

/// Select the appropriate tree for a ConfigSub mode.
/// Returns a reference to the sub-mode-specific tree.
pub fn config_sub_tree(sub_mode: &str) -> &'static Vec<CommandNode> {
    match sub_mode {
        "config-if" => config_if_tree(),
        "config-router" => config_router_tree(),
        "config-line" => config_line_tree(),
        "config-ext-nacl" => config_ext_nacl_tree(),
        "config-std-nacl" => config_std_nacl_tree(),
        _ => conf_tree(), // unknown sub-modes fall back to full conf tree
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub fn handle_hostname(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if negated {
        d.hostname = "Router".to_string();
        d.state.hostname = "Router".to_string();
    } else {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1].to_string();
            d.hostname = name.clone();
            d.state.hostname = name;
        }
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

/// Normalize IOS interface names, e.g. "loopback 0" -> "Loopback0",
/// "g1/0/9" -> "GigabitEthernet1/0/9", "vlan 100" -> "Vlan100".
pub fn normalize_interface_name(input: &str) -> String {
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
    let (type_part, num_part) = match parts.as_slice() {
        [t, n] => (*t, n.trim().to_string()),
        [t] => (*t, String::new()),
        _ => return trimmed.to_string(),
    };

    // Split type_part at the boundary where letters end and digits/slashes begin.
    // E.g. "gi1/0/9" -> ("gi", "1/0/9"), "GigabitEthernet1/0/1" -> ("GigabitEthernet", "1/0/1")
    let alpha_end = type_part.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(type_part.len());
    let alpha_prefix = &type_part[..alpha_end];
    let embedded_num = &type_part[alpha_end..];

    // Determine the effective number part
    let effective_num = if !embedded_num.is_empty() && !num_part.is_empty() {
        format!("{} {}", embedded_num, num_part)
    } else if !embedded_num.is_empty() {
        embedded_num.to_string()
    } else {
        num_part.clone()
    };

    let alpha_lower = alpha_prefix.to_lowercase();
    let canonical_type = match alpha_lower.as_str() {
        t if t.starts_with("gi") || t == "g" => "GigabitEthernet",
        t if t.starts_with("fa") || t == "f" => "FastEthernet",
        t if t.starts_with("te") => "TenGigabitEthernet",
        t if t.starts_with("hu") => "HundredGigE",
        t if t.starts_with("lo") => "Loopback",
        t if t.starts_with("vl") || t == "v" => "Vlan",
        t if t.starts_with("mg") => "Mgmt",
        t if t.starts_with("se") => "Serial",
        t if t.starts_with("tu") => "Tunnel",
        _ => {
            if effective_num.is_empty() {
                return trimmed.to_string();
            } else {
                return format!("{}{}", type_part, if num_part.is_empty() { "".to_string() } else { format!(" {}", num_part) });
            }
        }
    };

    format!("{}{}", canonical_type, effective_num)
}

pub fn handle_interface(d: &mut MockIosDevice, input: &str) {
    // "interface <name>" — enter config-if sub-mode
    // Input is the full line, e.g. "interface loopback 0" or "interface GigabitEthernet0/0"
    let raw_name = if let Some(rest) = input.trim().strip_prefix("interface").map(|s| s.trim()) {
        rest.to_string()
    } else {
        // Fallback: skip first token
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() >= 2 { parts[1..].join(" ") } else { "unknown".to_string() }
    };
    let iface_name = normalize_interface_name(&raw_name);
    d.mode = CliMode::ConfigSub("config-if".to_string());
    // Ensure the interface exists in state
    d.state.ensure_interface(&iface_name);
    d.current_interface = Some(iface_name.clone());
    d.running_config.push(format!("interface {}", iface_name));
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_router_ospf(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-router".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_router_bgp(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-router".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_router_eigrp(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-router".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ip_route(d: &mut MockIosDevice, input: &str) {
    let trimmed = input.trim();
    let negated = trimmed.starts_with("no");
    // Strip "no " prefix if present, then strip "ip route " prefix
    let route_part = if negated {
        trimmed.strip_prefix("no").unwrap_or(trimmed).trim()
    } else {
        trimmed
    };
    let route_part = route_part.strip_prefix("ip").unwrap_or(route_part).trim();
    let route_part = route_part.strip_prefix("route").unwrap_or(route_part).trim();

    let parts: Vec<&str> = route_part.split_whitespace().collect();
    if parts.len() >= 3 {
        if let (Ok(prefix), Ok(mask), Ok(next_hop)) = (
            parts[0].parse::<Ipv4Addr>(),
            parts[1].parse::<Ipv4Addr>(),
            parts[2].parse::<Ipv4Addr>(),
        ) {
            if negated {
                d.state.static_routes.retain(|r| {
                    !(r.prefix == prefix && r.mask == mask && r.next_hop == Some(next_hop))
                });
            } else {
                d.state.static_routes.push(StaticRoute {
                    prefix,
                    mask,
                    next_hop: Some(next_hop),
                    interface: None,
                    admin_distance: 1,
                });
            }
        }
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

// ─── IPv6 Handlers ───────────────────────────────────────────────────────────

pub fn handle_ipv6_unicast_routing(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    d.state.ipv6_unicast_routing = !negated;
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ipv6_enable(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            iface.ipv6_enabled = !negated;
        }
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ipv6_address(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    let trimmed = input.trim();

    if negated {
        // "no ipv6 address" — remove all IPv6 addresses from current interface
        if let Some(ref iface_name) = d.current_interface.clone() {
            if let Some(iface) = d.state.get_interface_mut(iface_name) {
                iface.ipv6_addresses.clear();
                iface.ipv6_enabled = false;
            }
        }
    } else {
        // Parse "ipv6 address <addr>/<prefix-len>" or "ipv6 address <addr> link-local"
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        // parts[0] = "ipv6", parts[1] = "address", parts[2] = addr/prefix or addr, parts[3..] = options
        if parts.len() >= 3 {
            let addr_str = parts[2];
            let is_link_local = parts.len() >= 4 && parts[3].eq_ignore_ascii_case("link-local");

            if is_link_local {
                // Explicit link-local: "ipv6 address FE80::1 link-local"
                if let Ok(addr) = addr_str.parse::<Ipv6Addr>() {
                    if let Some(ref iface_name) = d.current_interface.clone() {
                        if let Some(iface) = d.state.get_interface_mut(iface_name) {
                            // Remove any existing link-local
                            iface.ipv6_addresses.retain(|a| a.addr_type != Ipv6AddrType::LinkLocal);
                            iface.ipv6_addresses.push(Ipv6AddrConfig {
                                address: addr,
                                prefix_len: 128,
                                addr_type: Ipv6AddrType::LinkLocal,
                                eui64: false,
                            });
                        }
                    }
                }
            } else if let Some(slash_pos) = addr_str.find('/') {
                // Global address: "ipv6 address 2001:db8::1/64"
                let addr_part = &addr_str[..slash_pos];
                let prefix_part = &addr_str[slash_pos + 1..];
                if let (Ok(addr), Ok(prefix_len)) = (
                    addr_part.parse::<Ipv6Addr>(),
                    prefix_part.parse::<u8>(),
                ) {
                    if let Some(ref iface_name) = d.current_interface.clone() {
                        if let Some(iface) = d.state.get_interface_mut(iface_name) {
                            iface.ipv6_addresses.push(Ipv6AddrConfig {
                                address: addr,
                                prefix_len,
                                addr_type: Ipv6AddrType::Global,
                                eui64: false,
                            });
                        }
                    }
                }
            }
        }
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ipv6_route(d: &mut MockIosDevice, input: &str) {
    let trimmed = input.trim();
    let negated = trimmed.starts_with("no");

    // Strip "no " prefix if present, then "ipv6 route "
    let route_part = if negated {
        trimmed.strip_prefix("no").unwrap_or(trimmed).trim()
    } else {
        trimmed
    };
    let route_part = route_part.strip_prefix("ipv6").unwrap_or(route_part).trim();
    let route_part = route_part.strip_prefix("route").unwrap_or(route_part).trim();

    let parts: Vec<&str> = route_part.split_whitespace().collect();
    // Expected: <prefix/len> <next-hop or interface>
    if !parts.is_empty() {
        if let Some(slash_pos) = parts[0].find('/') {
            let prefix_str = &parts[0][..slash_pos];
            let len_str = &parts[0][slash_pos + 1..];
            if let (Ok(prefix), Ok(prefix_len)) = (
                prefix_str.parse::<Ipv6Addr>(),
                len_str.parse::<u8>(),
            ) {
                if negated {
                    d.state.ipv6_static_routes.retain(|r| {
                        !(r.prefix == prefix && r.prefix_len == prefix_len)
                    });
                } else if parts.len() >= 2 {
                    let next_hop = parts[1].parse::<Ipv6Addr>().ok();
                    let interface = if next_hop.is_none() {
                        Some(parts[1].to_string())
                    } else {
                        None
                    };
                    d.state.ipv6_static_routes.push(Ipv6StaticRoute {
                        prefix,
                        prefix_len,
                        next_hop,
                        interface,
                        admin_distance: 1,
                    });
                }
            }
        }
    }

    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ipv6_router_ospf(d: &mut MockIosDevice, input: &str) {
    // "ipv6 router ospf <pid>" — create process if needed, enter config-router
    let parts: Vec<&str> = input.split_whitespace().collect();
    if let Some(pid_str) = parts.last() {
        if let Ok(pid) = pid_str.parse::<u16>() {
            // Create process if it doesn't exist
            if !d.state.ospfv3_processes.iter().any(|p| p.process_id == pid) {
                d.state.ospfv3_processes.push(OspfV3Process::new(pid));
            }
        }
    }
    d.mode = CliMode::ConfigSub("config-router".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ipv6_ospf_interface(d: &mut MockIosDevice, input: &str) {
    // "ipv6 ospf <pid> area <area-id>" — assign interface to OSPFv3 area
    let negated = input.trim().starts_with("no");
    let parts: Vec<&str> = input.split_whitespace().collect();

    if negated {
        if let Some(ref iface_name) = d.current_interface.clone() {
            if let Some(iface) = d.state.get_interface_mut(iface_name) {
                iface.ospfv3_config = None;
            }
        }
    } else {
        // Find "ospf" position, then <pid>, then "area", then <area-id>
        let ospf_pos = parts.iter().position(|p| p.eq_ignore_ascii_case("ospf"));
        if let Some(pos) = ospf_pos {
            let pid = parts.get(pos + 1).and_then(|s| s.parse::<u16>().ok());
            let area_keyword = parts.get(pos + 2).map(|s| s.eq_ignore_ascii_case("area")).unwrap_or(false);
            let area_id = if area_keyword {
                parts.get(pos + 3).and_then(|s| s.parse::<u32>().ok())
            } else {
                None
            };

            if let (Some(pid), Some(area_id)) = (pid, area_id) {
                if let Some(ref iface_name) = d.current_interface.clone() {
                    // Ensure the OSPFv3 process exists and has this area
                    if let Some(proc) = d.state.ospfv3_processes.iter_mut().find(|p| p.process_id == pid) {
                        if !proc.areas.iter().any(|a| a.area_id == area_id) {
                            proc.areas.push(OspfV3Area::new(area_id));
                        }
                    }

                    if let Some(iface) = d.state.get_interface_mut(iface_name) {
                        iface.ospfv3_config = Some(InterfaceOspfV3Config {
                            process_id: pid,
                            area_id,
                            network_type: None,
                            cost: None,
                        });
                    }
                }
            }
        }
    }

    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ipv6_ospf_network(d: &mut MockIosDevice, input: &str) {
    // "ipv6 ospf network point-to-point" — set network type on interface
    let parts: Vec<&str> = input.split_whitespace().collect();
    let network_pos = parts.iter().position(|p| p.eq_ignore_ascii_case("network"));
    if let Some(pos) = network_pos {
        if let Some(net_type) = parts.get(pos + 1) {
            if let Some(ref iface_name) = d.current_interface.clone() {
                if let Some(iface) = d.state.get_interface_mut(iface_name) {
                    if let Some(ref mut ospf_cfg) = iface.ospfv3_config {
                        ospf_cfg.network_type = Some(net_type.to_string());
                    }
                }
            }
        }
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ip_address(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if negated {
        // "no ip address" — remove IP from current interface
        if let Some(ref iface_name) = d.current_interface.clone() {
            if let Some(iface) = d.state.get_interface_mut(iface_name) {
                iface.ip_address = None;
            }
        }
    } else {
        // Parse "ip address <addr> <mask>"
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() >= 4 {
            if let (Ok(addr), Ok(mask)) = (
                parts[2].parse::<Ipv4Addr>(),
                parts[3].parse::<Ipv4Addr>(),
            ) {
                if let Some(ref iface_name) = d.current_interface.clone() {
                    if let Some(iface) = d.state.get_interface_mut(iface_name) {
                        iface.ip_address = Some((addr, mask));
                    }
                }
            }
        }
    }
    // Store as indented sub-config line
    d.running_config.push(format!(" {}", input.trim()));
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ip_domain_name(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ip_name_server(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}


pub fn handle_line_vty(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-line".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_line_console(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-line".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_enable_secret(d: &mut MockIosDevice, input: &str) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    // input can be "enable secret <pw>" or "no enable secret"
    if !input.trim().starts_with("no") && parts.len() >= 3 {
        d.state.enable_secret = Some(parts[2..].join(" "));
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_enable_password(d: &mut MockIosDevice, input: &str) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if !input.trim().starts_with("no") && parts.len() >= 3 {
        d.state.enable_secret = Some(parts[2..].join(" "));
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_rest_of_line(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

// ─── Stub handlers for new global config commands ────────────────────────────

pub fn handle_banner_login(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_banner_exec(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_aaa(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_arp(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_class_map(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_clock(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_default(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_dot1x(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_lldp(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_monitor(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_policy_map(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_port_channel(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_power(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_privilege(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_tacacs_server(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_boot_system(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_cdp_run(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_crypto(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_do_stub(d: &mut MockIosDevice, input: &str) {
    // "do" is handled as a special prefix in dispatch_config;
    // this handler is only reached if the keyword is parsed from the tree
    // (e.g. during tab-completion or help).
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_errdisable(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_event(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_logging_host(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_logging_trap(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_logging_buffered(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_mac(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ntp_server(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_service_timestamps(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_service_password_encryption(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_service_pad(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_snmp_server(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_username(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_banner_motd(d: &mut MockIosDevice, input: &str) {
    if input.trim().starts_with("no") {
        d.state.banner_motd = String::new();
        let p = d.prompt();
        d.queue_output(&format!("{}", p));
        return;
    }
    // Parse: "banner motd <delim><text><delim>"
    let rest = input.trim()
        .strip_prefix("banner").map(|s| s.trim())
        .and_then(|s| s.strip_prefix("motd")).map(|s| s.trim())
        .unwrap_or("");

    if let Some(delim) = rest.chars().next() {
        let after_delim = &rest[delim.len_utf8()..];
        if let Some(end) = after_delim.find(delim) {
            d.state.banner_motd = after_delim[..end].to_string();
        } else {
            // No closing delimiter — use rest of line
            d.state.banner_motd = after_delim.to_string();
        }
    }

    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_shutdown(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            iface.admin_up = negated; // "no shutdown" = up, "shutdown" = down
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_description(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            if negated {
                iface.description.clear();
            } else {
                let desc = input.trim()
                    .strip_prefix("description").map(|s| s.trim())
                    .unwrap_or("");
                iface.description = desc.to_string();
            }
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_speed(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            if negated {
                iface.speed = "auto".to_string();
            } else {
                // Input is like "speed 100" or "speed auto" or "speed 1000"
                let parts: Vec<&str> = input.trim().split_whitespace().collect();
                if let Some(&val) = parts.get(1) {
                    iface.speed = val.to_string();
                }
            }
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_duplex(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            if negated {
                iface.duplex = "auto".to_string();
            } else {
                // Input is like "duplex full" or "duplex half" or "duplex auto"
                let parts: Vec<&str> = input.trim().split_whitespace().collect();
                if let Some(&val) = parts.get(1) {
                    iface.duplex = val.to_string();
                }
            }
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_switchport_mode(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            if negated {
                iface.switchport_mode = None;
            } else {
                // Parse "switchport mode <mode>"
                let parts: Vec<&str> = input.split_whitespace().collect();
                // parts: ["switchport", "mode", "<access|trunk|...>"]
                if let Some(&mode) = parts.get(2) {
                    iface.switchport_mode = Some(mode.to_string());
                }
            }
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_switchport_access_vlan(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            if negated {
                // "no switchport access vlan" resets to default (VLAN 1)
                iface.vlan = None;
            } else {
                // Parse "switchport access vlan <N>"
                let parts: Vec<&str> = input.split_whitespace().collect();
                // parts: ["switchport", "access", "vlan", "<N>"]
                if let Some(vlan_str) = parts.get(3) {
                    if let Ok(vlan_id) = vlan_str.parse::<u16>() {
                        iface.vlan = Some(vlan_id);
                    }
                }
            }
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_spanning_tree(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_vlan(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_config_exit(d: &mut MockIosDevice, _input: &str) {
    match &d.mode {
        CliMode::Config => d.mode = CliMode::PrivilegedExec,
        CliMode::ConfigSub(_) => d.mode = CliMode::Config,
        _ => {}
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_config_end(d: &mut MockIosDevice, _input: &str) {
    d.mode = CliMode::PrivilegedExec;
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_access_list(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    // Parse: [no] access-list <number> ...
    let parts: Vec<&str> = input.split_whitespace().collect();
    // When negated: ["no", "access-list", <number>, ...]
    // When not:     ["access-list", <number>, ...]
    let (list_num_idx, action_idx) = if negated { (2, 3) } else { (1, 2) };

    if parts.len() <= list_num_idx {
        let p = d.prompt();
        d.queue_output(&format!("% Incomplete command.\n{}", p));
        return;
    }

    let list_num = parts[list_num_idx];
    let list_num_owned = list_num.to_string();

    if negated {
        // Remove the entire ACL by this number
        d.state.access_lists.retain(|a| a.name != list_num_owned);
        let p = d.prompt();
        d.queue_output(&format!("{}", p));
        return;
    }

    if parts.len() < action_idx + 1 {
        let p = d.prompt();
        d.queue_output(&format!("% Incomplete command.\n{}", p));
        return;
    }

    let action = parts[action_idx].to_string();
    let protocol = parts.get(action_idx + 1).unwrap_or(&"ip").to_string();
    let source = parts.get(action_idx + 2).map(|s| s.to_string()).unwrap_or_else(|| "any".to_string());
    let destination = parts.get(action_idx + 3).map(|s| s.to_string()).unwrap_or_else(|| "any".to_string());
    let extra = parts.get(action_idx + 4..).map(|s| s.join(" ")).unwrap_or_default();

    let acl_type = if list_num.parse::<u32>().map(|n| n >= 100).unwrap_or(true) {
        "Extended".to_string()
    } else {
        "Standard".to_string()
    };

    // Find or create the access list
    let entry = AccessListEntry { action, protocol, source, destination, extra };
    let acl = d.state.access_lists.iter_mut().find(|a| a.name == list_num_owned);

    if let Some(acl) = acl {
        acl.entries.push(entry);
    } else {
        d.state.access_lists.push(AccessList {
            name: list_num_owned,
            acl_type,
            entries: vec![entry],
        });
    }

    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

/// Handler: `ip access-list extended|standard <name>` — create/enter named ACL.
pub fn handle_ip_access_list(d: &mut MockIosDevice, input: &str) {
    // input: "ip access-list extended|standard <name>"
    let parts: Vec<&str> = input.split_whitespace().collect();
    // parts[0]="ip", parts[1]="access-list", parts[2]=type, parts[3]=name
    let (acl_type, sub_mode_name, acl_name) = match (parts.get(2), parts.get(3)) {
        (Some(&"extended"), Some(name)) => ("Extended", "config-ext-nacl", *name),
        (Some(&"standard"), Some(name)) => ("Standard", "config-std-nacl", *name),
        _ => {
            let p = d.prompt();
            d.queue_output(&format!("% Incomplete command.\n{}", p));
            return;
        }
    };

    let acl_name_owned = acl_name.to_string();

    // Create the ACL if it doesn't already exist.
    if !d.state.access_lists.iter().any(|a| a.name == acl_name_owned) {
        d.state.access_lists.push(AccessList {
            name: acl_name_owned.clone(),
            acl_type: acl_type.to_string(),
            entries: vec![],
        });
    }

    d.current_acl_name = Some(acl_name_owned);
    d.mode = CliMode::ConfigSub(sub_mode_name.to_string());

    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

/// Handler: `no ip access-list extended|standard <name>` — remove named ACL.
pub fn handle_no_ip_access_list(d: &mut MockIosDevice, input: &str) {
    // input: "no ip access-list extended|standard <name>"
    let parts: Vec<&str> = input.split_whitespace().collect();
    // parts: ["no", "ip", "access-list", "extended"|"standard", "<name>"]
    if let Some(name) = parts.get(4) {
        let name_owned = name.to_string();
        d.state.access_lists.retain(|a| a.name != name_owned);
    }
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

/// Handler: `permit <rest>` / `deny <rest>` / `remark <rest>` inside a named ACL sub-mode.
pub fn handle_nacl_entry(d: &mut MockIosDevice, input: &str) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let action = match parts.first() {
        Some(&"permit") => "permit",
        Some(&"deny") => "deny",
        Some(&"remark") => "remark",
        _ => {
            let p = d.prompt();
            d.queue_output(&format!("% Invalid input.\n{}", p));
            return;
        }
    };

    // Determine if this is a standard or extended ACL from the current mode.
    let is_extended = matches!(&d.mode, CliMode::ConfigSub(s) if s == "config-ext-nacl");

    let protocol;
    let source;
    let destination;
    let extra;

    if action == "remark" {
        protocol = "remark".to_string();
        source = parts.get(1..).map(|s| s.join(" ")).unwrap_or_default();
        destination = String::new();
        extra = String::new();
    } else if is_extended {
        // Extended: permit/deny <protocol> <source> <dest> [extra...]
        protocol = parts.get(1).unwrap_or(&"ip").to_string();
        source = parts.get(2).unwrap_or(&"any").to_string();
        destination = parts.get(3).unwrap_or(&"any").to_string();
        extra = parts.get(4..).map(|s| s.join(" ")).unwrap_or_default();
    } else {
        // Standard: permit/deny <source> [extra...]
        protocol = String::new();
        source = parts.get(1).unwrap_or(&"any").to_string();
        destination = String::new();
        extra = parts.get(2..).map(|s| s.join(" ")).unwrap_or_default();
    }

    let entry = AccessListEntry {
        action: action.to_string(),
        protocol,
        source,
        destination,
        extra,
    };

    if let Some(ref acl_name) = d.current_acl_name.clone() {
        if let Some(acl) = d.state.access_lists.iter_mut().find(|a| &a.name == acl_name) {
            acl.entries.push(entry);
        }
    }

    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

/// Generic handler for config-router and config-line commands that stores
/// the raw line as unmodeled config.
pub fn handle_config_sub_rest(d: &mut MockIosDevice, input: &str) {
    d.state.unmodeled_config.push(format!(" {}", input.trim()));
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

// ─── ACL helpers ─────────────────────────────────────────────────────────────

/// Returns the standard permit/deny/remark sub-tree used by all numeric ACL entries.
fn acl_permit_deny_remark(handler: CmdHandler) -> Vec<CommandNode> {
    vec![
        keyword("deny", "Specify packets to reject")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Access list entry")
                    .handler(handler),
            ]),
        keyword("permit", "Specify packets to forward")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Access list entry")
                    .handler(handler),
            ]),
        keyword("remark", "Access list entry comment")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Comment text")
                    .handler(handler),
            ]),
    ]
}

// ─── Tree ─────────────────────────────────────────────────────────────────────

static CONF_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

pub fn conf_tree() -> &'static Vec<CommandNode> {
    CONF_TREE.get_or_init(build_conf_tree)
}

fn build_conf_tree() -> Vec<CommandNode> {
    let mut main_commands: Vec<CommandNode> = vec![
        // hostname <name>  — positive form requires argument; "no hostname" resets to default
        keyword("hostname", "Set system's network name")
            .mode(config_only())
            .no_handler(handle_hostname)
            .children(vec![
                param("<name>", ParamType::Word, "Hostname string")
                    .handler(handle_hostname),
            ]),

        // interface <type> <number>  [config only — enters config-if]
        // Keywords use proper case (matching real IOS help output).
        // find_matches() lowercases both sides, so "int gi 0/0" still works.
        keyword("interface", "Select an interface to configure")
            .mode(config_only())
            .children(vec![
                keyword("GigabitEthernet", "GigabitEthernet IEEE 802.3z")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("FastEthernet", "FastEthernet IEEE 802.3")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("TenGigabitEthernet", "Ten Gigabit Ethernet")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("HundredGigE", "Hundred Gigabit Ethernet")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("Loopback", "Loopback interface")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("Vlan", "Catalyst Vlans")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "VLAN interface number")
                            .handler(handle_interface),
                    ]),
                keyword("Tunnel", "Tunnel interface")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Tunnel interface number")
                            .handler(handle_interface),
                    ]),
                keyword("Serial", "Serial interface")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("Mgmt", "Management interface")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("Null", "Null interface")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                keyword("Port-channel", "Ethernet Channel of interfaces")
                    .children(vec![
                        param("<number>", ParamType::RestOfLine, "Interface number")
                            .handler(handle_interface),
                    ]),
                // Fallback: accept concatenated type+number like "GigabitEthernet1/0/1"
                param("<name>", ParamType::RestOfLine, "Interface name")
                    .handler(handle_interface),
            ]),

        // router ospf/bgp/eigrp  [config only]
        keyword("router", "Enable a routing process")
            .mode(config_only())
            .children(vec![
                keyword("ospf", "OSPF routing")
                    .children(vec![
                        param("<process-id>", ParamType::Number, "Process ID")
                            .handler(handle_router_ospf),
                    ]),
                keyword("bgp", "BGP routing")
                    .children(vec![
                        param("<as-number>", ParamType::Number, "AS number")
                            .handler(handle_router_bgp),
                    ]),
                keyword("eigrp", "EIGRP routing")
                    .children(vec![
                        param("<as-number>", ParamType::Number, "AS number")
                            .handler(handle_router_eigrp),
                    ]),
            ]),

        // access-list <number> <permit|deny|remark> ... [config only]
        keyword("access-list", "Add an access list entry")
            .mode(config_only())
            .children(vec![
                param("<1-99>", ParamType::NumberRange(1, 99), "IP standard access list")
                    .children(acl_permit_deny_remark(handle_access_list)),
                param("<100-199>", ParamType::NumberRange(100, 199), "IP extended access list")
                    .children(acl_permit_deny_remark(handle_access_list)),
                param("<1100-1199>", ParamType::NumberRange(1100, 1199), "Extended 48-bit MAC address access list")
                    .children(acl_permit_deny_remark(handle_access_list)),
                param("<1300-1999>", ParamType::NumberRange(1300, 1999), "IP standard access list (expanded range)")
                    .children(acl_permit_deny_remark(handle_access_list)),
                param("<200-299>", ParamType::NumberRange(200, 299), "Protocol type-code access list")
                    .children(acl_permit_deny_remark(handle_access_list)),
                param("<2000-2699>", ParamType::NumberRange(2000, 2699), "IP extended access list (expanded range)")
                    .children(acl_permit_deny_remark(handle_access_list)),
                param("<700-799>", ParamType::NumberRange(700, 799), "48-bit MAC address access list")
                    .children(acl_permit_deny_remark(handle_access_list)),
                keyword("rate-limit", "Simple rate-limit specific access list")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "Rate-limit access list parameters")
                            .handler(handle_access_list),
                    ]),
            ]),

        // ip ...
        keyword("ip", "Global IP configuration subcommands")
            .children(vec![
                // ip route <prefix> <mask> <nexthop>  [config only]
                keyword("route", "Establish static routes")
                    .mode(config_only())
                    .children(vec![
                        param("<prefix>", ParamType::Word, "Destination prefix")
                            .children(vec![
                                param("<mask>", ParamType::Word, "Destination mask")
                                    .children(vec![
                                        param("<nexthop>", ParamType::Word, "Forwarding router address")
                                            .handler(handle_ip_route),
                                    ]),
                            ]),
                    ]),
                // ip address <ip> <mask>  [config-if only]
                keyword("address", "Set the IP address of an interface")
                    .mode(config_if_only())
                    .children(vec![
                        param("<ip>", ParamType::Word, "IP address")
                            .children(vec![
                                param("<mask>", ParamType::Word, "Subnet mask")
                                    .handler(handle_ip_address),
                            ]),
                    ]),
                // ip domain-name <name>
                keyword("domain-name", "Define the default domain name")
                    .mode(config_only())
                    .children(vec![
                        param("<name>", ParamType::Word, "Domain name")
                            .handler(handle_ip_domain_name),
                    ]),
                // ip name-server <ip>
                keyword("name-server", "Specify address of name server")
                    .mode(config_only())
                    .children(vec![
                        param("<ip>", ParamType::Word, "Name server address")
                            .handler(handle_ip_name_server),
                    ]),
                // ip access-list extended|standard <name>  [config only]
                keyword("access-list", "Named access-list")
                    .mode(config_only())
                    .no_handler(handle_no_ip_access_list)
                    .children(vec![
                        keyword("extended", "Extended Access List")
                            .children(vec![
                                param("<name>", ParamType::Word, "Access-list name")
                                    .handler(handle_ip_access_list),
                            ]),
                        keyword("standard", "Standard Access List")
                            .children(vec![
                                param("<name>", ParamType::Word, "Access-list name")
                                    .handler(handle_ip_access_list),
                            ]),
                    ]),
            ]),

        // ipv6 ...
        keyword("ipv6", "Global IPv6 configuration subcommands")
            .children(vec![
                // ipv6 unicast-routing [config only]
                keyword("unicast-routing", "Enable IPv6 unicast routing")
                    .mode(config_only())
                    .handler(handle_ipv6_unicast_routing),
                // ipv6 route <prefix/len> <nexthop|interface> [config only]
                keyword("route", "Establish static IPv6 routes")
                    .mode(config_only())
                    .children(vec![
                        param("<prefix/len>", ParamType::Word, "IPv6 prefix/prefix-length")
                            .children(vec![
                                param("<nexthop>", ParamType::Word, "Next-hop IPv6 address or interface")
                                    .handler(handle_ipv6_route),
                            ]),
                    ]),
                // ipv6 router ospf <pid> [config only]
                keyword("router", "IPv6 router")
                    .mode(config_only())
                    .children(vec![
                        keyword("ospf", "OSPFv3")
                            .children(vec![
                                param("<process-id>", ParamType::Number, "Process ID")
                                    .handler(handle_ipv6_router_ospf),
                            ]),
                    ]),
                // ipv6 address <addr/prefix-len> or <addr> link-local [config-if only]
                keyword("address", "Set the IPv6 address of an interface")
                    .mode(config_if_only())
                    .handler(handle_ipv6_address)
                    .children(vec![
                        param("<addr/prefix-len>", ParamType::Word, "IPv6 address with prefix length (X:X:X:X::X/<0-128>)")
                            .handler(handle_ipv6_address)
                            .children(vec![
                                keyword("link-local", "Use as link-local address")
                                    .handler(handle_ipv6_address),
                            ]),
                    ]),
                // ipv6 enable [config-if only]
                keyword("enable", "Enable IPv6 on interface")
                    .mode(config_if_only())
                    .handler(handle_ipv6_enable),
                // ipv6 ospf <pid> area <area> [config-if only]
                keyword("ospf", "OSPFv3 interface commands")
                    .mode(config_if_only())
                    .children(vec![
                        param("<process-id>", ParamType::Number, "Process ID")
                            .children(vec![
                                keyword("area", "Set the OSPF area ID")
                                    .children(vec![
                                        param("<area-id>", ParamType::Number, "OSPF area ID as integer")
                                            .handler(handle_ipv6_ospf_interface),
                                    ]),
                            ]),
                        keyword("network", "Network type")
                            .children(vec![
                                keyword("point-to-point", "Specify OSPF point-to-point network")
                                    .handler(handle_ipv6_ospf_network),
                                keyword("broadcast", "Specify OSPF broadcast multi-access network")
                                    .handler(handle_ipv6_ospf_network),
                            ]),
                    ]),
            ]),

        // line vty/console
        keyword("line", "Configure a terminal line")
            .mode(config_only())
            .children(vec![
                keyword("vty", "Virtual terminal")
                    .children(vec![
                        param("<first>", ParamType::Number, "First line number")
                            .children(vec![
                                param("<last>", ParamType::Number, "Last line number")
                                    .handler(handle_line_vty),
                            ]),
                    ]),
                keyword("console", "Primary terminal line")
                    .children(vec![
                        param("<number>", ParamType::Number, "Line number")
                            .handler(handle_line_console),
                    ]),
            ]),

        // enable secret/password
        keyword("enable", "Modify enable password parameters")
            .mode(config_only())
            .children(vec![
                keyword("secret", "Assign the privileged level secret")
                    .children(vec![
                        param("<password>", ParamType::Word, "The secret")
                            .handler(handle_enable_secret),
                    ]),
                keyword("password", "Assign the privileged level password")
                    .children(vec![
                        param("<password>", ParamType::Word, "The password")
                            .handler(handle_enable_password),
                    ]),
            ]),

        // service timestamps/password-encryption/pad
        keyword("service", "Modify use of network based services")
            .mode(config_only())
            .children(vec![
                keyword("timestamps", "Timestamp debug/log messages")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "Timestamps options")
                            .handler(handle_service_timestamps),
                    ]),
                keyword("password-encryption", "Encrypt system passwords")
                    .handler(handle_service_password_encryption as CmdHandler),
                keyword("pad", "PAD commands")
                    .handler(handle_service_pad as CmdHandler),
                param("<rest>", ParamType::RestOfLine, "Service parameters")
                    .handler(handle_rest_of_line),
            ]),

        // logging host/trap/buffered
        keyword("logging", "Modify message logging facilities")
            .mode(config_only())
            .children(vec![
                keyword("host", "Set syslog server address and parameters")
                    .children(vec![
                        param("<hostname-or-ip>", ParamType::Word, "IP address or hostname of the syslog server")
                            .handler(handle_logging_host),
                    ]),
                keyword("trap", "Set syslog server logging level")
                    .children(vec![
                        param("<level>", ParamType::Word, "Logging severity level")
                            .handler(handle_logging_trap),
                    ]),
                keyword("buffered", "Set buffered logging parameters")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "Buffered logging parameters")
                            .handler(handle_logging_buffered),
                    ]),
                param("<rest>", ParamType::RestOfLine, "Logging parameters")
                    .handler(handle_rest_of_line),
            ]),

        // username <rest>
        keyword("username", "Establish User Name Authentication")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Username parameters")
                    .handler(handle_username),
            ]),

        // shutdown  [config-if only]
        keyword("shutdown", "Shutdown the selected interface")
            .mode(config_if_only())
            .handler(handle_shutdown),

        // description <rest>  [config-if only]
        keyword("description", "Interface specific description")
            .mode(config_if_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Description text")
                    .handler(handle_description),
            ]),

        // switchport mode <access|trunk|...> / switchport access vlan <N>  [config-if only]
        keyword("switchport", "Set switching mode characteristics")
            .mode(config_if_only())
            .children(vec![
                keyword("mode", "Set trunking mode of the interface")
                    .children(vec![
                        keyword("access", "Set trunking mode to ACCESS unconditionally")
                            .handler(handle_switchport_mode),
                        keyword("trunk", "Set trunking mode to TRUNK unconditionally")
                            .handler(handle_switchport_mode),
                        param("<mode>", ParamType::Word, "Switchport mode")
                            .handler(handle_switchport_mode),
                    ]),
                keyword("access", "Set access mode characteristics of the interface")
                    .children(vec![
                        keyword("vlan", "Set VLAN when interface is in access mode")
                            .children(vec![
                                param("<vlanid>", ParamType::Word, "VLAN ID of the VLAN when this port is in access mode")
                                    .handler(handle_switchport_access_vlan),
                            ]),
                    ]),
            ]),

        // spanning-tree <rest>
        keyword("spanning-tree", "Spanning Tree Subsystem")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Spanning tree parameters")
                    .handler(handle_spanning_tree),
            ]),

        // vlan <rest>
        keyword("vlan", "VLAN commands")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "VLAN parameters")
                    .handler(handle_vlan),
            ]),

        // boot system <rest>
        keyword("boot", "Modify system boot parameters")
            .mode(config_only())
            .children(vec![
                keyword("system", "Set system image")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "Boot image parameters")
                            .handler(handle_boot_system),
                    ]),
            ]),

        // cdp run
        keyword("cdp", "Global CDP configuration subcommands")
            .mode(config_only())
            .children(vec![
                keyword("run", "Enable CDP")
                    .handler(handle_cdp_run as CmdHandler),
            ]),

        // crypto <rest>
        keyword("crypto", "Encryption module")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Crypto parameters")
                    .handler(handle_crypto),
            ]),

        // do <exec-cmd>  — runs exec command from config mode
        keyword("do", "To run exec commands in config mode")
            .mode(config_only())
            .children(vec![
                param("<exec-cmd>", ParamType::RestOfLine, "Exec command to run")
                    .handler(handle_do_stub),
            ]),

        // errdisable <rest>
        keyword("errdisable", "Error disable")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Errdisable parameters")
                    .handler(handle_errdisable),
            ]),

        // event <rest>
        keyword("event", "Embedded event related commands")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Event parameters")
                    .handler(handle_event),
            ]),

        // mac <rest>
        keyword("mac", "Global MAC configuration subcommands")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "MAC parameters")
                    .handler(handle_mac),
            ]),

        // ntp server <address>
        keyword("ntp", "Configure NTP")
            .mode(config_only())
            .children(vec![
                keyword("server", "Configure NTP server")
                    .children(vec![
                        param("<address>", ParamType::Word, "IP address of NTP server")
                            .handler(handle_ntp_server),
                    ]),
            ]),

        // snmp-server <rest>
        keyword("snmp-server", "Modify SNMP engine parameters")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "SNMP server parameters")
                    .handler(handle_snmp_server),
            ]),

        // banner motd/login/exec
        keyword("banner", "Define a login banner")
            .mode(config_only())
            .children(vec![
                keyword("motd", "Set Message of the Day banner")
                    .children(vec![
                        param("<text>", ParamType::RestOfLine, "Banner text (delimiter char + text + delimiter)")
                            .handler(handle_banner_motd),
                    ]),
                keyword("login", "Set Login banner")
                    .children(vec![
                        param("<text>", ParamType::RestOfLine, "Banner text")
                            .handler(handle_banner_login),
                    ]),
                keyword("exec", "Set EXEC process creation banner")
                    .children(vec![
                        param("<text>", ParamType::RestOfLine, "Banner text")
                            .handler(handle_banner_exec),
                    ]),
            ]),
    ];

    // ── IOS 15.2 stub commands (alphabetical) ────────────────────────────────
    main_commands.insert(0, {
        // aaa — insert at front; we'll sort by pushing in right positions below
        keyword("aaa", "Authentication, Authorization and Accounting.")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "AAA parameters")
                    .handler(handle_aaa),
            ])
    });

    main_commands.push(
        keyword("arp", "Set a static ARP entry")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "ARP parameters")
                    .handler(handle_arp),
            ]),
    );
    main_commands.push(
        keyword("class-map", "Configure CPL Class Map")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Class map parameters")
                    .handler(handle_class_map),
            ]),
    );
    main_commands.push(
        keyword("clock", "Configure time-of-day clock")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Clock parameters")
                    .handler(handle_clock),
            ]),
    );
    main_commands.push(
        keyword("default", "Set a command to its defaults")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Command to set to defaults")
                    .handler(handle_default),
            ]),
    );
    main_commands.push(
        keyword("dot1x", "IEEE 802.1X Global Configuration Commands")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "802.1X parameters")
                    .handler(handle_dot1x),
            ]),
    );
    main_commands.push(
        keyword("lldp", "Global LLDP configuration subcommands")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "LLDP parameters")
                    .handler(handle_lldp),
            ]),
    );
    main_commands.push(
        keyword("monitor", "Monitoring different system events")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Monitor parameters")
                    .handler(handle_monitor),
            ]),
    );
    main_commands.push(
        keyword("policy-map", "Configure Policy Map")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Policy map parameters")
                    .handler(handle_policy_map),
            ]),
    );
    main_commands.push(
        keyword("port-channel", "EtherChannel configuration")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Port-channel parameters")
                    .handler(handle_port_channel),
            ]),
    );
    main_commands.push(
        keyword("power", "Power configure")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Power parameters")
                    .handler(handle_power),
            ]),
    );
    main_commands.push(
        keyword("privilege", "Command privilege parameters")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Privilege parameters")
                    .handler(handle_privilege),
            ]),
    );
    main_commands.push(
        keyword("tacacs-server", "Modify TACACS query parameters")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "TACACS server parameters")
                    .handler(handle_tacacs_server),
            ]),
    );

    main_commands.push(
        keyword("no", "Negate a command or set its defaults"),
    );
    main_commands.push(
        keyword("help", "Description of the interactive help system")
            .handler(crate::cmd_tree_exec::handle_help_command),
    );
    main_commands.push(
        keyword("exit", "Exit from current mode")
            .handler(handle_config_exit),
    );
    main_commands.push(
        keyword("end", "Exit to privileged EXEC mode")
            .handler(handle_config_end),
    );

    main_commands
}

// ─── Sub-mode trees ───────────────────────────────────────────────────────────

static CONFIG_IF_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

/// Command tree for config-if sub-mode (interface configuration).
pub fn config_if_tree() -> &'static Vec<CommandNode> {
    CONFIG_IF_TREE.get_or_init(build_config_if_tree)
}

fn build_config_if_tree() -> Vec<CommandNode> {
    let mut main_commands: Vec<CommandNode> = vec![
        // ip address <ip> <mask>  (bare "address" handler for "no ip address")
        keyword("ip", "IP configuration subcommands")
            .children(vec![
                keyword("address", "Set the IP address of an interface")
                    .handler(handle_ip_address)
                    .children(vec![
                        param("<ip>", ParamType::Word, "IP address")
                            .children(vec![
                                param("<mask>", ParamType::Word, "Subnet mask")
                                    .handler(handle_ip_address),
                            ]),
                    ]),
            ]),

        // ipv6 (interface config)
        keyword("ipv6", "IPv6 interface subcommands")
            .children(vec![
                keyword("address", "Set the IPv6 address of an interface")
                    .handler(handle_ipv6_address)
                    .children(vec![
                        param("<addr/prefix-len>", ParamType::Word, "IPv6 prefix (X:X:X:X::X/<0-128>)")
                            .handler(handle_ipv6_address)
                            .children(vec![
                                keyword("link-local", "Use as link-local address")
                                    .handler(handle_ipv6_address),
                            ]),
                    ]),
                keyword("enable", "Enable IPv6 on interface")
                    .handler(handle_ipv6_enable),
                keyword("ospf", "OSPFv3 interface commands")
                    .children(vec![
                        param("<process-id>", ParamType::Number, "Process ID")
                            .children(vec![
                                keyword("area", "Set the OSPF area ID")
                                    .children(vec![
                                        param("<area-id>", ParamType::Number, "OSPF area ID as integer")
                                            .handler(handle_ipv6_ospf_interface),
                                    ]),
                            ]),
                        keyword("network", "Network type")
                            .children(vec![
                                keyword("point-to-point", "Specify OSPF point-to-point network")
                                    .handler(handle_ipv6_ospf_network),
                                keyword("broadcast", "Specify OSPF broadcast multi-access network")
                                    .handler(handle_ipv6_ospf_network),
                            ]),
                    ]),
            ]),

        // shutdown
        keyword("shutdown", "Shutdown the selected interface")
            .handler(handle_shutdown),

        // description <rest>  (also handle bare "description" for "no description")
        keyword("description", "Interface specific description")
            .handler(handle_description)
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Description text")
                    .handler(handle_description),
            ]),

        // switchport mode <access|trunk|...> / switchport access vlan <N>
        keyword("switchport", "Set switching mode characteristics")
            .children(vec![
                keyword("mode", "Set trunking mode of the interface")
                    .children(vec![
                        keyword("access", "Set trunking mode to ACCESS unconditionally")
                            .handler(handle_switchport_mode),
                        keyword("trunk", "Set trunking mode to TRUNK unconditionally")
                            .handler(handle_switchport_mode),
                        param("<mode>", ParamType::Word, "Switchport mode")
                            .handler(handle_switchport_mode),
                    ]),
                keyword("access", "Set access mode characteristics of the interface")
                    .children(vec![
                        keyword("vlan", "Set VLAN when interface is in access mode")
                            .children(vec![
                                param("<vlanid>", ParamType::Word, "VLAN ID of the VLAN when this port is in access mode")
                                    .handler(handle_switchport_access_vlan),
                            ]),
                    ]),
                keyword("nonegotiate", "Device will not engage in negotiation protocol on this interface")
                    .handler(handle_config_sub_rest as CmdHandler),
                keyword("port-security", "Security related command")
                    .handler(handle_config_sub_rest as CmdHandler)
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "Port security parameters")
                            .handler(handle_config_sub_rest),
                    ]),
                keyword("trunk", "Set trunking characteristics of the interface")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "Trunk parameters")
                            .handler(handle_config_sub_rest),
                    ]),
            ]),

        // spanning-tree portfast|bpduguard|bpdufilter|link-type|guard|<rest>
        keyword("spanning-tree", "Spanning Tree Subsystem")
            .children(vec![
                keyword("portfast", "Enable portfast on this interface")
                    .handler(handle_config_sub_rest as CmdHandler)
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "portfast options")
                            .handler(handle_config_sub_rest),
                    ]),
                keyword("bpduguard", "Don't accept BPDUs on this interface")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "bpduguard options")
                            .handler(handle_config_sub_rest),
                    ]),
                keyword("bpdufilter", "Don't send or receive BPDUs on this interface")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "bpdufilter options")
                            .handler(handle_config_sub_rest),
                    ]),
                keyword("link-type", "Specify a link type for spanning tree")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "link type options")
                            .handler(handle_config_sub_rest),
                    ]),
                keyword("guard", "Change an interface's spanning tree guard mode")
                    .children(vec![
                        param("<rest>", ParamType::RestOfLine, "guard options")
                            .handler(handle_config_sub_rest),
                    ]),
                param("<rest>", ParamType::RestOfLine, "Spanning tree parameters")
                    .handler(handle_config_sub_rest),
            ]),

        keyword("speed", "Configure speed operation of the interface")
            .handler(handle_speed)  // for "no speed"
            .children(vec![
                keyword("10", "Force 10 Mbps operation").handler(handle_speed),
                keyword("100", "Force 100 Mbps operation").handler(handle_speed),
                keyword("1000", "Force 1000 Mbps operation").handler(handle_speed),
                keyword("auto", "Enable AUTO speed configuration").handler(handle_speed),
            ]),

        keyword("duplex", "Configure duplex operation")
            .handler(handle_duplex)  // for "no duplex"
            .children(vec![
                keyword("auto", "Enable AUTO duplex configuration").handler(handle_duplex),
                keyword("full", "Force full duplex operation").handler(handle_duplex),
                keyword("half", "Force half duplex operation").handler(handle_duplex),
            ]),

        // storm-control <rest>
        keyword("storm-control", "storm configuration")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Storm control parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // channel-group <rest>
        keyword("channel-group", "EtherChannel configuration")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "EtherChannel parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // power <rest>
        keyword("power", "Power configuration")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Power parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // port-security <rest>
        keyword("port-security", "Port security")
            .handler(handle_config_sub_rest as CmdHandler)
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Port security parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // keepalive (no args needed)
        keyword("keepalive", "Enable keepalive")
            .handler(handle_config_sub_rest as CmdHandler)
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Keepalive options")
                    .handler(handle_config_sub_rest),
            ]),

        // load-interval <rest>
        keyword("load-interval", "Specify interval for load calculation")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Load interval value")
                    .handler(handle_config_sub_rest),
            ]),

        // udld <rest>
        keyword("udld", "UDLD configuration")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "UDLD parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // logging <rest>
        keyword("logging", "Configure logging for interface")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Logging parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // snmp <rest>
        keyword("snmp", "Modify SNMP interface parameters")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "SNMP parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // lldp <rest>
        keyword("lldp", "LLDP interface subcommands")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "LLDP parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // dot1x <rest>
        keyword("dot1x", "Dot1x configuration")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Dot1x parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // mtu <rest>
        keyword("mtu", "Set the interface Maximum Transmission Unit")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "MTU value")
                    .handler(handle_config_sub_rest),
            ]),

        // service-policy <rest>
        keyword("service-policy", "Configure CPL Service Policy")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Service policy parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // carrier-delay <rest>
        keyword("carrier-delay", "Specify delay for interface transitions")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Carrier delay value")
                    .handler(handle_config_sub_rest),
            ]),

        // flowcontrol <rest>
        keyword("flowcontrol", "IEEE 802.3x Flow Control")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Flow control parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // mdix <rest>
        keyword("mdix", "Set MDIX mode")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "MDIX parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // negotiation <rest>
        keyword("negotiation", "Select autonegotiation mode")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Negotiation parameters")
                    .handler(handle_config_sub_rest),
            ]),
    ];

    main_commands.push(
        keyword("no", "Negate a command or set its defaults"),
    );
    main_commands.push(
        keyword("help", "Description of the interactive help system")
            .handler(crate::cmd_tree_exec::handle_help_command),
    );
    main_commands.push(
        keyword("exit", "Exit from current mode")
            .handler(handle_config_exit),
    );
    main_commands.push(
        keyword("end", "Exit to privileged EXEC mode")
            .handler(handle_config_end),
    );

    main_commands
}

static CONFIG_ROUTER_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

/// Command tree for config-router sub-mode (routing protocol configuration).
pub fn config_router_tree() -> &'static Vec<CommandNode> {
    CONFIG_ROUTER_TREE.get_or_init(build_config_router_tree)
}

fn build_config_router_tree() -> Vec<CommandNode> {
    let mut main_commands: Vec<CommandNode> = vec![
        // network <rest>
        keyword("network", "Enable routing on an IP network")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Network address and wildcard")
                    .handler(handle_config_sub_rest),
            ]),

        // router-id <ip>
        keyword("router-id", "Router ID for this routing process")
            .children(vec![
                param("<ip>", ParamType::Word, "Router ID (IP address)")
                    .handler(handle_config_sub_rest),
            ]),

        // area <rest>
        keyword("area", "OSPF area parameters")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Area parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // redistribute <rest>
        keyword("redistribute", "Redistribute information from another routing protocol")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Redistribution parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // passive-interface <rest>
        keyword("passive-interface", "Suppress routing updates on an interface")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Interface name")
                    .handler(handle_config_sub_rest),
            ]),

        // log-adjacency-changes
        keyword("log-adjacency-changes", "Log changes in adjacency state")
            .handler(handle_config_sub_rest as CmdHandler),

        // neighbor <rest>
        keyword("neighbor", "Specify a neighbor router")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Neighbor parameters")
                    .handler(handle_config_sub_rest),
            ]),
    ];

    main_commands.push(
        keyword("no", "Negate a command or set its defaults"),
    );
    main_commands.push(
        keyword("help", "Description of the interactive help system")
            .handler(crate::cmd_tree_exec::handle_help_command),
    );
    main_commands.push(
        keyword("exit", "Exit from current mode")
            .handler(handle_config_exit),
    );
    main_commands.push(
        keyword("end", "Exit to privileged EXEC mode")
            .handler(handle_config_end),
    );

    main_commands
}

static CONFIG_LINE_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

/// Command tree for config-line sub-mode (line configuration).
pub fn config_line_tree() -> &'static Vec<CommandNode> {
    CONFIG_LINE_TREE.get_or_init(build_config_line_tree)
}

fn build_config_line_tree() -> Vec<CommandNode> {
    let mut main_commands: Vec<CommandNode> = vec![
        // transport <rest>
        keyword("transport", "Define transport protocols for line")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Transport parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // exec-timeout <rest>
        keyword("exec-timeout", "Set the EXEC timeout")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Timeout parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // login <rest>
        keyword("login", "Enable password checking at login")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Login parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // privilege <rest>
        keyword("privilege", "Change privilege level for line")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Privilege parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // logging <rest>
        keyword("logging", "Modify message logging facilities")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Logging parameters")
                    .handler(handle_config_sub_rest),
            ]),

        // length <rest>
        keyword("length", "Set number of lines on a screen")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Number of lines")
                    .handler(handle_config_sub_rest),
            ]),

        // password <rest>
        keyword("password", "Set a password")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Password")
                    .handler(handle_config_sub_rest),
            ]),
    ];

    main_commands.push(
        keyword("no", "Negate a command or set its defaults"),
    );
    main_commands.push(
        keyword("help", "Description of the interactive help system")
            .handler(crate::cmd_tree_exec::handle_help_command),
    );
    main_commands.push(
        keyword("exit", "Exit from current mode")
            .handler(handle_config_exit),
    );
    main_commands.push(
        keyword("end", "Exit to privileged EXEC mode")
            .handler(handle_config_end),
    );

    main_commands
}

static CONFIG_EXT_NACL_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();
static CONFIG_STD_NACL_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

/// Command tree for config-ext-nacl sub-mode (extended named ACL configuration).
pub fn config_ext_nacl_tree() -> &'static Vec<CommandNode> {
    CONFIG_EXT_NACL_TREE.get_or_init(build_config_nacl_tree)
}

/// Command tree for config-std-nacl sub-mode (standard named ACL configuration).
pub fn config_std_nacl_tree() -> &'static Vec<CommandNode> {
    CONFIG_STD_NACL_TREE.get_or_init(build_config_nacl_tree)
}

fn build_config_nacl_tree() -> Vec<CommandNode> {
    let mut main_commands: Vec<CommandNode> = vec![
        // permit <rest>
        keyword("permit", "Specify packets to forward")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Permit conditions")
                    .handler(handle_nacl_entry),
            ]),

        // deny <rest>
        keyword("deny", "Specify packets to reject")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Deny conditions")
                    .handler(handle_nacl_entry),
            ]),

        // remark <rest>
        keyword("remark", "Access list entry comment")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Comment text")
                    .handler(handle_nacl_entry),
            ]),
    ];

    main_commands.push(
        keyword("no", "Negate a command or set its defaults"),
    );
    main_commands.push(
        keyword("help", "Description of the interactive help system")
            .handler(crate::cmd_tree_exec::handle_help_command),
    );
    main_commands.push(
        keyword("exit", "Exit from current mode")
            .handler(handle_config_exit),
    );
    main_commands.push(
        keyword("end", "Exit to privileged EXEC mode")
            .handler(handle_config_end),
    );

    main_commands
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_tree::parse;

    fn make_device() -> MockIosDevice {
        MockIosDevice::new("Router1")
    }

    #[test]
    fn test_conf_tree_builds() {
        let tree = conf_tree();
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_conf_hostname_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("hostname NewRouter", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_interface_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("interface GigabitEthernet 0/0", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_router_ospf_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("router ospf 1", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_ip_route_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("ip route 0.0.0.0 0.0.0.0 10.0.0.1", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_ip_address_config_if_only() {
        let tree = conf_tree();
        // ip address not available in Config (only ConfigSub)
        let result_config = parse("ip address 10.0.0.1 255.255.255.0", tree, &CliMode::Config);
        assert!(
            matches!(result_config, crate::cmd_tree::ParseResult::InvalidInput { .. }),
            "ip address should be invalid in config mode"
        );
        // but available in ConfigSub
        let result_sub = parse(
            "ip address 10.0.0.1 255.255.255.0",
            tree,
            &CliMode::ConfigSub("config-if".to_string()),
        );
        assert!(matches!(result_sub, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_shutdown_config_if_only() {
        let tree = conf_tree();
        let result_config = parse("shutdown", tree, &CliMode::Config);
        assert!(
            matches!(result_config, crate::cmd_tree::ParseResult::InvalidInput { .. }),
            "shutdown should be invalid in config mode"
        );
        let result_sub = parse("shutdown", tree, &CliMode::ConfigSub("config-if".to_string()));
        assert!(matches!(result_sub, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_exit_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("exit", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_end_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("end", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_no_command_parses() {
        // "no shutdown" should parse in config-if mode via parse_for_no
        use crate::cmd_tree::parse_for_no;
        let tree = config_if_tree();
        let mode = CliMode::ConfigSub("config-if".to_string());
        let result = parse_for_no("shutdown", tree, &mode, "no shutdown");
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "no shutdown should parse in config-if mode");
    }

    #[test]
    fn test_conf_no_ip_route_parses() {
        // "no ip route ..." should parse in config mode via parse_for_no
        use crate::cmd_tree::parse_for_no;
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse_for_no(
            "ip route 0.0.0.0 0.0.0.0 10.0.0.1",
            tree,
            &mode,
            "no ip route 0.0.0.0 0.0.0.0 10.0.0.1",
        );
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "no ip route should parse in config mode via parse_for_no");
    }

    #[test]
    fn test_conf_no_hostname_uses_no_handler() {
        // "no hostname" (no argument) should work via no_handler in parse_for_no
        use crate::cmd_tree::parse_for_no;
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse_for_no("hostname", tree, &mode, "no hostname");
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "no hostname should execute via no_handler");
    }

    #[test]
    fn test_conf_no_help_lists_commands() {
        // "no ?" should list commands from the main tree (excluding no/exit/end/help/do)
        use crate::cmd_tree::{help_for_no, HelpResult};
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = help_for_no("", tree, &mode);
        match result {
            HelpResult::Subcommands(subs) => {
                let names: Vec<&str> = subs.iter().map(|(k, _)| k.as_str()).collect();
                assert!(names.contains(&"hostname"), "no ? should list 'hostname'");
                assert!(names.contains(&"ip"), "no ? should list 'ip'");
                // These should NOT appear in no ?
                assert!(!names.contains(&"no"), "no ? should not list 'no'");
                assert!(!names.contains(&"exit"), "no ? should not list 'exit'");
                assert!(!names.contains(&"end"), "no ? should not list 'end'");
                assert!(!names.contains(&"help"), "no ? should not list 'help'");
                assert!(!names.contains(&"do"), "no ? should not list 'do'");
            }
            other => panic!("Expected Subcommands for 'no ?', got {:?}", other),
        }
    }

    #[test]
    fn test_conf_hostname_no_cr_in_help() {
        // "hostname ?" should NOT show <cr> since no_handler means positive form requires arg
        use crate::cmd_tree::{help, HelpResult};
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = help("hostname ", tree, &mode);
        match result {
            HelpResult::Subcommands(subs) => {
                let names: Vec<&str> = subs.iter().map(|(k, _)| k.as_str()).collect();
                assert!(!names.contains(&"<cr>"),
                    "hostname ? should NOT show <cr> (requires argument), got: {:?}", names);
            }
            other => panic!("Expected Subcommands, got {:?}", other),
        }
    }

    #[test]
    fn test_conf_no_hostname_shows_cr_in_help() {
        // "no hostname ?" should show <cr> since no_handler makes it complete without args
        use crate::cmd_tree::{help_for_no, HelpResult};
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = help_for_no("hostname ", tree, &mode);
        match result {
            HelpResult::Subcommands(subs) => {
                let names: Vec<&str> = subs.iter().map(|(k, _)| k.as_str()).collect();
                assert!(names.contains(&"<cr>"),
                    "no hostname ? should show <cr>, got: {:?}", names);
            }
            other => panic!("Expected Subcommands, got {:?}", other),
        }
    }

    #[test]
    fn test_conf_unknown_command_invalid_input() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("bogusconfigcmd", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::InvalidInput { .. }),
            "Unknown config command should give InvalidInput"
        );
    }

    #[test]
    fn test_conf_handler_hostname_updates_hostname() {
        let mut device = make_device();
        handle_hostname(&mut device, "hostname NewRouter");
        assert_eq!(device.hostname, "NewRouter");
    }

    #[test]
    fn test_conf_handler_interface_enters_config_if() {
        let mut device = make_device();
        handle_interface(&mut device, "interface GigabitEthernet0/0");
        assert!(matches!(device.mode, CliMode::ConfigSub(ref s) if s == "config-if"));
    }

    #[test]
    fn test_conf_handler_exit_from_config_goes_to_priv() {
        let mut device = make_device();
        // device starts in PrivExec; put it in Config
        device.mode = CliMode::Config;
        handle_config_exit(&mut device, "exit");
        assert_eq!(device.mode, CliMode::PrivilegedExec);
    }

    #[test]
    fn test_conf_handler_exit_from_config_sub_goes_to_config() {
        let mut device = make_device();
        device.mode = CliMode::ConfigSub("config-if".to_string());
        handle_config_exit(&mut device, "exit");
        assert_eq!(device.mode, CliMode::Config);
    }

    #[test]
    fn test_conf_handler_end_goes_to_priv() {
        let mut device = make_device();
        device.mode = CliMode::ConfigSub("config-if".to_string());
        handle_config_end(&mut device, "end");
        assert_eq!(device.mode, CliMode::PrivilegedExec);
    }

    #[test]
    fn test_conf_service_rest_of_line() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("service timestamps debug uptime", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_conf_spanning_tree_rest_of_line() {
        let tree = conf_tree();
        let mode = CliMode::ConfigSub("config-if".to_string());
        let result = parse("spanning-tree portfast", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_normalize_concatenated_gi() {
        assert_eq!(normalize_interface_name("g1/0/9"), "GigabitEthernet1/0/9");
    }

    #[test]
    fn test_normalize_concatenated_gi_two_letter() {
        assert_eq!(normalize_interface_name("gi1/0/9"), "GigabitEthernet1/0/9");
    }

    #[test]
    fn test_normalize_concatenated_te() {
        assert_eq!(normalize_interface_name("te1/0/1"), "TenGigabitEthernet1/0/1");
    }

    #[test]
    fn test_normalize_concatenated_fa() {
        assert_eq!(normalize_interface_name("fa0/1"), "FastEthernet0/1");
    }

    #[test]
    fn test_normalize_concatenated_lo() {
        assert_eq!(normalize_interface_name("lo0"), "Loopback0");
    }

    #[test]
    fn test_normalize_concatenated_vl() {
        assert_eq!(normalize_interface_name("vl1"), "Vlan1");
    }

    #[test]
    fn test_normalize_full_concatenated_name_passthrough() {
        assert_eq!(normalize_interface_name("GigabitEthernet1/0/9"), "GigabitEthernet1/0/9");
        assert_eq!(normalize_interface_name("Vlan1"), "Vlan1");
        assert_eq!(normalize_interface_name("Loopback0"), "Loopback0");
        assert_eq!(normalize_interface_name("TenGigabitEthernet1/0/1"), "TenGigabitEthernet1/0/1");
    }

    #[test]
    fn test_normalize_space_separated_still_works() {
        assert_eq!(normalize_interface_name("loopback 0"), "Loopback0");
        assert_eq!(normalize_interface_name("vlan 100"), "Vlan100");
        assert_eq!(normalize_interface_name("gi 1/0/9"), "GigabitEthernet1/0/9");
    }

    #[test]
    fn test_normalize_case_insensitive() {
        assert_eq!(normalize_interface_name("GIGABITETHERNET1/0/1"), "GigabitEthernet1/0/1");
        assert_eq!(normalize_interface_name("Gi1/0/1"), "GigabitEthernet1/0/1");
    }

    /// Verify that config mode ? help includes all newly added commands.
    #[test]
    fn test_config_mode_help_includes_new_commands() {
        use crate::cmd_tree::{help, HelpResult};

        let tree = conf_tree();
        let mode = CliMode::Config;

        // Get the top-level help (empty input before ?)
        let result = help("", tree, &mode);

        let keywords: Vec<String> = match result {
            HelpResult::Subcommands(subs) => subs.into_iter().map(|(name, _)| name).collect(),
            other => panic!("Expected Subcommands, got {:?}", other),
        };

        let expected = &[
            "banner",
            "boot",
            "cdp",
            "crypto",
            "do",
            "errdisable",
            "event",
            "logging",
            "mac",
            "ntp",
            "service",
            "snmp-server",
            "username",
        ];

        for &cmd in expected {
            assert!(
                keywords.iter().any(|k| k == cmd),
                "Config mode ? should include '{}', but got: {:?}",
                cmd,
                keywords,
            );
        }
    }

    /// Verify that newly added IOS 15.2 config mode stubs appear in ? help.
    #[test]
    fn test_config_mode_help_includes_ios152_stubs() {
        use crate::cmd_tree::{help, HelpResult};

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = help("", tree, &mode);

        let keywords: Vec<String> = match result {
            HelpResult::Subcommands(subs) => subs.into_iter().map(|(name, _)| name).collect(),
            other => panic!("Expected Subcommands, got {:?}", other),
        };

        let expected_stubs = &[
            "aaa",
            "arp",
            "class-map",
            "clock",
            "default",
            "dot1x",
            "lldp",
            "monitor",
            "policy-map",
            "port-channel",
            "power",
            "privilege",
            "tacacs-server",
        ];

        for &cmd in expected_stubs {
            assert!(
                keywords.iter().any(|k| k == cmd),
                "Config mode ? should include '{}', but got: {:?}",
                cmd,
                keywords,
            );
        }
    }

    /// Verify that the banner command has motd, login, and exec children in help.
    #[test]
    fn test_config_banner_help_has_subcommands() {
        use crate::cmd_tree::{help, HelpResult};

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = help("banner ", tree, &mode);
        let keywords: Vec<String> = match result {
            HelpResult::Subcommands(subs) => subs.into_iter().map(|(name, _)| name).collect(),
            other => panic!("Expected Subcommands for 'banner ', got {:?}", other),
        };

        for &sub in &["motd", "login", "exec"] {
            assert!(
                keywords.iter().any(|k| k == sub),
                "banner ? should include '{}', got: {:?}",
                sub,
                keywords,
            );
        }
    }

    /// Verify logging subcommands (host, trap, buffered) appear in help.
    #[test]
    fn test_config_logging_help_has_subcommands() {
        use crate::cmd_tree::{help, HelpResult};

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = help("logging ", tree, &mode);
        let keywords: Vec<String> = match result {
            HelpResult::Subcommands(subs) => subs.into_iter().map(|(name, _)| name).collect(),
            other => panic!("Expected Subcommands for 'logging ', got {:?}", other),
        };

        for &sub in &["host", "trap", "buffered"] {
            assert!(
                keywords.iter().any(|k| k == sub),
                "logging ? should include '{}', got: {:?}",
                sub,
                keywords,
            );
        }
    }

    /// Verify service subcommands (timestamps, password-encryption, pad) appear in help.
    #[test]
    fn test_config_service_help_has_subcommands() {
        use crate::cmd_tree::{help, HelpResult};

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = help("service ", tree, &mode);
        let keywords: Vec<String> = match result {
            HelpResult::Subcommands(subs) => subs.into_iter().map(|(name, _)| name).collect(),
            other => panic!("Expected Subcommands for 'service ', got {:?}", other),
        };

        for &sub in &["timestamps", "password-encryption", "pad"] {
            assert!(
                keywords.iter().any(|k| k == sub),
                "service ? should include '{}', got: {:?}",
                sub,
                keywords,
            );
        }
    }

    /// Verify that the new stub commands parse correctly in config mode.
    #[test]
    fn test_new_config_commands_parse() {
        let tree = conf_tree();
        let mode = CliMode::Config;

        let commands = &[
            "boot system flash:c3750-ipservicesk9-mz.122-55.SE10.bin",
            "cdp run",
            "crypto key generate rsa",
            "errdisable recovery cause all",
            "event manager applet TEST",
            "logging host 10.1.1.1",
            "logging trap informational",
            "logging buffered 64000",
            "mac address-table aging-time 300",
            "ntp server 10.0.0.1",
            "snmp-server community public RO",
            "username admin privilege 15 secret mypassword",
        ];

        for &cmd in commands {
            let result = parse(cmd, tree, &mode);
            assert!(
                matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
                "Command '{}' should parse in config mode",
                cmd,
            );
        }
    }

    /// Verify that `access-list ?` shows numbered range params, not `<rest>`.
    #[test]
    fn test_access_list_help_shows_ranges_not_rest() {
        use crate::cmd_tree::{help, HelpResult};

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = help("access-list ", tree, &mode);
        let names: Vec<String> = match result {
            HelpResult::Subcommands(subs) => subs.into_iter().map(|(name, _)| name).collect(),
            other => panic!("Expected Subcommands for 'access-list ', got {:?}", other),
        };

        // Must include the numeric range params
        for &expected in &["<1-99>", "<100-199>", "<1300-1999>", "<2000-2699>", "rate-limit"] {
            assert!(
                names.iter().any(|n| n == expected),
                "access-list ? should include '{}', got: {:?}",
                expected,
                names,
            );
        }

        // Must NOT include the old catch-all <rest>
        assert!(
            !names.iter().any(|n| n == "<rest>"),
            "access-list ? must not show '<rest>', got: {:?}",
            names,
        );
    }

    /// Verify that `access-list 1 ?` shows permit/deny/remark.
    #[test]
    fn test_access_list_number_help_shows_actions() {
        use crate::cmd_tree::{help, HelpResult};

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = help("access-list 1 ", tree, &mode);
        let names: Vec<String> = match result {
            HelpResult::Subcommands(subs) => subs.into_iter().map(|(name, _)| name).collect(),
            other => panic!("Expected Subcommands for 'access-list 1 ', got {:?}", other),
        };

        for &expected in &["deny", "permit", "remark"] {
            assert!(
                names.iter().any(|n| n == expected),
                "access-list 1 ? should include '{}', got: {:?}",
                expected,
                names,
            );
        }
    }

    /// Verify that `access-list 10 permit any` executes correctly.
    #[test]
    fn test_access_list_standard_permit_parses() {
        use crate::cmd_tree::parse;

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = parse("access-list 10 permit any", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "access-list 10 permit any should parse as Execute",
        );
    }

    /// Verify that `access-list 100 permit ip any any` executes correctly.
    #[test]
    fn test_access_list_extended_permit_parses() {
        use crate::cmd_tree::parse;

        let tree = conf_tree();
        let mode = CliMode::Config;

        let result = parse("access-list 100 permit ip any any", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "access-list 100 permit ip any any should parse as Execute",
        );
    }

    // --- speed / duplex handler tests ---

    /// Helper: set up a device in config-if mode on GigabitEthernet1/0/1.
    fn make_device_on_gi1() -> MockIosDevice {
        let mut d = make_device();
        handle_interface(&mut d, "interface GigabitEthernet1/0/1");
        d
    }

    #[test]
    fn test_speed_100_sets_interface_speed() {
        let mut d = make_device_on_gi1();
        handle_speed(&mut d, "speed 100");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.speed, "100", "speed should be 100 after speed 100");
    }

    #[test]
    fn test_speed_1000_sets_interface_speed() {
        let mut d = make_device_on_gi1();
        handle_speed(&mut d, "speed 1000");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.speed, "1000");
    }

    #[test]
    fn test_duplex_full_sets_interface_duplex() {
        let mut d = make_device_on_gi1();
        handle_duplex(&mut d, "duplex full");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.duplex, "full", "duplex should be full after duplex full");
    }

    #[test]
    fn test_duplex_half_sets_interface_duplex() {
        let mut d = make_device_on_gi1();
        handle_duplex(&mut d, "duplex half");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.duplex, "half");
    }

    #[test]
    fn test_no_speed_resets_to_auto() {
        let mut d = make_device_on_gi1();
        handle_speed(&mut d, "speed 100");
        handle_speed(&mut d, "no speed");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.speed, "auto", "no speed should reset to auto");
    }

    #[test]
    fn test_no_duplex_resets_to_auto() {
        let mut d = make_device_on_gi1();
        handle_duplex(&mut d, "duplex full");
        handle_duplex(&mut d, "no duplex");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        assert_eq!(iface.duplex, "auto", "no duplex should reset to auto");
    }

    #[test]
    fn test_speed_values_appear_in_show_interfaces() {
        let mut d = make_device_on_gi1();
        handle_speed(&mut d, "speed 100");
        handle_duplex(&mut d, "duplex full");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        let output = iface.generate_show_interface();
        assert!(output.contains("100Mb/s"), "show interfaces should show 100Mb/s, got: {}", output);
        assert!(output.contains("Full-duplex"), "show interfaces should show Full-duplex, got: {}", output);
    }

    #[test]
    fn test_auto_speed_duplex_appear_in_show_interfaces() {
        let mut d = make_device_on_gi1();
        handle_speed(&mut d, "no speed");
        handle_duplex(&mut d, "no duplex");
        let iface = d.state.get_interface("GigabitEthernet1/0/1").unwrap();
        let output = iface.generate_show_interface();
        assert!(output.contains("Auto-speed"), "show interfaces should show Auto-speed, got: {}", output);
        assert!(output.contains("Auto-duplex"), "show interfaces should show Auto-duplex, got: {}", output);
    }

    #[test]
    fn test_speed_tree_parses_in_config_if() {
        let tree = config_if_tree();
        let mode = CliMode::ConfigSub("config-if".to_string());
        for cmd in &["speed 10", "speed 100", "speed 1000", "speed auto"] {
            let result = parse(cmd, tree, &mode);
            assert!(
                matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
                "{} should parse as Execute in config-if", cmd
            );
        }
    }

    #[test]
    fn test_duplex_tree_parses_in_config_if() {
        let tree = config_if_tree();
        let mode = CliMode::ConfigSub("config-if".to_string());
        for cmd in &["duplex auto", "duplex full", "duplex half"] {
            let result = parse(cmd, tree, &mode);
            assert!(
                matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
                "{} should parse as Execute in config-if", cmd
            );
        }
    }

    // ─── Named ACL tests ─────────────────────────────────────────────────────

    #[test]
    fn test_ip_access_list_extended_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("ip access-list extended MY_ACL", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "ip access-list extended MY_ACL should parse as Execute"
        );
    }

    #[test]
    fn test_ip_access_list_standard_parses() {
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("ip access-list standard MY_STD_ACL", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "ip access-list standard MY_STD_ACL should parse as Execute"
        );
    }

    #[test]
    fn test_ip_access_list_extended_enters_ext_nacl_mode() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        assert!(
            matches!(&device.mode, CliMode::ConfigSub(s) if s == "config-ext-nacl"),
            "Should enter config-ext-nacl mode, got {:?}", device.mode
        );
        assert_eq!(device.current_acl_name, Some("MY_ACL".to_string()));
    }

    #[test]
    fn test_ip_access_list_standard_enters_std_nacl_mode() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list standard MY_STD");
        assert!(
            matches!(&device.mode, CliMode::ConfigSub(s) if s == "config-std-nacl"),
            "Should enter config-std-nacl mode, got {:?}", device.mode
        );
        assert_eq!(device.current_acl_name, Some("MY_STD".to_string()));
    }

    #[test]
    fn test_ip_access_list_creates_acl_in_state() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        assert!(
            device.state.access_lists.iter().any(|a| a.name == "MY_ACL" && a.acl_type == "Extended"),
            "ACL MY_ACL should be created in state"
        );
    }

    #[test]
    fn test_nacl_permit_entry_added() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        handle_nacl_entry(&mut device, "permit ip any any");
        let acl = device.state.access_lists.iter().find(|a| a.name == "MY_ACL").expect("ACL not found");
        assert_eq!(acl.entries.len(), 1);
        assert_eq!(acl.entries[0].action, "permit");
        assert_eq!(acl.entries[0].protocol, "ip");
        assert_eq!(acl.entries[0].source, "any");
        assert_eq!(acl.entries[0].destination, "any");
    }

    #[test]
    fn test_nacl_deny_entry_added() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        handle_nacl_entry(&mut device, "deny tcp host 1.2.3.4 any");
        let acl = device.state.access_lists.iter().find(|a| a.name == "MY_ACL").expect("ACL not found");
        assert_eq!(acl.entries.len(), 1);
        assert_eq!(acl.entries[0].action, "deny");
        assert_eq!(acl.entries[0].protocol, "tcp");
        assert_eq!(acl.entries[0].source, "host");
        assert_eq!(acl.entries[0].destination, "1.2.3.4");
    }

    #[test]
    fn test_nacl_remark_entry_added() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        handle_nacl_entry(&mut device, "remark This is a test");
        let acl = device.state.access_lists.iter().find(|a| a.name == "MY_ACL").expect("ACL not found");
        assert_eq!(acl.entries.len(), 1);
        assert_eq!(acl.entries[0].action, "remark");
    }

    #[test]
    fn test_nacl_exit_returns_to_config() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        assert!(matches!(&device.mode, CliMode::ConfigSub(_)));
        handle_config_exit(&mut device, "exit");
        assert_eq!(device.mode, CliMode::Config);
    }

    #[test]
    fn test_no_ip_access_list_removes_acl() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        handle_nacl_entry(&mut device, "permit ip any any");
        handle_config_exit(&mut device, "exit");
        assert!(device.state.access_lists.iter().any(|a| a.name == "MY_ACL"));
        handle_no_ip_access_list(&mut device, "no ip access-list extended MY_ACL");
        assert!(
            !device.state.access_lists.iter().any(|a| a.name == "MY_ACL"),
            "ACL should be removed after 'no ip access-list extended MY_ACL'"
        );
    }

    #[test]
    fn test_nacl_permit_parses_in_ext_nacl_mode() {
        let tree = config_ext_nacl_tree();
        let mode = CliMode::ConfigSub("config-ext-nacl".to_string());
        let result = parse("permit ip any any", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "permit ip any any should parse in config-ext-nacl mode"
        );
    }

    #[test]
    fn test_nacl_deny_parses_in_ext_nacl_mode() {
        let tree = config_ext_nacl_tree();
        let mode = CliMode::ConfigSub("config-ext-nacl".to_string());
        let result = parse("deny tcp host 1.2.3.4 any eq 80", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "deny should parse in config-ext-nacl mode"
        );
    }

    #[test]
    fn test_nacl_permit_parses_in_std_nacl_mode() {
        let tree = config_std_nacl_tree();
        let mode = CliMode::ConfigSub("config-std-nacl".to_string());
        let result = parse("permit 10.0.0.0 0.0.0.255", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "permit should parse in config-std-nacl mode"
        );
    }

    #[test]
    fn test_named_acl_in_running_config() {
        let mut device = make_device();
        device.mode = CliMode::Config;
        handle_ip_access_list(&mut device, "ip access-list extended MY_ACL");
        handle_nacl_entry(&mut device, "permit ip any any");
        handle_config_exit(&mut device, "exit");
        let rc = device.state.generate_running_config();
        assert!(rc.contains("ip access-list extended MY_ACL"), "running-config should contain named ACL header");
        assert!(rc.contains(" permit ip any any"), "running-config should contain permit entry");
    }

}
