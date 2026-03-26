//! mockios — Mock Cisco IOS CLI server
//!
//! Runs a simulated Cisco IOS device on stdin/stdout, telnet, or SSH.
//!
//! # Interactive mode (default):
//!   mockios
//!   mockios --hostname Switch1 --login admin:cisco
//!
//! # Telnet server:
//!   mockios --telnet 0.0.0.0:2323
//!   mockios --telnet 0.0.0.0:2323 --hostname Router1 --login admin:cisco
//!
//! # SSH server:
//!   mockios --ssh 0.0.0.0:2222
//!   mockios --ssh 0.0.0.0:2222 --hostname Router1 --login admin:cisco

use std::io::{self, BufRead, Write};
use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{error, info};

use mockios::{CliMode, MockIosDevice};
use ayclic::raw_transport::RawTransport;

#[derive(Parser)]
#[command(name = "mockios")]
#[command(about = "Mock Cisco IOS CLI — stdin/stdout, telnet, or SSH server")]
struct Cli {
    /// Device hostname
    #[arg(long, default_value = "Router1")]
    hostname: String,

    /// Require login (username:password)
    #[arg(long)]
    login: Option<String>,

    /// Enable password
    #[arg(long)]
    enable: Option<String>,

    /// Run as telnet server on this address:port
    #[arg(long)]
    telnet: Option<SocketAddr>,

    /// Run as SSH server on this address:port
    #[arg(long)]
    ssh: Option<SocketAddr>,

    /// IOS version string
    #[arg(long, default_value = "15.1(4)M")]
    version: String,

    /// Device model
    #[arg(long, default_value = "C2951")]
    model: String,
}

fn build_device(cli: &Cli) -> MockIosDevice {
    let mut device = MockIosDevice::new(&cli.hostname)
        .with_version(&cli.version)
        .with_model(&cli.model);

    if let Some(ref login) = cli.login {
        let parts: Vec<&str> = login.splitn(2, ':').collect();
        if parts.len() == 2 {
            device = device.with_login(parts[0], parts[1]);
        } else {
            eprintln!("Warning: --login should be username:password, got {:?}", login);
        }
    }

    if let Some(ref enable) = cli.enable {
        device = device.with_enable(enable);
    }

    device
}

/// Run interactive CLI on stdin/stdout
async fn run_interactive(cli: &Cli) {
    let mut device = build_device(cli);

    // Get initial output (prompt or login banner)
    let initial = device.receive(Duration::from_secs(1)).await.unwrap();
    print!("{}", String::from_utf8_lossy(&initial));
    io::stdout().flush().unwrap();

    // Read lines from stdin using blocking I/O on a separate thread
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(16);
    tokio::task::spawn_blocking(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    if tx.blocking_send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    while let Some(line) = rx.recv().await {
        if (line == "quit" || line == "exit")
            && (device.mode == CliMode::PrivilegedExec
                || device.mode == CliMode::UserExec
                || device.mode == CliMode::Login
                || device.mode == CliMode::LoginPassword)
        {
            break;
        }

        device
            .send(format!("{}\n", line).as_bytes())
            .await
            .unwrap();

        let output = device.receive(Duration::from_secs(1)).await.unwrap();
        if !output.is_empty() {
            print!("{}", String::from_utf8_lossy(&output));
            io::stdout().flush().unwrap();
        }
    }
}

/// Run as a telnet server
async fn run_telnet_server(cli: &Cli, addr: SocketAddr) {
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind telnet on {}: {}", addr, e));
    info!("MockIOS telnet server listening on {}", addr);
    eprintln!("MockIOS telnet server listening on {}", addr);
    eprintln!("Hostname: {}, Version: {}", cli.hostname, cli.version);

    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                error!("Accept failed: {}", e);
                continue;
            }
        };
        info!("Telnet connection from {}", peer);
        eprintln!("Connection from {}", peer);

        let cli_hostname = cli.hostname.clone();
        let cli_login = cli.login.clone();
        let cli_enable = cli.enable.clone();
        let cli_version = cli.version.clone();
        let cli_model = cli.model.clone();

        tokio::spawn(async move {
            let mut device = MockIosDevice::new(&cli_hostname)
                .with_version(&cli_version)
                .with_model(&cli_model);

            if let Some(ref login) = cli_login {
                let parts: Vec<&str> = login.splitn(2, ':').collect();
                if parts.len() == 2 {
                    device = device.with_login(parts[0], parts[1]);
                }
            }
            if let Some(ref enable) = cli_enable {
                device = device.with_enable(enable);
            }

            // Send initial prompt/banner
            let initial = device.receive(Duration::from_secs(1)).await.unwrap();
            if stream.write_all(&initial).await.is_err() {
                return;
            }

            let mut buf = vec![0u8; 4096];
            loop {
                let n = match stream.read(&mut buf).await {
                    Ok(0) => break, // connection closed
                    Ok(n) => n,
                    Err(_) => break,
                };

                // Feed input to the mock device
                if device.send(&buf[..n]).await.is_err() {
                    break;
                }

                // Get response
                let output = match device.receive(Duration::from_secs(1)).await {
                    Ok(data) => data,
                    Err(_) => break,
                };

                if !output.is_empty() {
                    if stream.write_all(&output).await.is_err() {
                        break;
                    }
                }

                // Check if device wants to disconnect (exit/quit)
                if device.is_reloading() {
                    break;
                }
            }
            info!("Connection from {} closed", peer);
        });
    }
}

/// Build an SSH_MSG_CHANNEL_DATA packet.
fn ssh_channel_data(channel_id: u32, data: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(9 + data.len());
    msg.push(94); // SSH_MSG_CHANNEL_DATA
    msg.extend_from_slice(&channel_id.to_be_bytes());
    msg.extend_from_slice(&(data.len() as u32).to_be_bytes());
    msg.extend_from_slice(data);
    msg
}

/// Build an SSH_MSG_CHANNEL_SUCCESS packet.
fn ssh_channel_success(channel_id: u32) -> Vec<u8> {
    let mut msg = Vec::with_capacity(5);
    msg.push(99); // SSH_MSG_CHANNEL_SUCCESS
    msg.extend_from_slice(&channel_id.to_be_bytes());
    msg
}

/// Run as an SSH server
async fn run_ssh_server(cli: &Cli, addr: SocketAddr) {
    let server = ayssh::server::TestSshServer::new(addr.port())
        .await
        .unwrap_or_else(|e| panic!("Failed to start SSH server on {}: {}", addr, e));

    info!("MockIOS SSH server listening on {}", server.local_addr());
    eprintln!("MockIOS SSH server listening on {}", server.local_addr());
    eprintln!("Hostname: {}, Version: {}", cli.hostname, cli.version);

    loop {
        let stream = match server.accept_stream().await {
            Ok(s) => s,
            Err(e) => {
                error!("Accept failed: {}", e);
                continue;
            }
        };

        let peer = stream.peer_addr().unwrap_or_else(|_| "unknown".parse().unwrap());
        info!("SSH connection from {}", peer);
        eprintln!("SSH connection from {}", peer);

        let (mut io, client_channel) = match server.handshake_and_auth(stream).await {
            Ok(result) => result,
            Err(e) => {
                error!("SSH handshake failed: {}", e);
                continue;
            }
        };

        let cli_hostname = cli.hostname.clone();
        let cli_version = cli.version.clone();
        let cli_model = cli.model.clone();
        // Note: --login/--enable are not used for SSH connections since
        // SSH authentication happens at the protocol level.

        tokio::spawn(async move {
            let mut device = MockIosDevice::new(&cli_hostname)
                .with_version(&cli_version)
                .with_model(&cli_model);

            // Send initial prompt
            let initial = device.receive(Duration::from_secs(1)).await.unwrap();
            if !initial.is_empty() {
                if io.send_message(&ssh_channel_data(client_channel, &initial)).await.is_err() {
                    return;
                }
            }

            loop {
                let msg = match io.recv_message().await {
                    Ok(data) => data,
                    Err(_) => break,
                };

                if msg.is_empty() {
                    continue;
                }

                match msg[0] {
                    94 => {
                        // SSH_MSG_CHANNEL_DATA
                        if msg.len() > 9 {
                            let data_len = u32::from_be_bytes([
                                msg[5], msg[6], msg[7], msg[8],
                            ]) as usize;
                            if msg.len() >= 9 + data_len {
                                let input = &msg[9..9 + data_len];
                                if device.send(input).await.is_err() {
                                    break;
                                }

                                let output = match device.receive(Duration::from_secs(1)).await {
                                    Ok(data) => data,
                                    Err(_) => break,
                                };

                                if !output.is_empty() {
                                    if io.send_message(&ssh_channel_data(client_channel, &output)).await.is_err() {
                                        break;
                                    }
                                }

                                // Check if device wants to disconnect (exit/quit)
                                if device.is_reloading() {
                                    break;
                                }
                            }
                        }
                    }
                    96 | 97 => break, // EOF or Close
                    98 => {
                        // SSH_MSG_CHANNEL_REQUEST — send success
                        let _ = io.send_message(&ssh_channel_success(client_channel)).await;
                    }
                    _ => {} // ignore other messages
                }
            }
            info!("SSH connection from {} closed", peer);
        });
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    if let Some(addr) = cli.telnet {
        run_telnet_server(&cli, addr).await;
    } else if let Some(addr) = cli.ssh {
        run_ssh_server(&cli, addr).await;
    } else {
        run_interactive(&cli).await;
    }
}
