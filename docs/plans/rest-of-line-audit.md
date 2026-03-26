# RestOfLine / Stub Handler Audit

Commands that use `<rest>` RestOfLine params and dump to `running_config` or
`unmodeled_config` without updating DeviceState. These are "fake" — they accept
input but don't model it, so `show` commands can't reflect the config.

## Config Mode — Main tree (conf_tree)

### Properly implemented (reads/writes DeviceState)
- `hostname <name>` — updates `state.hostname`
- `interface <type> <number>` — enters config-if, creates interface in state
- `ip address <ip> <mask>` — sets on current interface in state
- `ip route <prefix> <mask> <nh>` — adds to `state.static_routes`
- `shutdown` / `no shutdown` — toggles `admin_up` in state
- `no ip route` — removes from `state.static_routes`
- `ip domain-name` — updates `state.domain_name`
- `ip name-server` — updates `state.name_servers`
- `access-list` — adds to `state.access_lists`
- `banner motd` — updates `state.banner_motd`

### STUBBED — pushes to `running_config` Vec (NOT DeviceState)
These write to the OLD `running_config: Vec<String>` field, which is NOT
the source of truth for `show running-config`. The DeviceState's
`generate_running_config()` method doesn't see these.

| Command | Handler | What's missing |
|---------|---------|----------------|
| `service <rest>` | `handle_rest_of_line` | Should store in state (timestamps, password-encryption, etc.) |
| `logging <rest>` | `handle_rest_of_line` | Should store logging config in state for `show logging` |
| `username <rest>` | `handle_rest_of_line` | Should store usernames in state |
| `description <rest>` (config-if) | `handle_description` | **Partially done** — InterfaceState has `description` field but handler writes to `running_config` not state! |
| `switchport <rest>` (config-if) | `handle_switchport` | Should update `switchport_mode`/`vlan` in InterfaceState |
| `spanning-tree <rest>` | `handle_spanning_tree` | Could store for `show spanning-tree` |
| `vlan <rest>` | `handle_vlan` | Should update VlanState for `show vlan brief` |
| `enable secret <rest>` | `handle_enable_secret` | Should update `state.enable_secret` |
| `enable password <rest>` | `handle_enable_password` | Should update enable password in state |
| `no <rest>` (generic) | `handle_no` | Only handles `no shutdown` and `no ip route` — all other `no` forms are ignored |

### STUBBED — pushes to `unmodeled_config` (shown in show run but not structured)

These go into `state.unmodeled_config` which IS emitted by `generate_running_config()`,
but they're raw strings, not structured data.

| Command | Sub-mode | What's missing |
|---------|----------|----------------|
| `speed <rest>` | config-if | Should store in InterfaceState.speed |
| `duplex <rest>` | config-if | Should store in InterfaceState.duplex |
| `network <rest>` | config-router | Should model OSPF/BGP/EIGRP networks |
| `router-id <ip>` | config-router | Should store router ID |
| `area <rest>` | config-router | Should model OSPF areas |
| `redistribute <rest>` | config-router | Should model redistribution |
| `passive-interface <rest>` | config-router | Should model passive interfaces |
| `log-adjacency-changes` | config-router | Should store as flag |
| `neighbor <rest>` | config-router | Should model BGP/OSPF neighbors |
| `transport <rest>` | config-line | Should store transport config |
| `exec-timeout <rest>` | config-line | Should store timeout values |
| `login <rest>` | config-line | Should store login config |
| `privilege <rest>` | config-line | Should store privilege level |
| `logging <rest>` | config-line | Should store line logging config |
| `length <rest>` | config-line | Should store terminal length |
| `password <rest>` | config-line | Should store line password |
| All `no <rest>` in sub-modes | config-if/router/line | Generic no — doesn't negate anything |

## Exec Mode Stubs

| Command | Handler | What's missing |
|---------|---------|----------------|
| `debug <feature>` | `handle_debug` | Prints "on" but doesn't track debug state |
| `undebug <feature>` | `handle_undebug` | Prints "off" but doesn't track state |
| `clear <rest>` | `handle_clear` | Silent no-op — doesn't clear anything |
| `clock set <rest>` | `handle_clock_set` | Doesn't update clock state — `show clock` still shows hardcoded time |
| `ssh <rest>` | `handle_ssh` | Always "connection refused" |
| `telnet <host>` | `handle_telnet` | Always "connection refused" |

## Priority Fixes

### P0 — Broken (handler doesn't write to state even though state model exists)
1. **`description`** — InterfaceState has `description` field but `handle_description` writes to `running_config` not state
2. **`enable secret/password`** — `handle_enable_secret` writes to `running_config`, not `state.enable_secret`

### P1 — High value (common automation targets)
3. **`switchport mode`** / **`switchport access vlan`** — InterfaceState has `switchport_mode`/`vlan` fields
4. **`speed`** / **`duplex`** — InterfaceState has these fields
5. **`username`** — needed for auth simulation
6. **`no` in config-if** — should negate: `no description`, `no ip address`, `no switchport`
7. **`show clock` using real time** — `clock set` should update, or use system time

### P2 — Medium value
8. **`service timestamps`** — model for show run accuracy
9. **`logging`** — model for show logging accuracy
10. **`vlan`** config — update VlanState
11. **config-router** commands — model OSPF/BGP state for show ip ospf/bgp
12. **config-line** commands — model for show line accuracy

### P3 — Low priority
13. **`spanning-tree`** — model for show spanning-tree accuracy
14. **`debug` state tracking** — for show debugging
