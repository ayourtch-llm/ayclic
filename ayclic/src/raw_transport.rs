//! Vendor-neutral raw byte transport layer.
//!
//! Provides a `RawTransport` trait and implementations that wrap the
//! protocol-specific layers (telnet, SSH) without any vendor-specific
//! assumptions. All device interaction (login, prompts, enable mode)
//! is handled by TextFSMPlus templates, not by the transport.
//!
//! These wrappers are prototyped here in ayclic and are intended to
//! be upstreamed into aytelnet and ayssh once validated.

use std::net::SocketAddr;
use std::time::Duration;

use async_trait::async_trait;
use tracing::debug;

use crate::error::CiscoIosError;

/// Vendor-neutral raw byte transport.
///
/// Implementations wrap protocol-specific connections (Telnet, SSH)
/// and expose a uniform byte-level send/receive interface without
/// any device or vendor-specific behavior.
///
/// The `receive` method returns data as fast as possible — it only
/// blocks up to `timeout` when no data is available yet. This enables
/// the caller to do fast-paced incremental pattern matching via
/// TextFSMPlus `feed()`.
#[async_trait]
pub trait RawTransport: Send + std::fmt::Debug {
    /// Send raw bytes to the remote end.
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError>;

    /// Receive the next chunk of data from the remote end.
    ///
    /// - If data is immediately available, returns it RIGHT AWAY
    /// - Only blocks up to `timeout` if there is NO data yet
    /// - Returns empty Vec on timeout (not an error)
    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError>;

    /// Close the connection.
    async fn close(&mut self) -> Result<(), CiscoIosError>;
}

// === Vendor-neutral Telnet transport ===

/// Raw Telnet transport using `aytelnet::RawTelnetSession`.
///
/// Handles TELNET protocol negotiation internally but does NOT
/// perform any login, prompt detection, or vendor-specific behavior.
/// All interaction is left to the caller (typically via TextFSMPlus).
#[derive(Debug)]
pub struct RawTelnetTransport {
    session: aytelnet::RawTelnetSession,
}

impl RawTelnetTransport {
    /// Connect to a Telnet server at the given address.
    pub async fn connect(addr: SocketAddr) -> Result<Self, CiscoIosError> {
        let session = aytelnet::RawTelnetSession::connect(
            &addr.ip().to_string(),
            addr.port(),
        )
        .await
        .map_err(CiscoIosError::Telnet)?;

        Ok(Self { session })
    }

    /// Create from an already-connected RawTelnetSession.
    pub fn from_session(session: aytelnet::RawTelnetSession) -> Self {
        Self { session }
    }

    /// Create from an already-connected TelnetConnection.
    pub fn from_connection(conn: aytelnet::TelnetConnection) -> Self {
        Self {
            session: aytelnet::RawTelnetSession::from_connection(conn),
        }
    }
}

#[async_trait]
impl RawTransport for RawTelnetTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.session.send(data).await.map_err(CiscoIosError::Telnet)
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        self.session
            .receive(timeout)
            .await
            .map_err(CiscoIosError::Telnet)
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.session
            .disconnect()
            .await
            .map_err(CiscoIosError::Telnet)
    }
}

// === Vendor-neutral SSH transport ===

/// Raw SSH transport using `ayssh::RawSshSession`.
///
/// Handles SSH protocol (encryption, channels) internally but does NOT
/// perform any login interaction, prompt detection, or vendor-specific
/// behavior. SSH protocol-level authentication (password, pubkey, etc.)
/// is done during construction; all subsequent interaction is raw bytes.
#[derive(Debug)]
pub struct RawSshTransport {
    session: ayssh::RawSshSession,
}

/// SSH authentication method for establishing a connection.
#[derive(Debug, Clone)]
pub enum SshAuth {
    Password {
        username: String,
        password: String,
    },
    PubKey {
        username: String,
        private_key: Vec<u8>,
    },
    KbdInteractive {
        username: String,
        password: String,
    },
}

impl RawSshTransport {
    /// Connect to an SSH server, authenticate, and open a session channel
    /// with a PTY and shell.
    pub async fn connect(addr: SocketAddr, auth: SshAuth) -> Result<Self, CiscoIosError> {
        let host = addr.ip().to_string();
        let port = addr.port();

        let session = match auth {
            SshAuth::Password { username, password } => {
                ayssh::RawSshSession::connect_with_password(&host, port, &username, &password)
                    .await
                    .map_err(CiscoIosError::Ssh)?
            }
            SshAuth::PubKey {
                username,
                private_key,
            } => {
                ayssh::RawSshSession::connect_with_publickey(&host, port, &username, &private_key)
                    .await
                    .map_err(CiscoIosError::Ssh)?
            }
            SshAuth::KbdInteractive { username, password } => {
                // Keyboard-interactive: use password as the response
                let pwd = password.clone();
                ayssh::RawSshSession::connect_with_keyboard_interactive(
                    &host,
                    port,
                    &username,
                    move |_challenge| Ok(vec![pwd.clone()]),
                )
                .await
                .map_err(CiscoIosError::Ssh)?
            }
        };

        Ok(Self { session })
    }

    /// Create from an already-authenticated RawSshSession.
    pub fn from_session(session: ayssh::RawSshSession) -> Self {
        Self { session }
    }

    /// Create from an already-authenticated transport and channel ID.
    pub fn from_parts(transport: ayssh::Transport, channel_id: u32) -> Self {
        Self {
            session: ayssh::RawSshSession::from_parts(transport, channel_id),
        }
    }
}

#[async_trait]
impl RawTransport for RawSshTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.session.send(data).await.map_err(CiscoIosError::Ssh)
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        self.session
            .receive(timeout)
            .await
            .map_err(CiscoIosError::Ssh)
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.session
            .disconnect()
            .await
            .map_err(CiscoIosError::Ssh)
    }
}

/// Mock transport for testing the feed() integration loop
/// without real network connections.
#[cfg(test)]
#[derive(Debug)]
pub struct MockTransport {
    pub chunks: Vec<Vec<u8>>,
    index: usize,
    pub sent: Vec<Vec<u8>>,
}

#[cfg(test)]
impl MockTransport {
    pub fn new(chunks: Vec<Vec<u8>>) -> Self {
        Self {
            chunks,
            index: 0,
            sent: Vec::new(),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl RawTransport for MockTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.sent.push(data.to_vec());
        Ok(())
    }

    async fn receive(&mut self, _timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        if self.index < self.chunks.len() {
            let chunk = self.chunks[self.index].clone();
            self.index += 1;
            Ok(chunk)
        } else {
            Ok(vec![]) // simulate timeout
        }
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_transport_send_receive() {
        let mut transport = MockTransport::new(vec![
            b"Hello ".to_vec(),
            b"World".to_vec(),
        ]);

        transport.send(b"test").await.unwrap();
        assert_eq!(transport.sent[0], b"test");

        let chunk1 = transport.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(chunk1, b"Hello ");

        let chunk2 = transport.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(chunk2, b"World");

        let chunk3 = transport.receive(Duration::from_secs(1)).await.unwrap();
        assert!(chunk3.is_empty()); // timeout
    }

    #[tokio::test]
    async fn test_mock_transport_with_textfsmplus_feed() {
        use aytextfsmplus::*;

        let mut transport = MockTransport::new(vec![
            b"Username: ".to_vec(),
            b"Password: ".to_vec(),
            b"Router1#".to_vec(),
        ]);

        let mut fsm = TextFSMPlus::from_str(
            r#"Value Preset Username ()
Value Preset Password ()
Value Hostname (\S+)

Start
  ^Username:\s* -> Send ${Username} WaitPassword

WaitPassword
  ^Password:\s* -> Send ${Password} WaitPrompt

WaitPrompt
  ^${Hostname}# -> Done
"#,
        )
        .with_preset("Username", "admin")
        .with_preset("Password", "secret");

        let timeout = Duration::from_secs(5);
        let mut buffer = Vec::new();

        loop {
            let chunk = transport.receive(timeout).await.unwrap();
            if chunk.is_empty() {
                panic!("Unexpected timeout");
            }
            buffer.extend_from_slice(&chunk);

            let result = fsm.feed(&buffer, &NoVars, &NoFuncs);
            buffer.drain(..result.consumed);

            match result.action {
                InteractiveAction::Send(text) => {
                    transport.send(text.as_bytes()).await.unwrap();
                }
                InteractiveAction::Done => break,
                InteractiveAction::Error(msg) => panic!("Error: {:?}", msg),
                InteractiveAction::None => continue,
            }
        }

        // Verify the state machine completed
        assert_eq!(fsm.curr_state, "Done");

        // Verify what was sent
        assert_eq!(transport.sent.len(), 2);
        assert_eq!(transport.sent[0], b"admin");
        assert_eq!(transport.sent[1], b"secret");

        // Verify hostname was captured
        assert_eq!(
            fsm.curr_record.get("Hostname"),
            Some(&Value::Single("Router1".to_string()))
        );
    }
}
