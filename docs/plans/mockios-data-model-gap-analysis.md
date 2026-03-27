# MockIOS Data Model Gap Analysis

Date: 2026-03-27
Comparing mockios DeviceState against a real WS-C3560CX-12PD-S (IOS 15.2)

## Current Data Model

### DeviceState
- `hostname`, `version`, `model`, `serial_number`, `config_register`
- `uptime` (static string, not real-time tracked)
- `interfaces: Vec<InterfaceState>`
- `static_routes: Vec<StaticRoute>`
- `flash_files: HashMap<String, Vec<u8>>`, `flash_total_size: u64`
- `boot_variable`, `domain_name`, `name_servers: Vec<String>`
- `enable_secret: Option<String>`
- `banner_motd`
- `install_state: Option<InstallState>`
- `unmodeled_config: Vec<String>` (catch-all)
- `vlans: Vec<VlanState>`
- `access_lists: Vec<AccessList>`

### InterfaceState
- `name`, `description`, `admin_up`, `link_up`
- `ip_address: Option<(Ipv4Addr, Ipv4Addr)>`
- `speed`, `duplex` (strings)
- `mtu: u16`
- `switchport_mode: Option<String>`, `vlan: Option<u16>`
- `mac_address` (deterministic from name hash)
- Basic counters: input/output packets/bytes/errors

### VlanState
- `id`, `name`, `active`, `ports: Vec<String>`

## What's Missing — Prioritized

### Phase 1: Foundation (make it a switch, not a router) — P0

1. **Correct interface inventory**: Need Gi1/0/1-16 + Te1/0/1-2 + Vlan1 (switch naming 1/0/X, not router 0/X)
2. **Switchport model on InterfaceState**: enum `SwitchportMode { Access, Trunk, DynamicAuto, DynamicDesirable }`, `access_vlan: u16`, `trunk_native_vlan: u16`, `trunk_allowed_vlans`, `trunk_encapsulation`
3. **Default VLANs 1002-1005**: fddi-default, trcrf-default, fddinet-default, trbrf-default (always present, act/unsup)
4. **VLAN config handler**: Should create/modify VlanState entries, not dump to unmodeled_config
5. **Fix dns config handlers**: `handle_ip_domain_name` / `handle_ip_name_server` don't update state fields (bug)

### Phase 2: Realism (fool basic automation tools) — P1

6. **system_image: String** — shown in show version
7. **Real uptime tracking**: `boot_time: std::time::Instant` instead of static string
8. **last_reload_reason: String**
9. **base_mac_address: [u8; 6]** — derive per-interface MACs from it
10. **ip_routing_enabled: bool** — defaults to false on L2 switches
11. **ArpEntry struct + arp_table: Vec<ArpEntry>**
12. **MacAddressEntry struct + mac_address_table: Vec<MacAddressEntry>**
13. **PoE fields per interface**: power_allocated, power_class, admin_state
14. **Structured LineConfig** for console/vty: transport, exec-timeout, password, login method
15. **Structured UserAccount**: name, privilege, secret type, hash

### Phase 3: Deep fidelity (fool experienced engineers) — P2

16. VTP state: domain, mode, version, pruning
17. Per-VLAN spanning tree: SpanningTreeVlan with priority, root bridge, port roles/states
18. EtherChannel: channel_group on InterfaceState, PortChannelState struct
19. NTP/SNMP/syslog as structured config (not hardcoded show output)
20. Port-security per interface
21. DHCP snooping state
22. Named ACLs (ip access-list standard/extended)
23. Dynamic help column width calculation

## Help Text Formatting Issues

| Aspect | mockios | Real IOS | Match? |
|---|---|---|---|
| Column width | Fixed `{:<17}` | Dynamic based on longest keyword in context | No |
| `<name>` style | angle brackets | Real IOS uses uppercase: `LINE`, `WORD`, `A.B.C.D` | No |
| Keyword sorting | insertion order | alphabetical | Partial |

## Architecture Note

MockIosDevice has field duplication: `flash_files`, `boot_variable`, `flash_total_size`, `version`, `model` exist both as top-level fields AND inside `state: DeviceState`. Builder methods update both. Should resolve by making DeviceState the single source of truth.
