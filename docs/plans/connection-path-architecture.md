# Connection Path Architecture

## Status: DRAFT — Review Requested

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

Interactive hops are powered by a state-machine engine (see below).

### Hop and ConnectionPath

```rust
enum Hop {
    Transport(Transport),
    Interactive(InteractiveTemplate),
}

struct ConnectionPath {
    hops: Vec<Hop>,
}
```

A `ConnectionPath` is an ordered list of hops. The runtime processes them
sequentially. Each Transport hop establishes (or changes) the underlying
stream; each Interactive hop drives text-based interaction on it.

## State Machine Engine

### Design Principle: TextFSM Superset

The engine implements full TextFSM semantics and extends them with two
additions for driving interactive sessions. This means:

- Standard TextFSM templates (e.g., from ntc-templates) work unmodified
  for **parsing** command output.
- Extended templates add the ability to **send** data, enabling interactive
  session driving with the same engine.

### TextFSM Semantics (Preserved)

The following TextFSM features are supported with identical behavior:

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

Two additions to the TextFSM model:

#### 1. `Send` Line Action

```
^pattern -> Send ${ValueName} [NextState]
```

Sends text to the stream. The text supports `${ValueName}` substitution,
allowing captured or preset values to be sent. After sending, behaves like
`Next` (consume, advance).

#### 2. `Preset` Value Option

```
Value Preset VariableName (regex)
```

A Preset value is populated before the engine runs, from externally supplied
parameters. This is how credentials, commands, and other caller-supplied data
enter the template without being hardcoded.

#### 3. `Done` State

```
^pattern -> Done
```

`Done` is a terminal state indicating successful completion of an interactive
session. Analogous to TextFSM's `End`, but semantically distinct: `End` means
"stop processing," `Done` means "interaction completed successfully, the
stream is ready for use."

### Engine Modes

The same compiled template can operate in two modes:

| Mode | Input | Processing | Output |
|------|-------|-----------|--------|
| **Parse** | Block of text (e.g., `show` output) | Line-by-line | `Vec<Record>` (structured rows) |
| **Interactive** | Live async byte stream | Stream-oriented, pattern matching on accumulated data | Side effects (Send), terminal state |

In Parse mode, the engine behaves exactly like standard TextFSM: process
lines, capture values, emit records.

In Interactive mode, the engine reads from a stream, matches patterns against
accumulated data (not line-delimited — prompts don't end with newlines), and
can Send responses.

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
let template = InteractiveTemplate::from_file("cisco_ios_login.textfsm")?;
template.set_preset("Username", "admin");
template.set_preset("Password", "secret123");
template.set_preset("EnableSecret", "enable_secret");
template.drive(&mut stream).await?;
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
let path = ConnectionPath {
    hops: vec![
        // Hop 1: SSH from my machine to bastion (protocol-level auth)
        Hop::Transport(Transport::Ssh {
            target: "10.1.1.1:22".parse()?,
            auth: SshAuth::PubKey {
                username: "ops".into(),
                key_path: "~/.ssh/id_ed25519".into(),
            },
        }),

        // Hop 2: In bastion shell, SSH to target device (interactive)
        Hop::Interactive(
            InteractiveTemplate::from_file("ssh_jump.textfsm")?
                .with_preset("TargetUser", "operator")
                .with_preset("TargetHost", "10.200.0.5")
                .with_preset("JumpPassword", "hunter2")
        ),

        // Hop 3: Device login and enable (interactive)
        Hop::Interactive(
            InteractiveTemplate::from_file("cisco_ios_login.textfsm")?
                .with_preset("Username", "admin")
                .with_preset("Password", "device_pass")
                .with_preset("EnableSecret", "enable_pass")
        ),
    ],
};

// Execute the path — returns a connected, authenticated stream
let stream = path.connect().await?;

// Wrap in CiscoIosConn for command execution
let mut conn = CiscoIosConn::from_stream(stream);
let output = conn.run_cmd("show version").await?;

// Parse output using standard ntc-templates
let template = Template::from_file("cisco_ios_show_version.textfsm")?;
let records = template.parse(&output)?;
```

## Execution Runtime

### Connection Path Execution

```
let mut current_stream: Option<Box<dyn AsyncReadWrite>> = None;

for hop in path.hops {
    match hop {
        Hop::Transport(transport) => {
            // Opens a new TCP connection + runs protocol
            current_stream = Some(transport.connect().await?);
        }
        Hop::Interactive(template) => {
            // Drives interaction on the current stream
            let stream = current_stream.as_mut()
                .ok_or(Error::NoStream)?;
            template.drive(stream).await?;
            // Stream is unchanged — same bytes, new logical context
        }
    }
}
```

### Stream Ownership

Intermediate hops must stay alive while the final stream is in use. For
Transport hops, this means the SSH connection / Telnet connection object must
be held. The runtime should maintain a stack of transport layers:

```rust
struct EstablishedPath {
    /// Stack of transport layers, outermost first.
    /// The final entry's channel is the active stream.
    transport_stack: Vec<Box<dyn TransportLayer>>,
    /// The active stream for the final device
    stream: Box<dyn AsyncReadWrite + Send>,
}
```

When the `EstablishedPath` is dropped, transports are closed in reverse order.

## Integration with Existing Crate Structure

### Where Things Live

| Component | Crate |
|-----------|-------|
| Telnet protocol | `aytelnet` (unchanged) |
| SSH protocol | `ayssh` (unchanged) |
| TextFSM engine | `ayclic` (new module: `fsm`) |
| Transport enum + connect logic | `ayclic` (new module: `path`) |
| InteractiveTemplate | `ayclic` (uses `fsm` engine) |
| ConnectionPath + runtime | `ayclic` (new module: `path`) |
| CiscoIosConn | `ayclic` (updated to accept `EstablishedPath`) |

### Migration Path

1. **Phase 1**: Implement the TextFSM engine (`fsm` module) with Parse mode.
   Validate against ntc-templates.
2. **Phase 2**: Add Interactive extensions (Send, Preset, Done). Implement
   `InteractiveTemplate::drive()`.
3. **Phase 3**: Implement `ConnectionPath`, `Hop`, `Transport` types and the
   connection runtime.
4. **Phase 4**: Update `CiscoIosConn` to accept `ConnectionPath` or
   `EstablishedPath` as an alternative to direct connection parameters.
5. **Phase 5**: Write template libraries for common login/jump patterns.

Existing direct-connection API (`CiscoIosConn::new()` etc.) remains supported
as a convenience — it's equivalent to a single-hop ConnectionPath.

## Open Questions

1. **Timeout handling in Interactive mode**: Should timeouts be per-state,
   per-template, or per-hop? A stuck state machine needs to fail gracefully.
   Proposal: per-template default with optional per-state override.

2. **Stream vs. line matching in Interactive mode**: TextFSM is line-oriented.
   Interactive prompts often don't end with newlines. The engine needs to
   support matching against accumulated (non-line-delimited) stream data
   in Interactive mode while remaining line-oriented in Parse mode.

3. **SSH ProxyJump**: When two consecutive SSH hops exist and the SSH client
   supports ProxyJump natively, should the runtime optimize to use it?
   Or always decompose into explicit hops for consistency?
   Proposal: allow both — `Transport::SshWithProxy` as an optional optimized
   variant, but the explicit hop-by-hop path always works.

4. **Template discovery and loading**: Should templates be embedded in the
   binary, loaded from a directory, or fetched from a registry?
   Proposal: support all three — `include_str!` for built-in templates,
   filesystem for custom, optional registry for community templates.

5. **Credential management**: Preset values will contain secrets. Should the
   template system integrate with secret stores, or is that the caller's
   responsibility?
   Proposal: caller's responsibility. Templates receive populated Preset
   values; where those values come from is outside scope.

6. **Error recovery**: If an Interactive hop fails mid-way (wrong password,
   unexpected prompt), should the runtime attempt to back out (e.g., Ctrl-C,
   disconnect) or just fail and let the caller retry?
   Proposal: fail fast and let the caller retry with a fresh ConnectionPath.
   Error states in templates can provide diagnostic messages.

7. **Capture values across hops**: Should values captured in one Interactive
   hop be available to subsequent hops? Example: capture hostname in the
   login hop, use it in later command templates.
   Proposal: yes — maintain a value context across the ConnectionPath that
   accumulates captures from each hop.
