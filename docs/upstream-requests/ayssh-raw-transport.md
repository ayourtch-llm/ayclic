# Request: Add Vendor-Neutral Raw Transport API to ayssh

## Context

The `ayclic` crate (which depends on `ayssh`) is building a vendor-neutral
connection framework where device interaction (login, prompts, enable mode)
is driven by data-driven templates (TextFSMPlus) rather than hardcoded in
the transport layer.

Currently `ayssh` exposes:
- `Transport` — low-level SSH protocol (packets, encryption, channels)
- `Session` — session channel management (PTY, shell, exec)
- `SshClient` — connection + authentication
- `CiscoConn` — Cisco-specific: auto-login, `term len 0`, prompt detection

We need a middle layer: a simple `send(&[u8])` / `receive(timeout) -> Vec<u8>`
API over an authenticated SSH channel that does NO vendor-specific work.
Think of it as `CiscoConn` minus all the Cisco parts.

## What's Needed

### 1. Add `Debug` impl for `Transport`

`Transport` currently doesn't implement `Debug`. We need it for trait
objects and diagnostic output. A manual impl that avoids printing
sensitive crypto state:

```rust
impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("state", &self.state())
            .finish()
    }
}
```

### 2. Add `RawSshSession` struct

A vendor-neutral wrapper that handles SSH protocol internally and exposes
a raw byte-stream interface:

```rust
pub struct RawSshSession {
    transport: Transport,
    channel_id: u32,  // remote channel ID for sending
}

impl RawSshSession {
    /// Connect, authenticate, open a session channel with PTY+shell.
    ///
    /// After this returns, the caller has a raw byte stream to the
    /// remote shell. No vendor-specific commands are sent.
    pub async fn connect_with_password(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<Self, SshError> {
        // 1. SshClient::new(host, port)
        // 2. client.connect() to get Transport
        // 3. Authenticate (password)
        // 4. Session::open(&mut transport)
        // 5. Request PTY (vt100, 80x24)
        // 6. Request shell
        // 7. Return Self { transport, channel_id: session.remote_channel_id() }
    }

    pub async fn connect_with_publickey(
        host: &str,
        port: u16,
        username: &str,
        private_key: &[u8],
    ) -> Result<Self, SshError> {
        // Same flow but with pubkey auth
    }

    /// Create from an already-authenticated transport and channel.
    ///
    /// `channel_id` must be the REMOTE channel ID (for sending to server).
    pub fn from_parts(transport: Transport, channel_id: u32) -> Self {
        Self { transport, channel_id }
    }

    /// Send raw bytes to the remote shell.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), SshError> {
        self.transport.send_channel_data(self.channel_id, data).await
    }

    /// Receive raw bytes from the remote shell.
    ///
    /// CRITICAL SEMANTICS (must match CiscoConn::receive):
    /// - If data is immediately available, return it RIGHT AWAY
    /// - Only block up to `timeout` if there is NO data yet
    /// - Return empty Vec if timeout expires with no data (not an error)
    /// - Filter SSH protocol messages internally:
    ///   - SSH_MSG_CHANNEL_DATA (94): extract and return the payload
    ///   - SSH_MSG_CHANNEL_WINDOW_ADJUST (93): ignore, keep reading
    ///   - SSH_MSG_CHANNEL_EOF (96): return error (channel ended)
    ///   - SSH_MSG_CHANNEL_CLOSE (97): return error (channel closed)
    ///   - Other message types: ignore, keep reading
    pub async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, SshError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Ok(vec![]);
            }
            let remaining = deadline - now;
            match tokio::time::timeout(remaining, self.transport.recv_message()).await {
                Ok(Ok(msg)) if !msg.is_empty() => {
                    match msg[0] {
                        94 => { // SSH_MSG_CHANNEL_DATA
                            if msg.len() > 9 {
                                let data_len = u32::from_be_bytes(
                                    [msg[5], msg[6], msg[7], msg[8]]
                                ) as usize;
                                if msg.len() >= 9 + data_len {
                                    return Ok(msg[9..9 + data_len].to_vec());
                                }
                            }
                            return Ok(vec![]);
                        }
                        93 => continue,  // WINDOW_ADJUST
                        96 => return Err(SshError::ChannelError("EOF".into())),
                        97 => return Err(SshError::ChannelError("Closed".into())),
                        _ => continue,   // other messages
                    }
                }
                Ok(Ok(_)) => continue,
                Ok(Err(e)) => return Err(e),
                Err(_) => return Ok(vec![]),  // timeout
            }
        }
    }

    /// Close the channel.
    pub async fn disconnect(&mut self) -> Result<(), SshError> {
        self.transport.send_channel_close(self.channel_id).await
    }

    /// Get the underlying transport (for advanced use).
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Get the underlying transport mutably.
    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }
}
```

### 3. Expose Transport After Authentication

Currently `SshClient::connect_with_password()` returns `Session`, but we
need access to the `Transport` underneath. Two options:

**Option A** (preferred): `RawSshSession` handles everything internally
(as shown above) — it does the connect, auth, PTY, shell setup and holds
the `Transport` directly. This is self-contained and doesn't require
changing `SshClient`.

**Option B**: Add a method to `Session` to extract the transport:
```rust
impl Session {
    pub fn into_parts(self) -> (Transport, u32) { ... }
}
```

Option A is simpler and doesn't change existing APIs.

### 4. Export from lib.rs

Add `RawSshSession` to the public API in `lib.rs`.

## Why This Matters

The calling code (`ayclic`) will use `RawSshSession` like this:

```rust
let mut session = ayssh::RawSshSession::connect_with_password(
    "10.1.1.1", 22, "operator", "password123"
).await?;
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

This makes SSH connections vendor-neutral — the same transport works for
Cisco, Juniper, Arista, MikroTik, Linux, or any device that speaks SSH.
All vendor-specific behavior is in the TextFSMPlus template, not the
transport.

## Prototype Reference

We have a working prototype in `ayclic/src/raw_transport.rs` (the
`RawSshTransport` struct). It implements the `receive()` logic with
SSH message filtering and works for the `RawTransport` trait. The
`connect()` method is currently `todo!()` because we need the transport
exposed after auth — this is the main gap.

`CiscoConn` in `ayssh/src/cisco_conn.rs` is the best reference for how
to set up the connection (connect → auth → open session → PTY → shell),
since `RawSshSession` needs to do the same steps minus the Cisco-specific
parts (`term len 0`, prompt detection, etc.).

## Summary of Changes

| Change | Effort | Status |
|--------|--------|--------|
| `Debug` impl for `Transport` | Small | **Done** |
| `Debug` impls for all public types | Small | **Done** |
| `RawSshSession` struct | Medium (~100 lines) | **Done** |
| Export in `lib.rs` | Trivial | **Done** |
| Generic stream support for `Transport` | Medium | Planned (see below) |

## Reference: CiscoConn's Connection Flow

For reference, here's what `CiscoConn` does during setup (from
`cisco_conn.rs`). `RawSshSession` should do steps 1-5 and skip step 6:

1. `SshClient::new(host, port)` — create client
2. `client.connect()` — TCP connect + SSH handshake → `Transport`
3. Authenticate (password/pubkey/keyboard-interactive)
4. `Session::open(&mut transport)` — open session channel
5. Request PTY + shell (send channel requests via transport)
6. ~~Send `term len 0` and wait for prompt~~ ← SKIP THIS (vendor-specific)

---

## Future Request: Generic Stream Support for Protocol Stacking

### Context

The connection path architecture supports multi-hop access where protocols
can be **stacked** — e.g., running SSH over a Telnet session (terminal
server scenarios), running SSH over another SSH channel, or running SSH
over any arbitrary byte stream.

For this to work, `Transport` needs to accept any async byte stream as
its underlying I/O, not just `TcpStream`.

### What's Needed

Change `Transport` to accept a boxed async stream:

```rust
type BoxedStream = Box<dyn tokio::io::AsyncRead
                     + tokio::io::AsyncWrite
                     + Send
                     + Unpin>;

pub struct Transport {
    stream: BoxedStream,  // was: TcpStream
    // ... rest unchanged
}

impl Transport {
    /// Create over a TCP connection (existing behavior, unchanged API).
    pub fn new(stream: TcpStream) -> Self {
        Self::over_stream(Box::new(stream))
    }

    /// Run the SSH protocol over an arbitrary async byte stream.
    ///
    /// This enables protocol stacking — e.g., running SSH over
    /// a Telnet session, over another SSH channel, or over any
    /// other protocol's data stream.
    pub fn over_stream(stream: BoxedStream) -> Self {
        Self {
            stream,
            // ... same initialization as new()
        }
    }
}
```

### Also Update RawSshSession

Add connect methods that work over an existing stream:

```rust
impl RawSshSession {
    /// Existing TCP-based connections (unchanged)
    pub async fn connect_with_password(...) -> Result<Self> { ... }
    pub async fn connect_with_publickey(...) -> Result<Self> { ... }

    /// Run SSH over an arbitrary byte stream.
    ///
    /// Performs SSH handshake, authenticates, opens session channel,
    /// allocates PTY, and starts shell — all over the provided stream
    /// instead of a TCP connection.
    pub async fn connect_over_stream_with_password(
        stream: BoxedStream,
        username: &str,
        password: &str,
    ) -> Result<Self, SshError> {
        let mut transport = Transport::over_stream(stream);
        transport.handshake().await?;
        // ... authenticate, open session, PTY, shell ...
        Ok(Self::from_parts(transport, channel_id))
    }

    pub async fn connect_over_stream_with_publickey(
        stream: BoxedStream,
        username: &str,
        private_key: &[u8],
    ) -> Result<Self, SshError> {
        // same pattern
    }
}
```

### Why This Is Backwards-Compatible

- `TcpStream` implements `AsyncRead + AsyncWrite`, so boxing it is free.
- The existing `new()` and `connect_*()` APIs are unchanged — they box
  internally.
- `over_stream()` and `connect_over_stream_*()` are new constructors.
- All internal `stream.read()` / `stream.write()` calls work identically
  on `BoxedStream` as on `TcpStream` (same trait methods).
- No new dependencies required.

### How ayclic Will Use This

```rust
// ayclic has a TransportStream adapter that wraps any RawTransport
// as an AsyncRead + AsyncWrite:
let telnet_transport: Box<dyn RawTransport> = /* existing Telnet session */;
let stream = TransportStream::new(telnet_transport);

// Run SSH protocol over the Telnet session
let ssh_session = RawSshSession::connect_over_stream_with_password(
    Box::new(stream),
    "operator",
    "password123",
).await?;
// Now ssh_session.send()/receive() speaks SSH-over-Telnet
```

This enables scenarios like:
- **SSH over Telnet**: Connect to a terminal server via Telnet, then SSH
  to a device behind it at the protocol level (not shell-level `ssh` command)
- **SSH over SSH**: True nested SSH (not typing `ssh` in a shell), useful
  for environments where SSH port forwarding is restricted
- **SSH over any transport**: Future transports (serial, WebSocket, etc.)
  can all serve as the underlying stream for SSH

### Effort

| Change | Effort |
|--------|--------|
| Change `stream` field type to `BoxedStream` | Small (type swap) |
| Add `Transport::over_stream()` | Small |
| Add `RawSshSession::connect_over_stream_*()` | Medium (reuse existing connect flow) |
| Box `TcpStream` in `Transport::new()` | Trivial |

No behavioral changes, no new dependencies. Just widening the accepted
input type from concrete `TcpStream` to trait-object `BoxedStream`.
