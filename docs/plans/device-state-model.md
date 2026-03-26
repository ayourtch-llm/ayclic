# MockIOS Device State Model

## Status: STEPS 1-4 COMPLETE, STEP 5 REMAINING

## What's done

Steps 1-4 are implemented and working:

1. **DeviceState struct** (`mockios/src/device_state.rs`): InterfaceState, StaticRoute,
   VlanState types with generate_running_config(), generate_startup_config(),
   generate_show_interface(), generate_show_vlan_brief()

2. **DeviceState on MockIosDevice**: `pub state: DeviceState` field, builders update state

3. **Show handlers read from state**:
   - show running-config — generated from state with "Building configuration..." header
   - show startup-config — generated from state with "Using NNN bytes" header
   - show ip interface brief — reads state.interfaces
   - show ip route — full IOS format with Codes header, connected (C/L) + static (S/S*) routes
   - show interfaces [name] — detailed output with status, MAC, MTU, counters
   - show vlan brief — reads state.vlans
   - show version/boot/clock/history — from state
   - ping — checks routing table for reachability

4. **Config handlers write to state**:
   - hostname — updates state.hostname
   - interface — creates in state, normalizes names (loopback 0 → Loopback0, etc.)
   - ip address — sets on current interface
   - shutdown / no shutdown — toggles admin_up
   - ip route — adds to state.static_routes
   - no ip route — removes from state.static_routes

## Step 5 remaining: Remove running_config Vec<String>

The old `running_config: Vec<String>` field is still on MockIosDevice but is no longer
the source of truth. Show handlers read from `state`, config handlers write to `state`.
The field can be removed, but `with_running_config()` builder and any tests using it
need migration. Also `apply_config_text_to_state()` may need updating.

## What else could be added (future work)

- More config commands: `access-list`, `snmp-server`, `logging`, `ntp`, `banner`
- `show access-lists`, `show snmp`, `show ntp status`
- `show cdp neighbors` backed by state
- `show logging` with buffer
- `copy running-config startup-config` / `write memory` persisting to startup state
- VLAN database management (vlan config mode)
- `description` on interfaces stored in state
- `speed`/`duplex` config stored in state
- ACL model
