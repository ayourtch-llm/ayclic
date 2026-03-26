# ayclic Workspace Context

Quick-start context for continuing development. Read this first.

## What This Is

A Rust workspace for template-driven, vendor-neutral network device
management. Connects to Cisco IOS/IOS-XE devices via SSH and Telnet,
executes commands, applies configuration atomically. Built to support
multi-hop connection paths (jump hosts, console servers) where all
device-specific behavior (login, prompts, enable) is driven by
TextFSMPlus templates, not hardcoded Rust.

## Workspace Layout

```
ayclic/              Cargo workspace root
├── ayclic/          Core library crate
│   ├── src/
│   │   ├── conn.rs           CiscoIosConn (Cisco-specific, wraps GenericCliConn)
│   │   ├── generic_conn.rs   GenericCliConn (vendor-neutral CLI connection)
│   │   ├── path.rs           ConnectionPath, Hop, TransportSpec, drive_interactive()
│   │   ├── raw_transport.rs  RawTransport trait, Telnet/SSH/Mock/Logging wrappers
│   │   ├── templates.rs      Built-in Cisco IOS login/prompt templates
│   │   ├── transport.rs      Legacy CiscoTransport trait (for old constructors)
│   │   └── error.rs          CiscoIosError
│   └── examples/
│       ├── cisco-cmd.rs       CLI tool: run commands on devices
│       ├── cisco-ios-config.rs  Two-phase config management (prepare/apply/push)
│       └── acl-cfg-test.rs    ACL stress test (uses legacy path)
├── aytextfsmplus/   Extended TextFSM engine (workspace member)
│   ├── src/lib.rs   Parser + interactive engine (Send/Preset/Done/feed/aycalc)
│   └── tests/       Parser tests, interactive tests, ntc-templates verification
├── mockios/         Mock Cisco IOS device (workspace member)
│   ├── src/lib.rs   MockIosDevice: implements RawTransport, simulates IOS CLI
│   ├── src/main.rs  Executable: stdin/stdout, telnet server, SSH server
│   └── tests/       Server integration tests, parameterized device tests
├── docs/
│   ├── plans/connection-path-architecture.md  Full architecture spec
│   ├── upstream-requests/    Requests for aytelnet, ayssh (BoxedStream etc.)
│   └── downstream-comms/    Responses to ayiosupdate observations
└── README.md
```

## External Dependencies (sibling dirs)

| Crate | Path | Purpose |
|-------|------|---------|
| aytelnet | `../../aytelnet` | Async Telnet client, RawTelnetSession |
| ayssh | `../../ayssh` | Async SSH client/server, RawSshSession |
| aycalc | `../../aycalc` | Expression evaluator (GetVar/CallFunc traits) |
| aycicdiff | `../../aycicdiff` | Cisco IOS config diff (used by cisco-ios-config example) |
| ntc-templates | `~/nms/ntc-templates` | For TextFSM compatibility verification |

## Architecture Stack

```
CiscoIosConn::new()           ← convenience, builds path internally
  → ConnectionPath::connect() ← multi-hop: Transport + Interactive hops
    → RawTransport            ← vendor-neutral send/receive/close
      → RawTelnetTransport    ← wraps aytelnet::RawTelnetSession
      → RawSshTransport       ← wraps ayssh::RawSshSession
    → drive_interactive()     ← runs TextFSMPlus template on stream
      → TextFSMPlus::feed()  ← step-at-a-time buffer matching
        → aycalc::eval_with() ← expression evaluation in Send actions
  → GenericCliConn            ← vendor-neutral, stores prompt template
    → run_cmd()               ← fresh TextFSMPlus per call
    → run_cmd_with_template() ← one-off template override
```

## Key Design Decisions

1. **Transport vs Interactive**: Transport opens a new byte stream (TCP+protocol).
   Interactive drives text-based interaction on an existing stream. SSH auth is
   Transport (protocol-level). Typing "ssh" in a shell is Interactive.

2. **TextFSMPlus**: Superset of Google TextFSM. Adds `Send`, `Preset`, `Done`.
   ntc-templates compatible (1790/1818 pass). `feed()` for stream matching,
   `parse_line()` for line-by-line parsing. `(?m)` multiline regex enabled.

3. **GenericCliConn stores prompt template**: `run_cmd()` creates a fresh
   TextFSMPlus engine from the stored template string each call. Override
   per-command via `run_cmd_with_template()`.

4. **CiscoIosConn dual path**: `new()` uses template-driven ConnectionPath.
   `new_legacy()` uses old CiscoTelnet/CiscoConn directly. Internally holds
   `CiscoIosConnInner` enum (Generic vs Legacy).

5. **LoggingTransport**: Wraps any RawTransport with `TranscriptSink` trait.
   SharedTranscript via `Arc<Mutex<>>` for reading while transport is owned.
   `with_file_logging()` for fire-and-forget audit.

6. **mockios**: In-process `RawTransport` impl. Simulates IOS CLI: login, enable,
   config mode, copy, reload, verify /md5, install commands. Line-buffered
   (correct for automation). Parameterized tests run against both mockios and
   real devices.

## Current Test Status

- **ayclic**: 138 tests (conn, generic_conn, path, raw_transport, transport, templates)
- **aytextfsmplus**: 77 tests (parser, interactive, DataRecord, feed, ntc-templates)
- **mockios**: 32 unit tests + 10 parameterized device tests + 8 server integration
- **ntc-templates**: 1790/1818 compatibility (28 failures = stale upstream YAML)
- **Real device verified**: 192.168.0.130 (IOS 12.2) and 192.168.0.113 (IOS 15.2),
  both SSH and Telnet

## Known Issues

1. **mockios telnet server**: Uses raw TCP, not aytelnet protocol. Telnet clients
   send negotiation bytes that confuse the mock. Tracked in server_integration.rs
   (7 tests ignored).

2. **`run_cmd` output includes prompt**: `run_cmd` returns `buffer[..consumed]`
   which includes the prompt text. This matches legacy behavior but callers should
   be aware.

3. **`^.*#` prompt matching**: With `(?m)`, `^.*#` matches any line containing `#`.
   The Cisco IOS prompt template works for most output but may match URL fragments
   in rare cases. Hostname-specific prompts (`^Router1#`) are more precise.

4. **mockios stale prompt after enable flow**: After `drive_interactive` completes
   the enable login template, one extra prompt may be queued. Fixed by removing
   eager input_buffer processing (commit a74ad5d). The workaround of consuming
   with `term len 0` is still in test_enable_mode but shouldn't be needed.

## Real Device Credentials

- **192.168.0.130**: user=ayourtch pass=cisco123 (IOS 12.2, C3560C)
- **192.168.0.113**: user=ayourtch pass=cisco123 (IOS 15.2, C3560CX)
  - Has `aaa authorization exec default local` configured
  - Has `enable secret 321cisco` configured
  - Critical interfaces: GigabitEthernet1/0/13 and Vlan2 (DO NOT MODIFY)

## Downstream Consumer

**ayiosupdate** (`../ayiosupdate/`) — Cisco IOS/IOS-XE upgrade automation tool.
Uses ayclic + mockios for testing upgrade workflows. Observations and responses
tracked in `docs/downstream-comms/`.

## What's Next (Future Work)

- **Phase 5 templates**: More vendor login templates (Juniper, Arista, etc.)
- **Connection string DSL**: `compile_path("cisco-ssh{10.1.1.1, user=admin}")`
- **Protocol stacking**: `BoxedStream` / `over_stream()` in aytelnet/ayssh
  (request docs ready in `docs/upstream-requests/`)
- **mockios stack simulation**: `with_stack_member()` for `show switch` (#2)
- **Connection pool**: `connect_over()` + `into_transport()` pattern
- **`CiscoIosError` cleanup**: Vendor-neutral error type for path module
