//! cisco-cmd - Execute commands on Cisco IOS devices via Telnet or SSH
//!
//! Usage:
//!   cargo run --example cisco-cmd -- [OPTIONS] <target> <username> <password> <command>
//!   cargo run --example cisco-cmd -- --key <keyfile> <target> <username> <command>
//!
//! Options:
//!   --telnet            Use TELNET (default)
//!   --ssh               Use SSH with password authentication
//!   --kbd-interactive   Use SSH with keyboard-interactive authentication
//!   --key <file>        Use SSH with RSA public key authentication
//!
//! Examples:
//!   # Telnet (default):
//!   cargo run --example cisco-cmd -- 192.168.1.1 admin password "show version"
//!
//!   # SSH password:
//!   cargo run --example cisco-cmd -- --ssh 192.168.1.1 admin password "show version"
//!
//!   # SSH key:
//!   cargo run --example cisco-cmd -- --key ~/.ssh/id_rsa 192.168.1.1 admin "show version"
//!
//!   # SSH keyboard-interactive:
//!   cargo run --example cisco-cmd -- --kbd-interactive 192.168.1.1 admin password "show version"
//!
//!   # Multiple commands (semicolon-separated):
//!   cargo run --example cisco-cmd -- --ssh 192.168.1.1 admin pass "show version;show ip route"

use ayclic::{CiscoIosConn, ConnectionType};
use std::env;
use std::path::Path;
use tracing::{error, info};

fn print_usage(prog: &str) {
    eprintln!("Usage:");
    eprintln!("  {} [OPTIONS] <target> <username> <password> <command>", prog);
    eprintln!("  {} --key <keyfile> <target> <username> <command>", prog);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --telnet            Use TELNET connection");
    eprintln!("  --ssh               Use SSH with password authentication (default)");
    eprintln!("  --kbd-interactive   Use SSH with keyboard-interactive authentication");
    eprintln!("  --key <file>        Use SSH with RSA public key authentication");
    eprintln!();
    eprintln!("Multiple commands can be separated with semicolons.");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} 192.168.1.1 admin password \"show version\"", prog);
    eprintln!(
        "  {} --ssh 192.168.1.1 admin password \"show version\"",
        prog
    );
    eprintln!(
        "  {} --key ~/.ssh/id_rsa 192.168.1.1 admin \"show version\"",
        prog
    );
    eprintln!(
        "  {} --kbd-interactive 192.168.1.1 admin password \"show version\"",
        prog
    );
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

    // Parse flags
    let mut conntype = ConnectionType::Ssh;
    let mut key_file: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut skip_next = false;

    for (i, arg) in args.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        match arg.as_str() {
            "--telnet" => conntype = ConnectionType::Telnet,
            "--ssh" => conntype = ConnectionType::Ssh,
            "--kbd-interactive" => conntype = ConnectionType::SshKbdInteractive,
            "--key" => {
                conntype = ConnectionType::SshKey;
                if let Some(next) = args.get(i + 1) {
                    key_file = Some(next.clone());
                    skip_next = true;
                } else {
                    eprintln!("Error: --key requires a filename argument");
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                print_usage(&args[0]);
                std::process::exit(0);
            }
            _ => positional.push(arg.clone()),
        }
    }

    // Validate positional args
    let (target, username, password, command) = if conntype == ConnectionType::SshKey {
        // Key auth: target username command (no password)
        if positional.len() < 3 {
            print_usage(&args[0]);
            std::process::exit(1);
        }
        (
            positional[0].clone(),
            positional[1].clone(),
            String::new(),
            positional[2].clone(),
        )
    } else {
        // Password-based: target username password command
        if positional.len() < 4 {
            print_usage(&args[0]);
            std::process::exit(1);
        }
        (
            positional[0].clone(),
            positional[1].clone(),
            positional[2].clone(),
            positional[3].clone(),
        )
    };

    let auth_label = match conntype {
        ConnectionType::Telnet => "telnet",
        ConnectionType::Ssh => "SSH password",
        ConnectionType::SshKey => "SSH key",
        ConnectionType::SshKbdInteractive => "SSH keyboard-interactive",
    };

    info!("=== cisco-cmd ({}) ===", auth_label);
    info!("Target: {}", target);
    info!("Username: {}", username);
    info!("Command: {}", command);

    // Connect
    let mut conn = if conntype == ConnectionType::SshKey {
        let key_path = key_file.as_ref().expect("--key file required");

        // Show public key info if available
        let pub_key_path = format!("{}.pub", key_path);
        if Path::new(&pub_key_path).exists() {
            eprintln!("Public key: {}", pub_key_path);
        }

        let private_key = std::fs::read(key_path)
            .map_err(|e| format!("Failed to read private key {}: {}", key_path, e))?;

        CiscoIosConn::new_with_key(&target, &username, &private_key).await?
    } else {
        CiscoIosConn::new(&target, conntype, &username, &password).await?
    };

    eprintln!("Connected via {}!", auth_label);

    // Execute commands (semicolon-separated)
    for cmd in command.split(';') {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            continue;
        }
        match conn.run_cmd(cmd).await {
            Ok(output) => {
                println!("\n=== {} ===", cmd);
                println!("{}", output);
            }
            Err(e) => {
                error!("Error executing '{}': {}", cmd, e);
                eprintln!("\nError executing '{}': {}", cmd, e);
                std::process::exit(1);
            }
        }
    }

    conn.disconnect().await?;
    info!("=== Done ===");
    Ok(())
}
