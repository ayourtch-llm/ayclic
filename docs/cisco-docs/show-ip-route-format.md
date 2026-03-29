# Cisco IOS 15.2 - show ip route Output Format

Reference platform: WS-C3560CX-12PD-S running IOS 15.2(x)E
Sources: Cisco official documentation, Cisco Press CCNA guide, study resources

---

## 1. Full Route Codes Legend

The legend appears at the top of every `show ip route` output. In IOS 15.2
the full legend is:

```
Codes: L - local, C - connected, S - static, R - RIP, M - mobile, B - BGP
       D - EIGRP, EX - EIGRP external, O - OSPF, IA - OSPF inter area
       N1 - OSPF NSSA external type 1, N2 - OSPF NSSA external type 2
       E1 - OSPF external type 1, E2 - OSPF external type 2
       i - IS-IS, su - IS-IS summary, L1 - IS-IS level-1, L2 - IS-IS level-2
       ia - IS-IS inter area, * - candidate default, U - per-user static route
       o - ODR, P - periodic downloaded static route, H - NHRP, l - LISP
       a - application route
       + - replicated route, % - next hop override, p - overrides from PfR
```

Notes on legend formatting:
- First line begins with `Codes: ` (7 characters).
- Continuation lines are indented with 7 spaces to align under the `L`.
- Codes are separated by `, ` (comma-space).
- The `*` candidate default marker is on the same continuation line as `U`.
- The `+`, `%`, and `p` symbols appear on the last line.

The legend varies slightly between IOS releases. Older IOS (pre-12.2) does
not have `L - local` or `H - NHRP`. IOS 15.2 includes all codes above.

---

## 2. Gateway of Last Resort Line

Appears immediately after the codes legend (one blank line after legend),
before the route table entries.

### When a default route is set

```
Gateway of last resort is 10.0.0.2 to network 0.0.0.0
```

Format: `Gateway of last resort is <next-hop-ip> to network <network>`

The network is always `0.0.0.0` for a standard default route.

### When no default route is set

```
Gateway of last resort is not set
```

---

## 3. Complete Example Output

### Example: Switch with Connected/Local Routes, Static Default, and OSPF

```
Switch# show ip route
Codes: L - local, C - connected, S - static, R - RIP, M - mobile, B - BGP
       D - EIGRP, EX - EIGRP external, O - OSPF, IA - OSPF inter area
       N1 - OSPF NSSA external type 1, N2 - OSPF NSSA external type 2
       E1 - OSPF external type 1, E2 - OSPF external type 2
       i - IS-IS, su - IS-IS summary, L1 - IS-IS level-1, L2 - IS-IS level-2
       ia - IS-IS inter area, * - candidate default, U - per-user static route
       o - ODR, P - periodic downloaded static route, H - NHRP, l - LISP
       a - application route
       + - replicated route, % - next hop override, p - overrides from PfR

Gateway of last resort is 10.0.0.2 to network 0.0.0.0

      10.0.0.0/8 is variably subnetted, 2 subnets, 2 masks
C        10.0.0.0/24 is directly connected, GigabitEthernet0/1
L        10.0.0.1/32 is directly connected, GigabitEthernet0/1
      192.168.1.0/24 is variably subnetted, 2 subnets, 2 masks
C        192.168.1.0/24 is directly connected, Vlan10
L        192.168.1.1/32 is directly connected, Vlan10
      192.168.2.0/24 is variably subnetted, 2 subnets, 2 masks
C        192.168.2.0/24 is directly connected, Vlan20
L        192.168.2.1/32 is directly connected, Vlan20
O     192.168.3.0/24 [110/2] via 10.0.0.5, 00:05:42, GigabitEthernet0/1
S     172.16.0.0/16 [1/0] via 10.0.0.2
S*    0.0.0.0/0 [1/0] via 10.0.0.2
```

---

## 4. Route Entry Format

### General Format

```
<code>    <network/prefix> [<AD>/<metric>] via <next-hop>, <age>, <interface>
```

### Connected Route (C)

```
C        192.168.1.0/24 is directly connected, Vlan10
```

Format: `C` followed by 8 spaces, then `<network/prefix> is directly connected, <interface>`

No `[AD/metric]` brackets — connected routes have AD=0 and are never shown
with brackets.

### Local Route (L)

```
L        192.168.1.1/32 is directly connected, Vlan10
```

Local routes always have `/32` (host route) for the specific IP address
configured on the interface. Introduced in IOS 12.2(33)SXI4 and universally
present in IOS 15.x.

Format: identical to connected route but with `L` code and always `/32` mask.

### Static Route

```
S     172.16.0.0/16 [1/0] via 10.0.0.2
```

- Code: `S`
- Spaces: 5 spaces after `S`
- `[1/0]`: administrative distance 1, metric 0 (default for static routes)
- Via keyword followed by next-hop IP
- No age or interface shown (static routes don't have age timers)

Static route with explicit exit interface:
```
S     172.16.0.0/16 [1/0] via 10.0.0.2, GigabitEthernet0/1
```

Static route with only exit interface (no next-hop):
```
S     172.16.0.0/16 is directly connected, GigabitEthernet0/1
```

Static route with non-default administrative distance (floating static):
```
S     172.16.0.0/16 [254/0] via 10.0.1.2
```

### Default Static Route (S*)

```
S*    0.0.0.0/0 [1/0] via 10.0.0.2
```

The `*` immediately follows `S` with no space, then 4 spaces before the
network. The `*` marks this route as the candidate default (used as gateway
of last resort).

### OSPF Route

```
O     192.168.3.0/24 [110/2] via 10.0.0.5, 00:05:42, GigabitEthernet0/1
```

- Code: `O` (OSPF intra-area)
- `[110/2]`: administrative distance 110, metric (cost) 2
- `via 10.0.0.5`: next-hop IP
- `00:05:42`: time since last route update (hours:minutes:seconds)
- `GigabitEthernet0/1`: outgoing interface

OSPF inter-area:
```
O IA  192.168.4.0/24 [110/3] via 10.0.0.5, 00:05:42, GigabitEthernet0/1
```

OSPF external type 2:
```
O E2  10.1.0.0/16 [110/20] via 10.0.0.5, 00:05:42, GigabitEthernet0/1
```

### EIGRP Route

```
D     10.1.1.0/24 [90/2170112] via 10.0.0.5, 00:01:30, GigabitEthernet0/1
```

EIGRP external:
```
D EX  10.2.0.0/24 [170/2172416] via 10.0.0.5, 00:01:30, GigabitEthernet0/1
```

### RIP Route

```
R     10.3.0.0/24 [120/1] via 10.0.0.5, 00:00:18, GigabitEthernet0/1
```

---

## 5. Variably Subnetted Summary Lines

When a classful network block has subnets of different lengths (VLSM), IOS
groups them with a summary header:

```
      10.0.0.0/8 is variably subnetted, 2 subnets, 2 masks
C        10.0.0.0/24 is directly connected, GigabitEthernet0/1
L        10.0.0.1/32 is directly connected, GigabitEthernet0/1
```

Format of summary line:
`      <classful-network>/<classful-prefix> is variably subnetted, <N> subnets, <M> masks`

- Leading indent: 6 spaces
- The classful network is the class A/B/C boundary (e.g., `/8` for 10.x.x.x,
  `/16` for 172.16.x.x, `/24` for 192.168.1.x)
- `N subnets`: total number of subnet entries in this block
- `M masks`: number of distinct prefix lengths used

When all subnets have the same mask (single subnet length), the format is:
```
      10.0.0.0/24 is subnetted, 2 subnets
```
(No "variably" prefix, and no "masks" count.)

Route entries within the block are indented further:
- Summary header: 6 spaces indent
- Route entries: `C` or `L` code then 8 spaces (total ~9 chars before network)

---

## 6. Route Ordering

Routes are displayed grouped by classful network block, then within each block
by prefix length (more specific first). The classful blocks are sorted
numerically by the major network address.

Example ordering:
1. Summary header line for 10.0.0.0/8
2. All 10.x.x.x subnets (most specific first)
3. Summary header line for 172.16.0.0/16
4. All 172.16.x.x subnets
5. Summary header line for 192.168.1.0/24
6. All 192.168.1.x subnets
7. The default route (0.0.0.0/0) appears last

Within a subnet block, more specific routes appear before less specific:
```
      192.168.1.0/24 is variably subnetted, 3 subnets, 3 masks
C        192.168.1.0/24 is directly connected, Vlan10
L        192.168.1.1/32 is directly connected, Vlan10
O        192.168.1.128/25 [110/2] via 10.0.0.5, ...
```

---

## 7. Administrative Distance Reference

Standard administrative distances as shown in `show ip route` brackets:

| Route Source            | Code | AD   |
|-------------------------|------|------|
| Connected               | C    | 0    |
| Static                  | S    | 1    |
| EIGRP Summary           | D    | 5    |
| BGP External            | B    | 20   |
| EIGRP Internal          | D    | 90   |
| IGRP                    | I    | 100  |
| OSPF                    | O    | 110  |
| IS-IS                   | i    | 115  |
| RIP                     | R    | 120  |
| ODR                     | o    | 160  |
| EIGRP External          | D EX | 170  |
| BGP Internal (iBGP)     | B    | 200  |
| Unknown/Unreachable     | -    | 255  |

Local routes (L) have AD=0 but brackets are not shown in output.

---

## 8. Special Cases

### Multiple Next-Hops (ECMP / Load Balancing)

When multiple equal-cost paths exist, each is shown on its own line:
```
O     192.168.5.0/24 [110/2] via 10.0.0.5, 00:05:42, GigabitEthernet0/1
                     [110/2] via 10.0.0.6, 00:05:42, GigabitEthernet0/2
```
The second line starts at the same column as the opening `[` bracket.

### Null Route (Discard)

```
S     192.168.0.0/16 [1/0] is directly connected, Null0
```

### Summarized Route (Auto-Summary or Manual)

```
D     10.0.0.0/8 is a summary, 00:01:00, Null0
```

### No Routes in Table

When the routing table is empty (or only has connected routes removed):
```
Switch# show ip route
Codes: L - local, C - connected, ...
       ...

```
(Blank after the Gateway of last resort line.)

---

## 9. `show ip route <network>` (Specific Prefix Lookup)

```
Switch# show ip route 192.168.1.0
Routing entry for 192.168.1.0/24
  Known via "connected", distance 0, metric 0 (connected, via interface)
  Routing Descriptor Blocks:
  * directly connected, via Vlan10
      Route metric is 0, traffic share count is 1
```

For a static route:
```
Switch# show ip route 172.16.0.0
Routing entry for 172.16.0.0/16
  Known via "static", distance 1, metric 0
  Routing Descriptor Blocks:
  * 10.0.0.2
      Route metric is 0, traffic share count is 1
```
