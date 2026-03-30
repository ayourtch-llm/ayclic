# MockIOS Autonomous Improvement Cycle

You are continuing an autonomous improvement cycle to make the `mockios` crate (a mock Cisco IOS CLI simulator) indistinguishable from real Cisco IOS 15.2 on a WS-C3560CX-12PD-S switch.

## What to do

1. **Connect to the real IOS device** at 192.168.0.112 via telnet (login: ayourtch, pass: cisco123, `terminal length 0`). This is a lab device, feel free to run any show commands. There's also 192.168.0.130 (production, READ-ONLY, same credentials) for cross-platform comparison (IOS 12.2).

2. **Launch a fresh mockios** instance (`cargo run -p mockios -- --hostname MockRouter`) side-by-side.

3. **Compare outputs** command by command. Run the same command on both, diff the output, identify gaps.

4. **Fix gaps using TDD with Sonnet agents**: For each fix, dispatch a background Sonnet agent (`Agent tool, model="sonnet", run_in_background=true`) that:
   - Reads the relevant source files
   - Writes a failing test first
   - Implements the fix
   - Runs `cargo test --workspace`
   - Commits with a descriptive message + `Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>`

5. **Dispatch 2-3 agents in parallel** on independent files. Don't wait for one to finish before launching the next. Key files and their responsibilities:
   - `mockios/src/device_state.rs` — data model, show output generators (generate_show_*)
   - `mockios/src/cmd_tree_exec.rs` — exec mode handlers and command tree
   - `mockios/src/cmd_tree_conf.rs` — config mode handlers and command tree
   - `mockios/src/lib.rs` — MockIosDevice core logic, dispatch, pipe filtering
   - `mockios/src/cmd_tree.rs` — command tree parser, help system

6. **Commit frequently** — as soon as tests pass, commit. Don't batch multiple fixes.

7. **Keep a keepalive cron** (`mcp__tttt__tttt_cron_create`, every 15 minutes) to nudge yourself to continue.

## Architecture principles learned

- **Dynamic over static**: Compute display values from device state (e.g., VLAN port membership is computed from interface switchport_mode, not a static list)
- **`no_handler` for asymmetric commands**: Some commands require arguments in positive form but not in negated form (e.g., `hostname <name>` vs `no hostname`). Use the `no_handler` field on CommandNode.
- **`no` at dispatch level**: The `no` prefix is handled in `dispatch_config()` via `parse_for_no()`, NOT by cloning the command tree. Don't add `no` nodes to the tree.
- **`short_interface_name()`** always abbreviates (Gi1/0/1), **`abbreviate_interface_name()`** only abbreviates if > 23 chars
- **`wrap_comma_list()`** for port list wrapping with configurable width
- **Pipe filtering**: Already implemented in lib.rs (`PipeFilter` enum, `apply_pipe_filter`)

## Current state (session ended with)

- **51 commits, 499 mockios tests, 750 total workspace tests, all passing**
- Tests started at 335, so +164 new tests were added
- 8 reference docs in `docs/cisco-docs/` (exec commands, show commands, config commands, CLI behavior, running-config format, interfaces, routes, interface types)
- Gap analysis in `docs/cisco-docs/gap-analysis.md` and show-run diff in `docs/cisco-docs/show-run-diff-analysis.md`
- Convergence plan in `docs/plans/mockios-convergence-plan.md`
- No-command refactor spec in `docs/specs/no-command-refactor.md`

## What's done (don't redo these)

### Phase 1 — Critical fixes (COMPLETE)
- Route ordering in `show ip route` (default before connected)
- show inventory format (NAME+PID consecutive)
- Error message blank lines
- show vlan brief port wrapping (31-char width)
- Dynamic VLAN port membership (trunk ports excluded)
- Virtual interface link_up defaults (Vlan/Loopback always up)
- Removed end/quit from exec mode
- Te port defaults (admin-up, notconnect)
- Interface status states (notconnect/connected/disabled)
- `<cr>` glitch fixed via no_handler

### Phase 2 — Command completeness (MOSTLY COMPLETE)
- 21 exec command stubs, 13+ show ip subcommands, 13+ config mode commands
- show interfaces (detail/status/trunk/description/switchport/counters)
- show vlan / show vlan id, show ip arp/interface/route filtering
- show cdp/lldp (neighbors/detail), show spanning-tree (vlan/summary)
- show etherchannel summary, show port-security, show power inline
- show storm-control, show vtp status, show errdisable recovery
- show ip ssh, show ssh, show ip ospf neighbor
- write terminal/erase/network, terminal monitor
- show running-config interface <name>

### Phase 3 — Running config realism (MOSTLY COMPLETE)
- Line section (privilege level, exec-timeout, transport input)
- Only non-default switchport settings shown
- Physical interfaces before SVIs in ordering
- ip route after ip http/ssh
- ip forward-protocol nd
- Dynamic byte count

### Phase 4 — Behavioral fidelity (MOSTLY COMPLETE)
- Pipe filtering (include/exclude/begin/section/count)
- `do` command in config mode
- show interfaces detail with proper counters/DLY/hardware types
- Dynamic uptime in show version
- `no` handled at dispatch level (no tree cloning)

## What still needs work

### High priority
- **More config mode commands**: The `show running-config` still has many sections simpler than real IOS (interface-level: switchport nonegotiate, load-interval, udld, spanning-tree portfast; also logging, snmp-server, ntp, event-manager)
- **show interfaces <name> switchport**: Per-interface switchport view (just added but verify vs real)
- **Config mode help text**: Compare `conf t` then `?` output with real IOS, add missing commands
- **show ip interface brief**: Verify column widths match exactly per-character

### Medium priority
- **show running-config | section**: Pipe `section` mode should show entire config blocks between `!` delimiters
- **--More-- paging**: `terminal length` should trigger paging
- **Tab completion accuracy**: Verify tab completion matches real IOS prefix matching
- **show flash:**: Compare directory listing format with real device
- **show processes cpu**: Add realistic output
- **Config persistence**: `write memory` / `copy run start` should update startup-config

### Lower priority
- **ACL configuration**: Support `ip access-list extended` in config mode
- **SNMP configuration**: Support snmp-server commands
- **NTP configuration**: Support ntp server commands in config, reflect in show ntp
- **Spanning-tree per-interface config**: spanning-tree portfast, bpduguard, etc.
- **DHCP snooping per-interface**: ip dhcp snooping trust
- **Port-channel / EtherChannel**: Support channel-group config
- **VTY line groups**: Support different settings for vty 0-4, 5-10, 11-15

## How to verify progress

After each batch of fixes, restart the mockios and compare key outputs side-by-side with the real device:
```
show version
show running-config
show ip interface brief
show interfaces status
show vlan brief
show ip route
show ?
configure terminal → ?
```

Focus on making these outputs pixel-identical to real IOS.
