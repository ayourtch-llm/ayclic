# MockIOS Realism Improvement — Batch 1

Date: 2026-03-27
Comparing: Real SEED-001-S0244 (WS-C3560CX-12PD-S, IOS 15.2(7)E13) vs MockIOS

## Findings Summary

### show version — Major gaps
Real device has ~60 lines including:
- `Compiled Mon 15-Sep-25 13:05 by mcpre`
- `ROM: Bootstrap program is C3560CX boot loader`
- `BOOTLDR:` line
- `System restarted at` timestamp
- `Last reload reason:` line
- Crypto notice block (10 lines)
- License block (Level, Type, Next reload)
- Hardware detail: processor type `(APM86XXX)`, revision, memory (single value not split)
- Interface count lines (3 Virtual Ethernet, 16 Gigabit Ethernet, 2 Ten Gigabit Ethernet)
- `Last reset from power-on`
- `The password-recovery mechanism is disabled.`
- Flash/NVRAM sizes
- Hardware inventory (MAC, assembly numbers, serial numbers, model/revision)
- Switch table (Switch/Ports/Model/SW Version/SW Image)
- `Configuration register is 0xF`

MockIOS currently outputs ~12 lines, missing all of the above.

### show ip interface brief — Dead giveaway
Real device: Switch naming `GigabitEthernet1/0/X`, Vlan interfaces, TenGigabitEthernet, Loopback
MockIOS: Router naming `GigabitEthernet0/X`, only 2 interfaces

### show vlan brief — Missing defaults
Real device: VLANs 1002-1005 always present (fddi-default, trcrf-default, fddinet-default, trbrf-default) with `act/unsup` status
MockIOS: Only VLAN 1

### show running-config — Minimal vs 11K bytes
Real device: ~300 lines with service, AAA, spanning-tree, VTP, DHCP pools, crypto PKI, switchport config, errdisable, SNMP, multiple line vty blocks, NTP
MockIOS: ~20 lines, bare minimum

### show ? — 25 commands vs 200+
Real device lists ~200 commands alphabetically from `aaa` to `xsd-format`
MockIOS lists 25 commands

## Implementation Plan — Batch 1

### Change 1: Switch-correct interfaces and VLANs in DeviceState
**Files**: `device_state.rs`, `lib.rs`
- Replace GigabitEthernet0/0, 0/1 with Vlan1 + GigabitEthernet1/0/1..16 + TenGigabitEthernet1/0/1..2
- Add default VLANs 1002-1005
- Add `VlanState.unsupported: bool` field for `act/unsup` display
- New DeviceState fields: `base_mac`, `sw_image`, `last_reload_reason`, `service_password_encryption`, `spanning_tree_mode`, `vtp_mode`, `vtp_domain`

### Change 2: Overhaul show version output
**File**: `lib.rs`
- Add Compiled line, BOOTLDR, System restarted at, Last reload reason
- Add crypto notice block
- Add License block
- Add processor with revision, single memory value
- Count interfaces by type from state
- Add hardware inventory section
- Add Switch table
- Helper functions: `version_to_filename_suffix()`, `model_family()`

### Change 3: Enrich running-config with switch boilerplate
**File**: `device_state.rs`
- Add `service password-encryption`, `aaa new-model`, `ip routing`
- Add spanning-tree config
- Add VTP config
- Add proper line con 0 / line vty 0 4 / line vty 5 15 blocks
- Default switchport mode for Gi/Te interfaces

### Change 4: Fix all tests
- Update interface names from 0/X to 1/0/X throughout tests
- Update version, model defaults
- Add assertions for new show version content

## Execution Order
1. DeviceState struct changes + new defaults
2. Show version overhaul
3. Running-config enrichment
4. Test fixes
5. Verify with `cargo test --workspace`
