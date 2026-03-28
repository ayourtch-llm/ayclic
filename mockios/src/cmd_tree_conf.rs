//! Config-mode command tree definitions and handlers for MockIOS.

use std::net::Ipv4Addr;
use std::sync::OnceLock;

use crate::cmd_tree::{keyword, param, CliModeClass, CmdHandler, CommandNode, ModeFilter, ParamType};
use crate::device_state::{AccessList, AccessListEntry, StaticRoute};
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
    d.queue_output(&format!("\n{}", p));
}

/// Normalize IOS interface names, e.g. "loopback 0" → "Loopback0", "vlan 100" → "Vlan100".
pub fn normalize_interface_name(input: &str) -> String {
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
    let (type_part, num_part) = match parts.as_slice() {
        [t, n] => (*t, n.trim().to_string()),
        [t] => (*t, String::new()),
        _ => return trimmed.to_string(),
    };

    let type_lower = type_part.to_lowercase();
    let (canonical_type, separator) = match type_lower.as_str() {
        t if t.starts_with("lo") => ("Loopback", ""),
        t if t.starts_with("vl") => ("Vlan", ""),
        t if t.starts_with("gi") => ("GigabitEthernet", ""),
        t if t.starts_with("fa") => ("FastEthernet", ""),
        t if t.starts_with("te") => ("TenGigabitEthernet", ""),
        t if t.starts_with("hu") => ("HundredGigE", ""),
        t if t.starts_with("mg") => ("Mgmt", ""),
        t if t.starts_with("se") => ("Serial", ""),
        t if t.starts_with("tu") => ("Tunnel", ""),
        _ => (type_part, " "),
    };

    // If the type_part itself already contained a number suffix (e.g. "GigabitEthernet0/0"),
    // use it as-is and ignore num_part.
    let type_has_trailing_digit = type_part.chars().last().map_or(false, |c| c.is_ascii_digit() || c == '/');
    if type_has_trailing_digit && num_part.is_empty() {
        return format!("{}", type_part);
    }
    if type_has_trailing_digit {
        // Already fully specified like "GigabitEthernet0/0" with extra tokens — unlikely but safe
        return format!("{} {}", type_part, num_part);
    }

    if num_part.is_empty() {
        canonical_type.to_string()
    } else {
        format!("{}{}{}", canonical_type, separator, num_part)
    }
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
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_router_ospf(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-router".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_router_bgp(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-router".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_router_eigrp(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-router".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
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
    d.queue_output(&format!("\n{}", p));
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
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_ip_domain_name(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_ip_name_server(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}


pub fn handle_line_vty(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-line".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_line_console(d: &mut MockIosDevice, input: &str) {
    d.mode = CliMode::ConfigSub("config-line".to_string());
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_enable_secret(d: &mut MockIosDevice, input: &str) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    // input can be "enable secret <pw>" or "no enable secret"
    if !input.trim().starts_with("no") && parts.len() >= 3 {
        d.state.enable_secret = Some(parts[2..].join(" "));
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_enable_password(d: &mut MockIosDevice, input: &str) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if !input.trim().starts_with("no") && parts.len() >= 3 {
        d.state.enable_secret = Some(parts[2..].join(" "));
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_rest_of_line(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_banner_motd(d: &mut MockIosDevice, input: &str) {
    if input.trim().starts_with("no") {
        d.state.banner_motd = String::new();
        let p = d.prompt();
        d.queue_output(&format!("\n{}", p));
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
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_shutdown(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            iface.admin_up = negated; // "no shutdown" = up, "shutdown" = down
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
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
    d.queue_output(&format!("\n{}", p));
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
    d.queue_output(&format!("\n{}", p));
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
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_spanning_tree(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_vlan(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_config_exit(d: &mut MockIosDevice, _input: &str) {
    match &d.mode {
        CliMode::Config => d.mode = CliMode::PrivilegedExec,
        CliMode::ConfigSub(_) => d.mode = CliMode::Config,
        _ => {}
    }
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_config_end(d: &mut MockIosDevice, _input: &str) {
    d.mode = CliMode::PrivilegedExec;
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
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
        d.queue_output(&format!("\n% Incomplete command.\n{}", p));
        return;
    }

    let list_num = parts[list_num_idx];
    let list_num_owned = list_num.to_string();

    if negated {
        // Remove the entire ACL by this number
        d.state.access_lists.retain(|a| a.name != list_num_owned);
        let p = d.prompt();
        d.queue_output(&format!("\n{}", p));
        return;
    }

    if parts.len() < action_idx + 1 {
        let p = d.prompt();
        d.queue_output(&format!("\n% Incomplete command.\n{}", p));
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
    d.queue_output(&format!("\n{}", p));
}

/// Generic handler for config-router and config-line commands that stores
/// the raw line as unmodeled config.
pub fn handle_config_sub_rest(d: &mut MockIosDevice, input: &str) {
    d.state.unmodeled_config.push(format!(" {}", input.trim()));
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

// ─── Tree helpers ─────────────────────────────────────────────────────────────

/// Filter out node names that shouldn't be cloned into the "no" subtree.
fn should_exclude_from_no(node: &CommandNode) -> bool {
    if let crate::cmd_tree::TokenMatcher::Keyword(kw) = &node.matcher {
        matches!(kw.as_str(), "no" | "exit" | "end" | "help" | "do")
    } else {
        false
    }
}

/// Build the "no" keyword node whose children are a clone of the provided commands.
fn build_no_node(main_commands: &[CommandNode]) -> CommandNode {
    let no_children: Vec<CommandNode> = main_commands.iter()
        .filter(|n| !should_exclude_from_no(n))
        .cloned()
        .collect();
    keyword("no", "Negate a command or set its defaults")
        .children(no_children)
}

// ─── Tree ─────────────────────────────────────────────────────────────────────

static CONF_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

pub fn conf_tree() -> &'static Vec<CommandNode> {
    CONF_TREE.get_or_init(build_conf_tree)
}

fn build_conf_tree() -> Vec<CommandNode> {
    let mut main_commands: Vec<CommandNode> = vec![
        // hostname <name>  (bare handler for "no hostname")
        keyword("hostname", "Set system's network name")
            .mode(config_only())
            .handler(handle_hostname)
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

        // access-list <number|name> <permit|deny> ... [config only]
        keyword("access-list", "Add an access list entry")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Access list parameters")
                    .handler(handle_access_list),
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

        // service <rest>
        keyword("service", "Modify use of network based services")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Service parameters")
                    .handler(handle_rest_of_line),
            ]),

        // logging <rest>
        keyword("logging", "Modify message logging facilities")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Logging parameters")
                    .handler(handle_rest_of_line),
            ]),

        // username <rest>
        keyword("username", "Establish User Name Authentication")
            .mode(config_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Username parameters")
                    .handler(handle_rest_of_line),
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

        // banner motd <delim><text><delim>
        keyword("banner", "Define a login banner")
            .mode(config_only())
            .children(vec![
                keyword("motd", "Set Message of the Day banner")
                    .children(vec![
                        param("<text>", ParamType::RestOfLine, "Banner text (delimiter char + text + delimiter)")
                            .handler(handle_banner_motd),
                    ]),
            ]),
    ];

    // Build "no" with cloned children (excluding no/exit/end/help/do)
    let no_node = build_no_node(&main_commands);
    main_commands.push(no_node);

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
            ]),

        // spanning-tree <rest>
        keyword("spanning-tree", "Spanning Tree Subsystem")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Spanning tree parameters")
                    .handler(handle_spanning_tree),
            ]),

        // speed <rest>
        keyword("speed", "Configure speed operation of the interface")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Speed value")
                    .handler(handle_config_sub_rest),
            ]),

        // duplex <rest>
        keyword("duplex", "Configure duplex operation")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Duplex mode")
                    .handler(handle_config_sub_rest),
            ]),
    ];

    let no_node = build_no_node(&main_commands);
    main_commands.push(no_node);

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

    let no_node = build_no_node(&main_commands);
    main_commands.push(no_node);

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

    let no_node = build_no_node(&main_commands);
    main_commands.push(no_node);

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
        // "no shutdown" should parse in config-if mode (shutdown is config-if only)
        let tree = config_if_tree();
        let mode = CliMode::ConfigSub("config-if".to_string());
        let result = parse("no shutdown", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "no shutdown should parse in config-if mode");
    }

    #[test]
    fn test_conf_no_ip_route_parses() {
        // "no ip route ..." should parse in config mode
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("no ip route 0.0.0.0 0.0.0.0 10.0.0.1", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "no ip route should parse in config mode");
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
}
