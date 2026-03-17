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

/// Trait for receiving transcript entries.
///
/// Implement this to control where transcript data goes — a Vec,
/// a file, a channel, syslog, a ring buffer, etc.
pub trait TranscriptSink: Send + std::fmt::Debug {
    /// Record a transcript entry.
    fn record(&mut self, entry: TranscriptEntry);
}

/// Simple in-memory transcript sink that collects entries in a Vec.
/// Access the entries via the shared handle returned by `new_transcript()`.
#[derive(Debug, Default)]
pub struct VecTranscriptSink {
    entries: Vec<TranscriptEntry>,
}

impl VecTranscriptSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }

    /// Get the transcript as a human-readable string.
    pub fn to_display_string(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
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
        self.entries
            .iter()
            .filter(|e| e.direction == TranscriptDirection::Sent)
            .map(|e| String::from_utf8_lossy(&e.data).into_owned())
            .collect()
    }

    /// Get only the received data as a string.
    pub fn received_string(&self) -> String {
        self.entries
            .iter()
            .filter(|e| e.direction == TranscriptDirection::Received)
            .map(|e| String::from_utf8_lossy(&e.data).into_owned())
            .collect()
    }
}

impl TranscriptSink for VecTranscriptSink {
    fn record(&mut self, entry: TranscriptEntry) {
        self.entries.push(entry);
    }
}

/// Transcript sink that writes to a file in real time.
///
/// Each entry is written as it happens — useful for audit trails
/// and debugging sessions that may crash or hang.
pub struct FileTranscriptSink {
    file: std::fs::File,
}

impl std::fmt::Debug for FileTranscriptSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTranscriptSink").finish()
    }
}

impl FileTranscriptSink {
    /// Create from an open file (takes ownership).
    pub fn new(file: std::fs::File) -> Self {
        Self { file }
    }

    /// Create by opening a file for writing (truncates if exists).
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self { file })
    }

    /// Create by opening a file for appending.
    pub fn open_append(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { file })
    }
}

impl TranscriptSink for FileTranscriptSink {
    fn record(&mut self, entry: TranscriptEntry) {
        use std::io::Write;
        let prefix = match entry.direction {
            TranscriptDirection::Sent => ">>> ",
            TranscriptDirection::Received => "<<< ",
        };
        // Best-effort write — don't fail the transport on I/O errors
        let _ = write!(self.file, "{}", prefix);
        let _ = self.file.write_all(&entry.data);
        if !entry.data.ends_with(b"\n") {
            let _ = writeln!(self.file);
        }
        let _ = self.file.flush();
    }
}

/// Wrap a transport with file-based logging.
///
/// Returns a `Box<dyn RawTransport>` that logs all sent/received data
/// to the file in real time. No shared handle needed — the file is
/// owned by the transport and closed when dropped.
///
/// ```ignore
/// let transport = with_file_logging(transport, "/var/log/session.log")?;
/// let mut conn = GenericCliConn::from_transport(transport);
/// // Everything is logged to the file automatically
/// ```
pub fn with_file_logging(
    transport: Box<dyn RawTransport>,
    path: impl AsRef<std::path::Path>,
) -> std::io::Result<Box<dyn RawTransport>> {
    let sink = FileTranscriptSink::open(path)?;
    let shared = std::sync::Arc::new(std::sync::Mutex::new(sink));
    Ok(Box::new(LoggingTransport::new(transport, shared)))
}

/// Same as `with_file_logging` but appends to an existing file.
pub fn with_file_logging_append(
    transport: Box<dyn RawTransport>,
    path: impl AsRef<std::path::Path>,
) -> std::io::Result<Box<dyn RawTransport>> {
    let sink = FileTranscriptSink::open_append(path)?;
    let shared = std::sync::Arc::new(std::sync::Mutex::new(sink));
    Ok(Box::new(LoggingTransport::new(transport, shared)))
}

/// Shared transcript handle — wraps any `TranscriptSink` in
/// `Arc<Mutex<>>` so the caller can retain a handle while the
/// transport is owned by a `GenericCliConn` or `CiscoIosConn`.
pub type SharedTranscript<T> = std::sync::Arc<std::sync::Mutex<T>>;

/// Create a new shared in-memory transcript.
pub fn new_transcript() -> SharedTranscript<VecTranscriptSink> {
    std::sync::Arc::new(std::sync::Mutex::new(VecTranscriptSink::new()))
}

/// A transport wrapper that records all sent/received data to a
/// shared `TranscriptSink`.
///
/// Wraps any `RawTransport` and captures a full transcript of the
/// session. Since the sink is shared via `Arc<Mutex<>>`, the caller
/// can read it at any time — even while the transport is owned by
/// a `GenericCliConn` or `CiscoIosConn`.
///
/// # Example
///
/// ```ignore
/// let transcript = new_transcript();
/// let inner = RawSshTransport::connect(addr, auth).await?;
/// let transport = LoggingTransport::new(Box::new(inner), transcript.clone());
/// let mut conn = GenericCliConn::from_transport(Box::new(transport));
///
/// conn.run_cmd("show version", &vars, &funcs).await?;
///
/// // Read transcript without unwrapping the connection:
/// println!("{}", transcript.lock().unwrap().to_display_string());
/// ```
pub struct LoggingTransport<T: TranscriptSink> {
    inner: Box<dyn RawTransport>,
    sink: SharedTranscript<T>,
}

impl<T: TranscriptSink> std::fmt::Debug for LoggingTransport<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoggingTransport")
            .field("inner", &self.inner)
            .field("sink", &self.sink)
            .finish()
    }
}

impl<T: TranscriptSink + 'static> LoggingTransport<T> {
    /// Wrap a transport with logging to a shared sink.
    pub fn new(inner: Box<dyn RawTransport>, sink: SharedTranscript<T>) -> Self {
        Self { inner, sink }
    }

    /// Extract the inner transport.
    pub fn into_inner(self) -> Box<dyn RawTransport> {
        self.inner
    }
}

#[async_trait]
impl<T: TranscriptSink + 'static> RawTransport for LoggingTransport<T> {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.sink.lock().unwrap_or_else(|e| e.into_inner()).record(TranscriptEntry {
            direction: TranscriptDirection::Sent,
            data: data.to_vec(),
            timestamp: std::time::Instant::now(),
        });
        self.inner.send(data).await
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        let data = self.inner.receive(timeout).await?;
        if !data.is_empty() {
            self.sink.lock().unwrap_or_else(|e| e.into_inner()).record(TranscriptEntry {
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

        let transcript = new_transcript();
        let mut transport = LoggingTransport::new(Box::new(inner), transcript.clone());

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

        // Check transcript via shared handle
        let entries = transcript.lock().unwrap();
        assert_eq!(entries.entries().len(), 4); // 2 sends + 2 receives
        assert_eq!(entries.entries()[0].direction, TranscriptDirection::Sent);
        assert_eq!(entries.entries()[0].data, b"admin");
        assert_eq!(entries.entries()[1].direction, TranscriptDirection::Sent);
        assert_eq!(entries.entries()[1].data, b"\n");
        assert_eq!(entries.entries()[2].direction, TranscriptDirection::Received);
        assert_eq!(entries.entries()[2].data, b"Username: ");
        assert_eq!(entries.entries()[3].direction, TranscriptDirection::Received);
        assert_eq!(entries.entries()[3].data, b"Router1#");
    }

    #[tokio::test]
    async fn test_logging_transport_transcript_string() {
        let inner = MockTransport::new(vec![
            b"Password: ".to_vec(),
        ]);

        let transcript = new_transcript();
        let mut transport = LoggingTransport::new(Box::new(inner), transcript.clone());
        transport.send(b"secret\n").await.unwrap();
        transport.receive(Duration::from_secs(1)).await.unwrap();

        let s = transcript.lock().unwrap().to_display_string();
        assert!(s.contains(">>> secret"));
        assert!(s.contains("<<< Password: "));
    }

    #[tokio::test]
    async fn test_logging_transport_sent_received_strings() {
        let inner = MockTransport::new(vec![
            b"prompt#".to_vec(),
        ]);

        let transcript = new_transcript();
        let mut transport = LoggingTransport::new(Box::new(inner), transcript.clone());
        transport.send(b"show ver\n").await.unwrap();
        transport.receive(Duration::from_secs(1)).await.unwrap();

        let t = transcript.lock().unwrap();
        assert_eq!(t.sent_string(), "show ver\n");
        assert_eq!(t.received_string(), "prompt#");
    }

    #[tokio::test]
    async fn test_logging_transcript_readable_while_transport_owned() {
        let inner = MockTransport::new(vec![
            b"Router1#".to_vec(),
        ]);

        let transcript = new_transcript();
        let transport = LoggingTransport::new(Box::new(inner), transcript.clone());

        // Transport is now owned by GenericCliConn — we can't access it directly
        let mut conn = crate::generic_conn::GenericCliConn::from_transport(Box::new(transport))
            .with_cmd_timeout(Duration::from_secs(5))
            .with_prompt_template("Start\n  ^.*# -> Done\n");

        conn.run_cmd("show ver", &aytextfsmplus::NoVars, &aytextfsmplus::NoFuncs)
            .await
            .unwrap();

        // But we can still read the transcript!
        let t = transcript.lock().unwrap();
        assert!(t.sent_string().contains("show ver"));
        assert!(t.received_string().contains("Router1#"));
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
