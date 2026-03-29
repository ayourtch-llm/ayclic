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

pub fn handle_show_ip_route(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_route();
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
    // Extract optional interface name argument: "show interfaces [<name>]"
    let tokens: Vec<&str> = input.split_whitespace().collect();
    // tokens[0] = "show", tokens[1] = "interfaces", tokens[2..] = optional name
    let iface_name: Option<String> = if tokens.len() > 2 {
        // The rest of the line after "show interfaces " is the interface name.
        // Handle abbreviated or full form — collect everything from index 2.
        let raw = tokens[2..].join(" ");
        Some(crate::cmd_tree_conf::normalize_interface_name(&raw))
    } else {
        None
    };

    let p = d.prompt();

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

pub fn handle_show_interfaces_status(d: &mut MockIosDevice, _input: &str) {
    let table = d.state.generate_show_interfaces_status();
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", table, p));
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

pub fn handle_end_noop(d: &mut MockIosDevice, _input: &str) {
    // In exec mode, "end" is a no-op (real IOS ignores it silently).
    let p = d.prompt();
    d.queue_output(&format!("{}", p));
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
";
    let p = d.prompt();
    d.queue_output(&format!("{}\n{}", output, p));
}

pub fn handle_show_users(d: &mut MockIosDevice, _input: &str) {
    let output = "\
    Line       User       Host(s)              Idle       Location
*  0 con 0                idle                 00:00:00
";
    let p = d.prompt();
    d.queue_output(&format!("{}{}", output, p));
}

pub fn handle_show_ip_ospf(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("%% OSPF: No router process is configured\n{}", p));
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
    let output = "\
Syslog logging: enabled (0 messages dropped, 0 messages rate-limited,
    0 flushes, 0 overruns, xml disabled, filtering disabled)

    Console logging: level debugging, 0 messages logged, xml disabled,
                     filtering disabled
    Monitor logging: level debugging, 0 messages logged, xml disabled,
                     filtering disabled
    Buffer logging:  disabled, xml disabled,
                     filtering disabled

Log Buffer (4096 bytes):
";
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

pub fn handle_show_ntp_status(d: &mut MockIosDevice, _input: &str) {
    let output = "\
Clock is unsynchronized, stratum 16, no reference clock
nominal freq is 250.0000 Hz, actual freq is 250.0000 Hz, precision is 2**10
ntp uptime is 0 (1/100 of seconds), resolution is 4016
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
    show_stub(d, "ErrDisable Reason\nTimeout\n---------\n---------");
}

pub fn handle_show_etherchannel(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
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
        "Global LLDP Information:\n    Status: ACTIVE\n    LLDP advertisements are sent every 30 seconds\n    LLDP hold time advertised is 120 seconds\n    LLDP interface reinitialisation delay is 2 seconds",
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
    show_stub(d, "");
}

pub fn handle_show_power(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_protocols(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Routing Protocol is \"application\"\n  Sending updates every 0 seconds\nRouting Protocol is \"connected\"\n  Sending updates every 0 seconds",
    );
}

pub fn handle_show_sessions(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "% No connections open");
}

pub fn handle_show_ssh(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Connection   Version  Mode  Encryption  Hmac         State           Username\n0            2.0      IN    aes256-ctr  hmac-sha2-25 Session started  admin",
    );
}

pub fn handle_show_standby(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_storm_control(d: &mut MockIosDevice, _input: &str) {
    show_stub(d, "");
}

pub fn handle_show_switch(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "Switch/Stack Mac Address : 00a3.d14f.2280 - Local Mac Address\nMac persistance wait time: Indefinite\n                                   H/W   Current\nSwitch#   Role    Mac Address     Priority Version  State \n------------------------------------------------------------\n*1       Active   00a3.d14f.2280     15     0102    Ready",
    );
}

pub fn handle_show_vtp(d: &mut MockIosDevice, _input: &str) {
    show_stub(
        d,
        "VTP Version capable             : 1 to 3\nVTP version running             : 1\nVTP Domain Name                 :\nVTP Pruning Mode                : Disabled\nVTP Traps Generation            : Disabled\nDevice ID                       : 00a3.d14f.2280",
    );
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
        // show
        keyword("show", "Show running system information")
            .handler(handle_show_incomplete as CmdHandler)
            .children(vec![
                keyword("version", "System hardware and software status")
                    .handler(handle_show_version),
                keyword("history", "Display the session command history")
                    .handler(handle_show_history),
                keyword("running-config", "Current operating configuration")
                    .handler(handle_show_running_config),
                keyword("startup-config", "Contents of startup configuration")
                    .mode(priv_only())
                    .handler(handle_show_startup_config),
                keyword("clock", "Display the system clock")
                    .handler(handle_show_clock),
                keyword("ip", "IP information")
                    .handler(handle_show_ip_incomplete as CmdHandler)
                    .children(vec![
                        keyword("interface", "IP interface status and configuration")
                            .children(vec![
                                keyword("brief", "Brief summary of IP status")
                                    .handler(handle_show_ip_interface_brief),
                            ]),
                        keyword("route", "IP routing table")
                            .handler(handle_show_ip_route),
                        keyword("ospf", "OSPF information")
                            .handler(handle_show_ip_ospf),
                        keyword("protocols", "IP routing protocol process parameters and statistics")
                            .handler(handle_show_ip_protocols),
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
                        param("<name>", ParamType::RestOfLine, "Interface name")
                            .handler(handle_show_interfaces),
                    ]),
                keyword("vlan", "VLAN information")
                    .children(vec![
                        keyword("brief", "VTP all VLAN status in brief")
                            .handler(handle_show_vlan_brief),
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
                    .children(vec![
                        keyword("neighbors", "CDP neighbor entries")
                            .handler(handle_show_cdp_neighbors),
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
                            .handler(handle_show_mac_address_table),
                    ]),
                keyword("spanning-tree", "Spanning tree topology")
                    .handler(handle_show_spanning_tree),
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
                    .handler(handle_show_errdisable),
                keyword("etherchannel", "EtherChannel information")
                    .handler(handle_show_etherchannel),
                keyword("hosts", "IP domain-name, lookup style")
                    .handler(handle_show_hosts),
                keyword("license", "Show license information")
                    .handler(handle_show_license),
                keyword("lldp", "LLDP information")
                    .handler(handle_show_lldp),
                keyword("module", "Module information")
                    .handler(handle_show_module),
                keyword("platform", "Platform specific commands")
                    .handler(handle_show_platform),
                keyword("policy-map", "Show Policy Map")
                    .handler(handle_show_policy_map),
                keyword("port-security", "Show secure port information")
                    .handler(handle_show_port_security),
                keyword("power", "Switch Power")
                    .handler(handle_show_power),
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
                    .handler(handle_show_vtp),
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
        keyword("write", "Write running configuration to memory or network")
            .mode(priv_only())
            .handler(handle_write_memory as CmdHandler)
            .children(vec![
                keyword("memory", "Write to NV memory")
                    .handler(handle_write_memory),
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

        // ping
        keyword("ping", "Send echo messages")
            .children(vec![
                param("<target>", ParamType::Word, "Target address")
                    .handler(handle_ping),
            ]),

        // traceroute
        keyword("traceroute", "Trace route to destination")
            .children(vec![
                param("<target>", ParamType::Word, "Target address")
                    .handler(handle_traceroute),
            ]),

        // help — available in all modes
        keyword("help", "Description of the interactive help system")
            .handler(handle_help_command),

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
        keyword("quit", "Exit from the EXEC")
            .handler(handle_exit),

        // end — no-op in exec mode (real IOS ignores it silently)
        keyword("end", "Return to privileged EXEC mode (no-op in exec)")
            .handler(handle_end_noop),
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
}
