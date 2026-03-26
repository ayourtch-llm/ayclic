//! Config-mode command tree definitions and handlers for MockIOS.

use std::net::Ipv4Addr;
use std::sync::OnceLock;

use crate::cmd_tree::{keyword, param, CliModeClass, CommandNode, ModeFilter, ParamType};
use crate::device_state::StaticRoute;
use crate::{CliMode, MockIosDevice};

// ─── Mode helpers ─────────────────────────────────────────────────────────────

fn config_only() -> ModeFilter {
    ModeFilter::Only(vec![CliModeClass::Config])
}

fn config_if_only() -> ModeFilter {
    ModeFilter::Only(vec![CliModeClass::ConfigSub])
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub fn handle_hostname(d: &mut MockIosDevice, input: &str) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() >= 2 {
        let name = parts[1].to_string();
        d.hostname = name.clone();
        d.state.hostname = name;
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
    // Parse "ip route <prefix> <mask> <nexthop>"
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() >= 5 {
        if let (Ok(prefix), Ok(mask), Ok(next_hop)) = (
            parts[2].parse::<Ipv4Addr>(),
            parts[3].parse::<Ipv4Addr>(),
            parts[4].parse::<Ipv4Addr>(),
        ) {
            d.state.static_routes.push(StaticRoute {
                prefix,
                mask,
                next_hop: Some(next_hop),
                interface: None,
                admin_distance: 1,
            });
        }
    }
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_ip_address(d: &mut MockIosDevice, input: &str) {
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

pub fn handle_no(d: &mut MockIosDevice, input: &str) {
    let trimmed = input.trim();
    if trimmed == "no shutdown" {
        // no shutdown in config-if — bring interface up
        if let Some(ref iface_name) = d.current_interface.clone() {
            if let Some(iface) = d.state.get_interface_mut(iface_name) {
                iface.admin_up = true;
            }
        }
    } else if trimmed.starts_with("no ip route ") {
        // no ip route <prefix> <mask> <nexthop> — remove matching static route
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        // parts: ["no", "ip", "route", prefix, mask, nexthop]
        if parts.len() >= 6 {
            if let (Ok(prefix), Ok(mask), Ok(next_hop)) = (
                parts[3].parse::<Ipv4Addr>(),
                parts[4].parse::<Ipv4Addr>(),
                parts[5].parse::<Ipv4Addr>(),
            ) {
                d.state.static_routes.retain(|r| {
                    !(r.prefix == prefix
                        && r.mask == mask
                        && r.next_hop == Some(next_hop))
                });
            }
        }
    }
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
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_enable_password(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_rest_of_line(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(input.to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_shutdown(d: &mut MockIosDevice, _input: &str) {
    // Update state for current interface
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            iface.admin_up = false;
        }
    }
    d.running_config.push(" shutdown".to_string());
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_description(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(format!(" {}", input.trim()));
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_switchport(d: &mut MockIosDevice, input: &str) {
    d.running_config.push(format!(" {}", input.trim()));
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

// ─── Tree ─────────────────────────────────────────────────────────────────────

static CONF_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

pub fn conf_tree() -> &'static Vec<CommandNode> {
    CONF_TREE.get_or_init(build_conf_tree)
}

fn build_conf_tree() -> Vec<CommandNode> {
    vec![
        // hostname <name>
        keyword("hostname", "Set system's network name")
            .mode(config_only())
            .children(vec![
                param("<name>", ParamType::Word, "Hostname string")
                    .handler(handle_hostname),
            ]),

        // interface <name>  [config only — enters config-if]
        // Use RestOfLine so multi-word names like "loopback 0" or "vlan 100" are captured whole.
        keyword("interface", "Select an interface to configure")
            .mode(config_only())
            .children(vec![
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

        // no <rest-of-line>
        keyword("no", "Negate a command or set its defaults")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Command to negate")
                    .handler(handle_no),
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

        // switchport <rest>  [config-if only]
        keyword("switchport", "Set switching mode characteristics")
            .mode(config_if_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Switchport parameters")
                    .handler(handle_switchport),
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

        // exit
        keyword("exit", "Exit from current mode")
            .handler(handle_config_exit),

        // end
        keyword("end", "Exit to privileged EXEC mode")
            .handler(handle_config_end),
    ]
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
        let result = parse("interface GigabitEthernet0/0", tree, &mode);
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
        let tree = conf_tree();
        let mode = CliMode::Config;
        let result = parse("no shutdown", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
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
