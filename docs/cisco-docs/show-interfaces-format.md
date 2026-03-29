# Cisco IOS 15.2 - show interfaces Output Formats

Reference platform: WS-C3560CX-12PD-S running IOS 15.2(x)E
Sources: Cisco official documentation, Catalyst 3560/3560CX configuration guides

---

## 1. `show interfaces` (Full Detail Per Interface)

Displays exhaustive statistics for all interfaces, or a specific one with
`show interfaces GigabitEthernet1/0/1`.

### Status Line 1 (first line)

```
GigabitEthernet1/0/1 is up, line protocol is up (connected)
```

Possible states for the first element (Layer 1 / physical):
- `up`
- `down`
- `administratively down`

Possible states for `line protocol` (Layer 2):
- `up`
- `down`

The parenthetical `(connected)` appears on Catalyst switches (not routers) when
the interface is in connected state. For a trunk port it shows `(connected)`.

Other first-line status combinations:
```
GigabitEthernet1/0/2 is down, line protocol is down (notconnect)
GigabitEthernet1/0/3 is administratively down, line protocol is down (disabled)
GigabitEthernet1/0/4 is err-disabled, line protocol is down (err-disabled)
```

### Full Output Block for a Connected GigabitEthernet Switchport

```
GigabitEthernet1/0/1 is up, line protocol is up (connected)
  Hardware is Gigabit Ethernet, address is 0011.2233.4455 (bia 0011.2233.4455)
  MTU 1500 bytes, BW 1000000 Kbit/sec, DLY 10 usec,
     reliability 255/255, txload 1/255, rxload 1/255
  Encapsulation ARPA, loopback not set
  Keepalive set (10 sec)
  Full-duplex, 1000Mb/s, media type is 10/100/1000BaseTX
  input flow-control is off, output flow-control is unsupported
  ARP type: ARPA, ARP Timeout 04:00:00
  Last input 00:00:01, output 00:00:01, output hang never
  Last clearing of "show interface" counters never
  Input queue: 0/75/0/0 (size/max/drops/flushes); Total output drops: 0
  Queueing strategy: fifo
  Output queue: 0/40 (size/max)
  5 minute input rate 1000 bits/sec, 2 packets/sec
  5 minute output rate 2000 bits/sec, 4 packets/sec
     1000 packets input, 100000 bytes, 0 no buffer
     Received 100 broadcasts (50 multicasts)
     0 runts, 0 giants, 0 throttles
     0 input errors, 0 CRC, 0 frame, 0 overrun, 0 ignored
     0 watchdog, 50 multicast, 0 pause input
     0 input packets with dribble condition detected
     2000 packets output, 200000 bytes, 0 underruns
     0 output errors, 0 collisions, 1 interface resets
     0 unknown protocol drops
     0 babbles, 0 late collision, 0 deferred
     0 lost carrier, 0 no carrier, 0 pause output
     0 output buffer failures, 0 output buffers swapped out
```

### Notes on Key Fields

- **Hardware type**: `Gigabit Ethernet` for GigabitEthernet ports;
  `Ten Gigabit Ethernet` for TenGigabitEthernet SFP+ ports.
- **bia**: "burned-in address" — the factory MAC; shown in parentheses after
  the current (possibly overridden) MAC.
- **MTU**: default 1500 bytes on switchports.
- **BW / DLY**: GigabitEthernet = `1000000 Kbit/sec`, `10 usec`;
  TenGigabitEthernet = `10000000 Kbit/sec`, `10 usec`.
- **reliability**: 255/255 = fully reliable; uses exponential decay average.
- **txload / rxload**: 1/255 = nearly idle; 255/255 = 100% saturated.
- **media type**: `10/100/1000BaseTX` for copper RJ-45 GigabitEthernet ports;
  `SFP-10GBase-SR` or similar for SFP/SFP+ fiber; `SFP-1000BaseTX` for
  1G copper SFP; `Not Present` when no SFP is inserted.
- **flow-control**: on copper ports typically `input flow-control is off,
  output flow-control is unsupported`; on some SFP+ ports may show `desired`
  or `on`.
- **Input queue**: format is `size/max/drops/flushes`. Max is typically 75
  for GigabitEthernet.
- **Output queue**: `size/max`. Max is 40 for fifo strategy.
- **5 minute rates**: exponentially weighted average over 5 minutes.
- **Received X broadcasts (Y multicasts)**: broadcast count includes
  multicasts; the parenthetical shows how many of those were multicast.
- **dribble condition**: a frame slightly too long — not an error, just
  informational.
- **interface resets**: increments when the interface is reset (e.g.,
  negotiation failure, err-disable, manual shutdown/no shutdown).
- **unknown protocol drops**: frames received for an unrecognized protocol.
- **babbles**: transmit timeout; rare on Ethernet.
- **late collision**: collision after the first 512 bits — indicates duplex
  mismatch or cable too long.

### Administratively Down (Disabled) Interface

```
GigabitEthernet1/0/5 is administratively down, line protocol is down (disabled)
  Hardware is Gigabit Ethernet, address is 0011.2233.4459 (bia 0011.2233.4459)
  MTU 1500 bytes, BW 10000 Kbit/sec, DLY 1000 usec,
     reliability 255/255, txload 1/255, rxload 1/255
  Encapsulation ARPA, loopback not set
  Keepalive set (10 sec)
  Auto-duplex, Auto-speed, media type is 10/100/1000BaseTX
  input flow-control is off, output flow-control is unsupported
  ARP type: ARPA, ARP Timeout 04:00:00
  Last input never, output never, output hang never
  ...
```

Note: When administratively down, BW shows 10000 Kbit/sec (10 Mbps default)
and DLY shows 1000 usec, not the negotiated values.

### TenGigabitEthernet SFP+ Port Example

```
TenGigabitEthernet1/0/1 is up, line protocol is up (connected)
  Hardware is Ten Gigabit Ethernet, address is 0011.2233.44ff (bia 0011.2233.44ff)
  MTU 1500 bytes, BW 10000000 Kbit/sec, DLY 10 usec,
     reliability 255/255, txload 1/255, rxload 1/255
  Encapsulation ARPA, loopback not set
  Keepalive set (10 sec)
  Full-duplex, 10Gb/s, link type is force-up, media type is SFP-10GBase-SR
  input flow-control is off, output flow-control is off
  ARP type: ARPA, ARP Timeout 04:00:00
  ...
```

When no SFP is inserted:
```
TenGigabitEthernet1/0/2 is down, line protocol is down (notconnect)
  Hardware is Ten Gigabit Ethernet, address is 0011.2233.4500 (bia 0011.2233.4500)
  MTU 1500 bytes, BW 10000000 Kbit/sec, DLY 10 usec,
  ...
  Auto-duplex, Auto-speed, media type is Not Present
```

### VLAN Interface (SVI)

```
Vlan1 is up, line protocol is up
  Hardware is EtherSVI, address is 0011.2233.4401 (bia 0011.2233.4401)
  Internet address is 192.168.1.1/24
  MTU 1500 bytes, BW 1000000 Kbit/sec, DLY 10 usec,
     reliability 255/255, txload 1/255, rxload 1/255
  Encapsulation ARPA, loopback not set
  Keepalive not set
  ARP type: ARPA, ARP Timeout 04:00:00
  Last input 00:00:02, output 00:00:01, output hang never
  Last clearing of "show interface" counters never
  Input queue: 0/75/0/0 (size/max/drops/flushes); Total output drops: 0
  Queueing strategy: fifo
  Output queue: 0/40 (size/max)
  5 minute input rate 0 bits/sec, 0 packets/sec
  5 minute output rate 0 bits/sec, 0 packets/sec
     1234 packets input, 98765 bytes, 0 no buffer
     Received 0 broadcasts, 0 runts, 0 giants, 0 throttles
     0 input errors, 0 CRC, 0 frame, 0 overrun, 0 ignored
     5678 packets output, 43210 bytes, 0 underruns
     0 output errors, 1 interface resets
     0 unknown protocol drops
     0 output buffer failures, 0 output buffers swapped out
```

Note: SVI does not have duplex/speed/media type lines, and does not have
collision/late-collision/dribble counters.

---

## 2. `show interfaces status`

Summary table of all switchport interfaces. Accessed with:
`show interfaces status`

### Column Header Line

```
Port      Name               Status       Vlan       Duplex  Speed Type
```

### Column Details

| Column   | Width (approx) | Values                                          |
|----------|----------------|-------------------------------------------------|
| Port     | 9 chars        | `Gi1/0/1`, `Gi1/0/13`, `Te1/0/1`               |
| Name     | 18 chars       | Description string (truncated), or blank        |
| Status   | 12 chars       | `connected`, `notconnect`, `disabled`, `err-disabled` |
| Vlan     | 10 chars       | `1`–`4094`, `trunk`, `routed`                   |
| Duplex   | 6 chars        | `a-full`, `a-half`, `full`, `half`, `auto`      |
| Speed    | 6 chars        | `a-1000`, `a-100`, `a-10`, `1000`, `100`, `10`, `auto`, `a-10G` |
| Type     | variable       | `10/100/1000BaseTX`, `1000BaseSX`, `10GBase-SR`, `Not Present` |

The `a-` prefix on Duplex and Speed means **auto-negotiated** (the actual
negotiated value, not "auto" as a setting). Without `a-`, it was manually set.

### Example Output

```
Port      Name               Status       Vlan       Duplex  Speed Type
Gi1/0/1                      connected    1          a-full  a-1000 10/100/1000BaseTX
Gi1/0/2   uplink-sw2         connected    trunk      a-full  a-1000 10/100/1000BaseTX
Gi1/0/3                      notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/4                      disabled     1          auto    auto  10/100/1000BaseTX
Gi1/0/5                      err-disabled 10         auto    auto  10/100/1000BaseTX
Gi1/0/6                      connected    20         a-full  a-100  10/100/1000BaseTX
Gi1/0/7                      notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/8                      notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/9                      notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/10                     notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/11                     notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/12                     notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/13                     notconnect   1          auto    auto  10/100/1000BaseTX
Gi1/0/14                     notconnect   1          auto    auto  10/100/1000BaseTX
Te1/0/1                      notconnect   1          auto    auto  Not Present
Te1/0/2                      notconnect   1          auto    auto  Not Present
```

For the WS-C3560CX-12PD-S: Gi1/0/1–Gi1/0/12 are PoE+ copper ports,
Gi1/0/13 and Gi1/0/14 are non-PoE copper uplink ports, Te1/0/1 and Te1/0/2
are the SFP+ 10G uplink slots.

### Interface Name Abbreviations in `show interfaces status`

The command uses short abbreviations in the Port column:
- `GigabitEthernet1/0/1` → `Gi1/0/1`
- `TenGigabitEthernet1/0/1` → `Te1/0/1`
- `Vlan1` → `Vl1` (not shown in `show interfaces status` — SVIs are omitted)

---

## 3. `show interfaces GigabitEthernetX/Y/Z` (Per-Interface Detail)

Same format as the full `show interfaces` output shown in section 1, but
limited to one interface. Example:

```
Switch# show interfaces GigabitEthernet1/0/1
GigabitEthernet1/0/1 is up, line protocol is up (connected)
  Hardware is Gigabit Ethernet, address is 0011.2233.4455 (bia 0011.2233.4455)
  ...
```

The full name `GigabitEthernet1/0/1` (or abbreviated `gi1/0/1`, `gi 1/0/1`,
`Gi1/0/1`) is accepted on input. The output always uses the full name
`GigabitEthernet1/0/1`.

---

## 4. `show interfaces trunk`

Displays only trunk interfaces. Has four sections separated by blank lines.

```
Port        Mode             Encapsulation  Status        Native vlan
Gi1/0/2     on               802.1q         trunking      1
Te1/0/1     on               802.1q         trunking      1

Port        Vlans allowed on trunk
Gi1/0/2     1-4094
Te1/0/1     1-4094

Port        Vlans allowed and active in management domain
Gi1/0/2     1,10,20,30
Te1/0/1     1,10,20,30

Port        Vlans in spanning tree forwarding state and not pruned
Gi1/0/2     1,10,20,30
Te1/0/1     1,10,20
```

### Column Widths (Section 1)

| Column        | Width    | Values                                         |
|---------------|----------|------------------------------------------------|
| Port          | 11 chars | `Gi1/0/1`, `Te1/0/1`                           |
| Mode          | 16 chars | `on`, `off`, `desirable`, `auto`, `nonegotiate` |
| Encapsulation | 14 chars | `802.1q`, `isl`, `negotiate`                   |
| Status        | 13 chars | `trunking`, `not-trunking`                     |
| Native vlan   | variable | VLAN number (default 1)                        |

When no trunk ports exist: the command returns no output or a blank line.

---

## 5. `show interfaces switchport`

Displays per-interface Layer 2 switchport detail. Can be scoped to one
interface with `show interfaces GigabitEthernet1/0/1 switchport`.

### Access Port Example

```
Name: Gi1/0/1
Switchport: Enabled
Administrative Mode: static access
Operational Mode: static access
Administrative Trunking Encapsulation: dot1q
Operational Trunking Encapsulation: native
Negotiation of Trunking: Off
Access Mode VLAN: 10 (VLAN0010)
Trunking Native Mode VLAN: 1 (default)
Administrative Native VLAN tagging: enabled
Voice VLAN: none
Administrative private-vlan host-association: none
Administrative private-vlan mapping: none
Administrative private-vlan trunk native VLAN: none
Administrative private-vlan trunk Native VLAN tagging: enabled
Administrative private-vlan trunk encapsulation: dot1q
Administrative private-vlan trunk normal VLANs: none
Administrative private-vlan trunk associations: none
Administrative private-vlan trunk mappings: none
Operational private-vlan: none
Trunking VLANs Enabled: ALL
Pruning VLANs Enabled: 2-1001
Capture Mode Disabled
Capture VLANs Allowed: ALL
Protected: false
Unknown unicast blocked: disabled
Unknown multicast blocked: disabled
Appliance trust: none
```

### Trunk Port Example

```
Name: Gi1/0/2
Switchport: Enabled
Administrative Mode: trunk
Operational Mode: trunk
Administrative Trunking Encapsulation: dot1q
Operational Trunking Encapsulation: dot1q
Negotiation of Trunking: On
Access Mode VLAN: 1 (default)
Trunking Native Mode VLAN: 1 (default)
Administrative Native VLAN tagging: enabled
Voice VLAN: none
Administrative private-vlan host-association: none
Administrative private-vlan mapping: none
Administrative private-vlan trunk native VLAN: none
Administrative private-vlan trunk Native VLAN tagging: enabled
Administrative private-vlan trunk encapsulation: dot1q
Administrative private-vlan trunk normal VLANs: none
Administrative private-vlan trunk associations: none
Administrative private-vlan trunk mappings: none
Operational private-vlan: none
Trunking VLANs Enabled: 8,170
Pruning VLANs Enabled: 2-1001
Capture Mode Disabled
Capture VLANs Allowed: ALL
Protected: false
Unknown unicast blocked: disabled
Unknown multicast blocked: disabled
Appliance trust: none
```

### Key Field Values

| Field                              | Access Port         | Trunk Port           |
|------------------------------------|---------------------|----------------------|
| Administrative Mode                | `static access`     | `trunk`              |
| Operational Mode                   | `static access`     | `trunk`              |
| Administrative Trunking Encapsulation | `dot1q` or `negotiate` | `dot1q`         |
| Operational Trunking Encapsulation | `native`            | `dot1q`              |
| Negotiation of Trunking            | `Off`               | `On`                 |
| Access Mode VLAN                   | VLAN number (name)  | `1 (default)`        |
| Voice VLAN                         | `none` or `X (name)` | `none`              |

---

## 6. `show interfaces description`

Displays a table of all interfaces with their descriptions, status, and
protocol state.

```
Interface                      Status         Protocol Description
Gi1/0/1                        up             up       server-room-mgmt
Gi1/0/2                        up             up       uplink-to-core
Gi1/0/3                        down           down
Gi1/0/4                        admin down     down     unused
Gi1/0/5                        down           down
Gi1/0/6                        up             up       AP-office-1
Gi1/0/7                        down           down
Gi1/0/8                        down           down
Gi1/0/9                        down           down
Gi1/0/10                       down           down
Gi1/0/11                       down           down
Gi1/0/12                       down           down
Gi1/0/13                       down           down
Gi1/0/14                       down           down
Te1/0/1                        up             up       uplink-isr4451
Te1/0/2                        down           down
Vl1                            up             up       management
```

### Column Layout

| Column      | Width    | Values                                          |
|-------------|----------|-------------------------------------------------|
| Interface   | 30 chars | Full short form: `Gi1/0/1`, `Te1/0/1`, `Vl1`   |
| Status      | 14 chars | `up`, `down`, `admin down`                      |
| Protocol    | 8 chars  | `up`, `down`                                    |
| Description | variable | Free-text string, or blank if not configured    |

Notes:
- Status `admin down` corresponds to `administratively down` in `show interfaces`.
- Interfaces with no description configured show a blank Description field.
- All interfaces including SVIs (Vlan interfaces), loopbacks, and port-channels
  appear in the output.
- The interface names use the same short form as `show interfaces status`.

---

## 7. Interface Name Abbreviations — Context Summary

| Full Name                 | In `show int status` | In `show int desc` | In `show int trunk` | Config Input (accepted) |
|---------------------------|----------------------|--------------------|---------------------|-------------------------|
| GigabitEthernet1/0/1      | Gi1/0/1              | Gi1/0/1            | Gi1/0/1             | gi1/0/1, Gi1/0/1, GigabitEthernet1/0/1 |
| TenGigabitEthernet1/0/1   | Te1/0/1              | Te1/0/1            | Te1/0/1             | te1/0/1, Te1/0/1, TenGigabitEthernet1/0/1 |
| Vlan1                     | (not shown)          | Vl1                | (not shown)         | vlan1, Vlan1            |
| Loopback0                 | (not shown)          | Lo0                | (not shown)         | lo0, Loopback0          |

In the full `show interfaces` output (verbose), the complete name is always
used on the first line: `GigabitEthernet1/0/1`, `TenGigabitEthernet1/0/1`.

---

## 8. Status Values Reference

### Layer 1 / Physical Status

| Status               | Meaning                                           |
|----------------------|---------------------------------------------------|
| `up`                 | Physical signal detected, no shutdown             |
| `down`               | No physical signal (no cable or no link partner)  |
| `administratively down` | `shutdown` configured on interface            |
| `err-disabled`       | Port disabled by error recovery mechanism (BPDU guard, port-security, etc.) |

### Line Protocol Status

| Status  | Meaning                                                  |
|---------|----------------------------------------------------------|
| `up`    | Layer 2 keepalives working, interface fully operational  |
| `down`  | Layer 2 not established (physical down, or protocol issue) |

### `show interfaces status` Status Column

| Value         | Meaning                                                   |
|---------------|-----------------------------------------------------------|
| `connected`   | Port is up, link partner detected, forwarding             |
| `notconnect`  | Port is up administratively but no link partner           |
| `disabled`    | Port has been `shutdown` (administratively down)          |
| `err-disabled`| Port disabled by error recovery (BPDU guard, storm control, etc.) |
| `inactive`    | Access VLAN does not exist or is suspended                |

---

## 9. `show interfaces` — Indentation Convention

Lines 2 onwards are indented with two spaces:
```
GigabitEthernet1/0/1 is up, line protocol is up (connected)
  Hardware is Gigabit Ethernet, address is ...
  MTU 1500 bytes, BW 1000000 Kbit/sec, DLY 10 usec,
     reliability 255/255, txload 1/255, rxload 1/255
```

The `reliability/txload/rxload` continuation line uses 5-space indentation
(aligned after the comma on the preceding line).

The counter lines within the statistics block use 5-space indentation:
```
     1000 packets input, 100000 bytes, 0 no buffer
     Received 100 broadcasts (50 multicasts)
     0 runts, 0 giants, 0 throttles
```
