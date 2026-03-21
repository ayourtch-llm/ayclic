//! Parameterized device tests — run the same assertions against
//! mockios (telnet/SSH) and optionally against real devices.
//!
//! By default, tests run against mockios only. To also test against
//! real devices, set environment variables:
//!
//!   REAL_DEVICE_HOST=192.168.0.113
//!   REAL_DEVICE_USER=ayourtch
//!   REAL_DEVICE_PASS=cisco123
//!
//! cargo test -p mockios --test device_tests

use std::time::Duration;

use ayclic::{CiscoIosConn, ConnectionType};

/// Test target — either a mockios server or a real device.
struct TestTarget {
    label: String,
    host: String,
    port: u16,
    username: String,
    password: String,
    conntype: ConnectionType,
    /// Server handle (for mockios — None for real devices)
    _server: Option<tokio::task::JoinHandle<()>>,
}

impl TestTarget {
    fn label(&self) -> &str {
        &self.label
    }

    fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    async fn connect(&self) -> Result<CiscoIosConn, Box<dyn std::error::Error>> {
        let conn = CiscoIosConn::with_timeouts(
            &self.addr(),
            self.conntype.clone(),
            &self.username,
            &self.password,
            Duration::from_secs(30),
            Duration::from_secs(30),
        )
        .await?;
        Ok(conn)
    }
}

/// Start a mockios SSH server, return a TestTarget.
async fn mockios_ssh_target() -> TestTarget {
    let server = ayssh::server::TestSshServer::new(0).await.unwrap();
    let port = server.local_addr().port();

    let handle = tokio::spawn(async move {
        loop {
            let stream = match server.accept_stream().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let (mut io, ch) = match server.handshake_and_auth(stream).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                use ayclic::raw_transport::RawTransport;
                let mut device = mockios::MockIosDevice::new("MockSwitch");
                let initial = device.receive(Duration::from_secs(1)).await.unwrap();
                if !initial.is_empty() {
                    let _ = io.send_message(&ssh_channel_data(ch, &initial)).await;
                }
                loop {
                    let msg = match io.recv_message().await {
                        Ok(m) => m,
                        Err(_) => break,
                    };
                    if msg.is_empty() { continue; }
                    match msg[0] {
                        94 if msg.len() > 9 => {
                            let len = u32::from_be_bytes([msg[5], msg[6], msg[7], msg[8]]) as usize;
                            if msg.len() >= 9 + len {
                                let _ = device.send(&msg[9..9 + len]).await;
                                let out = device.receive(Duration::from_secs(1)).await.unwrap_or_default();
                                if !out.is_empty() {
                                    let _ = io.send_message(&ssh_channel_data(ch, &out)).await;
                                }
                            }
                        }
                        96 | 97 => break,
                        98 => { let _ = io.send_message(&ssh_channel_success(ch)).await; }
                        _ => {}
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    TestTarget {
        label: "mockios-ssh".into(),
        host: "127.0.0.1".into(),
        port,
        username: "test".into(),
        password: "test".into(),
        conntype: ConnectionType::Ssh,
        _server: Some(handle),
    }
}

fn ssh_channel_data(ch: u32, data: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(9 + data.len());
    m.push(94); m.extend_from_slice(&ch.to_be_bytes());
    m.extend_from_slice(&(data.len() as u32).to_be_bytes());
    m.extend_from_slice(data); m
}

fn ssh_channel_success(ch: u32) -> Vec<u8> {
    let mut m = Vec::with_capacity(5);
    m.push(99); m.extend_from_slice(&ch.to_be_bytes()); m
}

/// Get real device targets from environment (if configured).
fn real_device_targets() -> Vec<TestTarget> {
    let host = match std::env::var("REAL_DEVICE_HOST") {
        Ok(h) => h,
        Err(_) => return vec![],
    };
    let user = std::env::var("REAL_DEVICE_USER").unwrap_or_else(|_| "ayourtch".into());
    let pass = std::env::var("REAL_DEVICE_PASS").unwrap_or_else(|_| "cisco123".into());

    vec![
        TestTarget {
            label: format!("real-ssh-{}", host),
            host: host.clone(),
            port: 22,
            username: user.clone(),
            password: pass.clone(),
            conntype: ConnectionType::Ssh,
            _server: None,
        },
        TestTarget {
            label: format!("real-telnet-{}", host),
            host: host.clone(),
            port: 23,
            username: user,
            password: pass,
            conntype: ConnectionType::Telnet,
            _server: None,
        },
    ]
}

/// Collect all test targets (mockios + real if configured).
async fn all_targets() -> Vec<TestTarget> {
    let mut targets = vec![mockios_ssh_target().await];
    targets.extend(real_device_targets());
    targets
}

// === Parameterized test assertions ===

async fn assert_show_version_contains_cisco_ios(target: &TestTarget) {
    let mut conn = target.connect().await
        .unwrap_or_else(|e| panic!("[{}] connect failed: {}", target.label(), e));
    let output = conn.run_cmd("show version").await
        .unwrap_or_else(|e| panic!("[{}] show version failed: {}", target.label(), e));
    assert!(
        output.contains("Cisco IOS"),
        "[{}] show version should contain 'Cisco IOS', got:\n{}",
        target.label(), &output[..output.len().min(200)]
    );
    conn.disconnect().await.unwrap();
}

async fn assert_show_running_config_has_hostname(target: &TestTarget) {
    let mut conn = target.connect().await
        .unwrap_or_else(|e| panic!("[{}] connect failed: {}", target.label(), e));
    let output = conn.run_cmd("show running-config").await
        .unwrap_or_else(|e| panic!("[{}] show run failed: {}", target.label(), e));
    assert!(
        output.contains("hostname"),
        "[{}] show run should contain 'hostname', got:\n{}",
        target.label(), &output[..output.len().min(200)]
    );
    conn.disconnect().await.unwrap();
}

async fn assert_term_len_succeeds(target: &TestTarget) {
    let mut conn = target.connect().await
        .unwrap_or_else(|e| panic!("[{}] connect failed: {}", target.label(), e));
    // terminal length 0 should succeed (no error)
    let output = conn.run_cmd("terminal length 0").await
        .unwrap_or_else(|e| panic!("[{}] term len failed: {}", target.label(), e));
    // Should not contain error markers
    assert!(
        !output.contains("%Error"),
        "[{}] term len 0 should not error, got:\n{}",
        target.label(), output
    );
    conn.disconnect().await.unwrap();
}

async fn assert_show_version_then_show_run(target: &TestTarget) {
    let mut conn = target.connect().await
        .unwrap_or_else(|e| panic!("[{}] connect failed: {}", target.label(), e));

    let ver = conn.run_cmd("show version").await
        .unwrap_or_else(|e| panic!("[{}] show version failed: {}", target.label(), e));
    assert!(ver.contains("Cisco IOS"), "[{}] show version missing Cisco IOS", target.label());

    let run = conn.run_cmd("show running-config").await
        .unwrap_or_else(|e| panic!("[{}] show run failed: {}", target.label(), e));
    assert!(run.contains("hostname"), "[{}] show run missing hostname", target.label());

    conn.disconnect().await.unwrap();
}

// === Test functions ===

#[tokio::test]
async fn test_show_version() {
    for target in all_targets().await {
        eprintln!("Testing show version on {}", target.label());
        assert_show_version_contains_cisco_ios(&target).await;
    }
}

#[tokio::test]
async fn test_show_running_config() {
    for target in all_targets().await {
        eprintln!("Testing show running-config on {}", target.label());
        assert_show_running_config_has_hostname(&target).await;
    }
}

#[tokio::test]
async fn test_terminal_length() {
    for target in all_targets().await {
        eprintln!("Testing terminal length on {}", target.label());
        assert_term_len_succeeds(&target).await;
    }
}

#[tokio::test]
async fn test_multiple_commands() {
    for target in all_targets().await {
        eprintln!("Testing multiple commands on {}", target.label());
        assert_show_version_then_show_run(&target).await;
    }
}
