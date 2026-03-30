# show running-config: Real IOS vs MockIOS Comparison

Captured 2026-03-30 from SEED-001-S0244 (192.168.0.112) vs mockios with same hostname.

## Differences Found

### 1. Interface ordering
**Real IOS:** Physical ports first, then SVIs at the end:
```
interface GigabitEthernet1/0/1
...
interface TenGigabitEthernet1/0/2
...
interface Vlan1
interface Vlan2
interface Vlan127
```

**MockIOS:** SVIs first, then physical:
```
interface Vlan1
...
interface GigabitEthernet1/0/1
...
```

**Fix needed:** Reorder interfaces in running-config: physical first, then SVIs.

### 2. ip route position
**Real IOS:** `ip route` appears AFTER interfaces and AFTER `ip forward-protocol nd`:
```
ip forward-protocol nd
!
!
ip http server
ip http secure-server
ip route 0.0.0.0 0.0.0.0 10.100.252.201
ip ssh version 2
```

**MockIOS:** `ip route` appears BEFORE interfaces:
```
ip route 0.0.0.0 0.0.0.0 10.0.0.254
!
ip forward-protocol nd
ip http server
```

**Fix needed:** Move ip route after ip http/ssh section.

### 3. Real IOS has `!` separators between ip http and ip route
Real IOS has TWO `!` lines between `ip forward-protocol nd` and `ip http server`. MockIOS has none.

### 4. Missing switchport config lines
Real IOS shows `switchport mode trunk` and `switchport nonegotiate` on trunk ports. MockIOS correctly omits `switchport mode access` (fixed earlier) but still missing `switchport nonegotiate`.

### 5. Interface sub-command ordering
Real IOS:
```
interface GigabitEthernet1/0/1
 switchport mode trunk
 switchport nonegotiate
 no logging event link-status
 no logging event power-inline-status
 load-interval 30
 udld port aggressive
 spanning-tree portfast edge
 spanning-tree bpdufilter enable
 spanning-tree link-type point-to-point
 ip dhcp snooping information option allow-untrusted
```

MockIOS: Just `shutdown` if applicable. Much simpler.

### 6. Line VTY groups
**Real IOS:** Three VTY groups: 0-4, 5-10, 11-15 (with different settings)
**MockIOS:** Two VTY groups: 0-4, 5-15

### 7. Missing sections after line config
Real IOS has after line config:
- `ntp server 10.100.253.5`
- `event manager applet CALLHOME6` (with actions)
- `event manager applet CALLHOME` (with actions)

MockIOS has none of these (expected - these are complex features).

### 8. Loopback interface missing
Real IOS has `interface Loopback0` with IP and IPv6 config. MockIOS default state doesn't include one.

### 9. Empty interface blocks
**Real IOS:** Interfaces with no non-default config still show the interface line:
```
interface GigabitEthernet1/0/1
 switchport mode trunk
 ...
```

**MockIOS:** Empty interface blocks show just:
```
interface GigabitEthernet1/0/1
!
```

This is actually correct - real IOS does the same for truly empty interfaces.

## Priority Fixes
1. Interface ordering (physical before SVIs)
2. ip route position (after ip http/ssh)
3. Extra `!` separators to match real IOS patterns
