We're improving the mockios crate to make it indistinguishable from a real Cisco IOS device. Read these docs to get up to speed:

1. docs/plans/next-steps.md — TODO items and current stats
2. docs/plans/device-state-model.md — the structured state model architecture
3. docs/plans/command-tree.md — the command tree dispatch system
4. docs/plans/real-ios-comparison.md — detailed comparison findings from real IOS
5. docs/plans/no-prefix-design.md — how `no` works as a prefix modifier
6. docs/plans/rest-of-line-audit.md — audit of stub handlers that need proper implementation

## Current state (240 tests, all passing)

### Architecture
- **cmd_tree.rs** — command tree parser. Keywords use proper case, `find_matches()` lowercases both sides. Keywords beat params when both match.
- **cmd_tree_exec.rs** — exec mode. Show commands: version, run, startup, ip int brief, ip route (grouped by major net), interfaces, vlan, clock, boot, history, terminal, cdp, users, logging, arp, mac, spanning-tree, ip ospf, ip protocols, processes cpu, access-lists, flash, install, ntp, snmp, privilege, line, inventory, environment. Exec commands: help, enable (noop in priv), debug/undebug, clock set, clear, ssh, telnet, ping, traceroute, copy, delete, verify, dir, reload, write, configure.
- **cmd_tree_conf.rs** — config mode with sub-mode trees (config-if, config-router, config-line). **`no` is a prefix modifier** — its children are a clone of the parent tree, so `no shut<TAB>` completes, `no ?` shows commands. Handlers check `input.starts_with("no")` for negation.
- **device_state.rs** — DeviceState model: interfaces (name, description, admin_up, ip_address, speed, duplex, mtu, switchport, mac, counters), static routes, VLANs, access lists (AccessList/AccessListEntry), flash files, hostname, version, banner_motd, enable_secret, etc.
- **lib.rs** — MockIosDevice with send()/receive(), character echo, CLI editing (Emacs keys, arrows, history), tab completion (erases prefix, writes canonical keyword).

### What was done this session
- Interface type tab completion with proper-cased keywords (GigabitEthernet, Loopback, Vlan...)
- Case-insensitive keyword matching (both sides lowered in find_matches)
- Tab completion replaces typed prefix with canonical form
- show ip interface brief trailing space padding
- show running-config now has version and service timestamps lines
- `no` refactored from dead-end RestOfLine to cloned command tree children
- `description` bug fixed (was writing to old running_config, now writes to InterfaceState)
- `enable secret/password` bug fixed (now writes to state.enable_secret)
- Handler negation: shutdown, ip address, ip route, hostname, description, banner motd, access-list
- ~20 new show commands and exec command stubs
- ACL data model with config and show access-lists
- Banner motd config + display on connect

## What to work on next (priority order)

### P0 — Remaining stub handlers that should write to DeviceState
From docs/plans/rest-of-line-audit.md:
1. **switchport mode/access vlan** — InterfaceState has switchport_mode/vlan fields but handler writes to running_config
2. **speed/duplex** — InterfaceState has fields but handler uses handle_config_sub_rest (unmodeled)
3. **vlan config** — should update VlanState for show vlan brief
4. **service timestamps** — model for show run accuracy
5. **username** — needed for auth simulation
6. **logging** — model for show logging accuracy

### P1 — Feature gaps
7. **show clock using real time** — currently hardcoded, should use system time or track clock set
8. **Remove old running_config Vec<String>** — superseded by DeviceState, but still referenced
9. **IPv6 support** — ipv6 address, show ipv6 interface brief, show ipv6 route
10. **Sub-submodes** (config-router-af etc.)
11. **--More-- pagination** for long output
12. **show interfaces status** (switch port table format)

### P2 — Comparison-driven fixes
Keep comparing with real IOS (.113 lab, .130 live read-only) and fixing differences.
See docs/plans/real-ios-comparison.md for detailed findings.

## The workflow
1. Connect to real IOS devices (192.168.0.113 via telnet, user ayourtch/cisco123, enable 321cisco — lab device; 192.168.0.130 is LIVE read-only) and observe behavior
2. Compare with mockios (`cargo build -p mockios --release`, SSH: `./target/release/mockios --ssh 127.0.0.1:2222`)
3. Write failing tests in mockios/src/lib.rs capturing real IOS behavior
4. Dispatch Sonnet agents (`Agent` tool with `model: sonnet`) to fix the mockios code — always use TDD
5. Verify all tests pass (`cargo test -p mockios`), commit, repeat

## Important rules
- .130 is a LIVE network device — read-only, no changes
- .113 is lab — avoid GigabitEthernet1/0/13 and Vlan2
- Always delegate coding to Sonnet agents, review and commit yourself
- The mock device should always behave identically whether accessed via telnet or SSH — no mode flags
- All command output should be backed by the DeviceState model, not static strings
- `no` prefix must use cloned tree children, not RestOfLine — never regress this
