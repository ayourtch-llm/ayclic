# Request: Add Vendor-Neutral Raw Transport API to aytelnet

## Context

The `ayclic` crate (which depends on `aytelnet`) is building a vendor-neutral
connection framework where device interaction (login, prompts, enable mode)
is driven by data-driven templates (TextFSMPlus) rather than hardcoded in
the transport layer.

Currently `aytelnet` exposes two levels:
- `TelnetConnection` — protocol-correct, vendor-neutral
- `CiscoTelnet` — Cisco-specific login, prompt detection, `term len 0`

We need a middle layer: a simple `send(&[u8])` / `receive(timeout) -> Vec<u8>`
API over `TelnetConnection` that handles TELNET protocol events internally
but does NO vendor-specific work. Think of it as `CiscoTelnet` minus all
the Cisco parts.

## What's Needed

### 1. Add `Debug` impl for `TelnetConnection`

`TelnetConnection` currently doesn't implement `Debug`. We need it for
trait objects and diagnostic output. A simple manual impl is fine:

```rust
impl std::fmt::Debug for TelnetConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelnetConnection")
            .field("state", &self.state)
            .finish()
    }
}
```

### 2. Add `RawTelnetSession` struct

A vendor-neutral wrapper over `TelnetConnection` with these semantics:

```rust
pub struct RawTelnetSession {
    conn: TelnetConnection,
}

impl RawTelnetSession {
    /// Connect to a Telnet server and negotiate standard options.
    pub async fn connect(host: &str, port: u16) -> Result<Self> {
        // Use TelnetConnection::start_with_config with standard options
        // (echo, binary, suppress go-ahead)
    }

    /// Create from an already-connected TelnetConnection.
    pub fn from_connection(conn: TelnetConnection) -> Self {
        Self { conn }
    }

    /// Send raw bytes to the remote end.
    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.conn.send(data).await
    }

    /// Receive raw bytes from the remote end.
    ///
    /// CRITICAL SEMANTICS (must match CiscoTelnet::receive):
    /// - If data is immediately available, return it RIGHT AWAY
    /// - Only block up to `timeout` if there is NO data yet
    /// - Return empty Vec if timeout expires with no data (not an error)
    /// - Filter out TELNET protocol events (Commands, OptionNegotiated)
    ///   internally — only return Data events to the caller
    /// - On connection close, return error
    pub async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Ok(vec![]);
            }
            let remaining = deadline - now;
            match tokio::time::timeout(remaining, self.conn.receive()).await {
                Ok(Ok(TelnetEvent::Data(data))) => return Ok(data),
                Ok(Ok(TelnetEvent::Closed)) => return Err(TelnetError::Disconnected),
                Ok(Ok(TelnetEvent::Error(e))) => return Err(e),
                Ok(Ok(_)) => continue, // Command, OptionNegotiated — skip
                Ok(Err(e)) => return Err(e),
                Err(_) => return Ok(vec![]), // timeout
            }
        }
    }

    /// Close the connection.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.conn.disconnect().await
    }
}
```

The key point: `receive()` must have the **same "return immediately on data"
semantics** that `CiscoTelnet::receive()` has. This enables fast incremental
pattern matching by an external state machine.

### 3. Export from lib.rs

Add `RawTelnetSession` to the public API in `lib.rs`.

## Why This Matters

The calling code (`ayclic`) will use `RawTelnetSession` like this:

```rust
let mut session = aytelnet::RawTelnetSession::connect("10.1.1.1", 23).await?;
let mut buffer = Vec::new();

loop {
    let chunk = session.receive(Duration::from_secs(30)).await?;
    if !chunk.is_empty() {
        buffer.extend_from_slice(&chunk);
    }
    // TextFSMPlus template handles login, prompts, enable — not the transport
    let result = fsm.feed(&buffer, &vars, &funcs);
    buffer.drain(..result.consumed);
    match result.action {
        Send(text) => session.send(text.as_bytes()).await?,
        Done => break,
        Error(msg) => return Err(msg.into()),
        None => continue,
    }
}
```

This makes telnet connections vendor-neutral — the same transport works for
Cisco, Juniper, Arista, MikroTik, or any device that speaks telnet. All
vendor-specific behavior is in the TextFSMPlus template, not the transport.

## Prototype Reference

We have a working prototype in `ayclic/src/raw_transport.rs` (the
`RawTelnetTransport` struct). It works but has a manual `Debug` impl
workaround because `TelnetConnection` doesn't implement `Debug`. The
upstream implementation can use this as reference.

## Summary of Changes

| Change | Effort | Status |
|--------|--------|--------|
| `Debug` impl for `TelnetConnection` | Small | **Done** |
| `RawTelnetSession` struct | Small (~50 lines) | **Done** |
| Export in `lib.rs` | Trivial | **Done** |
| Generic stream support for `TelnetConnection` | Medium | Planned (see below) |

---

## Future Request: Generic Stream Support for Protocol Stacking

### Context

The connection path architecture supports multi-hop access where protocols
can be **stacked** — e.g., running Telnet over an SSH channel, or running
SSH over a Telnet session (for terminal servers that expose SSH on the
other side).

For this to work, `TelnetConnection` needs to accept any async byte stream
as its underlying I/O, not just `TcpStream`.

### What's Needed

Change `TelnetConnection` to accept a boxed async stream:

```rust
type BoxedStream = Box<dyn tokio::io::AsyncRead
                     + tokio::io::AsyncWrite
                     + Send
                     + Unpin>;

pub struct TelnetConnection {
    stream: Option<BoxedStream>,  // was: Option<TcpStream>
    // ... rest unchanged
}

impl TelnetConnection {
    /// Connect over TCP (existing behavior, unchanged API).
    pub async fn connect(host: &str, port: u16) -> Result<Self> {
        let tcp = TcpStream::connect(format!("{}:{}", host, port)).await?;
        Ok(Self::over_stream(Box::new(tcp)))
    }

    /// Run the TELNET protocol over an arbitrary async byte stream.
    ///
    /// This enables protocol stacking — e.g., running Telnet over
    /// an SSH channel or over another protocol's data stream.
    pub fn over_stream(stream: BoxedStream) -> Self {
        Self {
            stream: Some(stream),
            // ... same initialization as connect()
        }
    }
}
```

### Why This Is Backwards-Compatible

- `TcpStream` implements `AsyncRead + AsyncWrite`, so boxing it is free.
- The existing `connect()` API is unchanged — it just boxes internally.
- `over_stream()` is a new constructor, no existing code affected.
- All internal `stream.read()` / `stream.write()` calls work identically
  on `BoxedStream` as on `TcpStream` (same trait methods).

### How ayclic Will Use This

```rust
// ayclic has a TransportStream adapter that wraps any RawTransport
// as an AsyncRead + AsyncWrite:
let ssh_transport: Box<dyn RawTransport> = /* existing SSH connection */;
let stream = TransportStream::new(ssh_transport);

// Run Telnet protocol over the SSH channel
let telnet = TelnetConnection::over_stream(Box::new(stream));
let session = RawTelnetSession::from_connection(telnet);
// Now session.send()/receive() speaks Telnet-over-SSH
```

### Also Update `start_with_config`

If `start_with_config()` currently calls `connect()` internally, add
an `over_stream` variant:

```rust
/// Start with config over an existing stream.
pub async fn start_with_config_over(
    stream: BoxedStream,
    enable_echo: bool,
    enable_binary: bool,
    enable_suppress_ga: bool,
) -> Result<Self> {
    let mut conn = Self::over_stream(stream);
    // ... negotiate options (same as existing) ...
    Ok(conn)
}
```

### Effort

| Change | Effort |
|--------|--------|
| Change `stream` field type to `BoxedStream` | Small (type swap) |
| Add `over_stream()` constructor | Small |
| Add `start_with_config_over()` | Small |
| Box `TcpStream` in `connect()` | Trivial |

No behavioral changes, no new dependencies. Just widening the accepted
input type from concrete `TcpStream` to trait-object `BoxedStream`.
