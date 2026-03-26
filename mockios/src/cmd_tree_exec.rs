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

pub fn handle_show_vlan_brief(d: &mut MockIosDevice, _input: &str) {
    let table = d.state.generate_show_vlan_brief();
    let p = d.prompt();
    d.queue_output(&format!("\n{}\n{}", table, p));
}

pub fn handle_show_flash(d: &mut MockIosDevice, _input: &str) {
    d.handle_dir_command("");
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
    if d.enable_password.is_some() {
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

// ─── Tree ─────────────────────────────────────────────────────────────────────

static EXEC_TREE: OnceLock<Vec<CommandNode>> = OnceLock::new();

pub fn exec_tree() -> &'static Vec<CommandNode> {
    EXEC_TREE.get_or_init(build_exec_tree)
}

fn priv_only() -> ModeFilter {
    ModeFilter::Only(vec![CliModeClass::PrivExec])
}

fn user_only() -> ModeFilter {
    ModeFilter::Only(vec![CliModeClass::UserExec])
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
            ]),

        // configure [priv only]
        keyword("configure", "Enter configuration mode")
            .mode(priv_only())
            .handler(handle_configure_alone as CmdHandler)
            .children(vec![
                keyword("terminal", "Configure from the terminal")
                    .handler(handle_configure_terminal),
            ]),

        // enable [user only]
        keyword("enable", "Turn on privileged commands")
            .mode(user_only())
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
    fn test_exec_enable_hidden_in_priv() {
        let tree = exec_tree();
        let mode = CliMode::PrivilegedExec;
        let result = parse("enable", tree, &mode);
        assert!(
            matches!(result, crate::cmd_tree::ParseResult::InvalidInput { .. }),
            "enable should be invalid in priv exec"
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
