//! acl-cfg-test - Test atomic config push with large ACLs
//!
//! Usage:
//!   cargo run --example acl-cfg-test -- create-test <aclname>
//!   cargo run --example acl-cfg-test -- delete <aclname>
//!
//! create-test: Generates a 5000-line ACL and pushes it atomically.
//!              First removes any existing ACL with the same name,
//!              then creates the full ACL.
//!
//! delete:      Shows the running config for the ACL, then removes
//!              each ACE in reverse order, and finally deletes the ACL.

use ayclic::{CiscoIosConn, ConnectionType};
use ayclic::conn::ChangeSafety;
use std::env;
use std::time::{Duration, Instant};
use tracing::{error, info};

const TARGET: &str = "192.168.0.130";
const USERNAME: &str = "ayourtch";
const PASSWORD: &str = "cisco123";
const DEFAULT_ACL_SIZE: u32 = 100;

/// Generate a single ACL entry for the given index (0-based).
/// Returns the line WITHOUT leading space or sequence number —
/// those are added by the caller.
fn generate_ace(i: u32) -> String {
    let action = if i % 10 == 0 { "deny" } else { "permit" };

    // Cycle through protocols
    match i % 4 {
        0 => {
            // ip — src 10.x.y.0/24 -> dst 172.16.x.0/24
            let sa = (i / 256) % 256;
            let sb = i % 256;
            let da = ((i / 256) + 1) % 256;
            format!(
                "{} ip 10.{}.{}.0 0.0.0.255 172.16.{}.0 0.0.0.255",
                action, sa, sb, da
            )
        }
        1 => {
            // tcp — src 10.x.y.0/24 -> any eq <port>
            let sa = (i / 256) % 256;
            let sb = i % 256;
            let port = 1024 + (i % 64000);
            format!(
                "{} tcp 10.{}.{}.0 0.0.0.255 any eq {}",
                action, sa, sb, port
            )
        }
        2 => {
            // udp — src 10.x.y.0/24 -> any eq <port>
            let sa = (i / 256) % 256;
            let sb = i % 256;
            let port = 53 + (i % 64000);
            format!(
                "{} udp 10.{}.{}.0 0.0.0.255 any eq {}",
                action, sa, sb, port
            )
        }
        3 => {
            // icmp — src 10.x.y.0/24 -> any
            let sa = (i / 256) % 256;
            let sb = i % 256;
            format!("{} icmp 10.{}.{}.0 0.0.0.255 any", action, sa, sb)
        }
        _ => unreachable!(),
    }
}

/// Build the config snippet to create the ACL.
/// First removes any existing ACL, then creates the full one.
fn build_create_config(acl_name: &str, count: u32) -> String {
    let mut lines = Vec::new();

    // Remove existing ACL first
    lines.push(format!("no ip access-list extended {}", acl_name));

    // Create the ACL
    lines.push(format!("ip access-list extended {}", acl_name));

    // Generate ACEs with sequence numbers 10, 20, 30, ...
    for i in 0..count {
        let seq = (i + 1) * 10;
        let ace = generate_ace(i);
        lines.push(format!(" {} {}", seq, ace));
    }

    lines.join("\n")
}

/// Parse ACE lines from show running-config output.
/// Returns the trimmed ACE content (e.g. "permit ip 10.0.0.0 0.0.0.255 any").
/// Handles both formats:
///   - IOS 12.2: " permit ip ..." / " deny   ip ..."
///   - IOS 15.x: " 10 permit ip ..." / " 20 deny ip ..."
fn parse_ace_lines(show_output: &str) -> Vec<String> {
    let mut aces = Vec::new();
    for line in show_output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("permit") || trimmed.starts_with("deny") {
            // Normalize whitespace (IOS pads "deny" with extra spaces)
            let normalized: String = trimmed
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            aces.push(normalized);
        } else if let Some(first_word) = trimmed.split_whitespace().next() {
            // Check for sequence number prefix (IOS 15.x)
            if first_word.parse::<u32>().is_ok() {
                let rest = trimmed[first_word.len()..].trim();
                if rest.starts_with("permit") || rest.starts_with("deny") {
                    let normalized: String = rest
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    aces.push(normalized);
                }
            }
        }
    }
    aces
}

/// Build the config snippet to delete ACEs in reverse order, then delete the ACL.
fn build_delete_config(acl_name: &str, aces: &[String]) -> String {
    let mut lines = Vec::new();

    // Enter ACL config context
    lines.push(format!("ip access-list extended {}", acl_name));

    // Delete ACEs from last to first using "no <ace-content>"
    for ace in aces.iter().rev() {
        lines.push(format!(" no {}", ace));
    }

    // Exit ACL context, then delete the ACL itself
    lines.push("exit".to_string());
    lines.push(format!("no ip access-list extended {}", acl_name));

    lines.join("\n")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = env::args().collect();

    // Parse --safe flag
    let mut safe_reload = false;
    let mut filtered_args: Vec<String> = Vec::new();
    for arg in &args {
        if arg == "--safe" {
            safe_reload = true;
        } else {
            filtered_args.push(arg.clone());
        }
    }

    if filtered_args.len() < 3 {
        eprintln!("Usage:");
        eprintln!("  {} [--safe] create-test <aclname> [count]", filtered_args[0]);
        eprintln!("  {} [--safe] delete <aclname>", filtered_args[0]);
        eprintln!();
        eprintln!("  count: number of ACL entries (default {})", DEFAULT_ACL_SIZE);
        eprintln!("  --safe: schedule 'reload in 10' before applying (auto-cancelled on success)");
        std::process::exit(1);
    }

    let command = &filtered_args[1];
    let acl_name = &filtered_args[2];
    let acl_size: u32 = filtered_args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ACL_SIZE);
    let safety = if safe_reload {
        ChangeSafety::DelayedReload { minutes: 10 }
    } else {
        ChangeSafety::None
    };

    // Connect with 5-minute read timeout (applying large configs takes time)
    eprintln!("Connecting to {} ...", TARGET);
    let mut conn = CiscoIosConn::with_timeouts(
        TARGET,
        ConnectionType::Ssh,
        USERNAME,
        PASSWORD,
        Duration::from_secs(30),
        Duration::from_secs(300),
    )
    .await?;
    eprintln!("Connected.");

    match command.as_str() {
        "create-test" => {
            eprintln!(
                "Generating {} ACL entries for '{}'...",
                acl_size, acl_name
            );
            let config = build_create_config(acl_name, acl_size);
            let line_count = config.lines().count();
            eprintln!(
                "Config snippet: {} lines, {} bytes",
                line_count,
                config.len()
            );

            eprintln!("Pushing config atomically (this may take a few minutes)...");
            let start = Instant::now();
            let output = conn.config_atomic(&config, safety.clone()).await?;
            let elapsed = start.elapsed();

            eprintln!("Done in {:.1}s", elapsed.as_secs_f64());
            println!("=== copy output ===\n{}", output);

            // Verify the ACL exists in running config
            eprintln!("Verifying...");
            tokio::time::sleep(Duration::from_secs(2)).await;
            let verify = conn
                .run_cmd(&format!(
                    "show running-config | include ip access-list extended {}",
                    acl_name
                ))
                .await?;
            if verify.contains(&format!("ip access-list extended {}", acl_name)) {
                eprintln!("ACL '{}' exists in running config", acl_name);
            } else {
                eprintln!("Warning: ACL '{}' not found in running config", acl_name);
                eprintln!("  output: {}", verify.trim());
            }
        }

        "delete" => {
            eprintln!("Fetching running config for ACL '{}'...", acl_name);
            // Use "| begin" (works on IOS 12.2+) instead of "| section" (15.x+ only)
            let show_cmd = format!(
                "show running-config | begin ip access-list extended {}",
                acl_name
            );
            let show_output = conn.run_cmd(&show_cmd).await?;

            // Extract just the ACL block: from "ip access-list extended <name>"
            // until the next line that doesn't start with a space (next global command)
            let acl_header = format!("ip access-list extended {}", acl_name);
            let mut acl_block = String::new();
            let mut in_acl = false;
            for line in show_output.lines() {
                // Exact match: line must be the header (possibly with trailing whitespace)
                if line.trim() == acl_header {
                    in_acl = true;
                    acl_block.push_str(line);
                    acl_block.push('\n');
                } else if in_acl {
                    if line.starts_with(' ') || line.starts_with('\t') {
                        acl_block.push_str(line);
                        acl_block.push('\n');
                    } else {
                        break; // Next global config command
                    }
                }
            }

            println!("=== current config ===\n{}", acl_block);

            let aces = parse_ace_lines(&acl_block);
            if aces.is_empty() {
                eprintln!("No ACEs found for ACL '{}'. Nothing to delete.", acl_name);
                conn.disconnect().await?;
                return Ok(());
            }

            eprintln!(
                "Found {} ACEs. Building reverse-delete config...",
                aces.len()
            );

            let config = build_delete_config(acl_name, &aces);
            let line_count = config.lines().count();
            eprintln!(
                "Delete snippet: {} lines, {} bytes",
                line_count,
                config.len()
            );

            eprintln!("Pushing delete config atomically (this may take a few minutes)...");
            let start = Instant::now();
            let output = conn.config_atomic(&config, safety.clone()).await?;
            let elapsed = start.elapsed();

            eprintln!("Done in {:.1}s", elapsed.as_secs_f64());
            println!("=== copy output ===\n{}", output);

            // Verify deletion
            tokio::time::sleep(Duration::from_secs(2)).await;
            let verify = conn
                .run_cmd(&format!(
                    "show running-config | include ip access-list extended {}",
                    acl_name
                ))
                .await?;
            // Check if the ACL header appears as actual config (not just the command echo)
            let acl_still_exists = verify
                .lines()
                .any(|l| {
                    let t = l.trim();
                    t == format!("ip access-list extended {}", acl_name)
                });
            if !acl_still_exists {
                eprintln!("ACL '{}' successfully deleted.", acl_name);
            } else {
                eprintln!("Warning: ACL '{}' may still exist.", acl_name);
            }
        }

        other => {
            error!("Unknown command: {}", other);
            eprintln!("Unknown command: '{}'. Use 'create-test' or 'delete'.", other);
            std::process::exit(1);
        }
    }

    conn.disconnect().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ace_first_entry_is_deny_ip() {
        // i=0: i%10==0 so deny, i%4==0 so ip
        let ace = generate_ace(0);
        assert!(ace.starts_with("deny ip 10.0.0.0"));
    }

    #[test]
    fn test_generate_ace_deny_every_10th() {
        assert!(generate_ace(0).starts_with("deny"));
        assert!(generate_ace(10).starts_with("deny"));
        assert!(generate_ace(20).starts_with("deny"));
        assert!(generate_ace(1).starts_with("permit"));
        assert!(generate_ace(5).starts_with("permit"));
    }

    #[test]
    fn test_generate_ace_protocol_cycling() {
        assert!(generate_ace(0).contains(" ip "));
        assert!(generate_ace(1).contains(" tcp "));
        assert!(generate_ace(2).contains(" udp "));
        assert!(generate_ace(3).contains(" icmp "));
        assert!(generate_ace(4).contains(" ip "));
    }

    #[test]
    fn test_generate_ace_tcp_has_port() {
        let ace = generate_ace(1);
        assert!(ace.contains(" eq "));
    }

    #[test]
    fn test_generate_ace_udp_has_port() {
        let ace = generate_ace(2);
        assert!(ace.contains(" eq "));
    }

    #[test]
    fn test_generate_ace_all_unique() {
        let mut aces: Vec<String> = (0..DEFAULT_ACL_SIZE).map(generate_ace).collect();
        let total = aces.len();
        aces.sort();
        aces.dedup();
        assert_eq!(aces.len(), total, "All ACEs should be unique");
    }

    #[test]
    fn test_build_create_config_structure() {
        let config = build_create_config("TEST_ACL", DEFAULT_ACL_SIZE);
        let lines: Vec<&str> = config.lines().collect();

        // First line: delete existing
        assert_eq!(lines[0], "no ip access-list extended TEST_ACL");
        // Second line: create ACL
        assert_eq!(lines[1], "ip access-list extended TEST_ACL");
        // Remaining: DEFAULT_ACL_SIZE ACEs with sequence numbers
        assert_eq!(lines.len(), 2 + DEFAULT_ACL_SIZE as usize);
        // First ACE has seq 10
        assert!(lines[2].starts_with(" 10 "));
        // Last ACE has seq = DEFAULT_ACL_SIZE * 10
        let last_seq = format!(" {} ", DEFAULT_ACL_SIZE * 10);
        assert!(lines[1 + DEFAULT_ACL_SIZE as usize].starts_with(&last_seq));
    }

    #[test]
    fn test_build_create_config_sequence_numbers() {
        let config = build_create_config("TEST", DEFAULT_ACL_SIZE);
        for (i, line) in config.lines().skip(2).enumerate() {
            let expected_seq = (i as u32 + 1) * 10;
            let trimmed = line.trim();
            let first_word = trimmed.split_whitespace().next().unwrap();
            let seq: u32 = first_word.parse().unwrap();
            assert_eq!(seq, expected_seq);
        }
    }

    #[test]
    fn test_parse_ace_lines_ios12_format() {
        let output = r#"ip access-list extended TEST
 deny   ip 10.0.0.0 0.0.0.255 172.16.1.0 0.0.0.255
 permit tcp 10.0.1.0 0.0.0.255 any eq 1025
 permit udp 10.0.2.0 0.0.0.255 any eq 55
Router#"#;

        let aces = parse_ace_lines(output);
        assert_eq!(aces.len(), 3);
        assert_eq!(aces[0], "deny ip 10.0.0.0 0.0.0.255 172.16.1.0 0.0.0.255");
        assert_eq!(aces[1], "permit tcp 10.0.1.0 0.0.0.255 any eq 1025");
        assert_eq!(aces[2], "permit udp 10.0.2.0 0.0.0.255 any eq 55");
    }

    #[test]
    fn test_parse_ace_lines_ios15_with_seq_numbers() {
        let output = r#"ip access-list extended TEST
 10 permit ip any any
 20 deny tcp any any eq 80
Router#"#;
        let aces = parse_ace_lines(output);
        assert_eq!(aces.len(), 2);
        assert_eq!(aces[0], "permit ip any any");
        assert_eq!(aces[1], "deny tcp any any eq 80");
    }

    #[test]
    fn test_parse_ace_lines_empty() {
        let output = "ip access-list extended TEST\nRouter#";
        let aces = parse_ace_lines(output);
        assert!(aces.is_empty());
    }

    #[test]
    fn test_parse_ace_lines_ignores_non_ace_lines() {
        let output = r#"ip access-list extended TEST
 10 permit ip any any
 remark this is a comment
 20 deny tcp any any eq 80
Router#"#;
        let aces = parse_ace_lines(output);
        assert_eq!(aces.len(), 2);
    }

    #[test]
    fn test_build_delete_config_reverse_order() {
        let aces = vec![
            "permit ip 10.0.0.0 0.0.0.255 any".to_string(),
            "deny tcp any any eq 80".to_string(),
            "permit udp any any eq 53".to_string(),
        ];
        let config = build_delete_config("TEST_ACL", &aces);
        let lines: Vec<&str> = config.lines().collect();

        assert_eq!(lines[0], "ip access-list extended TEST_ACL");
        assert_eq!(lines[1], " no permit udp any any eq 53");
        assert_eq!(lines[2], " no deny tcp any any eq 80");
        assert_eq!(lines[3], " no permit ip 10.0.0.0 0.0.0.255 any");
        assert_eq!(lines[4], "exit");
        assert_eq!(lines[5], "no ip access-list extended TEST_ACL");
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn test_build_delete_config_single_entry() {
        let aces = vec!["permit ip any any".to_string()];
        let config = build_delete_config("MY_ACL", &aces);
        let lines: Vec<&str> = config.lines().collect();

        assert_eq!(lines[0], "ip access-list extended MY_ACL");
        assert_eq!(lines[1], " no permit ip any any");
        assert_eq!(lines[2], "exit");
        assert_eq!(lines[3], "no ip access-list extended MY_ACL");
    }
}
