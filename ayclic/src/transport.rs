use std::time::Duration;

use aho_corasick::AhoCorasick;
use async_trait::async_trait;
use tracing::debug;

use crate::error::CiscoIosError;

/// Low-level transport trait for Cisco device communication.
///
/// Implementations wrap protocol-specific connections (telnet, SSH)
/// and expose a uniform byte-level send/receive interface.
///
/// The `receive` method returns data as fast as possible — it only
/// blocks up to `timeout` when no data is available yet. This enables
/// the caller to do fast-paced incremental pattern matching.
#[async_trait]
pub trait CiscoTransport: Send + std::fmt::Debug {
    /// Send raw bytes to the device.
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError>;

    /// Receive the next chunk of data from the device.
    ///
    /// - If data is immediately available, returns it RIGHT AWAY
    /// - Only blocks up to `timeout` if there is NO data yet
    /// - Returns empty Vec on timeout (not an error)
    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError>;

    /// Close the connection.
    async fn close(&mut self) -> Result<(), CiscoIosError>;
}

// === Telnet transport wrapper ===

#[derive(Debug)]
pub struct TelnetTransport(pub(crate) aytelnet::CiscoTelnet);

#[async_trait]
impl CiscoTransport for TelnetTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.0.send(data).await.map_err(CiscoIosError::Telnet)
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        self.0.receive(timeout).await.map_err(CiscoIosError::Telnet)
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.0.disconnect().await.map_err(CiscoIosError::Telnet)
    }
}

// === SSH transport wrapper ===

#[derive(Debug)]
pub struct SshTransport(pub(crate) ayssh::CiscoConn);

#[async_trait]
impl CiscoTransport for SshTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), CiscoIosError> {
        self.0.send(data).await.map_err(CiscoIosError::Ssh)
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, CiscoIosError> {
        self.0.receive(timeout).await.map_err(CiscoIosError::Ssh)
    }

    async fn close(&mut self) -> Result<(), CiscoIosError> {
        self.0.disconnect().await.map_err(CiscoIosError::Ssh)
    }
}

// === Pattern matching ===

/// Receive data from a transport until one of the patterns in `matcher` is found.
///
/// - `initial_data`: previously accumulated bytes (e.g. from a prior timeout)
/// - Returns `(accumulated_bytes, matched_pattern_index)` on match
/// - Returns `Err(CiscoIosError::Timeout { accumulated })` on timeout,
///   allowing the caller to retry with the accumulated data
///
/// The caller can use this for fast-paced polling:
/// ```ignore
/// let mut buf = vec![];
/// loop {
///     match receive_until_match(&mut transport, &matcher, short_timeout, buf).await {
///         Ok((data, idx)) => { /* pattern idx matched */ break; }
///         Err(CiscoIosError::Timeout { accumulated }) => {
///             buf = accumulated; // keep accumulated data, do other work
///         }
///         Err(e) => return Err(e),
///     }
/// }
/// ```
pub async fn receive_until_match(
    transport: &mut dyn CiscoTransport,
    matcher: &AhoCorasick,
    timeout: Duration,
    initial_data: Vec<u8>,
) -> Result<(Vec<u8>, usize), CiscoIosError> {
    let mut buffer = initial_data;
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        // Check if any pattern already matches in the buffer
        if let Some(mat) = matcher.find(&buffer) {
            return Ok((buffer, mat.pattern().as_usize()));
        }

        // How much time remains?
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(CiscoIosError::Timeout {
                accumulated: buffer,
            });
        }
        let remaining = deadline - now;

        // Try to receive more data (returns fast if data is available)
        let chunk = transport.receive(remaining).await?;
        if chunk.is_empty() {
            // Timeout from the transport — no data arrived
            return Err(CiscoIosError::Timeout {
                accumulated: buffer,
            });
        }

        debug!(
            "receive_until_match: got {} bytes (total {})",
            chunk.len(),
            buffer.len() + chunk.len()
        );
        buffer.extend_from_slice(&chunk);
    }
}

/// What to do when a pattern is matched during interactive command execution.
#[derive(Debug, Clone)]
pub enum PromptAction {
    /// The command is done — return the accumulated output.
    Done,
    /// Send this response to the device and continue waiting.
    Respond(Vec<u8>),
}

/// Execute a command interactively, handling intermediate prompts.
///
/// Sends `cmd` (with trailing newline), then watches for any of the given
/// patterns. When a `Done` pattern matches, the accumulated output is returned.
/// When a `Respond` pattern matches, the response is sent and matching continues.
///
/// This enables handling IOS interactive commands like `copy` (which prompts
/// for confirmation) without needing `file prompt quiet`.
///
/// # Arguments
///
/// * `transport` — the device connection
/// * `cmd` — command to send (newline is appended automatically)
/// * `prompt_actions` — list of `(pattern, action)` pairs; order matters for
///   aho-corasick (first match in the data wins)
/// * `timeout` — overall timeout for the entire interaction
///
/// # Example
///
/// ```ignore
/// let output = run_interactive(
///     transport,
///     "copy flash:file running-config",
///     &[
///         ("#", PromptAction::Done),
///         ("]?", PromptAction::Respond(b"\n".to_vec())),
///         ("[confirm]", PromptAction::Respond(b"\n".to_vec())),
///     ],
///     Duration::from_secs(30),
/// ).await?;
/// ```
pub async fn run_interactive(
    transport: &mut dyn CiscoTransport,
    cmd: &str,
    prompt_actions: &[(&str, PromptAction)],
    timeout: Duration,
) -> Result<String, CiscoIosError> {
    let patterns: Vec<&str> = prompt_actions.iter().map(|(p, _)| *p).collect();
    let matcher = AhoCorasick::new(&patterns)
        .map_err(|e| CiscoIosError::HttpUploadError(format!("Invalid pattern: {}", e)))?;

    // Send the command
    transport
        .send(format!("{}\n", cmd).as_bytes())
        .await?;

    let deadline = tokio::time::Instant::now() + timeout;
    let mut buffer = Vec::new();

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(CiscoIosError::Timeout {
                accumulated: buffer,
            });
        }
        let remaining = deadline - now;

        match receive_until_match(transport, &matcher, remaining, buffer).await {
            Ok((data, pattern_idx)) => {
                let (_, action) = &prompt_actions[pattern_idx];
                match action {
                    PromptAction::Done => {
                        return String::from_utf8(data).map_err(|e| {
                            CiscoIosError::HttpUploadError(format!("Invalid UTF-8: {}", e))
                        });
                    }
                    PromptAction::Respond(response) => {
                        debug!(
                            "Interactive prompt matched pattern {:?}, sending response",
                            patterns[pattern_idx]
                        );
                        transport.send(response).await?;
                        // Advance past the match so it doesn't re-trigger
                        let mat = matcher.find(&data).unwrap();
                        buffer = data[mat.end()..].to_vec();
                    }
                }
            }
            Err(CiscoIosError::Timeout { accumulated }) => {
                return Err(CiscoIosError::Timeout { accumulated });
            }
            Err(e) => return Err(e),
        }
    }
}

/// Common IOS interactive prompt actions: auto-confirm `]?` and `[confirm]`.
pub const IOS_CONFIRM_PROMPTS: &[(&str, &[u8])] = &[
    ("]?", b"\n"),
    ("[confirm]", b"\n"),
    ("(yes/no)", b"yes\n"),
    ("(yes/no)?", b"yes\n"),
];

/// Build a prompt_actions list for interactive IOS commands.
/// Includes `#` as Done, plus the standard confirmation auto-responses.
pub fn ios_prompt_actions() -> Vec<(&'static str, PromptAction)> {
    let mut actions: Vec<(&str, PromptAction)> = Vec::new();
    actions.push(("#", PromptAction::Done));
    for (pattern, response) in IOS_CONFIRM_PROMPTS {
        actions.push((pattern, PromptAction::Respond(response.to_vec())));
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Mock transport for testing ===

    #[derive(Debug)]
    struct MockTransport {
        chunks: Vec<Vec<u8>>,
        index: usize,
        sent: Vec<Vec<u8>>,
    }

    impl MockTransport {
        fn new(chunks: Vec<Vec<u8>>) -> Self {
            Self {
                chunks,
                index: 0,
                sent: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl CiscoTransport for MockTransport {
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

    // === receive_until_match tests ===

    #[tokio::test]
    async fn test_match_in_initial_data() {
        let mut transport = MockTransport::new(vec![]);
        let matcher = AhoCorasick::new(&["#"]).unwrap();

        let (data, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            b"Router#".to_vec(),
        )
        .await
        .unwrap();

        assert_eq!(idx, 0);
        assert_eq!(data, b"Router#");
    }

    #[tokio::test]
    async fn test_match_after_one_receive() {
        let mut transport = MockTransport::new(vec![b"output\nRouter#".to_vec()]);
        let matcher = AhoCorasick::new(&["#"]).unwrap();

        let (data, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(idx, 0);
        assert_eq!(data, b"output\nRouter#");
    }

    #[tokio::test]
    async fn test_match_after_multiple_receives() {
        let mut transport = MockTransport::new(vec![
            b"show ver".to_vec(),
            b"sion\nCisco IOS".to_vec(),
            b"\nRouter#".to_vec(),
        ]);
        let matcher = AhoCorasick::new(&["#"]).unwrap();

        let (data, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(idx, 0);
        assert_eq!(data, b"show version\nCisco IOS\nRouter#");
    }

    #[tokio::test]
    async fn test_timeout_returns_accumulated_data() {
        let mut transport = MockTransport::new(vec![
            b"partial ".to_vec(),
            b"output".to_vec(),
            // then empty = timeout
        ]);
        let matcher = AhoCorasick::new(&["#"]).unwrap();

        let err = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap_err();

        match err {
            CiscoIosError::Timeout { accumulated } => {
                assert_eq!(accumulated, b"partial output");
            }
            other => panic!("Expected Timeout, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_timeout_with_no_data() {
        let mut transport = MockTransport::new(vec![]);
        let matcher = AhoCorasick::new(&["#"]).unwrap();

        let err = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap_err();

        match err {
            CiscoIosError::Timeout { accumulated } => {
                assert!(accumulated.is_empty());
            }
            other => panic!("Expected Timeout, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_multiple_patterns_returns_correct_index() {
        let mut transport = MockTransport::new(vec![b"Password: ".to_vec()]);
        let matcher = AhoCorasick::new(&["#", ">", "Password:"]).unwrap();

        let (data, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(idx, 2); // "Password:" is pattern index 2
        assert_eq!(data, b"Password: ");
    }

    #[tokio::test]
    async fn test_match_prompt_gt() {
        let mut transport = MockTransport::new(vec![b"Router>".to_vec()]);
        let matcher = AhoCorasick::new(&["#", ">"]).unwrap();

        let (_, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(idx, 1); // ">" is pattern index 1
    }

    #[tokio::test]
    async fn test_match_split_across_chunks() {
        // The delimiter "#" arrives in a separate chunk from the rest
        let mut transport = MockTransport::new(vec![
            b"Router".to_vec(),
            b"#".to_vec(),
        ]);
        let matcher = AhoCorasick::new(&["#"]).unwrap();

        let (data, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(idx, 0);
        assert_eq!(data, b"Router#");
    }

    #[tokio::test]
    async fn test_initial_data_plus_receive_combined() {
        // Some data from previous timeout, then more arrives
        let mut transport = MockTransport::new(vec![b" more data\nRouter#".to_vec()]);
        let matcher = AhoCorasick::new(&["#"]).unwrap();

        let (data, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            b"initial".to_vec(),
        )
        .await
        .unwrap();

        assert_eq!(idx, 0);
        assert_eq!(data, b"initial more data\nRouter#");
    }

    #[tokio::test]
    async fn test_multipattern_first_match_wins() {
        // Both "#" and ">" present, "#" comes first in the data
        let mut transport = MockTransport::new(vec![b"Router#stuff>".to_vec()]);
        let matcher = AhoCorasick::new(&["#", ">"]).unwrap();

        let (_, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(idx, 0); // "#" appears first in the data
    }

    #[tokio::test]
    async fn test_long_pattern_match() {
        let mut transport = MockTransport::new(vec![
            b"Enter password: ".to_vec(),
        ]);
        let matcher = AhoCorasick::new(&["#", "Enter password: ", "--More--"]).unwrap();

        let (_, idx) = receive_until_match(
            &mut transport,
            &matcher,
            Duration::from_secs(1),
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(idx, 1);
    }

    // === run_interactive tests ===

    #[tokio::test]
    async fn test_interactive_simple_command() {
        // Simple command, no interactive prompts — just waits for #
        let mut transport = MockTransport::new(vec![
            b"show version\nCisco IOS\nRouter#".to_vec(),
        ]);
        let prompts = ios_prompt_actions();
        let output = run_interactive(
            &mut transport,
            "show version",
            &prompts,
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert!(output.contains("Cisco IOS"));
        assert!(output.contains("Router#"));
        // First sent item is the command
        assert_eq!(transport.sent[0], b"show version\n");
    }

    #[tokio::test]
    async fn test_interactive_confirmation_prompt() {
        // copy command: device asks "]?" then completes with #
        let mut transport = MockTransport::new(vec![
            b"Destination filename [file.cfg]?".to_vec(),
            b"\n92 bytes copied\nRouter#".to_vec(),
        ]);
        let prompts = ios_prompt_actions();
        let output = run_interactive(
            &mut transport,
            "copy flash:file running-config",
            &prompts,
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert!(output.contains("92 bytes copied"));
        // Should have sent: command, then \n for the ]? prompt
        assert_eq!(transport.sent[0], b"copy flash:file running-config\n");
        assert_eq!(transport.sent[1], b"\n");
    }

    #[tokio::test]
    async fn test_interactive_multiple_prompts() {
        // Command with two confirmation prompts before completion
        let mut transport = MockTransport::new(vec![
            b"Source filename [file]?".to_vec(),
            b"\nDestination filename [dest]?".to_vec(),
            b"\nDone!\nRouter#".to_vec(),
        ]);
        let prompts = ios_prompt_actions();
        let output = run_interactive(
            &mut transport,
            "copy src dest",
            &prompts,
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert!(output.contains("Done!"));
        // command + two auto-responses
        assert_eq!(transport.sent.len(), 3);
        assert_eq!(transport.sent[1], b"\n");
        assert_eq!(transport.sent[2], b"\n");
    }

    #[tokio::test]
    async fn test_interactive_yes_no_prompt() {
        let mut transport = MockTransport::new(vec![
            b"Proceed with delete? (yes/no)".to_vec(),
            b"\nDeleted\nRouter#".to_vec(),
        ]);
        let prompts = ios_prompt_actions();
        let output = run_interactive(
            &mut transport,
            "delete flash:file",
            &prompts,
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert!(output.contains("Deleted"));
        assert_eq!(transport.sent[1], b"yes\n");
    }

    #[tokio::test]
    async fn test_interactive_custom_prompts() {
        let mut transport = MockTransport::new(vec![
            b"Enter secret: ".to_vec(),
            b"\nOK\nRouter#".to_vec(),
        ]);
        let prompts = vec![
            ("#", PromptAction::Done),
            ("Enter secret: ", PromptAction::Respond(b"mysecret\n".to_vec())),
        ];
        let output = run_interactive(
            &mut transport,
            "setup",
            &prompts,
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        assert!(output.contains("OK"));
        assert_eq!(transport.sent[1], b"mysecret\n");
    }

    #[tokio::test]
    async fn test_interactive_timeout() {
        let mut transport = MockTransport::new(vec![
            b"waiting...".to_vec(),
            // no more data → timeout
        ]);
        let prompts = ios_prompt_actions();
        let err = run_interactive(
            &mut transport,
            "slow-cmd",
            &prompts,
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();

        match err {
            CiscoIosError::Timeout { accumulated } => {
                assert!(String::from_utf8_lossy(&accumulated).contains("waiting..."));
            }
            other => panic!("Expected Timeout, got: {:?}", other),
        }
    }
}
