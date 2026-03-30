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

/// Start a mockios SSH server with enable mode, return a TestTarget.
async fn mockios_ssh_enable_target() -> TestTarget {
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
                // Device with enable — SSH auth gets you to user exec (>)
                let mut device = mockios::MockIosDevice::new("MockSwitch")
                    .with_enable("321cisco");
                // SSH auth happened at protocol level, so start at UserExec
                device.mode = mockios::CliMode::UserExec;

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
        label: "mockios-ssh-enable".into(),
        host: "127.0.0.1".into(),
        port,
        username: "test".into(),
        password: "test".into(),
        conntype: ConnectionType::Ssh,
        _server: Some(handle),
    }
}

/// Get real device targets for enable-mode testing.
fn real_device_enable_targets() -> Vec<TestTarget> {
    let host = match std::env::var("REAL_DEVICE_HOST") {
        Ok(h) => h,
        Err(_) => return vec![],
    };
    let user = std::env::var("REAL_DEVICE_ENABLE_USER").unwrap_or_else(|_| "testuser".into());
    let pass = std::env::var("REAL_DEVICE_ENABLE_PASS").unwrap_or_else(|_| "testpass".into());

    vec![
        TestTarget {
            label: format!("real-telnet-enable-{}", host),
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

// === Enable mode tests ===
// These test the user-exec → enable → privileged-exec flow.
// On mockios: MockIosDevice with with_enable()
// On real device: testuser (priv 1) + enable secret 321cisco

async fn assert_enable_flow(target: &TestTarget, enable_password: &str) {
    use ayclic::path::*;
    use ayclic::raw_transport::SshAuth;
    use ayclic::GenericCliConn;
    use aytextfsmplus::{NoFuncs, NoVars, TextFSMPlus};

    // Build path with login + enable template
    let enable_login_template = format!(
        r#"Value Preset Username ()
Value Preset Password ()
Value Preset EnablePassword ()

Start
  ^[Uu]sername:\s* -> Send ${{Username}} WaitPassword
  ^[Pp]assword:\s* -> Send ${{Password}} WaitPrompt
  ^.*# -> Send "terminal length 0" TermLen
  ^.*> -> Send "enable" Enable

WaitPassword
  ^[Pp]assword:\s* -> Send ${{Password}} WaitPrompt

WaitPrompt
  ^.*# -> Send "terminal length 0" TermLen
  ^.*> -> Send "enable" Enable
  ^% -> Error "login failed"

Enable
  ^[Pp]assword:\s* -> Send ${{EnablePassword}} WaitEnabled

WaitEnabled
  ^.*# -> Send "terminal length 0" TermLen
  ^% -> Error "enable failed"

TermLen
  ^.*# -> Done
  ^.*> -> Done
"#
    );

    let addr: std::net::SocketAddr = target.addr().parse().unwrap();

    let mut hops: Vec<Hop> = Vec::new();
    match target.conntype {
        ConnectionType::Telnet => {
            hops.push(Hop::Transport(TransportSpec::Telnet { target: addr }));
        }
        ConnectionType::Ssh => {
            hops.push(Hop::Transport(TransportSpec::Ssh {
                target: addr,
                auth: SshAuth::Password {
                    username: target.username.clone(),
                    password: target.password.clone(),
                },
            }));
        }
        _ => panic!("Unsupported conntype"),
    }

    hops.push(Hop::Interactive(
        TextFSMPlus::from_str(&enable_login_template)
            .with_preset("Username", &target.username)
            .with_preset("Password", &target.password)
            .with_preset("EnablePassword", enable_password),
    ));

    let path = ConnectionPath::new(hops).with_timeout(Duration::from_secs(30));

    let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs)
        .await
        .unwrap_or_else(|e| panic!("[{}] enable connect failed: {}", target.label(), e));

    conn = conn
        .with_prompt_template(ayclic::templates::CISCO_IOS_PROMPT)
        .with_cmd_timeout(Duration::from_secs(15));

    let output = conn
        .run_cmd("show version", &NoVars, &NoFuncs)
        .await
        .unwrap_or_else(|e| panic!("[{}] show version after enable failed: {}", target.label(), e));

    assert!(
        output.contains("Cisco IOS"),
        "[{}] show version should contain 'Cisco IOS' after enable, got:\n{}",
        target.label(),
        &output[..output.len().min(200)]
    );

    conn.close().await.unwrap();
}

#[tokio::test]
async fn test_enable_mode() {
    // mockios with enable
    let mockios = mockios_ssh_enable_target().await;
    eprintln!("Testing enable mode on {}", mockios.label());
    assert_enable_flow(&mockios, "321cisco").await;

    // Real device with enable (if configured)
    for target in real_device_enable_targets() {
        eprintln!("Testing enable mode on {}", target.label());
        assert_enable_flow(&target, "321cisco").await;
    }
}

// === Copy command tests (mockios only — non-destructive) ===

#[tokio::test]
async fn test_copy_to_flash_and_verify() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("CopyTest");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    // Copy http to flash (interactive: filename confirmation)
    device
        .send(b"copy http://10.0.0.1/test.cfg flash:test.cfg\n")
        .await
        .unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("filename"), "Expected filename prompt, got: {}", output);

    // Accept default filename
    device.send(b"\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("OK"), "Expected OK, got: {}", output);

    // File should exist in flash
    assert!(device.flash_files.contains_key("test.cfg"));

    // Verify MD5
    device.send(b"verify /md5 flash:test.cfg\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("verify /md5"));
    assert!(output.contains("="));

    // Delete the file
    device
        .send(b"delete /force flash:test.cfg\n")
        .await
        .unwrap();
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();
    assert!(!device.flash_files.contains_key("test.cfg"));
}

#[tokio::test]
async fn test_copy_to_running_config() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("CopyRunTest")
        .with_flash_file("new.cfg", b"ip route 10.0.0.0 255.0.0.0 10.0.0.1\n".to_vec());
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    // Copy flash to running-config
    device
        .send(b"copy flash:new.cfg running-config\n")
        .await
        .unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("running-config"));

    // Confirm
    device.send(b"\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    assert!(String::from_utf8_lossy(&data).contains("OK"));

    // Check running config was updated
    device.send(b"show running-config\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("ip route 10.0.0.0"));
}

#[tokio::test]
async fn test_copy_to_null() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("NullTest");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    // Copy to null: — should succeed silently
    device
        .send(b"copy http://10.0.0.1/done null:\n")
        .await
        .unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("NullTest#"));
}

#[tokio::test]
async fn test_unknown_command() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Router1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"gobbledygook\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("Unknown command") || output.contains("% "));
}

#[tokio::test]
async fn test_verify_md5_nonexistent_file() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Router1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device
        .send(b"verify /md5 flash:nonexistent.bin\n")
        .await
        .unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);
    assert!(output.contains("Error"), "Expected error for nonexistent file, got: {}", output);
}

#[tokio::test]
async fn test_show_vlan() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show vlan\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    // Should contain the brief table header
    assert!(output.contains("VLAN Name"), "show vlan missing VLAN Name header, got:\n{}", output);
    assert!(output.contains("Status"), "show vlan missing Status column, got:\n{}", output);
    // Should contain the SAID/MTU section header
    assert!(output.contains("SAID"), "show vlan missing SAID column, got:\n{}", output);
    assert!(output.contains("MTU"), "show vlan missing MTU column, got:\n{}", output);
    // VLAN 1 should appear
    assert!(output.contains("default"), "show vlan missing VLAN 1 'default', got:\n{}", output);
}

#[tokio::test]
async fn test_show_vlan_brief() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show vlan brief\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(output.contains("VLAN Name"), "show vlan brief missing header, got:\n{}", output);
    assert!(output.contains("default"), "show vlan brief missing VLAN 1, got:\n{}", output);
    // brief should NOT have the SAID section
    assert!(!output.contains("SAID"), "show vlan brief should not contain SAID section, got:\n{}", output);
}

#[tokio::test]
async fn test_show_vlan_id_existing() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show vlan id 1\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(output.contains("VLAN Name"), "show vlan id 1 missing header, got:\n{}", output);
    assert!(output.contains("default"), "show vlan id 1 missing VLAN name, got:\n{}", output);
    assert!(output.contains("SAID"), "show vlan id 1 missing SAID section, got:\n{}", output);
}

#[tokio::test]
async fn test_show_vlan_id_nonexistent() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show vlan id 999\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(
        output.contains("not found") || output.contains("999"),
        "show vlan id 999 should indicate not found, got:\n{}",
        output
    );
}

#[tokio::test]
async fn test_show_lldp() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show lldp\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(
        output.contains("Global LLDP Information"),
        "show lldp missing 'Global LLDP Information', got:\n{}",
        output
    );
    assert!(
        output.contains("Status: ACTIVE"),
        "show lldp missing 'Status: ACTIVE', got:\n{}",
        output
    );
    assert!(
        output.contains("30 seconds"),
        "show lldp missing advertisement interval, got:\n{}",
        output
    );
    assert!(
        output.contains("120 seconds"),
        "show lldp missing hold time, got:\n{}",
        output
    );
}

#[tokio::test]
async fn test_show_lldp_neighbors() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show lldp neighbors\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(
        output.contains("Capability codes"),
        "show lldp neighbors missing capability codes header, got:\n{}",
        output
    );
    assert!(
        output.contains("(R) Router"),
        "show lldp neighbors missing Router capability, got:\n{}",
        output
    );
    assert!(
        output.contains("Device ID"),
        "show lldp neighbors missing Device ID column, got:\n{}",
        output
    );
    assert!(
        output.contains("Local Intf"),
        "show lldp neighbors missing Local Intf column, got:\n{}",
        output
    );
    assert!(
        output.contains("Hold-time"),
        "show lldp neighbors missing Hold-time column, got:\n{}",
        output
    );
    assert!(
        output.contains("Capability"),
        "show lldp neighbors missing Capability column, got:\n{}",
        output
    );
    assert!(
        output.contains("Port ID"),
        "show lldp neighbors missing Port ID column, got:\n{}",
        output
    );
    assert!(
        output.contains("Total entries displayed: 0"),
        "show lldp neighbors missing total entries line, got:\n{}",
        output
    );
}

#[tokio::test]
async fn test_show_lldp_neighbors_detail() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show lldp neighbors detail\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(
        output.contains("Total entries displayed: 0"),
        "show lldp neighbors detail missing total entries line, got:\n{}",
        output
    );
}

#[tokio::test]
async fn test_show_ip_dhcp_snooping() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show ip dhcp snooping\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(
        output.contains("Switch DHCP snooping is enabled"),
        "show ip dhcp snooping missing enabled line, got:\n{}",
        output
    );
    assert!(
        output.contains("DHCP snooping is configured on following VLANs:"),
        "show ip dhcp snooping missing configured VLANs line, got:\n{}",
        output
    );
    assert!(
        output.contains("DHCP snooping is operational on following VLANs:"),
        "show ip dhcp snooping missing operational VLANs line, got:\n{}",
        output
    );
}

#[tokio::test]
async fn test_show_ip_dhcp_snooping_binding() {
    use ayclic::raw_transport::RawTransport;

    let mut device = mockios::MockIosDevice::new("Switch1");
    let _ = device.receive(Duration::from_secs(1)).await.unwrap();

    device.send(b"show ip dhcp snooping binding\n").await.unwrap();
    let data = device.receive(Duration::from_secs(1)).await.unwrap();
    let output = String::from_utf8_lossy(&data);

    assert!(
        output.contains("MacAddress"),
        "show ip dhcp snooping binding missing MacAddress header, got:\n{}",
        output
    );
    assert!(
        output.contains("IpAddress"),
        "show ip dhcp snooping binding missing IpAddress header, got:\n{}",
        output
    );
    assert!(
        output.contains("Total number of bindings: 0"),
        "show ip dhcp snooping binding missing total bindings line, got:\n{}",
        output
    );
}
