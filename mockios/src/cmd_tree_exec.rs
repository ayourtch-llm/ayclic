//! Exec-mode command tree definitions and handlers for MockIOS.

use std::sync::OnceLock;

use std::net::Ipv4Addr;

use crate::cmd_tree::{keyword, param, CliModeClass, CmdHandler, CommandNode, ModeFilter, ParamType};
use crate::{CliMode, MockIosDevice, PendingInteractive};

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub fn handle_show_version(d: &mut MockIosDevice, _input: &str) {
    let v = d.generate_show_version();
    let p = d.prompt();
    d.queue_output(&format!("\n{}\n{}", v, p));
}

pub fn handle_show_running_config(d: &mut MockIosDevice, _input: &str) {
    let config = d.state.generate_running_config();
    let p = d.prompt();
    d.queue_output(&format!("\n{}\n{}", config, p));
}

pub fn handle_show_startup_config(d: &mut MockIosDevice, _input: &str) {
    let config = d.state.generate_startup_config();
    let p = d.prompt();
    d.queue_output(&format!("\n{}\n{}", config, p));
}

pub fn handle_show_clock(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n*08:00:00.000 UTC Mon Jan 1 2024\n{}", p));
}

pub fn handle_show_ip_interface_brief(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_interface_brief();
}

pub fn handle_show_ip_route(d: &mut MockIosDevice, _input: &str) {
    d.handle_show_ip_route();
}

pub fn handle_show_ip_incomplete(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n% Incomplete command.\n{}", p));
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
        Some(tokens[2..].join(" "))
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
            d.queue_output(&format!("\n{}{}", output_text, p));
        }
        None => {
            // Show all interfaces
            let texts: Vec<String> = d.state.interfaces.iter()
                .map(|i| i.generate_show_interface())
                .collect();
            let all = texts.join("\n");
            d.queue_output(&format!("\n{}{}", all, p));
        }
    }
}

pub fn handle_show_access_lists(d: &mut MockIosDevice, _input: &str) {
    let mut output = String::from("\n");
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
    d.queue_output(&format!("\n{}\n{}", table, p));
}

pub fn handle_show_flash(d: &mut MockIosDevice, _input: &str) {
    d.handle_dir_command("");
}

pub fn handle_show_terminal(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "\nLine 0, Location: \"\", Type: \"\"\nLength: 0 lines, Width: 80 columns\nStatus: Ready, Active\nCapabilities: none\n{}",
        p
    ));
}

pub fn handle_show_history(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    let mut out = String::from("\n");
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
        "\n% Type \"show ?\" for a list of subcommands\n{}",
        p
    ));
}

pub fn handle_configure_terminal(d: &mut MockIosDevice, _input: &str) {
    d.mode = CliMode::Config;
    let p = d.prompt();
    d.queue_output(&format!(
        "\nEnter configuration commands, one per line.  End with CNTL/Z.\n{}",
        p
    ));
}

pub fn handle_configure_alone(d: &mut MockIosDevice, _input: &str) {
    d.pending_interactive = Some(PendingInteractive::ConfigureMethod);
    d.queue_output("\nConfiguring from terminal, memory, or network [terminal]? ");
}

pub fn handle_enable(d: &mut MockIosDevice, _input: &str) {
    if matches!(d.mode, CliMode::PrivilegedExec) {
        // Already in priv exec — no-op (real IOS behavior)
        let p = d.prompt();
        d.queue_output(&format!("\n{}", p));
    } else if d.enable_password.is_some() {
        d.pending_interactive = Some(PendingInteractive::EnablePassword);
        d.queue_output("\nPassword: ");
    } else {
        d.mode = CliMode::PrivilegedExec;
        let p = d.prompt();
        d.queue_output(&format!("\n{}", p));
    }
}

pub fn handle_disable(d: &mut MockIosDevice, _input: &str) {
    d.mode = CliMode::UserExec;
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_terminal_length(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_terminal_width(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
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
    d.queue_output("\nProceed with reload? [confirm]");
}

pub fn handle_reload_cancel(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "\n***\n*** --- SHUTDOWN ABORTED ---\n***\n{}",
        p
    ));
}

pub fn handle_reload_in(d: &mut MockIosDevice, _input: &str) {
    d.pending_interactive = Some(PendingInteractive::ReloadSave);
    d.queue_output("\nSystem configuration has been modified. Save? [yes/no]: ");
}

pub fn handle_write_memory(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\nBuilding configuration...\n[OK]\n{}", p));
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
            "\nType escape sequence to abort.\nSending 5, 100-byte ICMP Echos to {}, timeout is 2 seconds:\n!!!!!\nSuccess rate is 100 percent (5/5), round-trip min/avg/max = 1/1/1 ms\n{}",
            target_str, p
        ));
    } else {
        d.queue_output(&format!(
            "\nType escape sequence to abort.\nSending 5, 100-byte ICMP Echos to {}, timeout is 2 seconds:\n.....\nSuccess rate is 0 percent (0/5)\n{}",
            target_str, p
        ));
    }
}

pub fn handle_traceroute(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\nTracing route\n 1 1 msec\n{}", p));
}

pub fn handle_exit(d: &mut MockIosDevice, _input: &str) {
    // In exec mode, exit closes the session
    d.queue_output("\n");
    d.mode = CliMode::Reloading; // signals connection close
}

pub fn handle_end_noop(d: &mut MockIosDevice, _input: &str) {
    // In exec mode, "end" is a no-op (real IOS ignores it silently).
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
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
    d.queue_output(&format!("\n{}\n{}", text, p));
}

pub fn handle_clock_set(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_debug(d: &mut MockIosDevice, input: &str) {
    let feature = input.trim().strip_prefix("debug").map(|s| s.trim()).unwrap_or("unknown");
    let p = d.prompt();
    d.queue_output(&format!("\n{} debugging is on\n{}", feature, p));
}

pub fn handle_undebug_all(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\nAll possible debugging has been turned off\n{}", p));
}

pub fn handle_undebug(d: &mut MockIosDevice, input: &str) {
    let feature = input.trim().strip_prefix("undebug").map(|s| s.trim()).unwrap_or("unknown");
    let p = d.prompt();
    d.queue_output(&format!("\n{} debugging is off\n{}", feature, p));
}

pub fn handle_clear(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n{}", p));
}

pub fn handle_ssh(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n% Connection refused by remote host\n{}", p));
}

pub fn handle_telnet(d: &mut MockIosDevice, input: &str) {
    let host = input.split_whitespace().nth(1).unwrap_or("unknown");
    let p = d.prompt();
    d.queue_output(&format!("\nTrying {} ... \n% Connection refused by remote host\n{}", host, p));
}

pub fn handle_show_cdp_neighbors(d: &mut MockIosDevice, _input: &str) {
    let output = "\
Capability Codes: R - Router, T - Trans Bridge, B - Source Route Bridge
                  S - Switch, H - Host, I - IGMP, r - Repeater, P - Phone,
                  D - Remote, C - CVTA, M - Two-port Mac Relay

Device ID        Local Intrfce     Holdtme    Capability  Platform  Port ID
";
    let p = d.prompt();
    d.queue_output(&format!("\n{}\n{}", output, p));
}

pub fn handle_show_users(d: &mut MockIosDevice, _input: &str) {
    let output = "\
    Line       User       Host(s)              Idle       Location
*  0 con 0                idle                 00:00:00
";
    let p = d.prompt();
    d.queue_output(&format!("\n{}{}", output, p));
}

pub fn handle_show_ip_ospf(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!("\n%% OSPF: No router process is configured\n{}", p));
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
    d.queue_output(&format!("\n{}\n{}", output, p));
}

pub fn handle_show_processes_cpu(d: &mut MockIosDevice, _input: &str) {
    let output = "\
CPU utilization for five seconds: 5%/0%; one minute: 5%; five minutes: 5%
 PID Runtime(ms)     Invoked      uSecs   5Sec   1Min   5Min TTY Process";
    let p = d.prompt();
    d.queue_output(&format!("\n{}\n{}", output, p));
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
    d.queue_output(&format!("\n{}{}", output, p));
}

pub fn handle_show_arp(d: &mut MockIosDevice, _input: &str) {
    let p = d.prompt();
    d.queue_output(&format!(
        "\nProtocol  Address          Age (min)  Hardware Addr   Type   Interface\n{}",
        p
    ));
}

pub fn handle_show_mac_address_table(d: &mut MockIosDevice, _input: &str) {
    let output = "\
          Mac Address Table
-------------------------------------------

Vlan    Mac Address       Type        Ports
----    -----------       --------    -----
Total Mac Addresses for this criterion: 0
";
    let p = d.prompt();
    d.queue_output(&format!("\n{}{}", output, p));
}

pub fn handle_show_spanning_tree(d: &mut MockIosDevice, _input: &str) {
    let output = "\
VLAN0001
  Spanning tree enabled protocol rstp
  Root ID    Priority    32769
             Address     0000.0000.0000
             This bridge is the root
  Bridge ID  Priority    32769
             Address     0000.0000.0000
";
    let p = d.prompt();
    d.queue_output(&format!("\n{}{}", output, p));
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
                keyword("boot", "Boot and startup information")
                    .handler(handle_show_boot),
                keyword("interfaces", "Interface status and configuration")
                    .handler(handle_show_interfaces)
                    .children(vec![
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
}
