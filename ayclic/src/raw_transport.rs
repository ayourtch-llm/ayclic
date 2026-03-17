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

// === Logging transport wrapper ===

/// Direction of data in a transcript entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptDirection {
    /// Data sent to the remote end.
    Sent,
    /// Data received from the remote end.
    Received,
}

/// A single entry in the session transcript.
#[derive(Debug, Clone)]
pub struct TranscriptEntry {
    /// Direction of the data.
    pub direction: TranscriptDirection,
    /// The raw bytes.
    pub data: Vec<u8>,
    /// When this entry was recorded.
    pub timestamp: std::time::Instant,
}

/// A transport wrapper that records all sent/received data.
///
/// Wraps any `RawTransport` and captures a full transcript of the
/// session. The transcript is available via `transcript()` at any
/// time — during the session, after errors, or after disconnect.
///
/// # Example
///
/// ```ignore
/// let inner = RawSshTransport::connect(addr, auth).await?;
/// let mut transport = LoggingTransport::new(inner);
///
/// // ... use transport normally ...
///
/// // After the session, inspect what happened:
/// for entry in transport.transcript() {
///     match entry.direction {
///         TranscriptDirection::Sent => print!(">>> "),
///         TranscriptDirection::Received => print!("<<< "),
///     }
///     println!("{}", String::from_utf8_lossy(&entry.data));
/// }
/// ```
pub struct LoggingTransport {
    inner: Box<dyn RawTransport>,
    transcript: Vec<TranscriptEntry>,
}

impl std::fmt::Debug for LoggingTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoggingTransport")
            .field("inner", &self.inner)
            .field("transcript_entries", &self.transcript.len())
            .finish()
    }
}

impl LoggingTransport {
    /// Wrap a transport with logging.
    pub fn new(inner: Box<dyn RawTransport>) -> Self {
        Self {
            inner,
            transcript: Vec::new(),
        }
    }

    /// Get the full transcript.
    pub fn transcript(&self) -> &[TranscriptEntry] {
        &self.transcript
    }

    /// Take the transcript, leaving an empty one in its place.
    pub fn take_transcript(&mut self) -> Vec<TranscriptEntry> {
        std::mem::take(&mut self.transcript)
    }

    /// Get the transcript as a human-readable string.
    pub fn transcript_string(&self) -> String {
        let mut out = String::new();
        for entry in &self.transcript {
            let prefix = match entry.direction {
                TranscriptDirection::Sent => ">>> ",
                TranscriptDirection::Received => "<<< ",
            };
            out.push_str(prefix);
            out.push_str(&String::from_utf8_lossy(&entry.data));
            if !entry.data.ends_with(b"\n") {
                out.push('\n');
            }
        }
        out
    }

    /// Get only the sent data as a string.
    pub fn sent_string(&self) -> String {
        self.transcript
            .iter()
            .filter(|e| e.direction == TranscriptDirection::Sent)
            .map(|e| String::from_utf8_lossy(&e.data).into_owned())
            .collect()
    }

    /// Get only the received data as a string.
    pub fn received_string(&self) -> String {
        self.transcript
            .iter()
            .filter(|e| e.direction == TranscriptDirection::Received)
            .map(|e| String::from_utf8_lossy(&e.data).into_owned())
            .collect()
    }

    /// Extract the inner transport (e.g., to return to a pool).
    /// The transcript is lost unless you call `take_transcript()` first.
    pub fn into_inner(self) -> Box<dyn RawTransport> {
        self.inner
    }
}

#[async_trait]
impl RawTransport for LoggingTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.transcript.push(TranscriptEntry {
            direction: TranscriptDirection::Sent,
            data: data.to_vec(),
            timestamp: std::time::Instant::now(),
        });
        self.inner.send(data).await
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        let data = self.inner.receive(timeout).await?;
        if !data.is_empty() {
            self.transcript.push(TranscriptEntry {
                direction: TranscriptDirection::Received,
                data: data.clone(),
                timestamp: std::time::Instant::now(),
            });
        }
        Ok(data)
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.inner.close().await
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
    async fn test_logging_transport_records_transcript() {
        let inner = MockTransport::new(vec![
            b"Username: ".to_vec(),
            b"Router1#".to_vec(),
        ]);

        let mut transport = LoggingTransport::new(Box::new(inner));

        // Send some data
        transport.send(b"admin").await.unwrap();
        transport.send(b"\n").await.unwrap();

        // Receive data
        let chunk1 = transport.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(chunk1, b"Username: ");

        let chunk2 = transport.receive(Duration::from_secs(1)).await.unwrap();
        assert_eq!(chunk2, b"Router1#");

        // Empty receive (timeout) should NOT be recorded
        let chunk3 = transport.receive(Duration::from_secs(1)).await.unwrap();
        assert!(chunk3.is_empty());

        // Check transcript
        let transcript = transport.transcript();
        assert_eq!(transcript.len(), 4); // 2 sends + 2 receives (not the empty one)
        assert_eq!(transcript[0].direction, TranscriptDirection::Sent);
        assert_eq!(transcript[0].data, b"admin");
        assert_eq!(transcript[1].direction, TranscriptDirection::Sent);
        assert_eq!(transcript[1].data, b"\n");
        assert_eq!(transcript[2].direction, TranscriptDirection::Received);
        assert_eq!(transcript[2].data, b"Username: ");
        assert_eq!(transcript[3].direction, TranscriptDirection::Received);
        assert_eq!(transcript[3].data, b"Router1#");
    }

    #[tokio::test]
    async fn test_logging_transport_transcript_string() {
        let inner = MockTransport::new(vec![
            b"Password: ".to_vec(),
        ]);

        let mut transport = LoggingTransport::new(Box::new(inner));
        transport.send(b"secret\n").await.unwrap();
        transport.receive(Duration::from_secs(1)).await.unwrap();

        let s = transport.transcript_string();
        assert!(s.contains(">>> secret"));
        assert!(s.contains("<<< Password: "));
    }

    #[tokio::test]
    async fn test_logging_transport_sent_received_strings() {
        let inner = MockTransport::new(vec![
            b"prompt#".to_vec(),
        ]);

        let mut transport = LoggingTransport::new(Box::new(inner));
        transport.send(b"show ver\n").await.unwrap();
        transport.receive(Duration::from_secs(1)).await.unwrap();

        assert_eq!(transport.sent_string(), "show ver\n");
        assert_eq!(transport.received_string(), "prompt#");
    }

    #[tokio::test]
    async fn test_logging_transport_take_transcript() {
        let inner = MockTransport::new(vec![b"data".to_vec()]);
        let mut transport = LoggingTransport::new(Box::new(inner));
        transport.receive(Duration::from_secs(1)).await.unwrap();

        let taken = transport.take_transcript();
        assert_eq!(taken.len(), 1);
        assert!(transport.transcript().is_empty()); // emptied
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
