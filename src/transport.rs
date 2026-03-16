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

#[cfg(test)]
mod tests {
    use super::*;

    // === Mock transport for testing ===

    #[derive(Debug)]
    struct MockTransport {
        chunks: Vec<Vec<u8>>,
        index: usize,
    }

    impl MockTransport {
        fn new(chunks: Vec<Vec<u8>>) -> Self {
            Self { chunks, index: 0 }
        }
    }

    #[async_trait]
    impl CiscoTransport for MockTransport {
        async fn send(&mut self, _data: &[u8]) -> Result<(), CiscoIosError> {
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
}
