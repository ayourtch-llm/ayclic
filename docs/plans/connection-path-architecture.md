# Connection Path Architecture

## Status: Phase 1-4 IMPLEMENTED, Phase 5 planned

## Problem Statement

Connecting to a network device in production rarely means "open one TCP connection."
Real-world access patterns involve jump hosts, console servers, bastion proxies,
and multi-step interactive logins. Today, `ayclic` supports direct Telnet or SSH
connections to a single target. There is no way to express multi-hop paths, and
the interactive login logic is hardcoded rather than data-driven.

We need an architecture that:

1. Models multi-hop connection paths as first-class data.
2. Cleanly separates **Transport** (establishing a byte stream) from
   **Interactive** (text-based interaction on an existing stream).
3. Provides a state-machine engine for Interactive steps, compatible with
   (and extending) the widely-used TextFSM template format.
4. Enables reuse of the 1000+ existing ntc-templates for output parsing,
   using the same engine.

## Core Concepts

### Transport

A **Transport** opens a new byte stream, optionally including protocol-level
authentication that is inseparable from the protocol itself.

```rust
enum Transport {
    Telnet { target: SocketAddr },
    Ssh { target: SocketAddr, auth: SshAuth },
    // Future: Serial { device: PathBuf, baud: u32 },
    // Future: RawTcp { target: SocketAddr },
}

/// SSH-specific authentication — these are protocol-level mechanisms
/// that only make sense within the SSH handshake.
enum SshAuth {
    Password { username: String, password: String },
    PubKey { username: String, key_path: PathBuf },
    KbdInteractive { username: String, password: String },
}
```

Key properties:

- After a Transport hop completes, the caller has a bidirectional async byte
  stream.
- SSH authentication (pubkey, password, keyboard-interactive) is part of the
  SSH protocol. It lives inside `Transport::Ssh`, not in `Interactive`.
- Telnet has no protocol-level auth. A Telnet Transport gives a raw stream;
  any login happens via a subsequent Interactive hop.

### Interactive

An **Interactive** hop operates on the current stream using text pattern
matching. It drives a conversation: match what the device says, send a
response, transition to a new state.

This covers: device login, enable mode, typing `ssh` in a shell, confirming
host keys, navigating console server menus — anything that is "send text,
match text" on an already-open stream.

Interactive hops are powered by the `aytextfsmplus` state-machine engine.

### Hop and ConnectionPath

```rust
/// Specification for opening a new byte stream.
enum TransportSpec {
    Telnet { target: SocketAddr },
    Ssh { target: SocketAddr, auth: SshAuth },
}

enum Hop {
    Transport(TransportSpec),
    Interactive(TextFSMPlus),
}

struct ConnectionPath {
    hops: Vec<Hop>,
    interactive_timeout: Duration,  // default 30s
}
```

A `ConnectionPath` is an ordered list of hops. The runtime processes them
sequentially. Each Transport hop establishes (or changes) the underlying
stream; each Interactive hop drives text-based interaction on it.

### RawTransport Trait

The `RawTransport` trait provides vendor-neutral byte-level I/O over
any protocol (Telnet, SSH). Implementations delegate to upstream
`RawTelnetSession` (aytelnet) and `RawSshSession` (ayssh):

```rust
#[async_trait]
trait RawTransport: Send + Debug {
    async fn send(&mut self, data: &[u8]) -> Result<()>;
    async fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>>;
    async fn close(&mut self) -> Result<()>;
}
```

Implementations:
- `RawTelnetTransport` — thin wrapper over `aytelnet::RawTelnetSession`
- `RawSshTransport` — thin wrapper over `ayssh::RawSshSession`
- `MockTransport` — test harness (available in `#[cfg(test)]`)

## State Machine Engine (`aytextfsmplus` crate)

### Implementation Status: COMPLETE

The engine is implemented in the `aytextfsmplus` workspace crate, forked
from `textfsm-rs` and extended. It lives alongside `ayclic` in the same
Cargo workspace.

### TextFSM Compatibility

**1790/1818 ntc-templates tests pass (98.5%).** The remaining 28 failures
are all caused by stale YAML test data in the ntc-templates repository
(YAML expects fields from older template versions). Zero parser bugs,
zero ordering issues.

Key fixes applied during implementation:
- `Continue.Record` now correctly clears `curr_record` even when a record
  is discarded due to missing Required values.
- Required values must be non-empty to satisfy the requirement (matching
  Python TextFSM behavior).
- `IndexMap` used throughout for deterministic field ordering matching
  template declaration order.

### TextFSM Semantics (Preserved)

All standard TextFSM features are supported with identical behavior:

#### Value Declarations

```
Value [option[,option...]] name (regex)
```

Options: `Filldown`, `Key`, `Required`, `List`, `Fillup` — all standard
TextFSM options with standard behavior.

#### States

- Templates must have a `Start` state.
- `EOF` is an implicit terminal state that outputs the final record.
- `End` terminates processing immediately.
- States contain ordered rules; first match wins.

#### Rules

```
^regex [-> action[.record_action] [NextState]]
```

- Regex patterns support `${ValueName}` substitution.
- `$$` represents end-of-line.

#### Line Actions

| Action   | Behavior |
|----------|----------|
| Next     | Consume line, restart matching from top of (next) state |
| Continue | Keep current line, continue to next rule |
| Error    | Abort with error message |

#### Record Actions

| Action   | Behavior |
|----------|----------|
| NoRecord | Default; do nothing |
| Record   | Save current values as a record row; clear non-Filldown values |
| Clear    | Clear non-Filldown values without recording |
| ClearAll | Clear all values |

### Extensions for Interactive Mode

Three additions to the TextFSM model:

#### 1. `Send` Line Action

```
^pattern -> Send ${expression} [NextState]
```

Sends text to the stream. The text inside `${...}` is evaluated as an
**aycalc expression** (see below), allowing captured values, preset values,
and computed results to be sent. In Parse mode, `Send` is treated like
`Next` (no stream to send to).

Simple variable references (`${Password}`) work as expected — they're just
trivial aycalc expressions that resolve to a variable lookup. But the full
expression language is available for computed values:

```
^Challenge (\d+): -> Send ${compute_response(Challenge, SharedSecret)} Auth
^Token: -> Send ${totp(Seed)} WaitPrompt
^Password: -> Send ${Password} WaitPrompt
```

#### 2. `Preset` Value Option

```
Value Preset VariableName (regex)
```

A Preset value is populated before the engine runs, from externally supplied
parameters. This is how credentials, commands, and other caller-supplied data
enter the template without being hardcoded. Set via `set_preset()` or the
builder-style `with_preset()`.

#### 3. `Done` State

```
^pattern -> Done
```

`Done` is a terminal state indicating successful completion of an interactive
session. Analogous to TextFSM's `End`, but semantically distinct: `End` means
"stop processing," `Done` means "interaction completed successfully, the
stream is ready for use."

### Expression Evaluation with aycalc

All `${...}` expansions in Send actions are evaluated as expressions by the
[aycalc](https://github.com/ayourtch/aycalc/) embeddable calculator.

The `${...}` extraction handles arbitrary expressions including spaces,
operators, function calls, and nested braces — not limited to simple
variable names.

The integration works through aycalc's two extension traits, both supplied
by the caller (matching aycalc's own API pattern for full flexibility):

- **`GetVar`**: Variable resolution. The engine provides `ValueTableVars`
  which chains the internal TextFSM value table (captured + preset values)
  with an optional caller-supplied external `GetVar`. This means the value
  table is checked first, with fallback to external variables. `NoVars` is
  provided as a default for simple cases.
- **`CallFunc`**: Caller-supplied custom functions for computed credentials
  (challenge-response, TOTP, hashing, string manipulation, etc.). `NoFuncs`
  is provided as a default.

This means:
- Simple cases (`${Password}`) are just variable lookups.
- Dynamic cases (`${compute_response(Challenge, SharedSecret)}`) get full
  expression power, including arithmetic, string operations, and custom
  functions.
- The value table accumulates state during execution, so values captured
  early in the interaction are available to expressions in later states.
- The caller can provide additional variables and functions beyond what
  the template captures.

### Engine API

The engine (`TextFSMPlus`) provides three levels of API:

#### Parse Mode (standard TextFSM)

```rust
let mut fsm = TextFSMPlus::from_str(template);      // or from_file()
let records = fsm.parse_file("output.txt", None);    // Vec<DataRecord>
```

Line-by-line processing, emits structured records. Compatible with
ntc-templates.

#### Interactive Mode — Line-Oriented

```rust
let mut fsm = TextFSMPlus::from_str(template)
    .with_preset("Username", "admin")
    .with_preset("Password", "secret");

let action = fsm.parse_line_interactive("Username: ", &vars, &funcs);
// Returns InteractiveAction::Send("admin"), ::Done, ::Error, or ::None
```

Processes complete lines. Good for testing and simple integrations.

#### Interactive Mode — Buffer-Oriented (`feed()`)

```rust
let mut fsm = TextFSMPlus::from_str(template)
    .with_preset("Username", "admin");

let result = fsm.feed(buffer, &vars, &funcs);
// Returns FeedResult { action: InteractiveAction, consumed: usize }
```

Step-at-a-time API for stream-oriented usage:
- Accepts accumulated byte buffer (not line-delimited)
- Returns action + bytes consumed
- Caller controls the I/O loop, buffer management, and transport

The caller's loop:
1. Read bytes from stream, append to buffer
2. Call `fsm.feed(&buffer, &vars, &funcs)`
3. Remove `consumed` bytes from front of buffer
4. If `Send(text)` — write text to stream
5. If `Done` — success, stream is ready
6. If `Error(msg)` — fail
7. If `None` with `consumed == 0` — read more data, repeat

Captured values are accessible via `fsm.curr_record` at any time.

### InteractiveAction and FeedResult

```rust
enum InteractiveAction {
    None,               // No action, continue reading
    Send(String),       // Send this text to the stream
    Done,               // Interactive session completed successfully
    Error(Option<String>), // Error with optional diagnostic message
}

struct FeedResult {
    action: InteractiveAction,
    consumed: usize,    // Bytes to remove from front of buffer
}
```

## Template Examples

### Parsing: Show Interfaces (Standard TextFSM, Unmodified)

```
Value Required Interface (\S+)
Value Status (up|down|administratively down)
Value Protocol (up|down)

Start
  ^${Interface}\s+is\s+${Status},\s+line protocol is\s+${Protocol} -> Record
```

This template would work identically with the ntc-templates version. No
modifications needed.

### Interactive: Telnet Login to Cisco IOS

```
Value Preset Username ()
Value Preset Password ()
Value Preset EnableSecret ()
Value Hostname (\S+)

Start
  ^Username:\s* -> Send ${Username} WaitPassword
  ^Password:\s* -> Send ${Password} WaitPrompt

WaitPassword
  ^Password:\s* -> Send ${Password} WaitPrompt

WaitPrompt
  ^${Hostname}# -> Done
  ^${Hostname}> -> Send "enable" Enable
  ^% -> Error "login failed"

Enable
  ^Password:\s* -> Send ${EnableSecret} CheckEnable

CheckEnable
  ^${Hostname}# -> Done
  ^${Hostname}> -> Error "enable failed"
  ^% -> Error "enable authentication failed"
```

Usage:
```rust
let mut fsm = TextFSMPlus::from_file("cisco_ios_login.textfsm")
    .with_preset("Username", "admin")
    .with_preset("Password", "secret123")
    .with_preset("EnableSecret", "enable_secret");

// Step-at-a-time with feed():
loop {
    let n = stream.read(&mut tmp).await?;
    buffer.extend_from_slice(&tmp[..n]);
    let result = fsm.feed(&buffer, &NoVars, &NoFuncs);
    buffer.drain(..result.consumed);
    match result.action {
        InteractiveAction::Send(text) => stream.write_all(text.as_bytes()).await?,
        InteractiveAction::Done => break,
        InteractiveAction::Error(msg) => return Err(msg.into()),
        InteractiveAction::None => continue,
    }
}
```

### Interactive: SSH Jump Host

```
Value Preset JumpPassword ()
Value Preset TargetHost ()
Value Preset TargetUser ()

Start
  ^\$ -> Send "ssh ${TargetUser}@${TargetHost}" Connecting

Connecting
  ^yes/no -> Send "yes" Connecting
  ^[Pp]assword:\s* -> Send ${JumpPassword} WaitLanding
  ^Connection refused -> Error "connection refused"
  ^No route to host -> Error "no route to host"

WaitLanding
  ^[#\$>]\s*$$ -> Done
  ^[Pp]assword:\s* -> Error "authentication failed"
  ^Permission denied -> Error "permission denied"
```

### Interactive: Console Server Navigation

```
Value Preset ConsolePassword ()
Value Preset DeviceUser ()
Value Preset DevicePassword ()
Value Preset Port (\d+)

Start
  ^Enter selection: -> Send ${Port} DeviceConnect
  ^Username:\s* -> Send ${DeviceUser} ConsoleAuth
  ^\s*$$ -> Send "" Start

ConsoleAuth
  ^Password:\s* -> Send ${ConsolePassword} DeviceConnect

DeviceConnect
  ^Username:\s* -> Send ${DeviceUser} DeviceAuth
  ^Press RETURN -> Send "" DeviceWake

DeviceWake
  ^Username:\s* -> Send ${DeviceUser} DeviceAuth
  ^[#>] -> Done

DeviceAuth
  ^Password:\s* -> Send ${DevicePassword} DevicePrompt

DevicePrompt
  ^# -> Done
  ^> -> Done
  ^% -> Error "device authentication failed"
```

## Full Connection Path Example

```rust
use ayclic::path::*;
use ayclic::raw_transport::SshAuth;
use aytextfsmplus::{TextFSMPlus, NoVars, NoFuncs};

let path = ConnectionPath::new(vec![
    // Hop 1: SSH from my machine to bastion (protocol-level auth)
    Hop::Transport(TransportSpec::Ssh {
        target: "10.1.1.1:22".parse()?,
        auth: SshAuth::PubKey {
            username: "ops".into(),
            private_key: std::fs::read("~/.ssh/id_ed25519")?,
        },
    }),

    // Hop 2: In bastion shell, SSH to target device (interactive)
    Hop::Interactive(
        TextFSMPlus::from_file("ssh_jump.textfsm")
            .with_preset("TargetUser", "operator")
            .with_preset("TargetHost", "10.200.0.5")
            .with_preset("JumpPassword", "hunter2")
    ),

    // Hop 3: Device login and enable (interactive)
    Hop::Interactive(
        TextFSMPlus::from_file("cisco_ios_login.textfsm")
            .with_preset("Username", "admin")
            .with_preset("Password", "device_pass")
            .with_preset("EnableSecret", "enable_pass")
    ),
]).with_timeout(Duration::from_secs(60));

// Execute the path — returns an EstablishedPath with active transport
let mut established = path.connect(&NoVars, &NoFuncs).await?;

// Send commands directly
established.send(b"show version\n").await?;
let output = established.receive(Duration::from_secs(10)).await?;

// Or run another template for structured interaction
let mut cmd_fsm = TextFSMPlus::from_str("...");
established.run_interactive(&mut cmd_fsm, Duration::from_secs(30), &NoVars, &NoFuncs).await?;

// Parse output using standard ntc-templates
let mut parser = TextFSMPlus::from_file("cisco_ios_show_version.textfsm");
let records = parser.parse_file("output.txt", None);
```

## Execution Runtime

### Implementation Status: COMPLETE

The runtime is implemented in `ayclic/src/path.rs`.

### Connection Path Execution

`ConnectionPath::connect()` processes hops sequentially:

```rust
// Simplified — actual implementation in ayclic/src/path.rs
for (i, hop) in self.hops.into_iter().enumerate() {
    match hop {
        Hop::Transport(spec) => {
            // Opens a new TCP connection + runs protocol
            transport = Some(match spec {
                TransportSpec::Telnet { target } =>
                    Box::new(RawTelnetTransport::connect(target).await?),
                TransportSpec::Ssh { target, auth } =>
                    Box::new(RawSshTransport::connect(target, auth).await?),
            });
        }
        Hop::Interactive(mut fsm) => {
            // Drive TextFSMPlus template on current transport
            drive_interactive(&mut fsm, transport, timeout, &vars, &funcs).await?;
        }
    }
}
```

### drive_interactive()

The core interactive loop (`drive_interactive()` in `path.rs`):

1. Try to match current buffer against TextFSMPlus rules via `feed()`
2. If `Send(text)` — write text + newline to transport
3. If `Done` — return success
4. If `Error(msg)` — return error with diagnostic message
5. If `None` — read more data from transport, append to buffer, retry
6. Overall timeout enforced across the entire interaction

### EstablishedPath

```rust
struct EstablishedPath {
    /// The active transport to the final device.
    transport: Box<dyn RawTransport>,
}
```

Provides `send()`, `receive()`, `close()`, and `run_interactive()` for
post-connection template-driven interactions.

Note: current implementation holds a single transport (the final one).
For multi-hop scenarios where the first hop is SSH and subsequent hops
are interactive (typing `ssh` in a shell), the SSH transport stays alive
because the interactive hops operate on its stream — they don't create
new transports.

## GenericCliConn

### Implementation Status: COMPLETE

`GenericCliConn` (`ayclic/src/generic_conn.rs`) is the vendor-neutral CLI
connection layer. It sits between the transport layer and vendor-specific
wrappers like `CiscoIosConn`.

```rust
struct GenericCliConn {
    transport: Box<dyn RawTransport>,
    prompt_template: String,   // TextFSMPlus template for prompt detection
    cmd_timeout: Duration,
}
```

### Constructors

| Constructor | Use Case |
|-------------|----------|
| `connect(ConnectionPath)` | Full path from scratch |
| `connect_over(EstablishedPath, hops)` | Additional hops on existing connection (pool reuse) |
| `from_established(EstablishedPath)` | Wrap already-connected transport |
| `from_transport(Box<dyn RawTransport>)` | Wrap raw transport directly |

### Prompt Template

`GenericCliConn` stores a prompt template that `run_cmd()` uses for every
command. A fresh `TextFSMPlus` engine is created from the template string
for each call, ensuring clean state.

```rust
let mut conn = GenericCliConn::connect(path, &vars, &funcs).await?
    .with_prompt_template(CISCO_IOS_PROMPT)  // set once
    .with_cmd_timeout(Duration::from_secs(60));

conn.run_cmd("show version", &vars, &funcs).await?;          // uses stored template
conn.run_cmd("show interfaces", &vars, &funcs).await?;       // same template, fresh engine

// One-off override for special commands:
conn.run_cmd_with_template("copy run start", COPY_TEMPLATE, &vars, &funcs).await?;
```

### Transport Ownership

- `into_transport()` extracts the transport (consumes the connection)
- Enables pool return: checkout → connect_over → use → into_transport → checkin

## CiscoIosConn Integration

### Implementation Status: COMPLETE

`CiscoIosConn::new()` and `with_timeouts()` now use the template-driven
`ConnectionPath` architecture internally:

1. Parse target address into `SocketAddr` (with default ports: 22/SSH, 23/Telnet)
2. Build `ConnectionPath` with `TransportSpec` + Cisco login template
3. Connect via `GenericCliConn`
4. Set `CISCO_IOS_PROMPT` as the prompt template
5. Wrap as `CiscoIosConn` via `from_generic()`

Legacy constructors (`new_legacy()`, `with_timeouts_legacy()`) preserved
for backward compatibility — they use the old `CiscoTelnet`/`ayssh::CiscoConn`
directly.

Additional constructors:
- `from_generic(GenericCliConn)`: bridge any GenericCliConn to CiscoIosConn
- `from_path(ConnectionPath)`: connect via template-driven path

## Built-in Templates

### Implementation Status: PARTIAL (Cisco IOS only)

Located in `ayclic/src/templates.rs`:

| Template | Purpose |
|----------|---------|
| `CISCO_IOS_SSH_POST_LOGIN` | Post-SSH-auth: wait for prompt, send `terminal length 0` |
| `CISCO_IOS_TELNET_LOGIN` | Telnet login: username/password prompts, then `terminal length 0` |
| `CISCO_IOS_PROMPT` | Command prompt detection: `#` as Done, auto-confirm `]?`, `[confirm]`, `(yes/no)` |

## Session Transcript / Audit Logging

### Implementation Status: COMPLETE

`LoggingTransport` wraps any `RawTransport` and records all sent/received
data to a `TranscriptSink`. Since the sink is shared via `Arc<Mutex<>>`,
the caller can read the transcript at any time — even while the transport
is owned by a `GenericCliConn` or `CiscoIosConn`.

### TranscriptSink trait

```rust
trait TranscriptSink: Send + Debug {
    fn record(&mut self, entry: TranscriptEntry);
}
```

Implement to control where transcript data goes. Built-in implementations:

| Sink | Use Case |
|------|----------|
| `VecTranscriptSink` | In-memory, inspect later (debugging, testing) |
| `FileTranscriptSink` | Real-time file output (audit trails) |

### Usage Patterns

```rust
// In-memory transcript (shared handle, readable anytime):
let transcript = new_transcript();
let transport = LoggingTransport::new(inner, transcript.clone());
let mut conn = GenericCliConn::from_transport(Box::new(transport));
conn.run_cmd("show version", &vars, &funcs).await?;
println!("{}", transcript.lock().unwrap().to_display_string());

// Fire-and-forget file logging (no handle needed):
let transport = with_file_logging(inner, "/var/log/session.log")?;
let mut conn = GenericCliConn::from_transport(transport);

// Append to existing audit file:
let transport = with_file_logging_append(inner, "/var/log/audit.log")?;

// Custom sink (syslog, channel, ring buffer, etc.):
impl TranscriptSink for MySink { fn record(&mut self, entry: TranscriptEntry) { ... } }
```

Empty receives (timeouts) are NOT recorded — only actual data.

## Integration with Existing Crate Structure

### Where Things Live

| Component | Crate / Module | Status |
|-----------|----------------|--------|
| Telnet protocol | `aytelnet` (external) | Done |
| Telnet raw session | `aytelnet::RawTelnetSession` (upstream) | **Done** |
| SSH protocol | `ayssh` (external) | Done |
| SSH raw session | `ayssh::RawSshSession` (upstream) | **Done** |
| Expression evaluation | `aycalc` (external) | Done |
| TextFSM+ engine | `aytextfsmplus` (workspace member) | **Done** |
| RawTransport trait + wrappers | `ayclic::raw_transport` | **Done** |
| LoggingTransport + transcript | `ayclic::raw_transport` | **Done** |
| ConnectionPath + runtime | `ayclic::path` | **Done** |
| GenericCliConn | `ayclic::generic_conn` | **Done** |
| Built-in Cisco templates | `ayclic::templates` | **Done** |
| CiscoIosConn (template-driven) | `ayclic::conn` | **Done** |

### Upstream Integrations

Both `aytelnet` and `ayssh` were updated to provide vendor-neutral raw
session APIs, per the request documents in `docs/upstream-requests/`:

- **aytelnet**: Added `RawTelnetSession` (send/receive with TELNET protocol
  event filtering) and `Debug` for `TelnetConnection`.
- **ayssh**: Added `RawSshSession` (full connect+auth+PTY+shell setup,
  send/receive with SSH message filtering) and `Debug` for `Transport`
  and all public types.

The `ayclic` wrappers (`RawTelnetTransport`, `RawSshTransport`) are now
thin ~30-line delegations to the upstream types.

### Migration Path

1. **Phase 1**: Implement the TextFSM engine with Parse mode.
   Validate against ntc-templates.
   **Status: DONE** — `aytextfsmplus` crate, 1790/1818 ntc-templates pass.

2. **Phase 2**: Add Interactive extensions (Send, Preset, Done). Implement
   `feed()` step-at-a-time API with aycalc integration.
   **Status: DONE** — 56 tests, full aycalc integration, `GetVar`/`CallFunc`
   flexibility matching aycalc's own API.

3. **Phase 3**: Implement `ConnectionPath`, `Hop`, `TransportSpec`,
   `EstablishedPath`, `RawTransport` trait, and the connection runtime.
   Upstream `RawTelnetSession` and `RawSshSession` to aytelnet/ayssh.
   **Status: DONE** — 80 tests in ayclic, `drive_interactive()` core loop,
   `MockTransport` for testing without network.

4. **Phase 4**: `GenericCliConn` (vendor-neutral CLI connection) and update
   `CiscoIosConn` to use template-driven paths.
   **Status: DONE** — `GenericCliConn` with stored prompt template,
   `run_cmd()`, `run_cmd_with_template()`, `run_interactive()`,
   `connect()`, `connect_over()`, `into_transport()`.
   `CiscoIosConn::new()` now uses `ConnectionPath` + built-in Cisco
   templates internally. Legacy constructors preserved as `new_legacy()`.
   `LoggingTransport` with `TranscriptSink` trait, `VecTranscriptSink`,
   `FileTranscriptSink`, `with_file_logging()` convenience.

5. **Phase 5**: Write template libraries for common login/jump patterns.
   **Status: Partially done.** Built-in Cisco IOS templates exist
   (`CISCO_IOS_SSH_POST_LOGIN`, `CISCO_IOS_TELNET_LOGIN`,
   `CISCO_IOS_PROMPT`). Jump host and other vendor templates planned.

## Design Decisions

The following questions were raised during review and resolved:

### D1. Timeout handling in Interactive mode

**Decision**: Three-level override chain: **global default < per-template < per-state**.

The global default is a module-level `AtomicU64` named `DEFAULT_TIMEOUT_SECONDS`
(initially 30). Since it is atomic, callers can adjust it at runtime without
recompiling — useful for slow environments (lab over VPN) vs. fast (local
network). Templates can specify a default timeout that overrides the global.
Individual states can override the template default for specific slow steps.

**Implementation**: Two levels are implemented:
- `ConnectionPath::interactive_timeout` — per-path timeout (default 30s),
  configurable via `with_timeout()`.
- `drive_interactive()` enforces this as an overall deadline for each
  Interactive hop, with 5-second read intervals within.

The `feed()` API remains timeout-agnostic for callers who manage their own
loop. The global `AtomicU64` default is deferred — the per-path timeout
is sufficient for current use cases.

### D2. Stream vs. line matching in Interactive mode

**Decision**: Line-by-line in Parse mode (standard TextFSM), accumulated
buffer matching in Interactive mode.

**Implementation**: Both modes are implemented.
- `parse_line()` / `parse_file()`: line-by-line Parse mode.
- `parse_line_interactive()`: line-oriented Interactive mode (for testing).
- `feed()`: buffer-oriented Interactive mode (for real streams).

The `feed()` method matches regex patterns against the accumulated buffer.
Note that `^` anchors match at the start of the buffer, not at line
boundaries within it. The caller manages buffer accumulation and trimming
using the `consumed` count returned by `feed()`.

Compiling rules to aho-corasick is a natural optimization for later — the
existing `ayclic` codebase already proves this approach works.

### D3. SSH ProxyJump optimization

**Decision**: Removed from scope. The explicit hop-by-hop model is the
architecture. Native SSH ProxyJump may be added in the future as a parameter
to the SSH transport variant, but only when there is concrete operational
experience demonstrating the need. For future considerations only.

### D4. Template discovery and loading

**Decision**: `from_str(&str)` is the primitive API — parse a template from
its text content. `from_file(path)` is a convenience method built on top.

**Implementation**: Both are implemented on `TextFSMPlus` and
`TextFSMPlusParser`. Leading newlines are automatically trimmed for
ergonomic use with Rust raw string literals.

### D5. Credential management and computed values

**Decision**: Static credentials are supplied via Preset values (caller's
responsibility — where they come from is outside scope). Dynamic/computed
credentials use aycalc expressions in Send actions.

**Implementation**: Fully implemented.
- `set_preset()` and `with_preset()` (builder pattern) for static values.
- `${...}` in Send actions evaluated by aycalc with full expression support.
- `GetVar` chains internal value table with caller-supplied external provider.
- `CallFunc` is caller-supplied for custom functions.
- Both `GetVar` and `CallFunc` are passed to `expand_send_text()`, `feed()`,
  and `parse_line_interactive()`, matching aycalc's own API flexibility.
- `NoVars` and `NoFuncs` provided as defaults for simple cases.

### D6. Error recovery

**Decision**: Fail fast. When an Interactive hop reaches an `Error` state,
execution stops and the error (including the template-defined message) is
returned to the caller. No automatic recovery (Ctrl-C, disconnect, retry)
is attempted.

**Implementation**: `InteractiveAction::Error(Option<String>)` carries the
template-defined error message. The caller decides whether to retry.

### D7. Value sharing across hops

**Decision**: Deferred to the implementer of the `GetVar` trait on the
aycalc context instance. The architecture provides the mechanism (a mutable
aycalc context that persists across the `ConnectionPath` execution). Whether
values are shared across hops or isolated per-hop is a policy decision made
by whoever implements the `GetVar` trait — not prescribed by the architecture.

**Implementation**: `curr_record` is public on `TextFSMPlus`, so captured
values are always accessible. The `ValueTableVars` wrapper chains the
internal value table with an external `GetVar`, allowing cross-hop sharing
when the caller passes a shared context.

## Future Considerations

Items explicitly deferred, to be revisited when operational experience
demonstrates the need:

### Protocol Stacking (BoxedStream)

For true protocol-over-protocol stacking (SSH-over-Telnet, SSH-over-SSH
at the protocol level, not shell-level commands), both `aytelnet` and
`ayssh` need to accept generic async byte streams instead of `TcpStream`:

```rust
type BoxedStream = Box<dyn AsyncRead + AsyncWrite + Send + Unpin>;
```

- `TelnetConnection::over_stream(BoxedStream)` — run Telnet over any stream
- `Transport::over_stream(BoxedStream)` — run SSH over any stream
- `TransportStream` adapter in ayclic bridges `RawTransport` → `AsyncRead + AsyncWrite`

No micro-crate needed — `AsyncRead + AsyncWrite` from tokio is the shared
contract. Request documents with full API details are in
`docs/upstream-requests/`.

This is backwards-compatible (TcpStream implements the required traits).

### Other Deferred Items

- **SSH ProxyJump**: Native ProxyJump optimization for multi-SSH-hop paths.
- **Template registries**: Community template sharing beyond filesystem loading.
- **Connection string DSL**: `compile_path("cisco-ssh{10.1.1.1, user=admin}")` —
  a parser that compiles connection strings into `ConnectionPath` using a
  profile registry. Deferred: syntax design needs operational experience.
- **Connection pool**: Reusable jumphost connections via `connect_over()` +
  `into_transport()`. The architecture supports it; pool management logic
  is application-level.
- **Serial/Netconf transports**: Additional `TransportSpec` variants.
- **aho-corasick optimization**: Compile interactive rules to aho-corasick
  for faster multi-pattern matching in `feed()`.
- **Multiline regex in feed()**: Support `(?m)` flag or automatic line
  splitting so `^` matches after newlines within accumulated buffers.
- **DEFAULT_TIMEOUT_SECONDS**: Global atomic timeout default.
- **CiscoIosError cleanup**: Vendor-neutral error type for the path module.
- **Newline handling in Send**: `drive_interactive()` appends `\n` after
  every Send. Some devices may need `\r\n` or no newline.
- **Additional vendor templates**: Juniper, Arista, MikroTik, Linux login
  and prompt detection templates.
