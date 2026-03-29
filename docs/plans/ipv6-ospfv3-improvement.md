# IPv6 + OSPFv3 Data Model Improvement Plan

## Motivation

Comparing mockios against a real WS-C3560CX-12PD-S running IOS 15.2(7)E13,
the mock is missing:

1. **IPv6 addressing** — the real device has `ipv6 unicast-routing`, per-interface
   `ipv6 address` (link-local + global), `ipv6 enable`, and auto-generated
   EUI-64 link-local addresses.
2. **IPv6 routing table** — `show ipv6 route` with Connected, Local, Static,
   ND, OSPF route types.
3. **OSPFv3** — `ipv6 router ospf <pid>`, per-interface `ipv6 ospf <pid> area <area>`,
   `show ipv6 ospf`, `show ipv6 ospf interface brief`, `show ipv6 ospf neighbor`.
4. **Loopback interfaces** — the real device has `interface Loopback0` with both
   IPv4 and IPv6 addresses; mockios cannot create Loopback interfaces.
5. **`show ipv6 ?` help tree** — the real device lists ~35 subcommands under
   `show ipv6`; mockios has none.

## Design Principle: Forwarding-Engine-Ready

The data model must be structured so that a real forwarding engine can be
bolted on later. This means:

- **Addresses are typed structs**, not strings — use `Ipv6Addr` from `std::net`.
- **Prefix/length pairs** use a dedicated `Ipv6Prefix` struct: `(Ipv6Addr, u8)`.
- **The routing table is a first-class data structure**, not generated on-the-fly
  from interface state. Routes have origin (Connected, Local, Static, OSPF, etc.),
  next-hop, outgoing interface, administrative distance, and metric.
- **OSPFv3 state** is modeled as a process with areas, interfaces, neighbors,
  and LSA database — even if we only populate stubs initially.
- **Interface IPv6 state** tracks: enabled flag, link-local address, list of
  global/ULA addresses with prefix lengths, ND state.

## Data Model Changes (device_state.rs)

### New Types

```rust
use std::net::Ipv6Addr;

/// An IPv6 address with prefix length, as configured on an interface.
pub struct Ipv6AddrConfig {
    pub address: Ipv6Addr,
    pub prefix_len: u8,
    pub addr_type: Ipv6AddrType,
    pub eui64: bool,           // was this auto-generated via EUI-64?
}

pub enum Ipv6AddrType {
    LinkLocal,
    Global,
    UniqueLocal,
}

/// An entry in the IPv6 routing table.
pub struct Ipv6Route {
    pub prefix: Ipv6Addr,
    pub prefix_len: u8,
    pub route_type: Ipv6RouteType,
    pub admin_distance: u16,
    pub metric: u32,
    pub next_hop: Option<Ipv6Addr>,    // None for connected/local
    pub interface: Option<String>,      // outgoing interface name
}

pub enum Ipv6RouteType {
    Connected,     // C
    Local,         // L
    LocalConnected,// LC (loopback connected)
    Static,        // S
    OspfIntra,     // O
    OspfInter,     // OI
    OspfExt1,      // OE1
    OspfExt2,      // OE2
    NdDefault,     // ND
    NdPrefix,      // NDp
}

/// OSPFv3 process state.
pub struct OspfV3Process {
    pub process_id: u16,
    pub router_id: Ipv4Addr,           // OSPF uses IPv4 router-ID even for v3
    pub areas: Vec<OspfV3Area>,
    pub reference_bandwidth: u32,       // in Mbps, default 100
    pub spf_delay: u32,                 // initial delay ms
    pub spf_hold: u32,                  // min hold ms
    pub spf_max_wait: u32,              // max wait ms
}

pub struct OspfV3Area {
    pub area_id: u32,                   // 0 = backbone
    pub interfaces: Vec<String>,        // interface names assigned to this area
    pub area_type: OspfV3AreaType,
    pub spf_executions: u32,
    pub lsa_count: u32,
    pub lsa_checksum: u32,
}

pub enum OspfV3AreaType {
    Normal,
    Stub,
    Nssa,
}

/// IPv6 static route.
pub struct Ipv6StaticRoute {
    pub prefix: Ipv6Addr,
    pub prefix_len: u8,
    pub next_hop: Option<Ipv6Addr>,
    pub interface: Option<String>,
    pub admin_distance: u8,             // default 1
}
```

### InterfaceState additions

```rust
pub struct InterfaceState {
    // ... existing fields ...

    // IPv6 state
    pub ipv6_enabled: bool,
    pub ipv6_addresses: Vec<Ipv6AddrConfig>,
    // The link-local is auto-generated from MAC when ipv6_enabled or any
    // ipv6 address is configured, unless an explicit link-local is set.
}
```

### DeviceState additions

```rust
pub struct DeviceState {
    // ... existing fields ...

    pub ipv6_unicast_routing: bool,
    pub ipv6_routes: Vec<Ipv6Route>,         // computed routing table
    pub ipv6_static_routes: Vec<Ipv6StaticRoute>,
    pub ospfv3_processes: Vec<OspfV3Process>,
}
```

## Show Commands to Implement

### show ipv6 interface brief

Format observed from real device:
```
Vlan1                  [up/up]
    FE80::1A8B:45FF:FE17:F7C0
    2001:DB8::1
GigabitEthernet1/0/1   [down/down]
    unassigned
Loopback0              [up/up]
    FE80::10:127:0:0
    2A11:D940:2:7F00::
```

- Interface name left-aligned in ~23 chars, then `[status/protocol]`
- Each IPv6 address on its own indented line (4 spaces)
- `unassigned` if no IPv6 addresses

### show ipv6 route

Full codes header from real device, then entries like:
```
C   2001:DB8::/64 [0/0]
     via Vlan1, directly connected
L   2001:DB8::1/128 [0/0]
     via Vlan1, receive
```

### show ipv6 ospf

Process info with timers, area summary. Format captured from real device.

### show ipv6 ospf interface brief

```
Interface    PID   Area            Intf ID    Cost  State Nbrs F/C
Lo0          1     0               5428       1     LOOP  0/0
```

## Config Commands to Implement

### Global config
- `ipv6 unicast-routing` / `no ipv6 unicast-routing`
- `ipv6 route <prefix>/<len> {<next-hop> | <interface>} [<ad>]`
- `ipv6 router ospf <pid>` → enter config-router submode

### Interface config
- `ipv6 enable`
- `ipv6 address <addr>/<len>` — global address
- `ipv6 address <addr> link-local` — explicit link-local
- `ipv6 ospf <pid> area <area>`
- `ipv6 ospf network point-to-point`

### Config-router (OSPFv3) submode
- `router-id <ipv4-addr>`

## Implementation Order (TDD)

1. **Data model types** — add structs, derive defaults, no behavior yet
2. **Tests first** — write tests for `show ipv6 interface brief` expected output
3. **IPv6 address config commands** — `ipv6 address`, `ipv6 enable`
4. **Link-local auto-generation** — EUI-64 from MAC address
5. **`show ipv6 interface brief`** handler
6. **IPv6 routing table computation** — from interface addresses
7. **`show ipv6 route`** handler
8. **Loopback interface creation** — `interface Loopback<n>`
9. **OSPFv3 process creation** — `ipv6 router ospf <pid>`
10. **OSPFv3 interface assignment** — `ipv6 ospf <pid> area <area>`
11. **`show ipv6 ospf`** handler
12. **`show ipv6 ospf interface brief`** handler
13. **Running-config generation** — emit ipv6 lines
14. **Help tree entries** — `show ipv6 ?` subcommands

## EUI-64 Link-Local Generation Algorithm

Given MAC `18:8B:45:17:F7:80`:
1. Insert `FF:FE` in the middle: `18:8B:45:FF:FE:17:F7:80`
2. Flip the 7th bit (universal/local): `1A:8B:45:FF:FE:17:F7:80`
3. Prefix with `FE80::`: `FE80::1A8B:45FF:FE17:F780`

For Cisco dotted MAC `188b.4517.f780`:
- Split into bytes: `18 8B 45 17 F7 80`
- Apply EUI-64 as above

For Loopback interfaces with explicit link-local (e.g., `FE80::10:127:0:0`),
the configured value is used as-is, no EUI-64.
