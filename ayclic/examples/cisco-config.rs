//! cisco-config — Two-phase Cisco IOS config management
//!
//! Uses aycicdiff to compute config deltas and ayclic to apply them atomically.
//!
//! # Prepare phase: compute delta
//!
//!   cargo run --example cisco-config -- prepare \
//!     --device 10.1.1.1 --user admin --password secret \
//!     --target desired.cfg -o delta.cfg
//!
//! # Apply phase: apply delta atomically
//!
//!   cargo run --example cisco-config -- apply \
//!     --device 10.1.1.1 --user admin --password secret \
//!     --delta delta.cfg --safety-reload 5
//!
//! # One-shot: prepare + apply in one step
//!
//!   cargo run --example cisco-config -- push \
//!     --device 10.1.1.1 --user admin --password secret \
//!     --target desired.cfg --safety-reload 5

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use tracing::{error, info};

use ayclic::{CiscoIosConn, ChangeSafety, ConnectionType};

#[derive(Parser)]
#[command(name = "cisco-config")]
#[command(about = "Two-phase Cisco IOS config management with atomic apply")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Prepare: fetch running config, compute delta, write to file
    Prepare {
        #[command(flatten)]
        conn: ConnArgs,

        /// Path to target (desired) configuration file
        #[arg(short, long)]
        target: PathBuf,

        /// Output file for the computed delta (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Path to "show version" output (optional, for platform-aware defaults)
        #[arg(long)]
        version_file: Option<PathBuf>,

        /// Fetch "show version" from the device instead of a file
        #[arg(long, default_value_t = true)]
        fetch_version: bool,
    },

    /// Apply: push a pre-computed delta to the device atomically
    Apply {
        #[command(flatten)]
        conn: ConnArgs,

        /// Path to the delta config file to apply
        #[arg(short, long)]
        delta: PathBuf,

        /// Schedule a safety reload (minutes) before applying.
        /// If the device becomes unreachable, it will reload and revert.
        #[arg(long)]
        safety_reload: Option<u32>,
    },

    /// Push: prepare + apply in one step (compute delta, then apply)
    Push {
        #[command(flatten)]
        conn: ConnArgs,

        /// Path to target (desired) configuration file
        #[arg(short, long)]
        target: PathBuf,

        /// Schedule a safety reload (minutes) before applying
        #[arg(long)]
        safety_reload: Option<u32>,

        /// Path to "show version" output (optional)
        #[arg(long)]
        version_file: Option<PathBuf>,

        /// Also write the computed delta to this file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Parser)]
struct ConnArgs {
    /// Device address (IP or hostname, with optional :port)
    #[arg(short, long)]
    device: String,

    /// Username for authentication
    #[arg(short, long)]
    user: String,

    /// Password for authentication
    #[arg(short, long)]
    password: String,

    /// Connection type
    #[arg(long, value_enum, default_value_t = ConnType::Ssh)]
    conntype: ConnType,

    /// Connection timeout in seconds
    #[arg(long, default_value_t = 30)]
    timeout: u64,

    /// Read timeout in seconds
    #[arg(long, default_value_t = 60)]
    read_timeout: u64,

    /// Log session transcript to file
    #[arg(long)]
    transcript: Option<PathBuf>,
}

#[derive(ValueEnum, Clone)]
enum ConnType {
    Ssh,
    Telnet,
    KbdInteractive,
}

impl ConnType {
    fn to_connection_type(&self) -> ConnectionType {
        match self {
            ConnType::Ssh => ConnectionType::Ssh,
            ConnType::Telnet => ConnectionType::Telnet,
            ConnType::KbdInteractive => ConnectionType::SshKbdInteractive,
        }
    }
}

async fn connect(args: &ConnArgs) -> Result<CiscoIosConn, Box<dyn std::error::Error>> {
    let conn = CiscoIosConn::with_timeouts(
        &args.device,
        args.conntype.to_connection_type(),
        &args.user,
        &args.password,
        Duration::from_secs(args.timeout),
        Duration::from_secs(args.read_timeout),
    )
    .await?;
    Ok(conn)
}

async fn fetch_running_config(
    conn: &mut CiscoIosConn,
) -> Result<String, Box<dyn std::error::Error>> {
    info!("Fetching running configuration...");
    let output = conn.run_cmd("show running-config").await?;
    Ok(output)
}

async fn fetch_show_version(
    conn: &mut CiscoIosConn,
) -> Result<String, Box<dyn std::error::Error>> {
    info!("Fetching show version...");
    let output = conn.run_cmd("show version").await?;
    Ok(output)
}

fn compute_delta(
    running: &str,
    target: &str,
    show_version: Option<&str>,
) -> String {
    info!("Computing configuration delta...");
    aycicdiff::generate_delta(running, target, show_version)
}

async fn do_prepare(
    conn_args: &ConnArgs,
    target_path: &PathBuf,
    output_path: &Option<PathBuf>,
    version_file: &Option<PathBuf>,
    fetch_version: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let target_config = std::fs::read_to_string(target_path)
        .map_err(|e| format!("Failed to read target config {}: {}", target_path.display(), e))?;

    eprintln!("Connecting to {}...", conn_args.device);
    let mut conn = connect(conn_args).await?;
    eprintln!("Connected.");

    // Get running config
    let running = fetch_running_config(&mut conn).await?;

    // Get show version (from file or device)
    let show_version = if let Some(vf) = version_file {
        Some(std::fs::read_to_string(vf)
            .map_err(|e| format!("Failed to read version file {}: {}", vf.display(), e))?)
    } else if fetch_version {
        match fetch_show_version(&mut conn).await {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("Warning: could not fetch show version: {}", e);
                None
            }
        }
    } else {
        None
    };

    conn.disconnect().await?;

    // Compute delta
    let delta = compute_delta(&running, &target_config, show_version.as_deref());

    if delta.is_empty() {
        eprintln!("No changes needed — running config matches target.");
    } else {
        let line_count = delta.lines().count();
        eprintln!("Delta computed: {} lines of changes.", line_count);

        if let Some(out) = output_path {
            std::fs::write(out, &delta)
                .map_err(|e| format!("Failed to write delta to {}: {}", out.display(), e))?;
            eprintln!("Delta written to {}", out.display());
        } else {
            println!("{}", delta);
        }
    }

    Ok(delta)
}

async fn do_apply(
    conn_args: &ConnArgs,
    delta_path: &PathBuf,
    safety_reload: Option<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let delta = std::fs::read_to_string(delta_path)
        .map_err(|e| format!("Failed to read delta file {}: {}", delta_path.display(), e))?;

    if delta.trim().is_empty() {
        eprintln!("Delta file is empty — nothing to apply.");
        return Ok(());
    }

    let line_count = delta.lines().count();
    eprintln!(
        "Applying {} lines of config to {}...",
        line_count, conn_args.device
    );

    let safety = match safety_reload {
        Some(minutes) => {
            eprintln!(
                "Safety reload scheduled: device will reload in {} minutes if unreachable.",
                minutes
            );
            ChangeSafety::DelayedReload { minutes }
        }
        None => ChangeSafety::None,
    };

    eprintln!("Connecting to {}...", conn_args.device);
    let mut conn = connect(conn_args).await?;
    eprintln!("Connected.");

    match conn.config_atomic(&delta, safety).await {
        Ok(output) => {
            eprintln!("Configuration applied successfully!");
            if !output.trim().is_empty() {
                eprintln!("Device output: {}", output.trim());
            }
        }
        Err(e) => {
            error!("Failed to apply configuration: {}", e);
            return Err(e.into());
        }
    }

    conn.disconnect().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Prepare {
            conn,
            target,
            output,
            version_file,
            fetch_version,
        } => {
            do_prepare(&conn, &target, &output, &version_file, fetch_version).await?;
        }

        Command::Apply {
            conn,
            delta,
            safety_reload,
        } => {
            do_apply(&conn, &delta, safety_reload).await?;
        }

        Command::Push {
            conn,
            target,
            safety_reload,
            version_file,
            output,
        } => {
            // Phase 1: prepare
            eprintln!("=== Phase 1: Prepare ===");
            let delta = do_prepare(&conn, &target, &output, &version_file, true).await?;

            if delta.is_empty() {
                eprintln!("No changes to apply.");
                return Ok(());
            }

            // Phase 2: apply
            eprintln!();
            eprintln!("=== Phase 2: Apply ===");

            let safety = match safety_reload {
                Some(minutes) => {
                    eprintln!(
                        "Safety reload scheduled: {} minutes.",
                        minutes
                    );
                    ChangeSafety::DelayedReload { minutes }
                }
                None => ChangeSafety::None,
            };

            eprintln!("Connecting to {}...", conn.device);
            let mut ios_conn = connect(&conn).await?;
            eprintln!("Connected.");

            match ios_conn.config_atomic(&delta, safety).await {
                Ok(output) => {
                    eprintln!("Configuration applied successfully!");
                    if !output.trim().is_empty() {
                        eprintln!("Device output: {}", output.trim());
                    }
                }
                Err(e) => {
                    error!("Failed to apply configuration: {}", e);
                    return Err(e.into());
                }
            }

            ios_conn.disconnect().await?;
        }
    }

    Ok(())
}
