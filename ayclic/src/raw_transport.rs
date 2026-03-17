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

/// Raw Telnet transport wrapping `aytelnet::TelnetConnection`.
///
/// Handles TELNET protocol negotiation internally but does NOT
/// perform any login, prompt detection, or vendor-specific behavior.
/// All interaction is left to the caller (typically via TextFSMPlus).
///
/// TODO: Upstream `Debug` impl for `TelnetConnection` to aytelnet.
pub struct RawTelnetTransport {
    conn: aytelnet::TelnetConnection,
}

impl std::fmt::Debug for RawTelnetTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawTelnetTransport").finish()
    }
}

impl RawTelnetTransport {
    /// Connect to a Telnet server at the given address.
    pub async fn connect(addr: SocketAddr) -> Result<Self, CiscoIosError> {
        let conn = aytelnet::TelnetConnection::start_with_config(
            &addr.ip().to_string(),
            addr.port(),
            true,  // echo
            true,  // binary
            true,  // suppress go-ahead
        )
        .await
        .map_err(CiscoIosError::Telnet)?;

        Ok(Self { conn })
    }

    /// Create from an already-connected TelnetConnection.
    pub fn from_connection(conn: aytelnet::TelnetConnection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl RawTransport for RawTelnetTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.conn.send(data).await.map_err(CiscoIosError::Telnet)
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Ok(vec![]); // timeout
            }
            let remaining = deadline - now;

            match tokio::time::timeout(remaining, self.conn.receive()).await {
                Ok(Ok(event)) => {
                    use aytelnet::TelnetEvent;
                    match event {
                        TelnetEvent::Data(data) => return Ok(data),
                        TelnetEvent::Closed => {
                            return Err(CiscoIosError::Telnet(
                                aytelnet::TelnetError::Disconnected,
                            ))
                        }
                        TelnetEvent::Error(e) => return Err(CiscoIosError::Telnet(e)),
                        // Protocol commands and option negotiations — handle
                        // internally, keep reading for actual data
                        TelnetEvent::Command(_) | TelnetEvent::OptionNegotiated { .. } => {
                            debug!("RawTelnetTransport: protocol event, continuing");
                            continue;
                        }
                    }
                }
                Ok(Err(e)) => return Err(CiscoIosError::Telnet(e)),
                Err(_) => return Ok(vec![]), // timeout
            }
        }
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.conn
            .disconnect()
            .await
            .map_err(CiscoIosError::Telnet)
    }
}

// === Vendor-neutral SSH transport ===

/// Raw SSH transport wrapping `ayssh::Transport` + session channel.
///
/// Handles SSH protocol (encryption, channels) internally but does NOT
/// perform any login interaction, prompt detection, or vendor-specific
/// behavior. SSH protocol-level authentication (password, pubkey, etc.)
/// is done during construction; all subsequent interaction is raw bytes.
///
/// TODO: Upstream `Debug` impl for `Transport` to ayssh.
pub struct RawSshTransport {
    transport: ayssh::Transport,
    channel_id: u32,
}

impl std::fmt::Debug for RawSshTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawSshTransport")
            .field("channel_id", &self.channel_id)
            .finish()
    }
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
        let client = ayssh::SshClient::new(addr.ip().to_string(), addr.port());

        let mut session = match auth {
            SshAuth::Password { username, password } => {
                client
                    .connect_with_password(username, password)
                    .await
                    .map_err(CiscoIosError::Ssh)?
            }
            SshAuth::PubKey {
                username,
                private_key,
            } => {
                client
                    .connect_with_publickey(username, private_key)
                    .await
                    .map_err(CiscoIosError::Ssh)?
            }
            SshAuth::KbdInteractive { username, password } => {
                // Keyboard-interactive often falls back to password
                client
                    .connect_with_password(username, password)
                    .await
                    .map_err(CiscoIosError::Ssh)?
            }
        };

        // The session from connect_with_* should already have a channel.
        // Get the remote channel ID for sending data.
        let channel_id = session.remote_channel_id();

        // Request PTY and shell
        // Note: we need access to the transport that the session wraps.
        // The current ayssh API returns Session which owns the transport
        // interaction. We need to extract the transport.
        //
        // TODO: This is a prototype. The actual implementation will depend
        // on how ayssh exposes Transport+Session separation. For now, we
        // use CiscoConn's approach as a reference for what needs to happen.
        //
        // For the initial prototype, wrap CiscoConn's transport directly
        // since it handles PTY/shell setup internally.

        // FIXME: The ayssh Session API needs to be reviewed to determine
        // the best way to get raw Transport + channel_id after auth.
        // For now, this is a placeholder that documents the intended flow.

        todo!(
            "RawSshTransport::connect requires ayssh to expose Transport \
             separately from Session after authentication. This will be \
             implemented when the ayssh agent adds the necessary API."
        )
    }

    /// Create from an already-authenticated transport and channel ID.
    ///
    /// This is the primary constructor for use when the caller manages
    /// SSH connection setup externally (e.g., via ayssh directly).
    pub fn from_parts(transport: ayssh::Transport, channel_id: u32) -> Self {
        Self {
            transport,
            channel_id,
        }
    }
}

#[async_trait]
impl RawTransport for RawSshTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.transport
            .send_channel_data(self.channel_id, data)
            .await
            .map_err(CiscoIosError::Ssh)
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Ok(vec![]); // timeout
            }
            let remaining = deadline - now;

            match tokio::time::timeout(remaining, self.transport.recv_message()).await {
                Ok(Ok(msg)) if !msg.is_empty() => {
                    match msg[0] {
                        94 => {
                            // SSH_MSG_CHANNEL_DATA
                            if msg.len() > 9 {
                                let data_len = u32::from_be_bytes([
                                    msg[5], msg[6], msg[7], msg[8],
                                ]) as usize;
                                if msg.len() >= 9 + data_len {
                                    return Ok(msg[9..9 + data_len].to_vec());
                                }
                            }
                            return Ok(vec![]);
                        }
                        93 => {
                            // SSH_MSG_CHANNEL_WINDOW_ADJUST — ignore
                            debug!("RawSshTransport: window adjust, continuing");
                            continue;
                        }
                        96 => {
                            // SSH_MSG_CHANNEL_EOF
                            return Err(CiscoIosError::Ssh(
                                ayssh::SshError::ChannelError("Channel EOF".to_string()),
                            ));
                        }
                        97 => {
                            // SSH_MSG_CHANNEL_CLOSE
                            return Err(CiscoIosError::Ssh(
                                ayssh::SshError::ChannelError("Channel closed".to_string()),
                            ));
                        }
                        other => {
                            debug!("RawSshTransport: ignoring msg type {}", other);
                            continue;
                        }
                    }
                }
                Ok(Ok(_)) => continue, // empty message
                Ok(Err(e)) => return Err(CiscoIosError::Ssh(e)),
                Err(_) => return Ok(vec![]), // timeout
            }
        }
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.transport
            .send_channel_close(self.channel_id)
            .await
            .map_err(CiscoIosError::Ssh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock transport for testing the feed() integration loop
    /// without real network connections.
    #[derive(Debug)]
    pub struct MockTransport {
        pub chunks: Vec<Vec<u8>>,
        index: usize,
        pub sent: Vec<Vec<u8>>,
    }

    impl MockTransport {
        pub fn new(chunks: Vec<Vec<u8>>) -> Self {
            Self {
                chunks,
                index: 0,
                sent: Vec::new(),
            }
        }
    }

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
