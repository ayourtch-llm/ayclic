# Real IOS vs MockIOS Comparison Findings

Observed by comparing SEED-001-S0244 (WS-C3560CX, IOS 15.2(7)E10) with mockios Router1.

## Batch 1: Formatting Fixes (high impact, mechanical)

### 1.1 `show ip route` Codes header indentation
**Real IOS:**
```
Codes: L - local, C - connected, S - static, R - RIP, M - mobile, B - BGP
       D - EIGRP, EX - EIGRP external, O - OSPF, IA - OSPF inter area
       N1 - OSPF NSSA external type 1, N2 - OSPF NSSA external type 2
```
**MockIOS:**
```
Codes: L - local, C - connected, S - static, R - RIP, M - mobile, B - BGP
D - EIGRP, EX - EIGRP external, O - OSPF, IA - OSPF inter area
N1 - OSPF NSSA external type 1, N2 - OSPF NSSA external type 2
```
Missing 7-space indent on continuation lines.

### 1.2 `show ip route` variably-subnetted grouping
**Real IOS groups routes by major network:**
```
      10.0.0.0/8 is variably subnetted, 5 subnets, 2 masks
C        10.1.0.0/24 is directly connected, Vlan1
L        10.1.0.254/32 is directly connected, Vlan1
```
**MockIOS lists routes flat without grouping.**

### 1.3 `?` help header
**Real IOS shows a header:**
```
SEED-001-S0244#?
Exec commands:
  <1-99>           Session number to resume
  access-enable    Create a temporary Access-List entry
  ...
```
**MockIOS skips the header and `<1-99>` line.**

### 1.4 `?` help column alignment
**Real IOS:** 2-space indent, keyword padded to ~17 chars
**MockIOS:** 2-space indent, keyword padded to ~22 chars (wider than real)

### 1.5 `show ip interface brief` trailing space padding
**Real IOS pads Status/Protocol columns with trailing spaces:**
```
GigabitEthernet1/0/1   unassigned      YES unset  down                  down
```
**MockIOS has no trailing spaces after the last column.**

## Batch 2: Content/Behavior Improvements

### 2.1 `show running-config` structure
**Real IOS header:**
```
Building configuration...

Current configuration : 11695 bytes
!
! Last configuration change at 11:31:35 UTC Mon Feb 28 2000 by ayourtch
! NVRAM config last updated at 10:04:21 UTC Mon Feb 28 2000 by ayourtch
!
version 15.2
no service pad
service timestamps debug datetime msec
service timestamps log datetime msec
service password-encryption
...
```
**MockIOS is missing:** `version` line, `service` lines, config change timestamps,
`no service pad`, `service password-encryption`, etc.

### 2.2 `show ip route` static default admin distance
**Real IOS:** `S*    0.0.0.0/0 [254/0] via 192.168.0.1` — uses 254 for default static
**MockIOS:** `S*    0.0.0.0/0 [1/0] via 10.0.0.254` — uses 1

Note: Standard IOS static route AD is 1. The 254 on the real device is likely
explicitly configured. MockIOS using 1 is actually correct for default behavior.

### 2.3 Missing common exec commands
Real IOS has many commands mockios lacks:
- `clear` — reset counters/stats
- `clock set` — set system clock
- `debug` / `undebug` / `no debug` — debugging stubs
- `help` — description of help system
- `no` — negate/disable
- `ssh` / `telnet` — open connections (stub)
- `more` — display file contents
- `enable` should still appear in priv exec (real IOS shows it)

### 2.4 `show version` detail
Real IOS includes additional lines mockios lacks:
- `Compiled <date> by <user>`
- `BOOTLDR:` line
- `Last reload reason:`
- Crypto notice text block
- Detailed hardware (MAC, serial, model numbers table)

## Batch 3: Feature Additions

### 3.1 Interface `description` in device state
Real IOS stores and shows descriptions:
```
interface GigabitEthernet1/0/5
 description DISABLED-PORT
```
MockIOS has the config command but doesn't persist to DeviceState.

### 3.2 `show interfaces` format refinement
Compare detailed output format with real device.

### 3.3 `show` alone behavior
Both correctly show `% Type "show ?" for a list of subcommands` — MATCHES.

## Priority Order
1. Batch 1 items (1.1-1.5) — formatting only, no new features
2. Batch 2.1 (show run structure) — most visible to users
3. Batch 2.3 (missing commands) — completeness
4. Batch 3.1 (description) — feature
