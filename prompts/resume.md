We're improving the mockios crate to make it indistinguishable from a real Cisco IOS device. Read these docs to get up to speed:

1. docs/plans/next-steps.md — TODO items and current stats
2. docs/plans/device-state-model.md — the structured state model architecture
3. docs/plans/command-tree.md — the command tree dispatch system
4. docs/plans/real-ios-comparison.md — detailed comparison findings from real IOS
5. docs/plans/no-prefix-design.md — how `no` works as a prefix modifier
6. docs/plans/rest-of-line-audit.md — audit of stub handlers that need proper implementation
7. docs/plans/mockios-realism-batch1.md — live comparison with WS-C3560CX-12PD-S

## Current state (288 tests, all passing)

### Architecture
- **cmd_tree.rs** — command tree parser. Keywords use proper case, `find_matches()` lowercases both sides. Keywords beat params when both match. `?` help output is sorted alphabetically.
- **cmd_tree_exec.rs** — exec mode. 50+ show commands including: version, run, startup, ip int brief, ip route (grouped, indented codes), interfaces (+ status subcommand), vlan, clock (real time), boot, history, terminal, cdp, users, logging, arp (self-entries), mac, spanning-tree (per-VLAN blocks), ip ospf, ip protocols, processes cpu, access-lists, flash, install, ntp, snmp, privilege, line, inventory, environment, aaa, authentication, crypto, debugging, dhcp, dot1x, errdisable, etherchannel, hosts, license, lldp, module, platform, policy-map, port-security, power, protocols, sessions, ssh, standby, storm-control, switch, vtp. Exec commands: help, enable, debug/undebug, clock set, clear, ssh, telnet, ping, traceroute, copy, delete, verify, dir, reload, write, configure.
- **cmd_tree_conf.rs** — config mode with sub-mode trees (config-if, config-router, config-line). **`no` is a prefix modifier** — its children are a clone of the parent tree. Interface command accepts both `interface GigabitEthernet 1/0/1` and `interface GigabitEthernet1/0/1` (concatenated form).
- **device_state.rs** — DeviceState model: WS-C3560CX-12PD-S defaults with Vlan1 + Gi1/0/1..16 + Te1/0/1..2, default VLANs 1-5 + 1002-1005, base_mac, sw_image, spanning_tree_mode, vtp_mode, aaa_new_model, ip_routing, service_password_encryption. Helpers: abbreviate_interface_name(), short_interface_name(), mac_to_cisco_format(). Methods: generate_running_config(), generate_show_vlan_brief() (with port wrapping), generate_show_interfaces_status(), generate_show_spanning_tree(), generate_show_arp().
- **lib.rs** — MockIosDevice with send()/receive(), character echo, CLI editing (Emacs keys, arrows, history), tab completion. show version (~60 lines matching real device), show ip interface brief (correct abbreviation, method unset/NVRAM, admin down).

### What was done this session (2026-03-28)
- Switch-correct interfaces: Vlan1 + Gi1/0/1..16 + Te1/0/1..2 (replacing router-style Gi0/X)
- Default VLANs 1002-1005 with act/unsup status
- Show version overhaul (~60 lines matching real WS-C3560CX output)
- Interface name abbreviation only when >= 23 chars (real IOS behavior)
- Method unset/NVRAM in show ip interface brief
- VLAN port list wrapping at 52 chars with 48-col indent
- Removed spurious .SPA from system image filename
- Default shutdown state for unconnected ports (Gi1/0/5-16, Te1/0/1-2)
- Sanitized serials/MACs to plausible but fictional values
- New show interfaces status command
- Running-config enrichment (no service pad, aaa auth, switch provision, system mtu, lldp, ip http/ssh)
- Spanning-tree per-VLAN blocks with priority calculation and interface table
- Fixed interface command to accept concatenated type+number (GigabitEthernet1/0/1)
- IP route codes header 7-space indentation
- Show arp self-entries with per-interface MAC
- Show interfaces status column alignment fix
- Show clock using real system time
- 26 new stub show commands (50+ total)
- Alphabetical sorting of ? help output
- 10 commits, tests 242 → 288

## What to work on next (priority order)

### P0 — Remaining stub handlers that should write to DeviceState
1. **switchport mode/access vlan** — InterfaceState has fields but handler stubs
2. **speed/duplex** — InterfaceState has fields but handler stubs
3. **vlan config** — should update VlanState for show vlan brief
4. **username** — needed for auth simulation
5. **logging** — model for show logging accuracy

### P1 — Feature gaps
6. **Remove old running_config Vec<String>** — superseded by DeviceState, but still referenced
7. **IPv6 support** — ipv6 address, show ipv6 interface brief, show ipv6 route
8. **Sub-submodes** (config-router-af etc.)
9. **--More-- pagination** for long output
10. **show mac address-table** — needs real format with VLAN/MAC/Type/Ports
11. **More config-if commands** — real IOS has ~60, mockios has ~10
12. **More config mode commands** — real IOS has ~180, mockios has fewer

### P2 — Polish
13. **show inventory spacing** — minor differences vs real device
14. **Compiler warnings** — fix 4 warnings from build
15. **show ip interface brief leading blank line** — verify if there's an extra \n

## The workflow
1. Connect to real IOS devices (192.168.0.113 via SSH, user ayourtch/cisco123 — lab device; .130 is LIVE read-only) and observe behavior
2. Compare with mockios (`cargo build -p mockios --release`, SSH: `./target/release/mockios --ssh 127.0.0.1:2222`)
3. Write failing tests in mockios/src/lib.rs capturing real IOS behavior
4. Dispatch Sonnet agents (`Agent` tool with `model: sonnet`) to fix the mockios code — always use TDD
5. Verify all tests pass (`cargo test -p mockios`), commit, repeat

## Important rules
- .130 is a LIVE network device — read-only, no changes
- .113 is lab — avoid GigabitEthernet1/0/13 and Vlan2
- Serial numbers and MACs must be fictional but plausible (never commit real device identifiers)
- Always delegate coding to Sonnet agents, review and commit yourself
- The mock device should always behave identically whether accessed via telnet or SSH — no mode flags
- All command output should be backed by the DeviceState model, not static strings
- `no` prefix must use cloned tree children, not RestOfLine — never regress this
