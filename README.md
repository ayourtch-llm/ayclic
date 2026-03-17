# ayclic

A Rust workspace for template-driven, vendor-neutral network device management.

## Overview

`ayclic` provides a multi-hop, template-driven connection framework for
interacting with network devices (routers, switches, firewalls) over
Telnet and SSH. Device-specific behavior (login, prompts, enable mode)
is driven by TextFSMPlus templates rather than hardcoded logic, making
it straightforward to support any device with a text-based CLI.

## Workspace Crates

| Crate | Description |
|-------|-------------|
| **ayclic** | Core library: connection paths, generic CLI connection, Cisco IOS support, transport abstractions, audit logging |
| **aytextfsmplus** | Extended TextFSM engine: parses command output (ntc-templates compatible) and drives interactive sessions (Send/Preset/Done) with aycalc expression evaluation |

### External Dependencies

| Crate | Description |
|-------|-------------|
| [aytelnet](https://github.com/ayourtch/aytelnet) | Async Telnet client (protocol + vendor-neutral `RawTelnetSession`) |
| [ayssh](https://github.com/ayourtch/ayssh) | Async SSH client/server (protocol + vendor-neutral `RawSshSession`) |
| [aycalc](https://github.com/ayourtch/aycalc) | Embeddable expression evaluator (variables + custom functions) |
| [aycicdiff](https://github.com/ayourtch/aycicdiff) | Cisco IOS config diff engine (used by `cisco-ios-config` example) |

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Application Layer                     │
│  CiscoIosConn │ GenericCliConn │ cisco-ios-config tool   │
├─────────────────────────────────────────────────────────┤
│                  Connection Path Layer                   │
│  ConnectionPath → Hop(Transport | Interactive)           │
│  drive_interactive() │ feed() step-at-a-time API         │
├─────────────────────────────────────────────────────────┤
│               Template Engine (aytextfsmplus)            │
│  Parse mode (ntc-templates) │ Interactive mode (Send)    │
│  Preset values │ aycalc expressions │ Done/Error states  │
├─────────────────────────────────────────────────────────┤
│                   Transport Layer                        │
│  RawTransport trait │ LoggingTransport │ MockTransport   │
│  RawTelnetTransport │ RawSshTransport                    │
├─────────────────────────────────────────────────────────┤
│                  Protocol Libraries                      │
│  aytelnet::RawTelnetSession │ ayssh::RawSshSession       │
└─────────────────────────────────────────────────────────┘
```

## Key Concepts

### Transport vs Interactive

- **Transport** hops open a new byte stream (TCP + protocol handshake):
  Telnet or SSH with protocol-level authentication.
- **Interactive** hops drive text-based interaction on the current stream
  using TextFSMPlus templates: login, enable mode, typing commands in
  a shell, navigating menus.

### Connection Paths

A `ConnectionPath` is an ordered list of hops describing how to reach a
device. Multi-hop scenarios (bastion hosts, console servers) are modeled
naturally:

```rust
let path = ConnectionPath::new(vec![
    // Hop 1: SSH to bastion (protocol-level auth)
    Hop::Transport(TransportSpec::Ssh {
        target: "bastion:22".parse()?,
        auth: SshAuth::PubKey { username: "ops".into(), private_key: key },
    }),
    // Hop 2: Interactive — type "ssh" in bastion shell
    Hop::Interactive(
        TextFSMPlus::from_str(JUMP_TEMPLATE)
            .with_preset("TargetHost", "10.200.0.5")
            .with_preset("Password", "hunter2")
    ),
    // Hop 3: Interactive — Cisco IOS login
    Hop::Interactive(
        TextFSMPlus::from_str(CISCO_IOS_TELNET_LOGIN)
            .with_preset("Username", "admin")
            .with_preset("Password", "secret")
    ),
]);

let mut conn = GenericCliConn::connect(path, &NoVars, &NoFuncs).await?;
```

### TextFSMPlus Templates

The engine is a superset of Google's TextFSM with three extensions for
interactive session driving:

- **`Send`**: Send text to the stream (supports aycalc expressions)
- **`Preset`**: Values populated before the engine runs
- **`Done`**: Terminal state signaling successful completion

Standard TextFSM templates (e.g., from ntc-templates) work unmodified
for parsing command output. The same engine drives both parsing and
interactive sessions.

### Session Audit Logging

`LoggingTransport` wraps any transport and records all sent/received data.
The `TranscriptSink` trait controls where data goes:

```rust
// In-memory (readable via shared handle):
let transcript = new_transcript();
let transport = LoggingTransport::new(inner, transcript.clone());

// Fire-and-forget file logging:
let transport = with_file_logging(inner, "/var/log/session.log")?;

// Custom sink (syslog, channel, etc.):
impl TranscriptSink for MySink { ... }
```

## Examples

### cisco-cmd

Execute commands on a Cisco IOS device:

```bash
cargo run --example cisco-cmd -- --ssh 10.1.1.1 admin password "show version"
cargo run --example cisco-cmd -- --telnet 10.1.1.1 admin password "show ip route;show interfaces"
```

### cisco-ios-config

Two-phase config management with atomic apply:

```bash
# Prepare: fetch running config, compute delta
cargo run --example cisco-ios-config -- prepare \
  -d 10.1.1.1 -u admin -p secret \
  -t desired-config.cfg -o delta.cfg

# Review delta.cfg, then apply with safety reload
cargo run --example cisco-ios-config -- apply \
  -d 10.1.1.1 -u admin -p secret \
  --delta delta.cfg --safety-reload 5

# Or one-shot: prepare + apply
cargo run --example cisco-ios-config -- push \
  -d 10.1.1.1 -u admin -p secret \
  -t desired-config.cfg --safety-reload 5
```

The apply phase uses `config_atomic()` which:
1. Optionally schedules a safety reload (auto-reverts bad config)
2. Uploads config via HTTP to device flash
3. Verifies MD5 integrity
4. Applies with `copy flash: running-config`
5. Cancels safety reload on success

## Testing

```bash
# Run all workspace tests
cargo test --workspace

# Run ntc-templates compatibility check (requires ntc-templates checkout)
cargo run --release -p aytextfsmplus --example verify-ntctemplates -- /path/to/ntc-templates
```

The TextFSMPlus engine passes 1790/1818 ntc-templates tests (98.5%).
The 28 remaining failures are stale test data in the ntc-templates
repository, not parser bugs.

## Design Documentation

The full architecture spec with design decisions, API documentation,
and future considerations is in
[docs/plans/connection-path-architecture.md](docs/plans/connection-path-architecture.md).

Upstream API requests for protocol stacking support (BoxedStream) are in
[docs/upstream-requests/](docs/upstream-requests/).

## License

MIT
