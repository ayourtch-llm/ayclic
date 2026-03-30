We're improving the mockios crate to make it indistinguishable from a real Cisco IOS device. Read these docs to get up to speed:

1. docs/plans/next-steps.md — TODO items and current stats
2. docs/plans/device-state-model.md — the structured state model architecture
3. docs/plans/command-tree.md — the command tree dispatch system
4. docs/plans/real-ios-comparison.md — detailed comparison findings from real IOS
5. docs/plans/no-prefix-design.md — how `no` works as a prefix modifier
6. docs/plans/rest-of-line-audit.md — audit of stub handlers that need proper implementation
7. docs/plans/mockios-realism-batch1.md — live comparison with WS-C3560CX-12PD-S

## Current state (568 tests, all passing)

### Architecture
- **cmd_tree.rs** — command tree parser. Keywords use proper case, `find_matches()` lowercases both sides. Keywords beat params when both match. `?` help output is sorted alphabetically. `ParamType::NumberRange(min, max)` for range-validated numeric params.
- **cmd_tree_exec.rs** — exec mode. 50+ show commands including: version, run, startup, ip int brief, ip route (grouped, indented codes), interfaces (+ status subcommand), vlan, clock (real time), boot, history, terminal, cdp, users, logging (dynamic from state), arp (self-entries), mac (real format with VLAN/MAC/Type/Ports), spanning-tree (per-VLAN blocks), ip ospf, ip protocols, processes cpu, access-lists, flash, install, ntp, snmp, privilege, line, inventory, environment, aaa, authentication, crypto, debugging, dhcp, dot1x, errdisable, etherchannel, hosts, license, lldp, module, platform, policy-map, port-security, power, protocols, sessions, ssh, standby, storm-control, switch, vtp. Exec commands: help, enable, debug/undebug, clock set, clear, ssh, telnet, ping, traceroute, copy, delete, verify, dir, reload, write, configure.
- **cmd_tree_conf.rs** — config mode with sub-mode trees (config-if, config-router, config-line, config-ext-nacl, config-std-nacl, config-vlan). **`no` is a prefix modifier** — its children are a clone of the parent tree. Interface command accepts abbreviated forms: `g1/0/9` → `GigabitEthernet1/0/9`, `te1/0/1` → `TenGigabitEthernet1/0/1`, plus both space-separated and concatenated forms.
- **device_state.rs** — DeviceState model: WS-C3560CX-12PD-S defaults with Vlan1 + Gi1/0/1..16 + Te1/0/1..2, default VLANs 1-5 + 1002-1005, base_mac, sw_image, spanning_tree_mode, vtp_mode, aaa_new_model, ip_routing, service_password_encryption. UserAccount model. Logging state (buffered_size, console, monitor, hosts). Helpers: abbreviate_interface_name(), short_interface_name(), mac_to_cisco_format(). Methods: generate_running_config(), generate_show_vlan_brief() (with port wrapping), generate_show_interfaces_status(), generate_show_spanning_tree(), generate_show_arp().
- **lib.rs** — MockIosDevice with send()/receive(), character echo, CLI editing (Emacs keys, arrows, history), tab completion, pipe filtering (`| include`, `| exclude`, `| begin`, `| section`, `| count`). show version (~60 lines matching real device), show ip interface brief (correct abbreviation, method unset/NVRAM, admin down).

### What was done this session (2026-03-30)
- **Fixed `access-list ?`**: Now shows proper number ranges (`<1-99>`, `<100-199>`, etc.) instead of `<rest>`. Added `ParamType::NumberRange(min, max)` to cmd_tree.rs. 4 new tests.
- **Added `ip access-list extended/standard <name>`**: Enters config-ext-nacl / config-std-nacl sub-mode. Permit/deny/remark entries stored in AccessList model. Named ACLs render in block format in show running-config. `no ip access-list` removes. 14 new tests.
- **Fixed speed/duplex stubs**: Now write to InterfaceState. `speed 100` / `duplex full` reflected in `show interfaces`. Tree has proper keyword children (10/100/1000/auto, auto/full/half). 10 new tests.
- **Added vlan config sub-mode**: `vlan 100` enters config-vlan, creates VlanState. `name <word>` sets VLAN name. `no vlan` removes. Shows in `show vlan brief`. 6 new tests.
- **Added username state handler**: Parses `username <name> [privilege <n>] secret/password <pw>`. UserAccount model in DeviceState. Shows in running-config. 4 new tests.
- **Added logging config state**: `logging buffered/console/monitor/host` write to DeviceState. `show logging` reflects dynamic state. 12 new tests.
- **Fixed pipe filter CR/LF bug**: Filtered output was bypassing `queue_output()`, causing concatenated lines in telnet. 1 new test.
- **Fixed `?` help column width**: Narrowed from 17 to 15 chars to match real IOS.
- 7 commits, tests 517 → 568

### What was done previous session (2026-03-28, session 2)
- Fixed interface name abbreviation bug, removed extra blank lines
- 3 commits, tests 293 → 302

## What to work on next (priority order)

### P0 — All original P0 items are DONE

### P1 — Feature gaps
1. **Remove old running_config Vec<String>** — superseded by DeviceState, still 112 references (big refactor)
2. **IPv6 support** — ipv6 address, show ipv6 interface brief, show ipv6 route
3. **Sub-submodes** (config-router-af etc.)
4. **--More-- pagination** for long output
5. **More config-if commands** — real IOS has ~60, mockios has ~25
6. **More config mode commands** — real IOS has ~180, mockios has fewer
7. **Parameter value completion** — `int <TAB>` should list interfaces from device state

### P2 — Polish
8. **show inventory spacing** — minor differences vs real device
9. **Dynamic help column width** — currently fixed at 15 chars
10. **show ip interface brief trailing space padding** — real IOS pads Status/Protocol columns

### P3 — Cleanup
11. **Remove old running_config Vec<String> references** from handlers that still push to it

## The workflow
1. Connect to real IOS devices (192.168.0.113 via SSH, user ayourtch/cisco123 — lab device; .130 is LIVE read-only) and observe behavior
2. Compare with mockios (`cargo build -p mockios --release`, telnet: `./target/release/mockios --telnet 127.0.0.1:2323 --hostname Switch --login admin:cisco --enable cisco123`)
3. Write failing tests in mockios/src/lib.rs capturing real IOS behavior
4. Dispatch Sonnet agents (`Agent` tool with `model: sonnet`) to fix the mockios code — always use TDD
5. Verify all tests pass (`cargo test -p mockios --lib`), review diff, commit with small focused commits
6. Rebuild and verify live on terminals, repeat

## Important rules
- .130 is a LIVE network device — read-only, no changes
- .113 is lab — avoid GigabitEthernet1/0/13 and Vlan2
- Serial numbers and MACs must be fictional but plausible (never commit real device identifiers)
- Always delegate coding to Sonnet agents, review and commit yourself (Opus orchestrates)
- The mock device should always behave identically whether accessed via telnet or SSH — no mode flags
- All command output should be backed by the DeviceState model, not static strings
- `no` prefix must use cloned tree children, not RestOfLine — never regress this
- Small, focused commits after each unit of work — don't pile up changes
- Use tttt terminals for live comparison: real-ios (SSH to 192.168.0.113) and mock-client (telnet to 127.0.0.1:2323)
