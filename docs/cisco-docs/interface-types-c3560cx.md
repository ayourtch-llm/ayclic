# Cisco WS-C3560CX-12PD-S — Interface Types and Numbering

Platform: WS-C3560CX-12PD-S, Cisco Catalyst 3560-CX Series
Software: Cisco IOS 15.2(x)E

Sources: Cisco Catalyst 3560-CX Data Sheet, Hardware Installation Guide,
Cisco Community forums (interface numbering), official configuration guides.

---

## 1. Physical Port Layout

The WS-C3560CX-12PD-S has 16 physical ports:

| Port Group           | Count | Interface Names           | Description                        |
|----------------------|-------|---------------------------|------------------------------------|
| PoE+ copper downlinks | 12   | GigabitEthernet1/0/1–12  | 10/100/1000 RJ-45, PoE+ 802.3at   |
| Copper uplinks (non-PoE) | 2 | GigabitEthernet1/0/13–14 | 10/100/1000 RJ-45, no PoE          |
| SFP+ uplinks          | 2    | TenGigabitEthernet1/0/1–2 | 10G SFP+ slots (also accept 1G SFP) |

Total physical ports: 16 (12 + 2 + 2)

---

## 2. GigabitEthernet Ports (1/0/1 through 1/0/14)

### Downlink Ports (1/0/1 – 1/0/12)

These are the twelve PoE+ capable ports on the left side of the front panel.

- Interface names: `GigabitEthernet1/0/1` through `GigabitEthernet1/0/12`
- Speed: 10/100/1000 Mbps, auto-negotiation by default
- Duplex: auto-negotiation by default
- PoE: IEEE 802.3at PoE+ (up to 30W per port), combined budget 240W
- Media type: `10/100/1000BaseTX` (copper RJ-45)
- Default state: no shutdown (administratively up), but `notconnect` until
  a cable with active link partner is connected

### Uplink Ports (1/0/13 – 1/0/14)

These are the two non-PoE copper RJ-45 uplink ports on the right side of the
front panel, adjacent to the SFP+ slots.

- Interface names: `GigabitEthernet1/0/13` and `GigabitEthernet1/0/14`
- Speed: 10/100/1000 Mbps, auto-negotiation by default
- Duplex: auto-negotiation by default
- PoE: None (no Power over Ethernet capability)
- Media type: `10/100/1000BaseTX` (copper RJ-45)
- Default state: no shutdown, `notconnect` until link detected

---

## 3. TenGigabitEthernet Ports (1/0/1 – 1/0/2)

These are the two SFP+ slots on the right side of the front panel.

- Interface names: `TenGigabitEthernet1/0/1` and `TenGigabitEthernet1/0/2`
- Speed: 10 Gbps when SFP+ transceiver inserted; 1 Gbps when 1G SFP inserted
- Media type: depends on transceiver (e.g., `SFP-10GBase-SR`, `SFP-1000BaseSX`,
  `Not Present` when empty)
- Default state: no shutdown, `notconnect` when empty (no SFP inserted)

### Important: Dual Personality of SFP+ Slots

The SFP+ uplink slots on the WS-C3560CX-12PD-S have dual personalities:

- When a **10G SFP+** transceiver is inserted: use `TenGigabitEthernet1/0/1`
  or `TenGigabitEthernet1/0/2`
- When a **1G SFP** (Gigabit SFP, not SFP+) transceiver is inserted: use
  `GigabitEthernet1/0/15` or `GigabitEthernet1/0/16`

This means the switch has virtual interface names `GigabitEthernet1/0/15`
and `GigabitEthernet1/0/16` that map to the same physical SFP+ slots as
`TenGigabitEthernet1/0/1` and `TenGigabitEthernet1/0/2`. Only one of the
paired names is active at a time, depending on the transceiver type installed.

These GigabitEthernet1/0/15 and GigabitEthernet1/0/16 interfaces do not have
a physical connector of their own — they exist only when a 1G SFP module is
present in the corresponding SFP+ slot.

### Transceiver Type to Interface Name Mapping

| Slot     | SFP+ (10G) transceiver  | 1G SFP transceiver        |
|----------|-------------------------|---------------------------|
| Slot 1   | TenGigabitEthernet1/0/1 | GigabitEthernet1/0/15     |
| Slot 2   | TenGigabitEthernet1/0/2 | GigabitEthernet1/0/16     |

---

## 4. Virtual Interfaces

In addition to physical interfaces, the 3560CX-12PD-S supports:

### VLAN Interfaces (SVIs — Switched Virtual Interfaces)

- Interface names: `Vlan1`, `Vlan10`, `Vlan100`, etc.
- Up to 1024 VLANs can be created; SVIs are created as needed for Layer 3
- Default: `Vlan1` exists by default, `no shutdown` by default
- Used for in-band management and inter-VLAN routing

### Loopback Interfaces

- Interface names: `Loopback0`, `Loopback1`, etc.
- Software-only; always up as long as not shut down
- Not visible in `show interfaces status` by default

### Null Interface

- `Null0` — always present; used for black-hole routing
- Cannot be shut down

### Port-Channel Interfaces

- Interface names: `Port-channel1` through `Port-channel48`
- Created when configuring EtherChannel (LACP or PAgP)
- Shown as `Po1` in abbreviated output

---

## 5. Interface Numbering Scheme

All interfaces use the format: `<type><slot>/<module>/<port>`

For the WS-C3560CX-12PD-S (a standalone, non-stackable switch):
- **slot** = 1 (always 1 for this switch)
- **module** = 0 (always 0)
- **port** = 1–16 for GigabitEthernet; 1–2 for TenGigabitEthernet

This gives:
- `GigabitEthernet1/0/1` through `GigabitEthernet1/0/14` (physical copper)
- `GigabitEthernet1/0/15` and `GigabitEthernet1/0/16` (virtual, 1G SFP mode)
- `TenGigabitEthernet1/0/1` and `TenGigabitEthernet1/0/2` (physical SFP+)

---

## 6. Default Interface States

### All GigabitEthernet and TenGigabitEthernet Ports

Out of the box (factory default or after `write erase` / `erase startup-config`):
- `no shutdown` — interfaces are administratively up
- Access mode VLAN 1 (switchport access vlan 1)
- Switchport mode: dynamic desirable (3560 default) or dynamic auto
- Speed: auto
- Duplex: auto
- PoE: enabled on PoE-capable ports

Since no cable is connected, status in `show interfaces status` shows:
```
Gi1/0/1   notconnect   1    auto    auto  10/100/1000BaseTX
```

After `shutdown` is issued, status shows:
```
Gi1/0/4   disabled     1    auto    auto  10/100/1000BaseTX
```

### Vlan1

- Exists by default
- `no shutdown` by default
- No IP address assigned by default (shows `unassigned` in `show ip int brief`)
- Status: `up` if at least one port in VLAN 1 is connected; `down` if no
  ports in VLAN 1 are active

---

## 7. `show version` Interface Count Lines

The `show version` command reports the interface counts near the bottom of
the output. For a WS-C3560CX-12PD-S the relevant lines are:

```
Switch Ports Model              SW Version        SW Image
------ ----- -----              ----------        ----------
*    1 16    WS-C3560CX-12PD-S  15.2(7)E2         C3560CX-UNIVERSALK9-M
```

And in the hardware section:
```
16 Gigabit Ethernet interfaces
2 Ten Gigabit Ethernet interfaces
3 Virtual Ethernet interfaces
```

### Explanation of Interface Counts

| Line                               | Count | Notes                                           |
|------------------------------------|-------|-------------------------------------------------|
| `Gigabit Ethernet interfaces`      | 16    | GigabitEthernet1/0/1–14 (physical) plus 1/0/15 and 1/0/16 (virtual 1G SFP mode) |
| `Ten Gigabit Ethernet interfaces`  | 2     | TenGigabitEthernet1/0/1 and 1/0/2              |
| `Virtual Ethernet interfaces`      | 3     | Vlan1 (default) plus any other SVIs created; count increases as more SVIs are added |

Note: The `16 Gigabit Ethernet interfaces` count includes the 14 physical
copper ports plus the 2 virtual GigabitEthernet1/0/15 and 1/0/16 interfaces
that become active when 1G SFP modules are inserted. The count `3 Virtual
Ethernet interfaces` typically refers to active SVIs (Vlan interfaces).

### Full `show version` Output (Representative)

```
Cisco IOS Software, C3560CX Software (C3560CX-UNIVERSALK9-M), Version 15.2(7)E2, RELEASE SOFTWARE (fc4)
Technical Support: http://www.cisco.com/techsupport
Copyright (c) 1986-2019 by Cisco Systems, Inc.
Compiled Wed 20-Nov-19 07:59 by prod_rel_team

ROM: Bootstrap program is C3560CX boot loader
BOOTLDR: C3560CX Boot Loader (C3560CX-HBOOT-M) Version 15.2(7r)E, RELEASE SOFTWARE (fc1)

Switch uptime is 5 weeks, 2 days, 14 hours, 22 minutes
System returned to ROM by power-on
System restarted at 09:15:32 UTC Mon Jan 27 2025
System image file is "flash:c3560cx-universalk9-mz.152-7.E2.bin"
Last reload reason: power-on


This product contains cryptographic features and is subject to United
States and local country laws governing import, export, transfer and
use. Delivery of Cisco cryptographic products does not imply
third-party authority to import, export, distribute or use encryption.
Importers, exporters, distributors and users are responsible for
compliance with U.S. and local country laws. By using this product you
agree to comply with applicable laws and regulations. If you are unable
to comply with U.S. and local country laws, return this product
immediately.

A summary of U.S. laws governing Cisco cryptographic products may be found at:
http://www.cisco.com/wwl/export/crypto/tool/stqrg.html

If you require further assistance please contact us by sending email to
export@cisco.com.

cisco WS-C3560CX-12PD-S (APM86XXX) processor (revision A0) with 524288K bytes of memory.
Processor board ID FOC2046X0YZ
Last reset from power-on
1 Virtual Ethernet interface
16 Gigabit Ethernet interfaces
2 Ten Gigabit Ethernet interfaces
The password-recovery mechanism is enabled.

512K bytes of flash-simulated non-volatile configuration memory.
Base ethernet MAC Address       : 00:11:22:33:44:00
Motherboard assembly number     : 73-16471-03
Power supply part number        : 341-0437-01
Motherboard serial number       : FOC20460YZ1
Power supply serial number      : LIT20463XYZ
Model revision number           : A0
Motherboard revision number     : A0
Model number                    : WS-C3560CX-12PD-S
Daughterboard assembly number   : 73-16472-02
Daughterboard serial number     : FOC20460YZ2
System serial number            : FOC2046X0YZ
Top Assembly Part Number        : 68-5671-03
Top Assembly Revision Number    : A0
Version ID                      : V01
CLEI Code Number                : CMM1200ARA
Hardware Board Revision Number  : 0x04


Switch Ports Model              SW Version        SW Image
------ ----- -----              ----------        ----------
*    1 16    WS-C3560CX-12PD-S  15.2(7)E2         C3560CX-UNIVERSALK9-M


Configuration register is 0xF
```

Notes on `show version` output:
- `16` under Switch Ports reflects the total number of physical interface
  slots (14 copper + 2 SFP+).
- The `1 Virtual Ethernet interface` line at the top reflects the count of
  currently configured SVIs. If Vlan1 is the only SVI, it shows `1`. As more
  `interface Vlan` entries are created in config, this count increases.
- The `16 Gigabit Ethernet interfaces` and `2 Ten Gigabit Ethernet interfaces`
  lines reflect the total capacity including the dual-personality SFP+ slots.

---

## 8. Interface Type in `show interfaces`

The `Hardware is` line in `show interfaces` output differs by interface type:

| Interface Type         | Hardware Line                          |
|------------------------|----------------------------------------|
| GigabitEthernet (copper) | `Hardware is Gigabit Ethernet`       |
| TenGigabitEthernet (SFP+) | `Hardware is Ten Gigabit Ethernet` |
| Vlan (SVI)             | `Hardware is EtherSVI`                 |
| Loopback               | `Hardware is Loopback`                 |
| Port-channel           | `Hardware is EtherChannel`             |

---

## 9. PoE Budget Information

Visible in `show power inline`:

```
Switch# show power inline
Available:240.0(w)  Used:45.0(w)  Remaining:195.0(w)

Interface Admin  Oper       Power   Device              Class Max
                            (Watts)
--------- ------ ---------- ------- ------------------- ----- ----
Gi1/0/1   auto   on         15.4    Cisco AP            3     30.0
Gi1/0/2   auto   on         15.4    Cisco IP Phone      3     30.0
...
Gi1/0/12  auto   off        0.0     n/a                 n/a   30.0
Gi1/0/13  off    off        0.0     n/a                 n/a   n/a
Gi1/0/14  off    off        0.0     n/a                 n/a   n/a
```

Ports Gi1/0/13 and Gi1/0/14 show `n/a` for PoE since they are non-PoE ports.
Ports Te1/0/1 and Te1/0/2 are not shown in `show power inline` (SFP+ ports
do not support PoE).

---

## 10. Interface Range Syntax

The `interface range` command accepts:
```
Switch(config)# interface range GigabitEthernet1/0/1 - 12
Switch(config)# interface range GigabitEthernet1/0/1 - 14
Switch(config)# interface range TenGigabitEthernet1/0/1 - 2
Switch(config)# interface range GigabitEthernet1/0/1 - 12 , TenGigabitEthernet1/0/1 - 2
```

The macro form (requires pre-defined macro):
```
Switch(config)# define interface-range ACCESS_PORTS GigabitEthernet1/0/1 - 12
Switch(config)# interface range macro ACCESS_PORTS
```
