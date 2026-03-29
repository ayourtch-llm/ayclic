# MockIOS Convergence Plan

## Goal
Make mockios CLI output indistinguishable from a real Cisco IOS 15.2 device
(WS-C3560CX-12PD-S) in all observable behaviors.

## Reference Devices
- **SEED-001-S0244**: WS-C3560CX-12PD-S, IOS 15.2(7)E13 @ 192.168.0.112 (lab device, full access)
- **AY-LIVING**: WS-C3560CG-8TC-S, IOS 12.2(55)EX2 @ 192.168.0.130 (production, read-only)

## Phase 1: Critical Fixes (DONE/IN PROGRESS)
- [x] Fix show ip route ordering (default before connected groups)
- [x] Fix show inventory format (no blank between NAME/PID)
- [x] Add blank line after error messages
- [x] Fix show vlan brief port wrapping (31-char width)
- [x] Dynamic VLAN port membership (exclude trunk ports)
- [x] Virtual interface link_up defaults (Loopback/Vlan always up)
- [x] Remove end/quit from exec mode
- [x] Short interface names (Gi/Te) for VLAN contexts
- [ ] Fix interface status states (notconnect/connected/disabled)
- [ ] Fix Te port default speed/duplex in show interfaces status

## Phase 2: Exec Command Completeness
- [ ] Add ~21 missing exec command stubs (access-enable, archive, cd, etc.)
- [ ] Fix write help text to match real IOS
- [ ] Add show subcommand stubs for real IOS coverage

## Phase 3: Running Config Realism
- [ ] service timestamps debug/log datetime msec
- [ ] no ip source-route
- [ ] system mtu routing 1500
- [ ] lldp run
- [ ] Interface config: switchport nonegotiate, load-interval, spanning-tree portfast
- [ ] ACL configuration support
- [ ] Line config: exec-timeout, privilege level, transport input
- [ ] NTP configuration

## Phase 4: Behavioral Fidelity
- [ ] Fix command echo bug (echoed twice in interactive mode)
- [ ] Support | pipe filtering (include, exclude, section, begin)
- [ ] Support --More-- paging
- [ ] Dynamic Virtual Ethernet count in show version
- [ ] Accurate config byte count
- [ ] show interfaces full detail format matching

## Phase 5: Data Model for Forwarding
- [ ] Separate admin_status / oper_status / link_status cleanly
- [ ] FIB/RIB model for route lookup
- [ ] ARP table with aging
- [ ] MAC address table with aging
- [ ] STP state machine

## Architecture Principles
1. **Dynamic over static**: Compute display values from state (like VLAN ports)
2. **TDD**: Write failing test first, then implement
3. **Commit early**: Commit as soon as tests pass
4. **Elegant code**: Extract reusable utilities (wrap_comma_list, short_interface_name)
5. **Sonnet agents for implementation**: Opus orchestrates, Sonnet implements
