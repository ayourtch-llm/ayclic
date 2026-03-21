//! Integration tests: spin up mockios telnet/SSH servers and connect
//! with ayclic's template-driven connection path.

use std::time::Duration;

use ayclic::path::*;
use ayclic::raw_transport::SshAuth;
use ayclic::{CiscoIosConn, ConnectionType, GenericCliConn};
use aytextfsmplus::{NoFuncs, NoVars};

/// Helper: start a telnet mockios server on a random port, return the port.
async fn start_telnet_server(
    hostname: &str,
    login: Option<(&str, &str)>,
) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let hostname = hostname.to_string();
    let login = login.map(|(u, p)| (u.to_string(), p.to_string()));

    let handle = tokio::spawn(async move {
        // Accept one connection
        let (mut stream, _peer) = listener.accept().await.unwrap();

        let mut device = mockios::MockIosDevice::new(&hostname);
        if let Some((ref user, ref pass)) = login {
            device = device.with_login(user, pass);
        }

        use ayclic::raw_transport::RawTransport;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Send initial prompt/banner
        let initial = device.receive(Duration::from_secs(1)).await.unwrap();
        let _ = stream.write_all(&initial).await;

        let mut buf = vec![0u8; 4096];
        loop {
            let n = match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };

            if device.send(&buf[..n]).await.is_err() {
                break;
            }

            let output = match device.receive(Duration::from_secs(1)).await {
                Ok(data) => data,
                Err(_) => break,
            };

            if !output.is_empty() {
                if stream.write_all(&output).await.is_err() {
                    break;
                }
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;
    (port, handle)
}

/// Helper: start an SSH mockios server on a random port, return the port.
async fn start_ssh_server(
    hostname: &str,
) -> (u16, tokio::task::JoinHandle<()>) {
    let server = ayssh::server::TestSshServer::new(0).await.unwrap();
    let port = server.local_addr().port();
    let hostname = hostname.to_string();

    let handle = tokio::spawn(async move {
        // Accept one connection
        let stream = match server.accept_stream().await {
            Ok(s) => s,
            Err(_) => return,
        };

        let (mut io, client_channel) = match server.handshake_and_auth(stream).await {
            Ok(result) => result,
            Err(_) => return,
        };

        use ayclic::raw_transport::RawTransport;

        let mut device = mockios::MockIosDevice::new(&hostname);

        // Send initial prompt
        let initial = device.receive(Duration::from_secs(1)).await.unwrap();
        if !initial.is_empty() {
            let msg = ssh_channel_data(client_channel, &initial);
            let _ = io.send_message(&msg).await;
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
                        let data_len =
                            u32::from_be_bytes([msg[5], msg[6], msg[7], msg[8]]) as usize;
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
                                let resp = ssh_channel_data(client_channel, &output);
                                if io.send_message(&resp).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
                96 | 97 => break, // EOF or Close
                98 => {
                    // Channel request — send success
                    let resp = ssh_channel_success(client_channel);
                    let _ = io.send_message(&resp).await;
                }
                _ => {}
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (port, handle)
}

fn ssh_channel_data(channel_id: u32, data: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(9 + data.len());
    msg.push(94);
    msg.extend_from_slice(&channel_id.to_be_bytes());
    msg.extend_from_slice(&(data.len() as u32).to_be_bytes());
    msg.extend_from_slice(data);
    msg
}

fn ssh_channel_success(channel_id: u32) -> Vec<u8> {
    let mut msg = Vec::with_capacity(5);
    msg.push(99);
    msg.extend_from_slice(&channel_id.to_be_bytes());
    msg
}

// === Telnet integration tests ===

#[tokio::test]
#[ignore = "raw TCP mock server needs telnet protocol handling"]
async fn test_telnet_server_show_version() {
    let (port, server) = start_telnet_server("MockRouter", None).await;
    let addr = format!("127.0.0.1:{}", port);

    let prompt = ayclic::templates::CISCO_IOS_PROMPT;

    let path = ConnectionPath::new(vec![Hop::Transport(TransportSpec::Telnet {
        target: addr.parse().unwrap(),
    })]);

    let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs)
        .await
        .unwrap()
        .with_prompt_template(prompt)
        .with_cmd_timeout(Duration::from_secs(5));

    // Consume initial prompt
    let _ = conn.run_cmd("terminal length 0", &NoVars, &NoFuncs).await.unwrap();

    let output = conn.run_cmd("show version", &NoVars, &NoFuncs).await.unwrap();
    assert!(output.contains("Cisco IOS"), "Expected Cisco IOS in output, got: {}", output);
    assert!(output.contains("MockRouter"));

    conn.close().await.unwrap();
    server.abort();
}

#[tokio::test]
#[ignore = "telnet mock server needs proper telnet protocol handling for login flow"]
async fn test_telnet_server_with_login() {
    let (port, server) = start_telnet_server("LoginRouter", Some(("admin", "secret"))).await;
    let addr = format!("127.0.0.1:{}", port);

    let login_template = ayclic::templates::CISCO_IOS_TELNET_LOGIN;
    let prompt = r#"Start
  ^LoginRouter# -> Done
"#;

    let path = ConnectionPath::new(vec![
        Hop::Transport(TransportSpec::Telnet {
            target: addr.parse().unwrap(),
        }),
        Hop::Interactive(
            aytextfsmplus::TextFSMPlus::from_str(login_template)
                .with_preset("Username", "admin")
                .with_preset("Password", "secret"),
        ),
    ]);

    let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs)
        .await
        .unwrap()
        .with_prompt_template(prompt)
        .with_cmd_timeout(Duration::from_secs(5));

    let output = conn.run_cmd("show version", &NoVars, &NoFuncs).await.unwrap();
    assert!(output.contains("Cisco IOS"), "Expected Cisco IOS in output, got: {}", output);

    conn.close().await.unwrap();
    server.abort();
}

#[tokio::test]
#[ignore = "raw TCP mock server needs telnet protocol handling"]
async fn test_telnet_server_show_running_config() {
    let (port, server) = start_telnet_server("ConfigRouter", None).await;
    let addr = format!("127.0.0.1:{}", port);

    let prompt = ayclic::templates::CISCO_IOS_PROMPT;

    let path = ConnectionPath::new(vec![Hop::Transport(TransportSpec::Telnet {
        target: addr.parse().unwrap(),
    })]);

    let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs)
        .await
        .unwrap()
        .with_prompt_template(prompt)
        .with_cmd_timeout(Duration::from_secs(5));

    let _ = conn.run_cmd("terminal length 0", &NoVars, &NoFuncs).await.unwrap();
    let output = conn.run_cmd("show running-config", &NoVars, &NoFuncs).await.unwrap();
    assert!(output.contains("hostname ConfigRouter"), "Expected hostname in running config");
    assert!(output.contains("interface GigabitEthernet0/0"));

    conn.close().await.unwrap();
    server.abort();
}

// === SSH integration tests ===

#[tokio::test]
#[ignore = "GenericCliConn prompt matching needs investigation with mock SSH"]
async fn test_ssh_server_show_version() {
    let (port, server) = start_ssh_server("SshRouter").await;
    let addr = format!("127.0.0.1:{}", port);

    let prompt = ayclic::templates::CISCO_IOS_PROMPT;

    let path = ConnectionPath::new(vec![Hop::Transport(TransportSpec::Ssh {
        target: addr.parse().unwrap(),
        auth: SshAuth::Password {
            username: "test".into(),
            password: "test".into(),
        },
    })]);

    let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs)
        .await
        .unwrap()
        .with_prompt_template(prompt)
        .with_cmd_timeout(Duration::from_secs(5));

    // Consume initial prompt
    let _ = conn.run_cmd("terminal length 0", &NoVars, &NoFuncs).await.unwrap();

    let output = conn.run_cmd("show version", &NoVars, &NoFuncs).await.unwrap();
    assert!(output.contains("Cisco IOS"), "Expected Cisco IOS in output, got: {}", output);
    assert!(output.contains("SshRouter"));

    conn.close().await.unwrap();
    server.abort();
}

#[tokio::test]
#[ignore = "GenericCliConn prompt matching needs investigation with mock SSH"]
async fn test_ssh_server_show_running_config() {
    let (port, server) = start_ssh_server("SshSwitch").await;
    let addr = format!("127.0.0.1:{}", port);

    let prompt = ayclic::templates::CISCO_IOS_PROMPT;

    let path = ConnectionPath::new(vec![Hop::Transport(TransportSpec::Ssh {
        target: addr.parse().unwrap(),
        auth: SshAuth::Password {
            username: "test".into(),
            password: "test".into(),
        },
    })]);

    let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs)
        .await
        .unwrap()
        .with_prompt_template(prompt)
        .with_cmd_timeout(Duration::from_secs(5));

    let _ = conn.run_cmd("terminal length 0", &NoVars, &NoFuncs).await.unwrap();
    let output = conn.run_cmd("show running-config", &NoVars, &NoFuncs).await.unwrap();
    assert!(output.contains("hostname SshSwitch"));
    assert!(output.contains("interface GigabitEthernet0/0"));

    conn.close().await.unwrap();
    server.abort();
}

#[tokio::test]
#[ignore = "GenericCliConn prompt matching needs investigation with mock SSH"]
async fn test_ssh_server_multiple_commands() {
    let (port, server) = start_ssh_server("MultiCmd").await;
    let addr = format!("127.0.0.1:{}", port);

    let prompt = ayclic::templates::CISCO_IOS_PROMPT;

    let path = ConnectionPath::new(vec![Hop::Transport(TransportSpec::Ssh {
        target: addr.parse().unwrap(),
        auth: SshAuth::Password {
            username: "test".into(),
            password: "test".into(),
        },
    })]);

    let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs)
        .await
        .unwrap()
        .with_prompt_template(prompt)
        .with_cmd_timeout(Duration::from_secs(5));

    let _ = conn.run_cmd("terminal length 0", &NoVars, &NoFuncs).await.unwrap();

    let ver = conn.run_cmd("show version", &NoVars, &NoFuncs).await.unwrap();
    assert!(ver.contains("Cisco IOS"));

    let run = conn.run_cmd("show running-config", &NoVars, &NoFuncs).await.unwrap();
    assert!(run.contains("hostname MultiCmd"));

    conn.close().await.unwrap();
    server.abort();
}

// === CiscoIosConn integration test (uses template-driven new()) ===

#[tokio::test]
#[ignore = "telnet mock server needs proper telnet protocol handling for login flow"]
async fn test_cisco_ios_conn_via_telnet_mockios() {
    let (port, server) = start_telnet_server("CiscoMock", Some(("ayourtch", "cisco123"))).await;
    let addr = format!("127.0.0.1:{}", port);

    let mut conn = CiscoIosConn::new(&addr, ConnectionType::Telnet, "ayourtch", "cisco123")
        .await
        .unwrap();

    let output = conn.run_cmd("show version").await.unwrap();
    assert!(output.contains("Cisco IOS"), "Expected Cisco IOS, got: {}", output);

    conn.disconnect().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn test_cisco_ios_conn_via_ssh_mockios() {
    let (port, server) = start_ssh_server("CiscoSshMock").await;
    let addr = format!("127.0.0.1:{}", port);

    let mut conn = CiscoIosConn::new(&addr, ConnectionType::Ssh, "test", "test")
        .await
        .unwrap();

    let output = conn.run_cmd("show version").await.unwrap();
    assert!(output.contains("Cisco IOS"), "Expected Cisco IOS, got: {}", output);

    conn.disconnect().await.unwrap();
    server.abort();
}
