# MockIOS vs Real IOS Gap Analysis

Captured 2026-03-30 from:
- **SEED-001-S0244**: WS-C3560CX-12PD-S, IOS 15.2(7)E13 (192.168.0.112)
- **AY-LIVING**: WS-C3560CG-8TC-S, IOS 12.2(55)EX2 (192.168.0.130) - PRODUCTION, read-only

## Critical Gaps

### 1. Command Echo Bug
MockIOS echoes commands twice in interactive mode (e.g., `show ip interface brief` appears twice).
Real IOS only shows the command once after the prompt.

### 2. Exec Mode Command Count
- **Real IOS**: ~90+ privileged exec commands
- **MockIOS**: ~25 commands
- Missing critical commands: `access-enable`, `archive`, `calendar`, `cd`, `connect`, `crypto`, `disconnect`, `do-exec`, `erase`, `format`, `ip`, `license`, `lock`, `logging`, `login`, `macro`, `mkdir`, `monitor`, `more`, `name-connection`, `ntp`, `pwd`, `release`, `remote`, `rename`, `renew`, `resume`, `rmdir`, `rsh`, `send`, `session`, `set`, `setup`, `software`, `switch`, `systat`, `tclsh`, `test`, `tunnel`, `udld`, `vmps`, `vtp`, `where`, `which-route`

### 3. Show Subcommand Count
- **Real IOS**: ~220+ `show` subcommands
- **MockIOS**: ~45 `show` subcommands
- Most gaps are stub-able (just need help text + "not implemented" or minimal output)

### 4. Show IP Route Ordering
Real IOS output:
```
Gateway of last resort is 192.168.0.1 to network 0.0.0.0

S*    0.0.0.0/0 [254/0] via 192.168.0.1
      10.0.0.0/8 is variably subnetted, 5 subnets, 2 masks
C        10.1.0.0/24 is directly connected, Vlan1
L        10.1.0.254/32 is directly connected, Vlan1
```

MockIOS output:
```
Gateway of last resort is 10.0.0.254 to network 0.0.0.0

      10.0.0.0/8 is variably subnetted, 2 subnets, 2 masks
C        10.0.0.0/24 is directly connected, Vlan1
L        10.0.0.1/32 is directly connected, Vlan1
S*       0.0.0.0/0 [1/0] via 10.0.0.254
```

**Issues:**
- Route ordering wrong: Real IOS shows default route FIRST, then connected routes grouped by major network
- Default static route admin distance: real shows `[254/0]`, mockios shows `[1/0]`. Static default should be AD 1, but the real device shows 254 (likely configured that way). Need to verify default AD handling.
- The `S*` line appears before the connected route groupings in real IOS

### 5. Show Interfaces Status - Te Port Details
Real IOS:
```
Te1/0/1   DISABLED-PORT      notconnect   1            full    10G Not Present
```
MockIOS:
```
Te1/0/1                      disabled     1            auto   auto Not Present
```
**Issues:**
- Te ports show `full` duplex and `10G` speed in real IOS (not `auto`)
- Real IOS shows `notconnect` status for non-shutdown ports with no link, vs `disabled` for shutdown ports. MockIOS shows `disabled` for both.

### 6. Interface Status States
Real IOS interface status values:
- `connected` - link up
- `notconnect` - admin up but no link
- `disabled` - admin shutdown
- `err-disabled` - error disabled

MockIOS only distinguishes `disabled` (shutdown) and others.

### 7. Show Running-Config Missing Features
Real IOS running-config has many sections mockios doesn't support:
- `no service pad`
- `service unsupported-transceiver` (present in mockios ✓)
- `service timestamps debug datetime msec` / `service timestamps log datetime msec`
- `no ip source-route`
- `system mtu routing 1500`
- `lldp run`
- Interface-level: `switchport nonegotiate`, `no logging event link-status`, `load-interval`, `udld port aggressive`, `spanning-tree portfast edge`, `spanning-tree bpdufilter`, `ip dhcp snooping`
- `ip forward-protocol nd`
- `ip http server` / `ip http secure-server` (present in mockios ✓)
- ACL definitions (`ip access-list extended ...`)
- `kron` scheduler
- `event manager` applets
- SNMP configuration
- Line configuration details (exec-timeout, privilege level, transport input)
- NTP configuration

### 8. Help Text Differences
Real IOS `write` help: `Write running configuration to memory, network, or terminal`
MockIOS `write` help: `Write running configuration to memory or network`

Real IOS `?` formatting uses 2-space indent consistently and right-aligns descriptions. MockIOS does this correctly for most commands.

### 9. Missing `end` Command Behavior
MockIOS has `end` in exec mode with help "Return to privileged EXEC mode (no-op in exec)". Real IOS does NOT have `end` in privileged exec mode - it only exists in config mode.

### 10. Missing `quit` Command
MockIOS has `quit` in exec mode. Real IOS does NOT have `quit` in privileged exec mode.

## Medium Priority Gaps

### 11. Interface Naming in Show Interfaces Status
Real IOS abbreviates interface names in `show interfaces status`:
- `Gi1/0/1` (not `GigabitEthernet1/0/1`)
- `Te1/0/1` (not `TenGigabitEthernet1/0/1`)

MockIOS correctly abbreviates these. ✓

### 12. VLAN/Loopback Interface Support
Real IOS has Vlan1, Vlan2, Vlan127, Loopback0 - all dynamically created.
MockIOS only creates Vlan1 by default. Need to support dynamic VLAN interface creation and Loopback interfaces.

### 13. Show Version - Virtual Ethernet Count
Real IOS: "3 Virtual Ethernet interfaces" (varies by config)
MockIOS: "1 Virtual Ethernet interfaces" (hardcoded)
Should be dynamic based on number of VLAN interfaces.

### 14. `show ?` - Trailing `<cr>` Entry
Real IOS `show ?` ends with:
```
  <cr>
```
(just `<cr>` with empty description)

This doesn't appear in the real `show ?` output actually - need to verify. MockIOS shows it.

### 15. Second Platform Differences (AY-LIVING, 12.2)
- Different software line: `C3560C Software (C3560c405ex-UNIVERSALK9-M), Version 12.2(55)EX2`
- Has `Image text-base:` line (absent in 15.2)
- Different processor: `(PowerPC)` vs `(APM86XXX)`
- Different port count: 10 Gigabit Ethernet, 2 Virtual Ethernet
- Different switch table alignment (narrower Model column)
- `Top Assembly Part Number: 800-35076-03` vs `68-5409-02`

## Low Priority / Cosmetic

### 16. Switch Table Trailing Spaces
Real IOS has trailing spaces in the switch table header and data rows. MockIOS doesn't pad to the same width.

### 17. Show Running-Config Byte Count
MockIOS: `Current configuration : 2006 bytes`
The byte count should accurately reflect the actual config size.

### 18. Missing `show` Bare Command Behavior
Both real and mock correctly show `% Type "show ?" for a list of subcommands` for bare `show`. ✓

## Prioritized Fix Plan

### Phase 1 - Critical Fixes (High Impact)
1. Fix command echo bug
2. Fix route ordering in `show ip route`
3. Remove `end` and `quit` from exec mode
4. Fix interface status states (notconnect vs disabled)
5. Fix Te port default speed/duplex display

### Phase 2 - Missing Exec Commands
6. Add ~30 most common missing exec commands (even as stubs)
7. Add ~50 most common missing show subcommands

### Phase 3 - Running Config Realism
8. Add service timestamps config support
9. Add interface-level config options (switchport nonegotiate, spanning-tree, etc.)
10. Support Loopback interfaces
11. Support multiple VLAN interfaces
12. Support ACL configuration

### Phase 4 - Behavioral Fidelity
13. Fix help text minor differences
14. Support `|` pipe filtering (include, exclude, section, begin)
15. Add `--More--` paging support
16. Dynamic Virtual Ethernet count in show version
