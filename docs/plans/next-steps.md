# MockIOS Next Steps

## Immediate TODO (from user feedback)

### 1. Parameter value completion for Tab
`int <TAB>` in config mode should list interfaces from device state.
Needs a completer function on Param nodes in cmd_tree.rs:
```rust
Param { name, param_type, completer: Option<fn(&MockIosDevice) -> Vec<String>> }
```
The `try_tab_complete()` and `help()` functions need to call the completer
when the cursor is at a Param position.

### 2. Sub-submodes
IOS has nested sub-modes: config-router-af (address-family), config-ext-nacl, etc.
Architecture already supports this — ConfigSub(String) can be any name.
Just need to register more trees in `config_sub_tree()` as they're needed.

### 3. IPv6 support
Currently no IPv6 addressing, routing, or show commands.
Needs: ipv6 address on interfaces, show ipv6 interface brief,
show ipv6 route, ipv6 route config, ping ipv6.

### 4. Remove old running_config Vec<String>
Step 5 of device-state-model.md. The field is unused now — handlers
read from DeviceState. Remove it and migrate with_running_config() builder.

## Other improvements
- Ctrl+Y (yank/paste), Ctrl+T (transpose), Esc+F/B/D (word ops)
- `terminal editing` / `terminal no editing` toggle
- `--More--` pagination for long output
- `show cdp neighbors` backed by state
- `show logging` with buffer
- `copy running-config startup-config` persisting state
- `access-list` model and `show access-lists`
- `show interfaces status` (switch port table format)
- Line wrapping for commands longer than terminal width
- MOTD/login banners

## Current stats
- 225 unit tests + 10 device tests + 11 SSH integration tests
- Structured DeviceState model with interfaces, routes, VLANs, ACLs
- Command tree with abbreviation matching, ?, tab, help (proper-cased interface types)
- Full CLI editing (Emacs keys, arrows, history)
- Telnet (aytelnet protocol) and SSH servers
- Sub-mode-specific command trees (config-if, config-router, config-line)
- show commands: version, run, startup, ip int brief, ip route (grouped), interfaces,
  vlan, clock, boot, history, terminal, cdp, users, logging, arp, mac, spanning-tree,
  ip ospf, ip protocols, processes cpu, access-lists, flash, install
- Exec commands: help, enable (priv noop), debug/undebug, clock set, clear, ssh, telnet
- Config: access-list with data model, interface type tab completion
