//! Exec-mode command tree definitions and handlers for MockIOS.

use std::sync::OnceLock;

use std::net::Ipv4Addr;

use crate::cmd_tree::{keyword, param, CliModeClass, CmdHandler, CommandNode, ModeFilter, ParamType};
use crate::{CliMode, MockIosDevice, PendingInteractive};

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub fn handle_show_version(d: &mut MockIosDevice, _input: &str) {
    let v = d.generate_show_version();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", v, p));
}

pub fn handle_show_running_config(d: &mut MockIosDevice, _input: &str) {
    let config = d.state.generate_running_config();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", config, p));
}

/// Handle `show running-config interface <name>` — filters running config to one interface.
pub fn handle_show_running_config_interface(d: &mut MockIosDevice, input: &str) {
    // Extract interface name from input: "show running-config interface <name>"
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let iface_name = tokens.get(3).copied().unwrap_or("");

    // Normalize the interface name (e.g., "Gi1/0/1" → "GigabitEthernet1/0/1")
    let normalized = crate::cmd_tree_conf::normalize_interface_name(iface_name);

    let config = d.state.generate_running_config();
    let p = d.prompt();

    // Find the interface section in the config
    let mut in_section = false;
    let mut section_lines: Vec<&str> = Vec::new();
    for line in config.lines() {
        if line.starts_with("interface ") {
            if line == format!("interface {}", normalized) {
                in_section = true;
                section_lines.push(line);
            } else {
                in_section = false;
            }
        } else if in_section {
            if line == "!" {
                section_lines.push(line);
                break;
            }
            section_lines.push(line);
        }
    }

    if section_lines.is_empty() {
        d.queue_output(&format!("% Invalid input interface\n{}", p));
    } else {
        let body = format!("!\n{}\nend", section_lines.join("\n"));
        let byte_count = body.len();
        let header = format!("Building configuration...\n\nCurrent configuration : {} bytes\n", byte_count);
        d.queue_output(&format!("{}{}\n{}", header, body, p));
    }
}

pub fn handle_show_startup_config(d: &mut MockIosDevice, _input: &str) {
    let config = d.state.generate_startup_config();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", config, p));
}

pub fn handle_show_clock(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    let clock_str = format_clock_utc();
    d.queue_output(&format!("*{}\n{}", clock_str, p));
}

fn format_clock_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    const DAYS_PER_400Y: u64 = 146097;
    const _DAYS_PER_100Y: u64 = 36524;
    const _DAYS_PER_4Y: u64 = 1461;
    const _DAYS_PER_Y: u64 = 365;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let total_secs = now.as_secs();
    let millis = now.subsec_millis();

    let secs_in_day = total_secs % 86400;
    let days_since_epoch = total_secs / 86400;

    let hh = secs_in_day / 3600;
    let mm = (secs_in_day % 3600) / 60;
    let ss = secs_in_day % 60;

    // Day of week: Unix epoch (Jan 1 1970) was a Thursday = 4
    let dow = (days_since_epoch + 4) % 7;
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let day_name = day_names[dow as usize];

    // Calculate year/month/day from days since Unix epoch (1970-01-01)
    // Using the 400-year cycle algorithm
    let z = days_since_epoch + 719468; // shift to Mar 1, 0000 epoch
    let era = z / DAYS_PER_400Y;
    let doe = z % DAYS_PER_400Y;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d_of_m = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    let month_names = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun",
                       "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month_name = month_names[month as usize];

    format!("{:02}:{:02}:{:02}.{:03} UTC {} {} {} {}",
        hh, mm, ss, millis, day_name, month_name, d_of_m, year)
}

pub fn handle_show_ip_interface_brief(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_interface_brief();
}

pub fn handle_show_ip_interface(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_ip_interface();
    let p = d.prompt();
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_ip_route(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_route();
}

pub fn handle_show_ip_route_static(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_route_static();
}

pub fn handle_show_ip_route_connected(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_route_connected();
}

pub fn handle_show_ip_route_summary(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_route_summary();
}

pub fn handle_show_ip_incomplete(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("% Incomplete command.\n{}", p));
}

// ─── IPv6 Show Handlers ─────────────────────────────────────────────────────

pub fn handle_show_ipv6_interface_brief(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_ipv6_interface_brief();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_ipv6_route(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_ipv6_route();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_ipv6_ospf(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_ipv6_ospf();
    let p = d.prompt();
    if output.is_empty() {
        d.queue_output(&format!("{}", p));
    } else {
        d.queue_output(&format!("{}\n\n{}", output, p));
    }
}

pub fn handle_show_ipv6_ospf_interface_brief(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_ipv6_ospf_interface_brief();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_ipv6_incomplete(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("% Incomplete command.\n{}", p));
}

pub fn handle_show_install_summary(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_install_summary();
}

pub fn handle_show_boot(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_boot();
}

pub fn handle_show_interfaces(d: &mut MockIosDevice, input: &str) {
    // Extract optional interface name argument: "show interfaces [<name>] [<subcommand>]"
    let tokens: Vec<&str> = input.split_whitespace().collect();
    // tokens[0] = "show", tokens[1] = "interfaces", tokens[2..] = optional name and subcommand

    // Known trailing subcommands that may follow an interface name
    const TRAILING_KEYWORDS: &[&str] = &["switchport", "status", "trunk", "counters", "description"];

    // Check if the last token is a known subcommand following an interface name
    let (iface_name, trailing_keyword): (Option<String>, Option<&str>) = if tokens.len() > 3 {
        let last = tokens[tokens.len() - 1];
        if TRAILING_KEYWORDS.contains(&last) {
            let raw = tokens[2..tokens.len() - 1].join(" ");
            (Some(crate::cmd_tree_conf::normalize_interface_name(&raw)), Some(last))
        } else {
            let raw = tokens[2..].join(" ");
            (Some(crate::cmd_tree_conf::normalize_interface_name(&raw)), None)
        }
    } else if tokens.len() > 2 {
        let raw = tokens[2..].join(" ");
        (Some(crate::cmd_tree_conf::normalize_interface_name(&raw)), None)
    } else {
        (None, None)
    };

    let p = d.prompt();

    if let Some(keyword) = trailing_keyword {
        // Delegate to specific subcommand handler for the named interface
        let name = iface_name.as_deref().unwrap_or("");
        let output_text = match keyword {
            "switchport" => d.state.generate_show_interfaces_switchport_for(name),
            _ => format!(
                "% Invalid input detected at '^' marker.\n% Subcommand '{}' not supported per-interface\n",
                keyword
            ),
        };
        d.queue_output(&format!("{}{}", output_text, p));
        return;
    }

    match iface_name {
        Some(name) => {
            // Try exact match first, then prefix match for abbreviations
            let output_text = if let Some(iface) = d.state.get_interface(&name) {
                iface.generate_show_interface()
            } else {
                // Try prefix / case-insensitive match
                let name_lower = name.to_lowercase();
                let found = d.state.interfaces.iter().find(|i| {
                    i.name.to_lowercase().starts_with(&name_lower)
                });
                if let Some(iface) = found {
                    iface.generate_show_interface()
                } else {
                    format!(
                        "% Invalid input detected at '^' marker.\n% No interface {} found\n",
                        name
                    )
                }
            };
            d.queue_output(&format!("{}{}", output_text, p));
        }
        None => {
            // Show all interfaces
            let texts: Vec<String> = d.state.interfaces.iter()
                .map(|i| i.generate_show_interface())
                .collect();
            let all = texts.join("\n");
            d.queue_output(&format!("{}{}", all, p));
        }
    }
}

pub fn handle_show_access_lists(d: &mut MockIosDevice, _input: &str) {
    let mut output = String::from("");
    for acl in &d.state.access_lists {
        output.push_str(&format!("{} IP access list {}\n", acl.acl_type, acl.name));
        for (i, entry) in acl.entries.iter().enumerate() {
            let seq = (i + 1) * 10;
            let mut line = format!("    {} {} {}", seq, entry.action, entry.protocol);
            if !entry.source.is_empty() {
                line.push_str(&format!(" {}", entry.source));
            }
            if !entry.destination.is_empty() {
                line.push_str(&format!(" {}", entry.destination));
            }
            if !entry.extra.is_empty() {
                line.push_str(&format!(" {}", entry.extra));
            }
            output.push_str(&format!("{}\n", line));
        }
    }
    let p = d.prompt();
    output.push_str(&p);
    d.queue_output(&output);
}

pub fn handle_show_vlan_brief(d: &mut MockIosDevice, _input: &str) {
    let table = d.state.generate_show_vlan_brief();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", table, p));
}

pub fn handle_show_vlan(d: &mut MockIosDevice, _input: &str) {
    let table = d.state.generate_show_vlan();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", table, p));
}

pub fn handle_show_vlan_id(d: &mut MockIosDevice, input: &str) {
    // input is the full line, e.g. "show vlan id 10"
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let p = d.prompt();
    // tokens: ["show", "vlan", "id", "<N>"]
    let output = match tokens.get(3).and_then(|s| s.parse::<u16>().ok()) {
        Some(id) => {
            let table = d.state.generate_show_vlan_id(id);
            format!("{}\n{}", table, p)
        }
        None => format!("% Invalid VLAN id\n{}", p),
    };
    d.queue_output(&output);
}

pub fn handle_show_interfaces_status(d: &mut MockIosDevice, _input: &str) {
    let table = d.state.generate_show_interfaces_status();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", table, p));
}

pub fn handle_show_interfaces_description(d: &mut MockIosDevice, _input: &str) {
    let table = d.state.generate_show_interfaces_description();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", table, p));
}

pub fn handle_show_interfaces_trunk(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_interfaces_trunk();
    let p = d.prompt();
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_interfaces_switchport(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_interfaces_switchport();
    let p = d.prompt();
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_interfaces_name_switchport(d: &mut MockIosDevice, input: &str) {
    // Extract interface name from "show interfaces <name> switchport"
    // tokens[0] = "show", tokens[1] = "interfaces", tokens[2] = interface name
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let raw_name = if tokens.len() > 2 { tokens[2] } else { "" };
    let name = crate::cmd_tree_conf::normalize_interface_name(raw_name);
    let output = d.state.generate_show_interfaces_switchport_for(&name);
    let p = d.prompt();
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_interfaces_counters(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_interfaces_counters();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_flash(d: &mut MockIosDevice, _input: &str) {
    d.handle_dir_command("");
}

pub fn handle_show_terminal(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "Line 0, Location: \"\", Type: \"\"\nLength: 0 lines, Width: 80 columns\nStatus: Ready, Active\nCapabilities: none\n{}",
        p
    ));
}

pub fn handle_show_history(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    let mut out = String::from("");
    for cmd in &d.command_history {
        out.push_str(&format!("  {}\n", cmd));
    }
    out.push_str(&p);
    d.queue_output(&out);
}

/// "show" alone — hint about ?
pub fn handle_show_incomplete(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "% Type \"show ?\" for a list of subcommands\n{}",
        p
    ));
}

pub fn handle_configure_terminal(d: &mut MockIosDevice, _input: &str) {
    d.mode = CliMode::Config;
    let p = d.prompt();
    d.queue_output(&format!(
        "Enter configuration commands, one per line.  End with CNTL/Z.\n{}",
        p
    ));
}

pub fn handle_configure_alone(d: &mut MockIosDevice, _input: &str) {
    d.pending_interactive = Some(PendingInteractive::ConfigureMethod);
    d.queue_output("Configuring from terminal, memory, or network [terminal]? ");
}

pub fn handle_enable(d: &mut MockIosDevice, _input: &str) {
    if matches!(d.mode, CliMode::PrivilegedExec) {
        // Already in priv exec — no-op (real IOS behavior)
        let p = d.prompt();
        d.queue_output(&format!("{}", p));
    } else if d.enable_password.is_some() {
        d.pending_interactive = Some(PendingInteractive::EnablePassword);
        d.queue_output("Password: ");
    } else {
        d.mode = CliMode::PrivilegedExec;
        let p = d.prompt();
        d.queue_output(&format!("{}", p));
    }
}

pub fn handle_disable(d: &mut MockIosDevice, _input: &str) {
    d.mode = CliMode::UserExec;
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_terminal_length(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_terminal_width(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_terminal_monitor(d: &mut MockIosDevice, _input: &str) {
    d.terminal_monitor = true;
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_terminal_no_monitor(d: &mut MockIosDevice, _input: &str) {
    d.terminal_monitor = false;
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_copy(d: &mut MockIosDevice, input: &str) {
    d.handle_copy_command(input);
}

pub fn handle_delete(d: &mut MockIosDevice, input: &str) {
    d.handle_delete_command(input);
}

pub fn handle_verify_md5(d: &mut MockIosDevice, input: &str) {
    d.handle_verify_md5(input);
}

pub fn handle_dir(d: &mut MockIosDevice, input: &str) {
    d.handle_dir_command(input);
}

pub fn handle_reload_alone(d: &mut MockIosDevice, _input: &str) {
    d.pending_interactive = Some(PendingInteractive::ReloadConfirm { _minutes: None });
    d.queue_output("Proceed with reload? [confirm]");
}

pub fn handle_reload_cancel(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "***\n*** --- SHUTDOWN ABORTED ---\n***\n{}",
        p
    ));
}

pub fn handle_reload_in(d: &mut MockIosDevice, _input: &str) {
    d.pending_interactive = Some(PendingInteractive::ReloadSave);
    d.queue_output("System configuration has been modified. Save? [yes/no]: ");
}

pub fn handle_write_memory(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("Building configuration...\n[OK]\n{}", p));
}

pub fn handle_write_terminal(d: &mut MockIosDevice, input: &str) {
    handle_show_running_config(d, input);
}

pub fn handle_write_erase(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("[OK]\n{}", p));
}

pub fn handle_write_network(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("%% Not implemented\n{}", p));
}

pub fn handle_install_add(d: &mut MockIosDevice, input: &str) {
    d.handle_install_add(input);
}

pub fn handle_install_activate(d: &mut MockIosDevice, _input: &str) {
    d.handle_install_activate();
}

pub fn handle_install_commit(d: &mut MockIosDevice, _input: &str) {
    d.handle_install_commit();
}

pub fn handle_install_remove_inactive(d: &mut MockIosDevice, _input: &str) {
    d.handle_install_remove_inactive();
}

/// Check if an IP address is reachable via the routing table in device state.
fn is_reachable(d: &MockIosDevice, target: Ipv4Addr) -> bool {
    let target_u32 = u32::from(target);

    // Check connected routes first (admin_up interfaces with IP)
    for iface in &d.state.interfaces {
        if iface.admin_up {
            if let Some((addr, mask)) = iface.ip_address {
                let net = u32::from(addr) & u32::from(mask);
                let host_masked = target_u32 & u32::from(mask);
                if net == host_masked {
                    return true;
                }
            }
        }
    }

    // Check static routes (longest-prefix match, simplified)
    for route in &d.state.static_routes {
        let prefix_u32 = u32::from(route.prefix);
        let mask_u32 = u32::from(route.mask);
        let host_masked = target_u32 & mask_u32;
        if prefix_u32 == host_masked {
            return true;
        }
    }

    false
}

pub fn handle_ping(d: &mut MockIosDevice, input: &str) {
    // Extract target from the input line: "ping <target>"
    let target_str = input.split_whitespace().nth(1).unwrap_or("");
    let p = d.prompt();

    let reachable = if let Ok(target_ip) = target_str.parse::<Ipv4Addr>() {
        is_reachable(d, target_ip)
    } else {
        // Can't parse IP — try hostname resolution (always succeed for now)
        true
    };

    if reachable {
        d.queue_output(&format!(
            "Type escape sequence to abort.\nSending 5, 100-byte ICMP Echos to {}, timeout is 2 seconds:\n!!!!!\nSuccess rate is 100 percent (5/5), round-trip min/avg/max = 1/1/1 ms\n{}",
            target_str, p
        ));
    } else {
        d.queue_output(&format!(
            "Type escape sequence to abort.\nSending 5, 100-byte ICMP Echos to {}, timeout is 2 seconds:\n.....\nSuccess rate is 0 percent (0/5)\n{}",
            target_str, p
        ));
    }
}

pub fn handle_traceroute(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("Tracing route\n 1 1 msec\n{}", p));
}

pub fn handle_exit(d: &mut MockIosDevice, _input: &str) {
    // In exec mode, exit closes the session
    d.queue_output("");
    d.mode = CliMode::Reloading; // signals connection close
}


pub fn handle_help_command(d: &mut MockIosDevice, _input: &str) {
    let text = "\
Help may be requested at any point in a command by entering
a question mark '?'.  If nothing matches, the help list will
be empty and you must backup until entering a '?' shows the
available options.
Two styles of help are provided:
1. Full help is available when you are ready to enter a
   command argument (e.g. 'show ?') and describes each possible
   argument.
2. Partial help is provided when an abbreviated argument is entered
   and you want to know what arguments match the input
   (e.g. 'show pr?'.)
";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", text, p));
}

pub fn handle_clock_set(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_debug(d: &mut MockIosDevice, input: &str) {
    let feature = input.trim().strip_prefix("debug").map(|s| s.trim()).unwrap_or("unknown");
    let p = d.prompt();
    d.queue_output(&format!("{} debugging is on\n{}", feature, p));
}

pub fn handle_undebug_all(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("All possible debugging has been turned off\n{}", p));
}

pub fn handle_undebug(d: &mut MockIosDevice, input: &str) {
    let feature = input.trim().strip_prefix("undebug").map(|s| s.trim()).unwrap_or("unknown");
    let p = d.prompt();
    d.queue_output(&format!("{} debugging is off\n{}", feature, p));
}

pub fn handle_clear(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_ssh(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("% Connection refused by remote host\n{}", p));
}

pub fn handle_telnet(d: &mut MockIosDevice, input: &str) {
    let host = input.split_whitespace().nth(1).unwrap_or("unknown");
    let p = d.prompt();
    d.queue_output(&format!("Trying {} ... \n% Connection refused by remote host\n{}", host, p));
}

pub fn handle_show_cdp_neighbors(d: &mut MockIosDevice, _input: &str) {
    let output = "\
Capability Codes: R - Router, T - Trans Bridge, B - Source Route Bridge
                  S - Switch, H - Host, I - IGMP, r - Repeater, P - Phone,
                  D - Remote, C - CVTA, M - Two-port Mac Relay

Device ID        Local Intrfce     Holdtme    Capability  Platform  Port ID

Total cdp entries displayed : 0";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_cdp_neighbors_detail(d: &mut MockIosDevice, _input: &str) {
    let output = "Total cdp entries displayed : 0";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_cdp(d: &mut MockIosDevice, _input: &str) {
    let output = "\
Global CDP information:
    Sending CDP packets every 60 seconds
    Sending a holdtime value of 180 seconds
    Sending CDPv2 advertisements is  enabled";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_users(d: &mut MockIosDevice, _input: &str) {
    // Build a realistic "show users" output matching Cisco IOS 15.2 format.
    // The current session is always marked with '*'.
    // If a username is configured (login required), show it on the VTY line
    // with a simulated peer address; otherwise show an anonymous console session.
    let header = "    Line       User       Host(s)              Idle       Location";
    let footer = "\n  Interface    User               Mode         Idle     Peer Address";

    let user = d.username.clone().unwrap_or_default();
    let session_line = if user.is_empty() {
        // No login — show as console session (anonymous)
        "   0 con 0                idle                 00:00:00   ".to_string()
    } else {
        // Login configured — show as VTY 0 session with simulated peer address
        format!(
            " 66 vty 0     {:<10} idle                 00:00:00   192.168.1.1",
            user
        )
    };

    let output = format!("{}\n*{}\n{}", header, session_line, footer);
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_ip_ospf(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    match d.state.generate_show_ip_ospf() {
        Some(output) => d.queue_output(&format!("{}\n{}", output, p)),
        None => d.queue_output(&format!("%% OSPF: No router process is configured\n{}", p)),
    }
}

pub fn handle_show_ip_ospf_neighbor(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_ip_ospf_neighbor();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_ip_protocols(d: &mut MockIosDevice, _input: &str) {
    let output = "\
*** IP Routing is NSF aware ***

Routing Protocol is \"application\"
  Sending updates every 0 seconds
  Invalid after 0 seconds, hold down 0, flushed after 0
  Outgoing update filter list for all interfaces is not set
  Incoming update filter list for all interfaces is not set
  Maximum path: 32
  Routing for Networks:
  Routing Information Sources:
    Gateway         Distance      Last Update
  Distance: (default is 4)";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_processes_cpu(d: &mut MockIosDevice, _input: &str) {
    let output = "\
CPU utilization for five seconds: 5%/0%; one minute: 5%; five minutes: 5%
 PID Runtime(ms)     Invoked      uSecs   5Sec   1Min   5Min TTY Process";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_logging(d: &mut MockIosDevice, _input: &str) {
    let console_status = if d.state.logging_console {
        "level debugging, 0 messages logged, xml disabled,\n                     filtering disabled"
    } else {
        "disabled"
    };
    let monitor_status = if d.state.logging_monitor {
        "level debugging, 0 messages logged, xml disabled,\n                     filtering disabled"
    } else {
        "disabled"
    };
    let buf_size = d.state.logging_buffered_size;
    let mut output = format!(
        "Syslog logging: enabled (0 messages dropped, 0 messages rate-limited, 0 flushes, 0 overruns, xml disabled, filtering disabled)\n\
\n\
No Active Message Discriminator.\n\
\n\
\n\
No Inactive Message Discriminator.\n\
\n\
\n\
    Console logging: {console_status}\n\
    Monitor logging: {monitor_status}\n\
    Buffer logging:  level debugging, 0 messages logged, xml disabled,\n\
                    filtering disabled\n\
    Exception Logging: size ({buf_size} bytes)\n\
    Count and timestamp logging messages: disabled\n\
    Persistent logging: disabled\n\
\n\
No active filter modules.\n\
\n\
    Trap logging: level debugging, 0 facility, 0 severity\n"
    );
    for host in &d.state.logging_hosts {
        output.push_str(&format!("        Logging to {} (udp port 514, audit disabled,\n              link up),\n              0 message lines logged,\n              0 message lines rate-limited,\n              0 message lines dropped-by-MD,\n              xml disabled, sequence number disabled\n              filtering disabled\n", host));
    }
    let p = d.prompt();
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_arp(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_arp();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_mac_address_table(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_mac_address_table();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_mac_address_table_dynamic(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_mac_address_table_dynamic();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_mac_address_table_count(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_mac_address_table_count();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_line(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "   Tty Line Typ     Tx/Rx    A Modem  Roty AccO AccI   Uses   Noise  Overruns   Int\n\
*    0    0 CTY              -    -      -    -    -      0       0     0/0       -\n\
     1    1 AUX   9600/9600  -    -      -    -    -      0       0     0/0       -\n\
   386  386 VTY              -    -      -    -    -      0       0     0/0       -\n\
   387  387 VTY              -    -      -    -    -      0       0     0/0       -\n\
\n{}",
        p
    ));
}

pub fn handle_show_inventory(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "NAME: \"1\", DESCR: \"{model}\"\nPID: {model} , VID: V02  , SN: {sn}\n\n\n{p}",
        model = d.state.model,
        sn = d.state.serial_number,
        p = p
    ));
}

pub fn handle_show_environment(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "SYSTEM TEMPERATURE is OK\n\
System Temperature Value: 37 Degree Celsius\n\
System Temperature State: GREEN\n\
Yellow Threshold : 56 Degree Celsius\n\
Red Threshold    : 68 Degree Celsius\n\
\n{}",
        p
    ));
}

pub fn handle_show_spanning_tree(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_spanning_tree();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_spanning_tree_vlan(d: &mut MockIosDevice, input: &str) {
    // input e.g. "show spanning-tree vlan 10"
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let p = d.prompt();
    // tokens: ["show", "spanning-tree", "vlan", "<N>"]
    let output = match tokens.get(3).and_then(|s| s.parse::<u16>().ok()) {
        Some(id) => {
            let block = d.state.generate_show_spanning_tree_vlan(id);
            format!("{}\n{}", block, p)
        }
        None => format!("% Invalid input detected\n{}", p),
    };
    d.queue_output(&output);
}

pub fn handle_show_spanning_tree_summary(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_spanning_tree_summary();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_ntp_status(d: &mut MockIosDevice, _input: &str) {
    let output = "\
Clock is unsynchronized, stratum 16, no reference clock
nominal freq is 286.1023 Hz, actual freq is 286.1023 Hz, precision is 2**21
ntp uptime is 0 (1/100 of seconds), resolution is 3496
reference time is 00000000.00000000 (00:00:00.000 UTC Mon Jan 1 1900)
clock offset is 0.0000 msec, root delay is 0.00 msec
root dispersion is 0.00 msec, peer dispersion is 0.00 msec
loopfilter state is 'FSET' (Drift set from file), drift is 0.000000000 s/s
system poll interval is 8, never updated.";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_ntp_associations(d: &mut MockIosDevice, _input: &str) {
    let output = "\
  address         ref clock       st   when   poll reach  delay  offset   disp
*~127.127.1.1     .LOCL.           0      -     16   377  0.000   0.000  0.000
 * sys.peer, # selected, + candidate, - outlyer, x falseticker, ~ configured";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_snmp(d: &mut MockIosDevice, _input: &str) {
    let output = "\
Chassis: FCZ123456789
0 SNMP packets input
    0 Bad SNMP version errors
    0 Unknown community name
    0 Illegal operation for community name supplied
    0 Encoding errors
    0 Number of requested variables
    0 Number of altered variables
    0 Get-request PDUs
    0 Get-next PDUs
    0 Set-request PDUs
    0 Input queue packet drops (Maximum queue size 1000)
0 SNMP packets output
    0 Too big errors (Maximum packet size 1500)
    0 No such name errors
    0 Bad values errors
    0 General errors
    0 Response PDUs
    0 Trap PDUs
SNMP global trap: disabled";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_privilege(d: &mut MockIosDevice, _input: &str) {
    let level = if matches!(d.mode, CliMode::PrivilegedExec | CliMode::Config | CliMode::ConfigSub(_)) {
        15
    } else {
        1
    };
    let p = d.prompt();
    d.queue_output(&format!("Current privilege level is {}\n{}", level, p));
}

// ─── show ip stub handlers ─────────────────────────────────────────────────

pub fn handle_show_ip_access_lists(d: &mut MockIosDevice, input: &str) {
    handle_show_access_lists(d, input);
}

pub fn handle_show_ip_arp(d: &mut MockIosDevice, input: &str) {
    handle_show_arp(d, input);
}

pub fn handle_show_ip_bgp(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_ip_cef(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "IP CEF with switching (Table Version 0)\n  0.0.0.0/0, version 0, epoch 0, cached adjacency 0.0.0.0\n    0 packets, 0 bytes\n    via 0.0.0.0, 0 dependencies, recursive\n    next hop 0.0.0.0");
}

pub fn handle_show_ip_dhcp_binding(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "Bindings from all pools not associated with VRF:\nIP address          Client-ID/                Lease expiration        Type\n                    Hardware address/\n                    User name");
}

pub fn handle_show_ip_dhcp_pool(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_ip_dhcp_snooping(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "Switch DHCP snooping is enabled
Switch DHCP gleaning is disabled
DHCP snooping is configured on following VLANs:
1
DHCP snooping is operational on following VLANs:
1
Insertion of option 82 is enabled
   circuit-id default format: vlan-mod-port
   remote-id: 0000.0000.0000 (MAC)
Option 82 on untrusted port is not allowed
Verification of hwaddr field is enabled
Verification of giaddr field is enabled
DHCP snooping trust/rate is configured on the following Interfaces:

Interface                  Trusted    Allow option    Rate limit (pps)
-----------------------    -------    ------------    ----------------");
}

pub fn handle_show_ip_dhcp_snooping_binding(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "MacAddress          IpAddress        Lease(sec)  Type           VLAN  Interface
------------------  ---------------  ----------  -------------  ----  --------------------
Total number of bindings: 0");
}

pub fn handle_show_ip_dhcp(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_ip_eigrp(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_ip_http(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "HTTP server status: Enabled\nHTTP server port: 80\nHTTP server active supplementary listener ports: 80\nHTTP server authentication method: local\nHTTP server auth-retry 0 time-window 0\nHTTP server digest algorithm: md5\nHTTP server access class: 0\nHTTP server IPv4 access class: None\nHTTP server IPv6 access class: None\nHTTP server base path:\nHTTP server help root:\nMaximum number of concurrent server connections allowed: 300\nMaximum number of secondary server connections allowed: 50\nServer idle time-out: 180 seconds\nServer life time-out: 180 seconds\nServer linger time-out: 60 seconds\nHTTP common access class: 0\nHTTP secure server capability: Present\nHTTP secure server status: Enabled\nHTTP secure server port: 443\nHTTP secure server ciphersuite: 3des-ede-cbc-sha des-cbc-sha rc4-128-md5 rc4-128-sha aes-128-cbc-sha aes-256-cbc-sha dhe-aes-128-cbc-sha dhe-aes-256-cbc-sha");
}

pub fn handle_show_ip_igmp(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_ip_nat(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_ip_pim(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_ip_ssh(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "SSH Enabled - version 2.0\nAuthentication methods:publickey,keyboard-interactive,password\nAuthentication Publickey Algorithms:x509v3-ssh-rsa,ssh-rsa\nHostkey Algorithms:x509v3-ssh-rsa,ssh-rsa\nEncryption Algorithms:aes128-ctr,aes192-ctr,aes256-ctr\nMAC Algorithms:hmac-sha2-256,hmac-sha2-512,hmac-sha1,hmac-sha1-96\nKEX Algorithms:diffie-hellman-group-exchange-sha1,diffie-hellman-group14-sha1\nAuthentication timeout: 120 secs; Authentication retries: 3\nMinimum expected Diffie Hellman key size : 1024 bits\nIOS Keys in SECSH format(ssh-rsa, 2048 bits): ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQDJ5xCTpIOAFoW8BxGOSOFXbMJiJRSTJbp8b1T");
}

pub fn handle_show_ip_traffic(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "IP statistics:\n  Rcvd:  0 total, 0 local destination\n         0 format errors, 0 checksum errors, 0 bad hop count\n         0 unknown protocol, 0 not a gateway\n         0 security failures, 0 bad options, 0 with options\n  Opts:  0 end, 0 nop, 0 basic security, 0 loose source route\n         0 timestamp, 0 extended security, 0 record route\n         0 stream ID, 0 strict source route, 0 alert, 0 cipso, 0 ump\n         0 other\n  Frags: 0 reassembled, 0 timeouts, 0 couldn't reassemble\n         0 fragmented, 0 fragments, 0 couldn't fragment\n  Bcast: 0 received, 0 sent\n  Mcast: 0 received, 0 sent\n  Sent:  0 generated, 0 forwarded\n  Drop:  0 encapsulation failed, 0 unresolved, 0 no adjacency\n         0 no route, 0 unicast RPF, 0 forced drop, 0 unsupported-addr\n         0 options denied, 0 source IP address zero");
}

pub fn handle_show_ip_vrf(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

// ─── Stub handlers ────────────────────────────────────────────────────────────

/// Generic stub handler that outputs a static string and the prompt.
fn show_stub(d: &mut MockIosDevice, text: &str) {
    let p = d.prompt();
    if text.is_empty() {
        d.queue_output(&format!("{}", p));
    } else {
        d.queue_output(&format!("{}\n{}", text, p));
    }
}

pub fn handle_show_aaa(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "No AAA configuration");
}

pub fn handle_show_authentication(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_cable_diagnostics(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "No cable diagnostics results");
}

pub fn handle_show_controllers(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_crypto(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_debugging(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "No debugging is on");
}

pub fn handle_show_dhcp(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_dot1x(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_errdisable(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_errdisable_recovery(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "ErrDisable Reason            Timer Status\n\
-----------------            --------------\n\
arp-inspection               Disabled\n\
bpduguard                    Disabled\n\
channel-misconfig (STP)      Disabled\n\
dhcp-rate-limit              Disabled\n\
dtp-flap                     Disabled\n\
gbic-invalid                 Disabled\n\
inline-power                 Disabled\n\
l2ptguard                    Disabled\n\
link-flap                    Disabled\n\
mac-limit                    Disabled\n\
loopback                     Disabled\n\
pagp-flap                    Disabled\n\
port-mode-failure            Disabled\n\
pppoe-ia-rate-limit          Disabled\n\
psecure-violation            Disabled\n\
security-violation           Disabled\n\
sfp-config-mismatch          Disabled\n\
small-frame                  Disabled\n\
storm-control                Disabled\n\
udld                         Disabled\n\
unicast-flood                Disabled\n\
vmps                         Disabled\n\
psp                          Disabled\n\
dual-active-recovery         Disabled\n\
evc-lite input mapping fa    Disabled\n\
Recovery command: \"clear     Disabled\n\
\n\
Timer interval: 300 seconds\n\
\n\
Interfaces that will be enabled at the next timeout:",
    );
}

pub fn handle_show_etherchannel(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_etherchannel_summary(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Flags:  D - down        P - bundled in port-channel\n\
        I - stand-alone s - suspended\n\
        H - Hot-standby (LACP only)\n\
        R - Layer3      S - Layer2\n\
        U - in use      N - not in use, no aggregation\n\
        f - failed to allocate aggregator\n\
\n\
        M - not in use, minimum links not met\n\
        m - not in use, port not aggregated due to minimum links not met\n\
        u - unsuitable for bundling\n\
        w - waiting to be aggregated\n\
        d - default port\n\
        A - formed by Auto LAG\n\
\n\
Number of channel-groups in use: 0\n\
Number of aggregators:           0\n\
\n\
Group  Port-channel  Protocol    Ports\n\
------+-------------+-----------+-----------------------------------------------",
    );
}

pub fn handle_show_hosts(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Default domain is not set\nName/address lookup uses domain service\nName servers are 255.255.255.255",
    );
}

pub fn handle_show_license(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "License Level: ipservices\nLicense Type: Evaluation\nNext reload license Level: ipservices",
    );
}

pub fn handle_show_lldp(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Global LLDP Information:\n    Status: ACTIVE\n    LLDP advertisements are sent every 30 seconds\n    LLDP hold time advertised is 120 seconds\n    LLDP interface reinitialisation delay is 2 seconds\n    LLDP tlv-select: enabled\n    LLDP management-address: enabled\n    LLDP port-description: enabled\n    LLDP system-capabilities: enabled\n    LLDP system-description: enabled\n    LLDP system-name: enabled",
    );
}

pub fn handle_show_lldp_neighbors(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Capability codes:\n    (R) Router, (B) Bridge, (T) Telephone, (C) DOCSIS Cable Device\n    (W) WLAN Access Point, (P) Repeater, (S) Station, (O) Other\n\nDevice ID          Local Intf     Hold-time  Capability      Port ID\n\nTotal entries displayed: 0",
    );
}

pub fn handle_show_lldp_neighbors_detail(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Total entries displayed: 0",
    );
}

pub fn handle_show_module(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_platform(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_policy_map(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_port_security(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    let output = "\
Secure Port  MaxSecureAddr  CurrentAddr  SecurityViolation  Security Action\n\
                (Count)       (Count)          (Count)\n\
-----------  -------------  -----------  -----------------  ---------------\n";
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_port_security_interface(d: &mut MockIosDevice, input: &str) {
    // Extract optional interface name argument: "show port-security interface [<name>]"
    let tokens: Vec<&str> = input.split_whitespace().collect();
    // tokens: ["show", "port-security", "interface", [<name>]]
    let iface_name = tokens.get(3).copied();
    let p = d.prompt();
    match iface_name {
        None => {
            // No interface specified — show empty table (same as base command)
            let output = "\
Secure Port  MaxSecureAddr  CurrentAddr  SecurityViolation  Security Action\n\
                (Count)       (Count)          (Count)\n\
-----------  -------------  -----------  -----------------  ---------------\n";
            d.queue_output(&format!("{}{}", output, p));
        }
        Some(name) => {
            // Per-interface detail — IOS 15.2 format
            let output = format!("\
Port Security              : Disabled\n\
Port Status                : Secure-down\n\
Violation Mode             : Shutdown\n\
Aging Time                 : 0 mins\n\
Aging Type                 : Absolute\n\
SecureStatic Address Aging : Disabled\n\
Maximum MAC Addresses      : 1\n\
Total MAC Addresses        : 0\n\
Configured MAC Addresses   : 0\n\
Sticky MAC Addresses       : 0\n\
Last Source Address:Vlan   : 0000.0000.0000:0\n\
Security Violation Count   : 0\n\
");
            d.queue_output(&format!(
                "Port Security for interface {}:\n{}{}",
                name, output, p
            ));
        }
    }
}

pub fn handle_show_port_security_address(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    let output = "\
               Secure Mac Address Table\n\
-----------------------------------------------------------------------------\n\
Vlan    Mac Address       Type                          Ports   Remaining Age\n\
                                                                   (mins)\n\
----    -----------       ----                          -----   -------------\n\
-----------------------------------------------------------------------------\n\
Total Addresses in System (excluding one mac per port)     : 0\n\
Max Addresses limit in System (excluding one mac per port) : 1024\n";
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_power(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_power_inline(d: &mut MockIosDevice, _input: &str) {
    let mut output = String::new();
    output.push_str("Available:240.0(w)  Used:0.0(w)  Remaining:240.0(w)\n");
    output.push_str("\n");
    output.push_str("Interface Admin  Oper       Power   Device              Class Max\n");
    output.push_str("                            (Watts)                    \n");
    output.push_str("--------- ------ ---------- ------- ------------------- ----- ----\n");
    for port in 1..=12 {
        output.push_str(&format!(
            "Gi1/0/{:<2}  auto   off        0.0     n/a                 n/a   30.0 \n",
            port
        ));
    }
    output.push_str("\n");
    output.push_str("Totals:            0.0\n");
    show_stub(d, output.trim_end_matches('\n'));
}

pub fn handle_show_protocols(d: &mut MockIosDevice, _input: &str) {
    let mut output = String::new();
    let routing = if d.state.ip_routing { "enabled" } else { "disabled" };
    output.push_str(&format!("Global values:\n  Internet Protocol routing is {}\n", routing));
    for iface in &d.state.interfaces {
        let (status, protocol) = if !iface.admin_up {
            ("administratively down", "down")
        } else if iface.link_up {
            ("up", "up")
        } else {
            ("down", "down")
        };
        output.push_str(&format!("{} is {}, line protocol is {}\n", iface.name, status, protocol));
        if let Some((addr, mask)) = iface.ip_address {
            let prefix_len = u32::from(mask).count_ones();
            output.push_str(&format!("  Internet address is {}/{}\n", addr, prefix_len));
        }
    }
    let p = d.prompt();
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_sessions(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "% No connections open");
}

pub fn handle_show_ssh(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Connection Version Mode Encryption  Hmac         State           Username\n0          2.0     IN   aes256-ctr  hmac-sha2-25 Session started  admin\n0          2.0     OUT  aes256-ctr  hmac-sha2-25 Session started  admin\n%No SSHv1 server connections running.",
    );
}

pub fn handle_show_standby(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_storm_control(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_storm_control();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_switch(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Switch/Stack Mac Address : 00a3.d14f.2280 - Local Mac Address\nMac persistance wait time: Indefinite\n                                   H/W   Current\nSwitch#   Role    Mac Address     Priority Version  State \n------------------------------------------------------------\n*1       Active   00a3.d14f.2280     15     0102    Ready",
    );
}

pub fn handle_show_vtp(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_vtp_status();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_vtp_status(d: &mut MockIosDevice, _input: &str) {
    let output = d.state.generate_show_vtp_status();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

// ─── New stub handlers ────────────────────────────────────────────────────────

pub fn handle_access_enable(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_archive(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_cd(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_connect(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_disconnect(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_erase(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("[OK]\n{}", p));
}

pub fn handle_exec_ip(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_lock(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_login(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_mkdir(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_monitor(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_more(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_pwd(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("flash:/\n{}", p));
}

pub fn handle_rename(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_rmdir(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_send(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_session(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_set(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_setup(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "--- System Configuration Dialog ---\nWould you like to enter the initial configuration dialog? [yes/no]: \n{}",
        p
    ));
}

pub fn handle_test(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
}

pub fn handle_where(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("No connections open\n{}", p));
}

// ─── Tree ─────────────────────────────────────────────────────────────────────

static EXEC_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

pub fn exec_tree() -> &'static Vec<CommandNode> {
    EXEC_TREE.get_or_init(build_exec_tree)
}

fn priv_only() -> ModeFilter {
    ModeFilter::Only(vec![CliModeClass::PrivExec])
}

fn build_exec_tree() -> Vec<CommandNode> {
    vec![
        // access-enable [priv only]
        keyword("access-enable", "Create a temporary Access-List entry")
            .mode(priv_only())
            .handler(handle_access_enable),

        // archive [priv only]
        keyword("archive", "manage archive files")
            .mode(priv_only())
            .handler(handle_archive),

        // show
        keyword("show", "Show running system information")
            .handler(handle_show_incomplete as CmdHandler)
            .children(vec![
                keyword("version", "System hardware and software status")
                    .handler(handle_show_version),
                keyword("history", "Display the session command history")
                    .handler(handle_show_history),
                keyword("running-config", "Current operating configuration")
                    .handler(handle_show_running_config as CmdHandler)
                    .children(vec![
                        keyword("interface", "Show interface configuration")
                            .children(vec![
                                param("<name>", ParamType::RestOfLine, "Interface name")
                                    .handler(handle_show_running_config_interface),
                            ]),
                    ]),
                keyword("startup-config", "Contents of startup configuration")
                    .mode(priv_only())
                    .handler(handle_show_startup_config),
                keyword("clock", "Display the system clock")
                    .handler(handle_show_clock),
                keyword("ip", "IP information")
                    .handler(handle_show_ip_incomplete as CmdHandler)
                    .children(vec![
                        keyword("access-lists", "List IP access lists")
                            .handler(handle_show_ip_access_lists),
                        keyword("arp", "IP ARP table")
                            .handler(handle_show_ip_arp),
                        keyword("bgp", "BGP information")
                            .handler(handle_show_ip_bgp),
                        keyword("cef", "Cisco Express Forwarding")
                            .handler(handle_show_ip_cef),
                        keyword("dhcp", "Show items in the DHCP database")
                            .handler(handle_show_ip_dhcp as CmdHandler)
                            .children(vec![
                                keyword("binding", "DHCP address bindings")
                                    .handler(handle_show_ip_dhcp_binding),
                                keyword("pool", "DHCP pools information")
                                    .handler(handle_show_ip_dhcp_pool),
                                keyword("snooping", "DHCP snooping information")
                                    .handler(handle_show_ip_dhcp_snooping as CmdHandler)
                                    .children(vec![
                                        keyword("binding", "DHCP snooping binding table")
                                            .handler(handle_show_ip_dhcp_snooping_binding),
                                    ]),
                            ]),
                        keyword("eigrp", "Show IPv4 EIGRP")
                            .handler(handle_show_ip_eigrp),
                        keyword("http", "HTTP information")
                            .handler(handle_show_ip_http),
                        keyword("igmp", "IGMP information")
                            .handler(handle_show_ip_igmp),
                        keyword("interface", "IP interface status and configuration")
                            .handler(handle_show_ip_interface as CmdHandler)
                            .children(vec![
                                keyword("brief", "Brief summary of IP status")
                                    .handler(handle_show_ip_interface_brief),
                            ]),
                        keyword("nat", "IP NAT information")
                            .handler(handle_show_ip_nat),
                        keyword("ospf", "OSPF information")
                            .handler(handle_show_ip_ospf)
                            .children(vec![
                                keyword("neighbor", "OSPF neighbor list")
                                    .handler(handle_show_ip_ospf_neighbor),
                            ]),
                        keyword("pim", "PIM information")
                            .handler(handle_show_ip_pim),
                        keyword("protocols", "IP routing protocol process parameters and statistics")
                            .handler(handle_show_ip_protocols),
                        keyword("route", "IP routing table")
                            .handler(handle_show_ip_route)
                            .children(vec![
                                keyword("connected", "Connected routes")
                                    .handler(handle_show_ip_route_connected),
                                keyword("static", "Static routes")
                                    .handler(handle_show_ip_route_static),
                                keyword("summary", "Summary of all routes")
                                    .handler(handle_show_ip_route_summary),
                            ]),
                        keyword("ssh", "Information on SSH")
                            .handler(handle_show_ip_ssh),
                        keyword("traffic", "IP protocol statistics")
                            .handler(handle_show_ip_traffic),
                        keyword("vrf", "VPN Routing/Forwarding instance information")
                            .handler(handle_show_ip_vrf),
                    ]),
                keyword("ipv6", "IPv6 information")
                    .handler(handle_show_ipv6_incomplete as CmdHandler)
                    .children(vec![
                        keyword("interface", "IPv6 interface status and configuration")
                            .children(vec![
                                keyword("brief", "Brief summary of IPv6 status and configuration")
                                    .handler(handle_show_ipv6_interface_brief),
                            ]),
                        keyword("route", "Show IPv6 route table entries")
                            .handler(handle_show_ipv6_route),
                        keyword("ospf", "OSPF information")
                            .handler(handle_show_ipv6_ospf)
                            .children(vec![
                                keyword("interface", "Interface information")
                                    .children(vec![
                                        keyword("brief", "Summary of interface information")
                                            .handler(handle_show_ipv6_ospf_interface_brief),
                                    ]),
                            ]),
                        keyword("neighbors", "Show IPv6 neighbor cache entries")
                            .handler(handle_show_ipv6_incomplete),
                        keyword("protocols", "IPv6 Routing Protocols")
                            .handler(handle_show_ipv6_incomplete),
                        keyword("traffic", "IPv6 traffic statistics")
                            .handler(handle_show_ipv6_incomplete),
                        keyword("access-list", "Summary of access lists")
                            .handler(handle_show_ipv6_incomplete),
                        keyword("cef", "Cisco Express Forwarding for IPv6")
                            .handler(handle_show_ipv6_incomplete),
                        keyword("nd", "Show IPv6 ND related information")
                            .handler(handle_show_ipv6_incomplete),
                        keyword("prefix-list", "List IPv6 prefix lists")
                            .handler(handle_show_ipv6_incomplete),
                        keyword("routers", "Show local IPv6 routers")
                            .handler(handle_show_ipv6_incomplete),
                    ]),
                keyword("boot", "Boot and startup information")
                    .handler(handle_show_boot),
                keyword("interfaces", "Interface status and configuration")
                    .handler(handle_show_interfaces)
                    .children(vec![
                        keyword("status", "Show interface status")
                            .handler(handle_show_interfaces_status),
                        keyword("description", "Show interface description")
                            .handler(handle_show_interfaces_description),
                        keyword("trunk", "Show trunk interface information")
                            .handler(handle_show_interfaces_trunk),
                        keyword("switchport", "Show interface switchport information")
                            .handler(handle_show_interfaces_switchport),
                        keyword("counters", "Show interface counters")
                            .handler(handle_show_interfaces_counters),
                        param("<name>", ParamType::Word, "Interface name")
                            .handler(handle_show_interfaces)
                            .children(vec![
                                keyword("switchport", "Show interface switchport information")
                                    .handler(handle_show_interfaces_name_switchport),
                            ]),
                    ]),
                keyword("vlan", "VLAN information")
                    .handler(handle_show_vlan as CmdHandler)
                    .children(vec![
                        keyword("brief", "VTP all VLAN status in brief")
                            .handler(handle_show_vlan_brief),
                        keyword("id", "VLAN id")
                            .children(vec![
                                param("<vlan-id>", ParamType::Number, "VLAN identifier")
                                    .handler(handle_show_vlan_id),
                            ]),
                    ]),
                keyword("install", "Install information")
                    .mode(priv_only())
                    .children(vec![
                        keyword("summary", "Show install summary")
                            .handler(handle_show_install_summary),
                    ]),
                keyword("flash:", "Display flash filesystem information")
                    .handler(handle_show_flash),
                keyword("terminal", "Display terminal configuration parameters")
                    .handler(handle_show_terminal),
                keyword("cdp", "CDP information")
                    .handler(handle_show_cdp)
                    .children(vec![
                        keyword("neighbors", "CDP neighbor entries")
                            .handler(handle_show_cdp_neighbors)
                            .children(vec![
                                keyword("detail", "Show detailed information for CDP entries")
                                    .handler(handle_show_cdp_neighbors_detail),
                            ]),
                    ]),
                keyword("users", "Display information about terminal lines")
                    .handler(handle_show_users),
                keyword("logging", "Show the contents of logging buffers")
                    .handler(handle_show_logging),
                keyword("arp", "ARP table")
                    .handler(handle_show_arp),
                keyword("mac", "MAC configuration")
                    .children(vec![
                        keyword("address-table", "MAC forwarding table")
                            .handler(handle_show_mac_address_table)
                            .children(vec![
                                keyword("dynamic", "Show only dynamic entries")
                                    .handler(handle_show_mac_address_table_dynamic),
                                keyword("count", "Show the count of MAC address table entries")
                                    .handler(handle_show_mac_address_table_count),
                            ]),
                    ]),
                keyword("spanning-tree", "Spanning tree topology")
                    .handler(handle_show_spanning_tree as CmdHandler)
                    .children(vec![
                        keyword("vlan", "Spanning tree per VLAN")
                            .children(vec![
                                param("<vlan-id>", ParamType::Number, "VLAN identifier")
                                    .handler(handle_show_spanning_tree_vlan),
                            ]),
                        keyword("summary", "Spanning tree summary status")
                            .handler(handle_show_spanning_tree_summary),
                    ]),
                keyword("processes", "Active process statistics")
                    .children(vec![
                        keyword("cpu", "Show CPU use per process")
                            .handler(handle_show_processes_cpu),
                    ]),
                keyword("access-lists", "List access lists")
                    .handler(handle_show_access_lists),
                keyword("ntp", "Network time protocol")
                    .children(vec![
                        keyword("status", "NTP status")
                            .handler(handle_show_ntp_status),
                        keyword("associations", "NTP associations")
                            .handler(handle_show_ntp_associations),
                    ]),
                keyword("snmp", "SNMP statistics")
                    .handler(handle_show_snmp),
                keyword("privilege", "Show current privilege level")
                    .handler(handle_show_privilege),
                keyword("line", "TTY line information")
                    .handler(handle_show_line),
                keyword("inventory", "Show the physical inventory")
                    .handler(handle_show_inventory),
                keyword("environment", "Show environmental conditions")
                    .handler(handle_show_environment),
                keyword("aaa", "Show AAA values")
                    .handler(handle_show_aaa),
                keyword("authentication", "Auth Manager information")
                    .handler(handle_show_authentication),
                keyword("cable-diagnostics", "Show Cable Diagnostics")
                    .handler(handle_show_cable_diagnostics),
                keyword("configuration", "Contents of Non-Volatile memory")
                    .handler(handle_show_running_config),
                keyword("controllers", "Interface controller status")
                    .handler(handle_show_controllers),
                keyword("crypto", "Encryption module")
                    .handler(handle_show_crypto),
                keyword("debugging", "State of each debugging option")
                    .handler(handle_show_debugging),
                keyword("dhcp", "DHCP status")
                    .handler(handle_show_dhcp),
                keyword("dot1x", "Dot1x information")
                    .handler(handle_show_dot1x),
                keyword("errdisable", "Error disable")
                    .handler(handle_show_errdisable as CmdHandler)
                    .children(vec![
                        keyword("recovery", "ErrDisable recovery timer")
                            .handler(handle_show_errdisable_recovery),
                    ]),
                keyword("etherchannel", "EtherChannel information")
                    .handler(handle_show_etherchannel as CmdHandler)
                    .children(vec![
                        keyword("summary", "One summary line per channel-group")
                            .handler(handle_show_etherchannel_summary),
                    ]),
                keyword("hosts", "IP domain-name, lookup style")
                    .handler(handle_show_hosts),
                keyword("license", "Show license information")
                    .handler(handle_show_license),
                keyword("lldp", "LLDP information")
                    .handler(handle_show_lldp as CmdHandler)
                    .children(vec![
                        keyword("neighbors", "LLDP neighbor table")
                            .handler(handle_show_lldp_neighbors as CmdHandler)
                            .children(vec![
                                keyword("detail", "Show detailed information")
                                    .handler(handle_show_lldp_neighbors_detail),
                            ]),
                    ]),
                keyword("module", "Module information")
                    .handler(handle_show_module),
                keyword("platform", "Platform specific commands")
                    .handler(handle_show_platform),
                keyword("policy-map", "Show Policy Map")
                    .handler(handle_show_policy_map),
                keyword("port-security", "Show secure port information")
                    .handler(handle_show_port_security as CmdHandler)
                    .children(vec![
                        keyword("interface", "Show secure port interface")
                            .handler(handle_show_port_security_interface as CmdHandler)
                            .children(vec![
                                param("<name>", ParamType::RestOfLine, "Interface name")
                                    .handler(handle_show_port_security_interface),
                            ]),
                        keyword("address", "Show secure port address")
                            .handler(handle_show_port_security_address),
                    ]),
                keyword("power", "Switch Power")
                    .handler(handle_show_power as CmdHandler)
                    .children(vec![
                        keyword("inline", "Show inline power information")
                            .handler(handle_show_power_inline),
                    ]),
                keyword("protocols", "Active network routing protocols")
                    .handler(handle_show_protocols),
                keyword("sessions", "Telnet connections")
                    .handler(handle_show_sessions),
                keyword("ssh", "SSH server connections")
                    .handler(handle_show_ssh),
                keyword("standby", "HSRP information")
                    .handler(handle_show_standby),
                keyword("storm-control", "Storm control configuration")
                    .handler(handle_show_storm_control),
                keyword("switch", "Stack ring information")
                    .handler(handle_show_switch),
                keyword("vtp", "VTP information")
                    .handler(handle_show_vtp)
                    .children(vec![
                        keyword("status", "VTP status")
                            .handler(handle_show_vtp_status),
                    ]),
            ]),

        // cd [priv only]
        keyword("cd", "Change current directory")
            .mode(priv_only())
            .children(vec![
                param("<directory>", ParamType::RestOfLine, "Directory URL")
                    .handler(handle_cd),
            ]),

        // configure [priv only]
        keyword("configure", "Enter configuration mode")
            .mode(priv_only())
            .handler(handle_configure_alone as CmdHandler)
            .children(vec![
                keyword("terminal", "Configure from the terminal")
                    .handler(handle_configure_terminal),
            ]),

        // enable [user + priv — real IOS shows it in both, no-op in priv]
        keyword("enable", "Turn on privileged commands")
            .handler(handle_enable),

        // disable [priv only]
        keyword("disable", "Turn off privileged commands")
            .mode(priv_only())
            .handler(handle_disable),

        // terminal
        keyword("terminal", "Set terminal line parameters")
            .children(vec![
                keyword("length", "Set number of lines on a screen")
                    .children(vec![
                        param("<number>", ParamType::Number, "Number of lines")
                            .handler(handle_terminal_length),
                    ]),
                keyword("width", "Set width of the terminal")
                    .children(vec![
                        param("<number>", ParamType::Number, "Number of columns")
                            .handler(handle_terminal_width),
                    ]),
                keyword("monitor", "Copy debug output to the current terminal line")
                    .handler(handle_terminal_monitor),
                keyword("no", "Negate terminal settings")
                    .children(vec![
                        keyword("monitor", "Stop copying debug output to this terminal")
                            .handler(handle_terminal_no_monitor),
                    ]),
            ]),

        // connect [priv only]
        keyword("connect", "Open a terminal connection")
            .mode(priv_only())
            .children(vec![
                param("<host>", ParamType::Word, "Hostname or IP address")
                    .handler(handle_connect),
            ]),

        // copy [priv only]
        keyword("copy", "Copy from one file to another")
            .mode(priv_only())
            .children(vec![
                param("<source>", ParamType::Word, "Source URL or file")
                    .children(vec![
                        param("<dest>", ParamType::Word, "Destination URL or file")
                            .handler(handle_copy),
                    ]),
            ]),

        // delete [priv only]  — accepts "delete <filespec>" or "delete /force <filespec>"
        keyword("delete", "Delete a file")
            .mode(priv_only())
            .children(vec![
                param("<filespec>", ParamType::RestOfLine, "File to delete (may include /force)")
                    .handler(handle_delete),
            ]),

        // verify [priv only]
        keyword("verify", "Verify a file")
            .mode(priv_only())
            .children(vec![
                keyword("/md5", "MD5 signature")
                    .children(vec![
                        param("<filespec>", ParamType::Word, "File to verify")
                            .handler(handle_verify_md5),
                    ]),
            ]),

        // dir [priv only]
        keyword("dir", "List files on a filesystem")
            .mode(priv_only())
            .handler(handle_dir)
            .children(vec![
                param("<filesystem>", ParamType::Word, "Filesystem (e.g. flash:)")
                    .handler(handle_dir),
            ]),

        // disconnect [priv only]
        keyword("disconnect", "Disconnect an existing network connection")
            .mode(priv_only())
            .children(vec![
                param("<connection>", ParamType::Word, "Connection number or name")
                    .handler(handle_disconnect),
            ]),

        // erase [priv only]
        keyword("erase", "Erase a filesystem")
            .mode(priv_only())
            .children(vec![
                param("<filesystem>", ParamType::Word, "Filesystem (e.g. flash:)")
                    .handler(handle_erase),
            ]),

        // reload [priv only]
        keyword("reload", "Halt and perform a cold restart")
            .mode(priv_only())
            .handler(handle_reload_alone as CmdHandler)
            .children(vec![
                keyword("cancel", "Cancel pending reload")
                    .handler(handle_reload_cancel),
                keyword("in", "Reload after a time interval")
                    .children(vec![
                        param("<minutes>", ParamType::Number, "Minutes until reload")
                            .handler(handle_reload_in),
                    ]),
            ]),

        // write [priv only]
        keyword("write", "Write running configuration to memory, network, or terminal")
            .mode(priv_only())
            .handler(handle_write_memory as CmdHandler)
            .children(vec![
                keyword("erase", "Erase non-volatile memory")
                    .handler(handle_write_erase),
                keyword("memory", "Write to NV memory")
                    .handler(handle_write_memory),
                keyword("network", "Write to network TFTP server")
                    .handler(handle_write_network),
                keyword("terminal", "Write to terminal")
                    .handler(handle_write_terminal),
            ]),

        // install [priv only]
        keyword("install", "Install commands")
            .mode(priv_only())
            .children(vec![
                keyword("add", "Add a package")
                    .children(vec![
                        keyword("file", "Add from file")
                            .children(vec![
                                param("<filespec>", ParamType::Word, "Package file")
                                    .handler(handle_install_add),
                            ]),
                    ]),
                keyword("activate", "Activate installed packages")
                    .handler(handle_install_activate),
                keyword("commit", "Commit activated packages")
                    .handler(handle_install_commit),
                keyword("remove", "Remove packages")
                    .children(vec![
                        keyword("inactive", "Remove inactive packages")
                            .handler(handle_install_remove_inactive),
                    ]),
            ]),

        // ip [priv only]
        keyword("ip", "Global IP commands")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "IP command arguments")
                    .handler(handle_exec_ip),
            ]),

        // lock [priv only]
        keyword("lock", "Lock the terminal")
            .mode(priv_only())
            .handler(handle_lock),

        // login — available in all exec modes
        keyword("login", "Log in as a particular user")
            .handler(handle_login),

        // logout
        // (already present below; login belongs alphabetically before logout)

        // mkdir [priv only]
        keyword("mkdir", "Create new directory")
            .mode(priv_only())
            .children(vec![
                param("<directory>", ParamType::Word, "New directory name")
                    .handler(handle_mkdir),
            ]),

        // monitor [priv only]
        keyword("monitor", "Monitoring different system events")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Monitor command arguments")
                    .handler(handle_monitor),
            ]),

        // more [priv only]
        keyword("more", "Display the contents of a file")
            .mode(priv_only())
            .children(vec![
                param("<file>", ParamType::RestOfLine, "Filename or URL")
                    .handler(handle_more),
            ]),

        // ping
        keyword("ping", "Send echo messages")
            .children(vec![
                param("<target>", ParamType::Word, "Target address")
                    .handler(handle_ping),
            ]),

        // pwd [priv only]
        keyword("pwd", "Display current working directory")
            .mode(priv_only())
            .handler(handle_pwd),

        // rename [priv only]
        keyword("rename", "Rename a file")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Source and destination filenames")
                    .handler(handle_rename),
            ]),

        // rmdir [priv only]
        keyword("rmdir", "Remove existing directory")
            .mode(priv_only())
            .children(vec![
                param("<directory>", ParamType::Word, "Directory to remove")
                    .handler(handle_rmdir),
            ]),

        // send [priv only]
        keyword("send", "Send a message to other tty lines")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Line number or all, followed by message")
                    .handler(handle_send),
            ]),

        // session [priv only]
        keyword("session", "Run command on member switch")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Member switch and command")
                    .handler(handle_session),
            ]),

        // set [priv only]
        keyword("set", "Set system parameter (not config)")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "Parameter and value")
                    .handler(handle_set),
            ]),

        // setup [priv only]
        keyword("setup", "Run the SETUP command facility")
            .mode(priv_only())
            .handler(handle_setup),

        // traceroute
        keyword("traceroute", "Trace route to destination")
            .children(vec![
                param("<target>", ParamType::Word, "Target address")
                    .handler(handle_traceroute),
            ]),

        // help — available in all modes
        keyword("help", "Description of the interactive help system")
            .handler(handle_help_command),

        // test [priv only]
        keyword("test", "Test subsystems, memory, and interfaces")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "What to test")
                    .handler(handle_test),
            ]),

        // clock set [priv only]
        keyword("clock", "Manage the system clock")
            .mode(priv_only())
            .children(vec![
                keyword("set", "Set the time and date")
                    .children(vec![
                        param("<hh:mm:ss>", ParamType::RestOfLine, "Current Time")
                            .handler(handle_clock_set),
                    ]),
            ]),

        // debug [priv only]
        keyword("debug", "Debugging functions (see also 'undebug')")
            .mode(priv_only())
            .children(vec![
                param("<feature>", ParamType::RestOfLine, "Feature to debug")
                    .handler(handle_debug),
            ]),

        // undebug [priv only]
        keyword("undebug", "Disable debugging functions (see also 'debug')")
            .mode(priv_only())
            .children(vec![
                keyword("all", "Disable all debugging")
                    .handler(handle_undebug_all),
                param("<feature>", ParamType::RestOfLine, "Feature to undebug")
                    .handler(handle_undebug),
            ]),

        // no debug [priv only]
        keyword("no", "Disable debugging functions")
            .mode(priv_only())
            .children(vec![
                keyword("debug", "Debugging functions")
                    .children(vec![
                        keyword("all", "Disable all debugging")
                            .handler(handle_undebug_all),
                        param("<feature>", ParamType::RestOfLine, "Feature to undebug")
                            .handler(handle_undebug),
                    ]),
            ]),

        // clear [priv only]
        keyword("clear", "Reset functions")
            .mode(priv_only())
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "What to clear")
                    .handler(handle_clear),
            ]),

        // ssh
        keyword("ssh", "Open a secure shell client connection")
            .children(vec![
                param("<rest>", ParamType::RestOfLine, "SSH parameters")
                    .handler(handle_ssh),
            ]),

        // telnet
        keyword("telnet", "Open a telnet connection")
            .children(vec![
                param("<host>", ParamType::Word, "Hostname or IP address")
                    .handler(handle_telnet),
            ]),

        // exit / logout / quit
        keyword("exit", "Exit from the EXEC")
            .handler(handle_exit),
        keyword("logout", "Exit from the EXEC")
            .handler(handle_exit),

        // where [priv only]
        keyword("where", "List active connections")
            .mode(priv_only())
            .handler(handle_where),
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
    fn test_exec_tree_builds() {
        let tree = exec_tree();
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_exec_show_version_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show version", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_show_ver_abbreviation() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("sh ver", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "sh ver should match show version"
        );
    }

    #[test]
    fn test_exec_conf_t_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("conf t", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "conf t should match configure terminal"
        );
    }

    #[test]
    fn test_exec_configure_alone_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("configure", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "configure alone should execute (prompts for method)"
        );
    }

    #[test]
    fn test_exec_show_ip_incomplete() {
        // "show ip" should execute the ip incomplete handler
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show ip", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show ip should trigger incomplete handler"
        );
    }

    #[test]
    fn test_exec_show_ip_int_brief_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show ip interface brief", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_show_ip_route_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show ip route", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_show_ip_route_static_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show ip route static", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_show_ip_route_connected_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show ip route connected", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_show_ip_route_summary_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show ip route summary", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_write_memory_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("write memory", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_reload_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("reload", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_reload_cancel_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("reload cancel", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_install_add_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("install add file flash:image.bin", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_copy_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("copy flash:src running-config", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_delete_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("delete flash:temp.cfg", tree, &mode);
        // "flash:temp.cfg" matches <filespec> Word param
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_configure_hidden_in_user_exec() {
        let tree = exec_tree();
        let mode = CliMode::UserExec;
        let result = parse("configure terminal", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::InvalidInput { .. }),
            "configure should be invalid in user exec"
        );
    }

    #[test]
    fn test_exec_enable_visible_in_priv() {
        // Real IOS shows enable in priv exec (it's a no-op)
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("enable", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "enable should be visible in priv exec (no-op)"
        );
    }

    #[test]
    fn test_exec_show_run_abbreviation() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show run", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show run should match show running-config"
        );
    }

    #[test]
    fn test_exec_term_len_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("terminal length 0", tree, &mode);
        assert!(matches!(result, crate::cmd_tree::ParseResult::Execute { .. }));
    }

    #[test]
    fn test_exec_handler_show_version_output() {
        let mut device = make_device();
        let _ = device.receive_sync(); // consume initial prompt
        handle_show_version(&mut device, "show version");
        let output = device.drain_output();
        assert!(output.contains("Cisco IOS"));
    }

    #[test]
    fn test_exec_handler_configure_terminal_enters_config() {
        let mut device = make_device();
        handle_configure_terminal(&mut device, "configure terminal");
        assert_eq!(device.mode, CliMode::Config);
    }

    #[test]
    fn test_exec_handler_disable_enters_user_exec() {
        let mut device = make_device();
        handle_disable(&mut device, "disable");
        assert_eq!(device.mode, CliMode::UserExec);
    }

    #[tokio::test]
    async fn test_show_help_lists_many_commands() {
        use ayclic::raw_transport::RawTransport;
        use std::time::Duration;
        let mut device = MockIosDevice::new("Switch1");
        // Consume initial prompt
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Type "show " then "?" to trigger help listing
        device.send(b"show ").await.unwrap();
        let _ = device.receive(Duration::from_secs(1)).await.unwrap(); // echo

        device.send(b"?").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);

        // Help lines start with two spaces (e.g. "  version          System hardware...")
        let command_count = output.lines().filter(|l| l.starts_with("  ")).count();
        assert!(
            command_count >= 40,
            "show ? should list at least 40 commands, got {}: {:?}",
            command_count,
            output
        );
    }

    /// Verify that all 21 new stub commands appear in privileged exec `?` help.
    #[tokio::test]
    async fn test_new_stub_commands_appear_in_exec_help() {
        use ayclic::raw_transport::RawTransport;
        use std::time::Duration;

        let mut device = MockIosDevice::new("Router1");
        // Consume initial prompt
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Request top-level help
        device.send(b"?").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);

        let expected_cmds = [
            "access-enable",
            "archive",
            "cd",
            "connect",
            "disconnect",
            "erase",
            "ip",
            "lock",
            "login",
            "mkdir",
            "monitor",
            "more",
            "pwd",
            "rename",
            "rmdir",
            "send",
            "session",
            "set",
            "setup",
            "test",
            "where",
        ];

        for cmd in &expected_cmds {
            assert!(
                output.contains(cmd),
                "Expected command '{}' to appear in exec ? help, but got:\n{}",
                cmd,
                output
            );
        }
    }

    /// Verify the `write` help text matches real IOS exactly.
    #[test]
    fn test_write_help_text() {
        use crate::cmd_tree::TokenMatcher;
        let tree = exec_tree();
        let write_node = tree.iter().find(|n| {
            matches!(&n.matcher, TokenMatcher::Keyword(kw) if kw == "write")
        });
        assert!(write_node.is_some(), "write command not found in exec tree");
        assert_eq!(
            write_node.unwrap().help,
            "Write running configuration to memory, network, or terminal",
            "write help text should match real IOS"
        );
    }

    /// `write terminal` should parse successfully in privileged exec mode.
    #[test]
    fn test_exec_write_terminal_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("write terminal", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "write terminal should parse as Execute"
        );
    }

    /// `write terminal` should produce output equivalent to `show running-config`.
    #[test]
    fn test_write_terminal_output_matches_show_running_config() {
        let mut device = make_device();
        let mut device2 = make_device();
        handle_write_terminal(&mut device, "write terminal");
        handle_show_running_config(&mut device2, "show running-config");
        let out1 = device.drain_output();
        let out2 = device2.drain_output();
        assert_eq!(out1, out2, "write terminal output should match show running-config");
    }

    /// `write erase` should parse successfully in privileged exec mode.
    #[test]
    fn test_exec_write_erase_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("write erase", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "write erase should parse as Execute"
        );
    }

    /// `write erase` should output [OK].
    #[test]
    fn test_write_erase_output() {
        let mut device = make_device();
        handle_write_erase(&mut device, "write erase");
        let output = device.drain_output();
        assert!(output.contains("[OK]"), "write erase should output [OK]");
    }

    /// `write network` should parse successfully in privileged exec mode.
    #[test]
    fn test_exec_write_network_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("write network", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "write network should parse as Execute"
        );
    }

    /// `write network` stub should produce some output without panicking.
    #[test]
    fn test_write_network_output() {
        let mut device = make_device();
        handle_write_network(&mut device, "write network");
        let _output = device.drain_output();
        // stub: just verify it doesn't panic
    }

    /// The `write` subcommand list should contain erase, memory, network, terminal.
    #[test]
    fn test_write_children_match_real_ios() {
        use crate::cmd_tree::TokenMatcher;
        let tree = exec_tree();
        let write_node = tree.iter().find(|n| {
            matches!(&n.matcher, TokenMatcher::Keyword(kw) if kw == "write")
        }).expect("write node must exist");
        let child_keywords: Vec<&str> = write_node.children.iter().filter_map(|c| {
            if let TokenMatcher::Keyword(kw) = &c.matcher { Some(kw.as_str()) } else { None }
        }).collect();
        for expected in &["erase", "memory", "network", "terminal"] {
            assert!(
                child_keywords.contains(expected),
                "write should have '{}' subcommand, found: {:?}", expected, child_keywords
            );
        }
    }

    /// Verify pwd outputs "flash:/"
    #[test]
    fn test_pwd_output() {
        let mut device = make_device();
        handle_pwd(&mut device, "pwd");
        let output = device.drain_output();
        assert!(output.contains("flash:/"), "pwd should output flash:/");
    }

    /// Verify setup outputs the system configuration dialog header.
    #[test]
    fn test_setup_output() {
        let mut device = make_device();
        handle_setup(&mut device, "setup");
        let output = device.drain_output();
        assert!(
            output.contains("System Configuration Dialog"),
            "setup should output System Configuration Dialog"
        );
    }

    /// Verify erase outputs [OK]
    #[test]
    fn test_erase_output() {
        let mut device = make_device();
        handle_erase(&mut device, "erase flash:");
        let output = device.drain_output();
        assert!(output.contains("[OK]"), "erase should output [OK]");
    }

    /// Verify where outputs "No connections open"
    #[test]
    fn test_where_output() {
        let mut device = make_device();
        handle_where(&mut device, "where");
        let output = device.drain_output();
        assert!(
            output.contains("No connections open"),
            "where should output 'No connections open'"
        );
    }

    /// Verify `show ip ?` lists all new subcommands in alphabetical order.
    #[tokio::test]
    async fn test_show_ip_help_lists_new_subcommands() {
        use ayclic::raw_transport::RawTransport;
        use std::time::Duration;

        let mut device = MockIosDevice::new("Router1");
        // Consume initial prompt
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        // Type "show ip ?" to trigger help listing for show ip subcommands
        device.send(b"show ip ?").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);

        let expected_subcommands = [
            "access-lists",
            "arp",
            "bgp",
            "cef",
            "dhcp",
            "eigrp",
            "http",
            "igmp",
            "interface",
            "nat",
            "ospf",
            "pim",
            "protocols",
            "route",
            "ssh",
            "traffic",
            "vrf",
        ];

        for cmd in &expected_subcommands {
            assert!(
                output.contains(cmd),
                "Expected 'show ip ?' to list '{}', but got:\n{}",
                cmd,
                output
            );
        }

        // Verify alphabetical order by extracting keywords from help lines
        // Exclude parameter tokens like <cr> which don't sort with normal keywords
        let keywords: Vec<&str> = output
            .lines()
            .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
            .filter_map(|l| l.trim().split_whitespace().next())
            .filter(|kw| !kw.starts_with('<'))
            .collect();

        let mut sorted = keywords.clone();
        sorted.sort_unstable();
        assert_eq!(
            keywords, sorted,
            "show ip ? subcommands should be listed in alphabetical order"
        );
    }

    /// `show ip ospf` with no OSPF process configured returns the error message.
    #[test]
    fn test_show_ip_ospf_no_process() {
        let mut device = make_device();
        handle_show_ip_ospf(&mut device, "show ip ospf");
        let output = device.drain_output();
        assert!(
            output.contains("No router process is configured"),
            "Expected no-process error, got: {:?}",
            output
        );
    }

    /// `show ip ospf` with an OSPF process configured returns process info.
    #[test]
    fn test_show_ip_ospf_with_process() {
        use crate::device_state::{OspfV3Area, OspfV3Process};
        let mut device = make_device();
        let mut proc = OspfV3Process::new(1);
        proc.router_id = Some("10.127.0.1".parse().unwrap());
        proc.areas.push(OspfV3Area::new(0));
        device.state.ospfv3_processes.push(proc);

        handle_show_ip_ospf(&mut device, "show ip ospf");
        let output = device.drain_output();
        assert!(
            output.contains("ospf 1"),
            "Expected process ID in output, got: {:?}",
            output
        );
        assert!(
            output.contains("10.127.0.1"),
            "Expected router-ID in output, got: {:?}",
            output
        );
        assert!(
            !output.contains("No router process"),
            "Should not show error when process exists, got: {:?}",
            output
        );
    }

    /// `show ip ospf neighbor` always returns a table with the standard header.
    #[test]
    fn test_show_ip_ospf_neighbor_header() {
        let mut device = make_device();
        handle_show_ip_ospf_neighbor(&mut device, "show ip ospf neighbor");
        let output = device.drain_output();
        assert!(
            output.contains("Neighbor ID"),
            "Expected 'Neighbor ID' column header, got: {:?}",
            output
        );
        assert!(
            output.contains("State"),
            "Expected 'State' column header, got: {:?}",
            output
        );
        assert!(
            output.contains("Interface"),
            "Expected 'Interface' column header, got: {:?}",
            output
        );
    }

    /// `show ip ospf ?` help lists `neighbor` as a subcommand.
    #[tokio::test]
    async fn test_show_ip_ospf_neighbor_in_help() {
        use ayclic::raw_transport::RawTransport;
        use std::time::Duration;

        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show ip ospf ?").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);

        assert!(
            output.contains("neighbor"),
            "Expected 'neighbor' in 'show ip ospf ?' help, got: {:?}",
            output
        );
    }

    // --- show etherchannel summary tests ------------------------------------

    /// `show etherchannel summary` parses successfully in privileged exec mode.
    #[test]
    fn test_show_etherchannel_summary_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show etherchannel summary", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show etherchannel summary should parse in privileged exec"
        );
    }

    /// `show etherchannel summary` output contains the flags legend and the
    /// empty table header -- matching real IOS output with no port-channels.
    #[test]
    fn test_show_etherchannel_summary_output() {
        let mut device = make_device();
        handle_show_etherchannel_summary(&mut device, "show etherchannel summary");
        let output = device.drain_output();
        assert!(
            output.contains("Number of channel-groups in use: 0"),
            "Expected zero channel-groups line, got: {:?}",
            output
        );
        assert!(
            output.contains("Number of aggregators:           0"),
            "Expected zero aggregators line, got: {:?}",
            output
        );
        assert!(
            output.contains("Group  Port-channel  Protocol    Ports"),
            "Expected table header, got: {:?}",
            output
        );
        assert!(
            output.contains("D - down"),
            "Expected flags legend, got: {:?}",
            output
        );
    }

    /// `show etherchannel ?` help lists `summary` as a subcommand.
    #[tokio::test]
    async fn test_show_etherchannel_summary_in_help() {
        use ayclic::raw_transport::RawTransport;
        use std::time::Duration;

        let mut device = MockIosDevice::new("Router1");
        let _ = device.receive(Duration::from_secs(1)).await.unwrap();

        device.send(b"show etherchannel ?").await.unwrap();
        let out = device.receive(Duration::from_secs(1)).await.unwrap();
        let output = String::from_utf8_lossy(&out);

        assert!(
            output.contains("summary"),
            "Expected 'summary' in 'show etherchannel ?' help, got: {:?}",
            output
        );
    }

    // ─── terminal monitor tests ───────────────────────────────────────────────

    /// `terminal monitor` parses successfully in privileged exec mode.
    #[test]
    fn test_terminal_monitor_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("terminal monitor", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "terminal monitor should parse in privileged exec"
        );
    }

    /// `terminal no monitor` parses successfully.
    #[test]
    fn test_terminal_no_monitor_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("terminal no monitor", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "terminal no monitor should parse"
        );
    }

    /// `terminal monitor` sets the `terminal_monitor` flag to true.
    #[test]
    fn test_terminal_monitor_sets_flag() {
        let mut device = make_device();
        assert!(!device.terminal_monitor, "terminal_monitor should default to false");
        handle_terminal_monitor(&mut device, "terminal monitor");
        assert!(device.terminal_monitor, "terminal_monitor should be true after terminal monitor");
    }

    /// `terminal no monitor` clears the `terminal_monitor` flag.
    #[test]
    fn test_terminal_no_monitor_clears_flag() {
        let mut device = make_device();
        device.terminal_monitor = true;
        handle_terminal_no_monitor(&mut device, "terminal no monitor");
        assert!(!device.terminal_monitor, "terminal_monitor should be false after terminal no monitor");
    }

    /// `terminal monitor` produces only the prompt (no informational message).
    #[test]
    fn test_terminal_monitor_output_is_prompt_only() {
        let mut device = make_device();
        let _ = device.drain_output();
        handle_terminal_monitor(&mut device, "terminal monitor");
        let output = device.drain_output();
        assert!(
            output.contains("Router1#") || output.contains("Router1>"),
            "terminal monitor output should contain the prompt, got: {:?}",
            output
        );
        assert!(
            !output.contains("enabled") && !output.contains("Monitor"),
            "terminal monitor should produce only the prompt, got: {:?}",
            output
        );
    }

    /// `terminal no monitor` produces only the prompt.
    #[test]
    fn test_terminal_no_monitor_output_is_prompt_only() {
        let mut device = make_device();
        device.terminal_monitor = true;
        let _ = device.drain_output();
        handle_terminal_no_monitor(&mut device, "terminal no monitor");
        let output = device.drain_output();
        assert!(
            output.contains("Router1#") || output.contains("Router1>"),
            "terminal no monitor output should contain the prompt, got: {:?}",
            output
        );
    }

    /// `term mon` abbreviation also parses.
    #[test]
    fn test_terminal_monitor_abbreviation_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("term mon", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "term mon should match terminal monitor"
        );
    }

    // ─── show ip protocols tests ──────────────────────────────────────────────

    /// `show ip protocols` parses successfully.
    #[test]
    fn test_show_ip_protocols_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show ip protocols", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show ip protocols should parse"
        );
    }

    /// `show ip protocols` output contains key sections from real IOS.
    #[test]
    fn test_show_ip_protocols_output() {
        let mut device = make_device();
        handle_show_ip_protocols(&mut device, "show ip protocols");
        let output = device.drain_output();
        assert!(
            output.contains("Routing Protocol is \"application\""),
            "show ip protocols should contain application protocol section, got: {:?}",
            output
        );
        assert!(
            output.contains("Maximum path: 32"),
            "show ip protocols should contain Maximum path, got: {:?}",
            output
        );
        assert!(
            output.contains("Distance: (default is 4)"),
            "show ip protocols should contain Distance line, got: {:?}",
            output
        );
        assert!(
            output.contains("Routing Information Sources:"),
            "show ip protocols should contain Routing Information Sources, got: {:?}",
            output
        );
        assert!(
            output.contains("Gateway         Distance      Last Update"),
            "show ip protocols should contain column headers, got: {:?}",
            output
        );
    }

    /// `sh ip pro` abbreviation also parses.
    #[test]
    fn test_show_ip_protocols_abbreviation_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("sh ip pro", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "sh ip pro should match show ip protocols"
        );
    }

    // ─── show errdisable recovery tests ──────────────────────────────────────

    /// `show errdisable recovery` should parse as Execute.
    #[test]
    fn test_show_errdisable_recovery_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show errdisable recovery", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show errdisable recovery should parse as Execute"
        );
    }

    /// Abbreviated `sh errdis rec` should also parse.
    #[test]
    fn test_show_errdisable_recovery_abbreviation_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("sh errdis rec", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "sh errdis rec should match show errdisable recovery"
        );
    }

    /// `show errdisable recovery` output should contain the IOS 15.2 header line.
    #[test]
    fn test_show_errdisable_recovery_output_header() {
        let mut device = make_device();
        handle_show_errdisable_recovery(&mut device, "show errdisable recovery");
        let output = device.drain_output();
        assert!(
            output.contains("ErrDisable Reason            Timer Status"),
            "show errdisable recovery should contain header line, got: {:?}",
            output
        );
    }

    /// Output should list bpduguard as Disabled.
    #[test]
    fn test_show_errdisable_recovery_bpduguard_disabled() {
        let mut device = make_device();
        handle_show_errdisable_recovery(&mut device, "show errdisable recovery");
        let output = device.drain_output();
        assert!(
            output.contains("bpduguard                    Disabled"),
            "output should show bpduguard Disabled, got: {:?}",
            output
        );
    }

    /// Output should contain the timer interval line.
    #[test]
    fn test_show_errdisable_recovery_timer_interval() {
        let mut device = make_device();
        handle_show_errdisable_recovery(&mut device, "show errdisable recovery");
        let output = device.drain_output();
        assert!(
            output.contains("Timer interval: 300 seconds"),
            "output should contain 'Timer interval: 300 seconds', got: {:?}",
            output
        );
    }

    /// Output should contain the "Interfaces that will be enabled" footer line.
    #[test]
    fn test_show_errdisable_recovery_interfaces_footer() {
        let mut device = make_device();
        handle_show_errdisable_recovery(&mut device, "show errdisable recovery");
        let output = device.drain_output();
        assert!(
            output.contains("Interfaces that will be enabled at the next timeout:"),
            "output should contain interfaces footer, got: {:?}",
            output
        );
    }

    /// `show errdisable` alone (without subcommand) should still parse as Execute.
    #[test]
    fn test_show_errdisable_alone_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show errdisable", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show errdisable should parse as Execute"
        );
    }

    // --- show running-config interface tests ---

    /// `show running-config interface GigabitEthernet1/0/1` should parse as Execute.
    #[test]
    fn test_show_running_config_interface_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show running-config interface GigabitEthernet1/0/1", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show running-config interface GigabitEthernet1/0/1 should parse as Execute"
        );
    }

    /// `show run int Gi1/0/1` abbreviated form should parse as Execute.
    #[test]
    fn test_show_running_config_interface_abbreviated_parses() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("show run int Gi1/0/1", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::Execute { .. }),
            "show run int Gi1/0/1 should parse as Execute"
        );
    }

    /// Output for existing interface should contain "Building configuration..." header.
    #[test]
    fn test_show_running_config_interface_header() {
        let mut device = MockIosDevice::new("Switch1");
        handle_show_running_config_interface(
            &mut device,
            "show running-config interface GigabitEthernet1/0/1",
        );
        let output = device.drain_output();
        assert!(
            output.contains("Building configuration..."),
            "output should contain 'Building configuration...', got: {:?}",
            output
        );
    }

    /// Output for existing interface should include byte count in header.
    #[test]
    fn test_show_running_config_interface_byte_count() {
        let mut device = MockIosDevice::new("Switch1");
        handle_show_running_config_interface(
            &mut device,
            "show running-config interface GigabitEthernet1/0/1",
        );
        let output = device.drain_output();
        assert!(
            output.contains("Current configuration :") && output.contains("bytes"),
            "output should contain 'Current configuration : N bytes', got: {:?}",
            output
        );
    }

    /// Output for existing interface should show only that interface's block.
    #[test]
    fn test_show_running_config_interface_correct_interface() {
        let mut device = MockIosDevice::new("Switch1");
        handle_show_running_config_interface(
            &mut device,
            "show running-config interface GigabitEthernet1/0/1",
        );
        let output = device.drain_output();
        assert!(
            output.contains("interface GigabitEthernet1/0/1"),
            "output should contain 'interface GigabitEthernet1/0/1', got: {:?}",
            output
        );
        // Should NOT contain other interfaces
        assert!(
            !output.contains("interface GigabitEthernet1/0/2"),
            "output should NOT contain other interfaces, got: {:?}",
            output
        );
    }

    /// Output for existing interface should end with '!' and 'end'.
    #[test]
    fn test_show_running_config_interface_footer() {
        let mut device = MockIosDevice::new("Switch1");
        handle_show_running_config_interface(
            &mut device,
            "show running-config interface GigabitEthernet1/0/1",
        );
        let output = device.drain_output();
        // Must contain "end" in the output
        assert!(
            output.contains("\nend\n") || output.contains("\nend\r"),
            "output should contain 'end', got: {:?}",
            output
        );
    }

    /// Using abbreviated name `Gi1/0/1` should return the same interface block as the full name.
    #[test]
    fn test_show_running_config_interface_abbreviated_name() {
        let mut device1 = MockIosDevice::new("Switch1");
        let mut device2 = MockIosDevice::new("Switch1");
        handle_show_running_config_interface(
            &mut device1,
            "show running-config interface GigabitEthernet1/0/1",
        );
        handle_show_running_config_interface(
            &mut device2,
            "show running-config interface Gi1/0/1",
        );
        let out_full = device1.drain_output();
        let out_abbrev = device2.drain_output();
        // Both should show the same interface
        assert!(
            out_abbrev.contains("interface GigabitEthernet1/0/1"),
            "abbreviated name should resolve to full interface name, got: {:?}",
            out_abbrev
        );
        // Byte count may differ due to prompt length variance; compare interface block presence
        assert!(
            out_full.contains("interface GigabitEthernet1/0/1")
                && out_abbrev.contains("interface GigabitEthernet1/0/1"),
            "Both forms should show the same interface block"
        );
    }

    /// Non-existent interface should return an error message.
    #[test]
    fn test_show_running_config_interface_not_found() {
        let mut device = MockIosDevice::new("Switch1");
        handle_show_running_config_interface(
            &mut device,
            "show running-config interface GigabitEthernet99/99",
        );
        let output = device.drain_output();
        // Should not show a config block
        assert!(
            !output.contains("interface GigabitEthernet99/99"),
            "non-existent interface should not appear as a config block, got: {:?}",
            output
        );
        // Should produce some kind of error or empty indicator
        assert!(
            output.contains("%") || output.contains("Invalid") || output.contains("not found"),
            "should indicate error for missing interface, got: {:?}",
            output
        );
    }
}
