We're improving the mockios crate to make it indistinguishable from a real Cisco IOS device. Read these docs to get up to speed:

1. docs/plans/next-steps.md — immediate TODO items and current stats
2. docs/plans/device-state-model.md — the structured state model architecture
3. docs/plans/command-tree.md — the command tree dispatch system
4. docs/plans/cli-editing-keys.md — CLI editing key reference
5. docs/plans/real-ios-comparison.md — **NEW** detailed comparison findings from real IOS

## What was just completed this session

### Interface type tab completion (COMMITTED c446f8f)
- `interface ?` now shows proper-cased type keywords (GigabitEthernet, Loopback, Vlan, etc.)
- `interface Gi<TAB>` completes to `GigabitEthernet` with proper case
- Changed `find_matches()` in cmd_tree.rs to case-insensitive comparison (both sides lowered)
- Tab completion now erases typed prefix and replaces with canonical keyword form
- 200 tests passing (3 new)

### aytelnet warning fix (COMMITTED 1ceb20f in ../aytelnet)
- Removed unused `buffer_contains` and `buffer_ends_with` functions

## What was dispatched but NOT YET LANDED

Three Sonnet agents were dispatched for Batch 1 formatting fixes. They may have completed but their changes were NOT reviewed, tested, or committed. **Check git status first** — if the working tree is dirty, review the changes carefully before proceeding. If clean, re-dispatch these fixes:

### 1. show ip route — variably subnetted grouping
Real IOS groups routes under classful major network headers:
```
      10.0.0.0/8 is variably subnetted, 5 subnets, 2 masks
C        10.1.0.0/24 is directly connected, Vlan1
L        10.1.0.254/32 is directly connected, Vlan1
```
MockIOS currently lists routes flat. Fix is in `handle_show_ip_route()` in lib.rs (~line 628).

### 2. ? help header and column alignment
Real IOS shows "Exec commands:" header before `?` output at top level, "Configure commands:" in config mode. MockIOS has no header. Also fix help column padding from ~22 chars to ~17 chars to match real IOS.

### 3. show ip interface brief trailing space padding
Real IOS pads the Protocol column with trailing spaces. Fix format string in device_state.rs.

## What to work on next (priority order)

### Batch 1 remaining (if not landed above)
Complete and commit the three formatting fixes above.

### Batch 2: Content improvements
- **show running-config structure**: Add `version 15.2`, `service timestamps debug datetime msec`, `service timestamps log datetime msec`, `no service pad`, config change timestamp comments
- **Missing common exec commands**: `clear` (stub), `clock set`, `debug`/`undebug`/`no debug` (stubs), `help` (description of help system), `enable` visible in priv exec
- **show version detail**: Add `Compiled` line, `BOOTLDR:` line, `Last reload reason:` line

### Batch 3: Feature additions
- Interface `description` persisted to DeviceState (command exists but doesn't save)
- More show commands backed by state

## The workflow
1. Connect to real IOS devices (192.168.0.113 via telnet, user ayourtch/cisco123, enable 321cisco — this is a lab device; 192.168.0.130 is LIVE read-only) and observe behavior
2. Compare with mockios (build with `cargo build -p mockios --release`, run SSH server with `./target/release/mockios --ssh 127.0.0.1:2222`)
3. Write failing tests in mockios/src/lib.rs capturing real IOS behavior
4. Dispatch Sonnet agents (`Agent` tool with `model: sonnet`) to fix the mockios code — always use TDD
5. Verify all tests pass (`cargo test -p mockios`), commit, repeat

## Key architectural points
- mockios/src/cmd_tree.rs — command tree parser with abbreviation matching, ? help, tab completion. **Keywords now use proper case** and find_matches() lowercases both sides.
- mockios/src/cmd_tree_exec.rs — exec mode commands and handlers
- mockios/src/cmd_tree_conf.rs — config mode trees (separate trees per sub-mode: config-if, config-router, config-line). **Interface command now has keyword children for each type** instead of generic RestOfLine param.
- mockios/src/device_state.rs — structured DeviceState model (interfaces, routes, VLANs)
- mockios/src/lib.rs — MockIosDevice with send()/receive(), character echo, CLI editing, escape sequences. **Tab completion returns (erase_count, insert_text)** for proper-case replacement.
- Handlers are fn(&mut MockIosDevice, &str) — they read/write DeviceState, not static strings

## Important rules
- .130 is a LIVE network device — read-only, no changes
- .113 is lab — avoid GigabitEthernet1/0/13 and Vlan2
- Always delegate coding to Sonnet agents, review and commit yourself
- The mock device should always behave identically whether accessed via telnet or SSH — no mode flags
- All command output should be backed by the DeviceState model, not static strings
