# Cisco IOS 15.2 Show Commands Reference
## Platform: Catalyst 3560-CX Series Switches

This document details all `show` subcommands available in privileged EXEC mode on
Cisco IOS 15.2 for the Catalyst 3560-CX. The `show` command is used to display
system state, configuration, and statistics.

---

## Complete `show ?` List

```
Switch# show ?
  aaa                     Show AAA values
  access-expression       List access expression
  access-lists            List access lists
  adjacency               Adjacent nodes
  aliases                 Display alias commands
  archive                 Archive activity information
  arp                     ARP table
  authentication          Auth Manager information
  auto                    Show Automation Template
  boot                    Boot attributes
  buffers                 Buffer pool statistics
  cdp                     CDP information
  class-map               Show QoS Class Map
  clock                   Display the system clock
  cluster                 Show cluster information
  cns                     CNS subsystem information
  controllers             Interface controller status
  copp                    Control Plane Policing (CoPP)
  copyright               Display copyright information
  crypto                  Encryption module
  current-boot            Current boot attributes
  debugging               State of each debugging option
  dhcp                    Dynamic Host Configuration Protocol status
  diagnostic              Show diagnostic information
  dot1q-tunnel            802.1Q Tunnel Information
  dot1x                   IEEE 802.1X
  dtp                     DTP information
  eap                     Show EAP information
  energywise              Show EnergyWise information
  env                     Show environmental sensor information
  errdisable              Error disabled mechanism
  etherchannel            EtherChannel information
  event                   Event information
  fallback                Fallback information
  file                    Show filesystem information
  flash:                  display information about flash: file system
  flow                    Flow information
  hardware                Hardware information
  history                 Display the session command history
  hosts                   IP domain-name, lookup style, nameservers, and host table
  idprom                  IDPROM information
  interfaces              Interface status and configuration
  inventory               Show the physical inventory
  ip                      IP information
  ipv6                    IPv6 information
  isis                    IS-IS information
  key                     Key information
  lacp                    LACP information
  license                 Show license information
  line                    TTY line information
  lldp                    LLDP information
  location                Display the system location
  logging                 Show the contents of logging buffers
  mac                     MAC configuration
  mac address-table       MAC forwarding table
  memory                  Memory statistics
  mls                     Show MultiLayer Switching information
  module                  Show module information
  monitor                 Show monitor information
  mroute                  IP multicast routing table
  mrm                     IP Multicast Routing Monitor
  ntp                     Network time protocol
  onep                    ONEP information
  pagp                    PAgP information
  parser                  Show parser information
  platform                Show platform information
  policy-map              Show QoS Policy Map
  port-security           Secure port information
  power                   Show power information
  privilege               Show current privilege level
  processes               Active process statistics
  queue                   Show queue contents
  radius                  RADIUS server
  registry                Function registry information
  reload                  Reload system information
  rmon                    rmon statistics
  route-map               route-map information
  running-config          Current operating configuration
  sdm                     SDM configuration
  sessions                Information about Telnet connections
  snmp                    snmp statistics
  spanning-tree           Spanning tree topology
  ssh                     Information about SSH
  startup-config          Contents of startup configuration
  storm-control           Storm control information
  system                  Show system information
  tacacs                  Shows TACACS+ server statistics
  tech-support            Show system information for Tech-Support
  terminal                Display terminal configuration parameters
  udld                    UDLD status
  users                   Display information about terminal lines
  version                 System hardware and software status
  vlan                    VTP VLAN status
  vmps                    VMPS information
  vtp                     VTP information
```

---

## Detailed Show Command Reference

### `show aaa`
Display AAA (Authentication, Authorization, Accounting) information.

```
Switch# show aaa ?
  dead-criteria  Dead Criteria information for AAA servers
  local          Show Local Method Options
  method-lists   Method-list information
  servers        AAA server information
  sessions       AAA sessions information
  subsys         AAA Subsystem information
  user           Show info about a particular aaa user
```

### `show access-lists`
Display all configured access control lists.

```
Switch# show access-lists
Switch# show access-lists 1
Switch# show access-lists MYACL
```

Example output:
```
Switch# show access-lists
Standard IP access list 1
    10 permit 192.168.1.0, wildcard bits 0.0.0.255
Extended IP access list 100
    10 permit tcp 192.168.0.0 0.0.255.255 any eq 80
    20 deny ip any any
```

### `show arp`
Display the ARP (Address Resolution Protocol) cache.

```
Switch# show arp
```

Example output:
```
Switch# show arp
Protocol  Address          Age (min)  Hardware Addr   Type   Interface
Internet  192.168.1.1             -   0011.2233.4455  ARPA   Vlan1
Internet  192.168.1.100          12   aabb.ccdd.eeff  ARPA   Vlan1
Internet  192.168.1.254           5   0000.1111.2222  ARPA   Vlan1
```

Fields:
- **Protocol**: Network protocol (Internet for IPv4)
- **Address**: IP address
- **Age (min)**: Minutes since ARP entry was last used; `-` means local address
- **Hardware Addr**: MAC address
- **Type**: Encapsulation type (ARPA for Ethernet)
- **Interface**: Interface the host is reachable on

### `show boot`
Display boot attributes and boot image settings.

```
Switch# show boot
BOOT path-list      : flash:c3560cx-universalk9-mz.150-2.SE11/c3560cx-universalk9-mz.150-2.SE11.bin
Config file         : flash:/config.text
Private Config file : flash:/private-config.text
Enable Break        : yes
Manual Boot         : no
HELPER path-list    :
Auto upgrade        : yes
Auto upgrade path   :
NVRAM/Config file
      buffer size:   524288
Timeout for Config
           Download:    0 seconds
Config Download
       via DHCP:       disabled (next boot: disabled)
```

### `show cdp`
Display CDP (Cisco Discovery Protocol) information.

```
Switch# show cdp ?
  entry        Information for specific neighbor entry
  interface    CDP interface status and configuration
  neighbors    CDP neighbor entries
  traffic      CDP statistics
```

**`show cdp neighbors`**:
```
Switch# show cdp neighbors
Capability Codes: R - Router, T - Trans Bridge, B - Source Route Bridge
                  S - Switch, H - Host, I - IGMP, r - Repeater, P - Phone,
                  D - Remote, C - CVTA, M - Two-port Mac Relay

Device ID        Local Intrfce     Holdtme    Capability  Platform  Port ID
Router1          Gig 0/1           132           R S I     ISR4321   Gig 0/0/1
Switch2          Gig 0/2           156             S I     WS-C2960  Gig 0/1

Total cdp entries displayed : 2
```

**`show cdp neighbors detail`**:
```
Switch# show cdp neighbors detail
-------------------------
Device ID: Router1
Entry address(es):
  IP address: 192.168.1.1
Platform: Cisco ISR4321,  Capabilities: Router Switch IGMP
Interface: GigabitEthernet0/1,  Port ID (outgoing port): GigabitEthernet0/0/1
Holdtime : 132 sec

Version :
Cisco IOS Software, Version 15.4(3)M, RELEASE SOFTWARE (fc2)
...
advertisement version: 2
VTP Management Domain: ''
Native VLAN: 1
Duplex: full
Management address(es):
  IP address: 192.168.1.1
```

Columns in `show cdp neighbors`:
- **Device ID**: Hostname of the neighboring device
- **Local Intrfce**: Local interface connected to the neighbor
- **Holdtme**: Time (seconds) before entry expires if no new CDP update received
- **Capability**: Type of device (R=Router, S=Switch, etc.)
- **Platform**: Hardware model of neighbor
- **Port ID**: Neighbor's interface name connected to us

### `show clock`
Display the current system clock.

```
Switch# show clock
*14:30:45.123 UTC Mon Mar 25 2024
```

The `*` indicates the time is not authoritative (not synchronized with NTP).
Without `*`, the time is considered authoritative.

```
Switch# show clock detail
*14:30:45.123 UTC Mon Mar 25 2024
Time source is NTP
```

### `show controllers`
Display interface controller hardware status.

```
Switch# show controllers ?
  ethernet-controller  Show ethernet controller status
  GigabitEthernet      GigabitEthernet IEEE 802.3z
  TenGigabitEthernet   TenGigabitEthernet IEEE 802.3
```

### `show debugging`
Display all currently enabled debugging flags.

```
Switch# show debugging
```

### `show dhcp`
Display DHCP information.

```
Switch# show dhcp ?
  lease    DHCP lease information
  server   DHCP server information
```

For DHCP pool information (if the switch acts as DHCP server):
```
Switch# show ip dhcp pool
Switch# show ip dhcp binding
Switch# show ip dhcp statistics
```

### `show dot1x`
Display IEEE 802.1X port authentication information.

```
Switch# show dot1x ?
  all         Show all 802.1X information
  interface   802.1X information for a specific interface
  statistics  802.1X statistics
```

Example:
```
Switch# show dot1x all
Sysauthcontrol              Enabled
Dot1x Protocol Version      3
```

### `show dtp`
Display DTP (Dynamic Trunking Protocol) information.

```
Switch# show dtp
Global DTP information
  Sending DTP Hello packets every 30 seconds
  Dynamic Trunk timeout is 300 seconds
  5 interfaces using DTP
```

### `show environment`
Display environmental sensor information (temperature, power supply status).

```
Switch# show environment
Switch# show environment all
Switch# show environment temperature
```

### `show errdisable`
Display error-disabled port information.

```
Switch# show errdisable ?
  detect     Error disabled detect
  flap-values  Error disabled flap setting values
  information  Error disabled information
  recovery   Error disabled recovery timer values
```

Example:
```
Switch# show errdisable recovery
ErrDisable Reason            Timer Status   Timer Interval
-----------------            -------------- --------------
arp-inspection               Disabled        300
bpduguard                    Enabled         300
channel-misconfig            Disabled        300
dhcp-rate-limit              Disabled        300
dtp-flap                     Disabled        300
flap-setting                 Disabled        300
gbic-invalid                 Disabled        300
l2ptguard                    Disabled        300
link-flap                    Disabled        300
loopback                     Disabled        300
pagp-flap                    Disabled        300
port-mode-failure            Disabled        300
psecure-violation            Disabled        300
security-violation           Disabled        300
sfp-config-mismatch          Disabled        300
storm-control                Disabled        300
udld                         Disabled        300
unicast-flood                Disabled        300
vmps                         Disabled        300
```

### `show etherchannel`
Display EtherChannel (port-channel / link aggregation) information.

```
Switch# show etherchannel ?
  <1-48>     Channel group number
  detail     Detail information
  load-balance  Load-balance/frame-distribution scheme among ports in port-channel
  port       Port information
  port-channel  Port-channel information
  protocol   protocol information
  summary    One-line summary per channel-group
```

Example `show etherchannel summary`:
```
Switch# show etherchannel summary
Flags:  D - down        P - bundled in port-channel
        I - stand-alone s - suspended
        H - Hot-standby (LACP only)
        R - Layer3      S - Layer2
        U - in use      N - not in use, no aggregation
        f - failed to allocate aggregator

        M - not in use, minimum links not met
        u - unsuitable for bundling
        w - waiting to be aggregated
        d - default port


Number of channel-groups in use: 1
Number of aggregators:           1

Group  Port-channel  Protocol    Ports
------+-------------+-----------+-----------------------------------------------
1      Po1(SU)         LACP      Gi0/1(P)    Gi0/2(P)
```

### `show interfaces`
Display detailed interface status and configuration. This is one of the most-used commands.

```
Switch# show interfaces ?
  GigabitEthernet       GigabitEthernet IEEE 802.3z
  TenGigabitEthernet    TenGigabitEthernet IEEE 802.3
  Vlan                  Catalyst Vlans
  counters              Show interface counters
  description           Interface name and description
  etherchannel          Ethernet channel information
  fastethernet          FastEthernet IEEE 802.3
  flowcontrol           per port flow control information
  link                  Show interface link status
  pruning               Show interface VTP pruning information
  status                Show interface status
  summary               Show interface summary
  switchport            Show interface switchport information
  trunk                 Show interface trunk information
```

**`show interfaces GigabitEthernet0/1`** (typical output):
```
GigabitEthernet0/1 is up, line protocol is up (connected)
  Hardware is Gigabit Ethernet, address is 0011.2233.4455 (bia 0011.2233.4455)
  Description: Uplink to Router
  MTU 1500 bytes, BW 1000000 Kbit/sec, DLY 10 usec,
     reliability 255/255, txload 1/255, rxload 1/255
  Encapsulation ARPA, loopback not set
  Keepalive set (10 sec)
  Full-duplex, 1000Mb/s, media type is 10/100/1000BaseTX
  input flow-control is off, output flow-control is unsupported
  ARP type: ARPA, ARP Timeout 04:00:00
  Last input 00:00:01, output 00:00:00, output hang never
  Last clearing of "show interface" counters never
  Input queue: 0/75/0/0 (size/max/drops/flushes); Total output drops: 0
  Queueing strategy: fifo
  Output queue: 0/40 (size/max)
  5 minute input rate 1000 bits/sec, 1 packets/sec
  5 minute output rate 2000 bits/sec, 2 packets/sec
     12345 packets input, 1234567 bytes, 0 no buffer
     Received 1234 broadcasts (1000 multicasts)
     0 runts, 0 giants, 0 throttles
     0 input errors, 0 CRC, 0 frame, 0 overrun, 0 ignored
     0 watchdog, 1000 multicast, 0 pause input
     0 input packets with dribble condition detected
     23456 packets output, 2345678 bytes, 0 underruns
     0 output errors, 0 collisions, 1 interface resets
     0 unknown protocol drops
     0 babbles, 0 late collision, 0 deferred
     0 lost carrier, 0 no carrier, 0 pause output
     0 output buffer failures, 0 output buffers swapped out
```

Field descriptions:
- **is up/down**: Physical layer status (is the cable plugged in and signal detected)
- **line protocol is up/down**: Data link layer status
- **Hardware is**: Interface hardware type
- **address is**: Current MAC address; **bia** = burned-in (factory) address
- **MTU**: Maximum Transmission Unit in bytes
- **BW**: Bandwidth in Kbit/sec
- **DLY**: Propagation delay in microseconds
- **reliability**: Link reliability (255/255 = 100%)
- **txload/rxload**: Transmit/receive load (1/255 = near 0%)
- **Encapsulation**: Layer 2 encapsulation type
- **Full-duplex/Half-duplex**: Duplex setting
- **Last input/output**: Time since last packet was received/sent
- **Input queue**: size/max/drops/flushes
- **5 minute input/output rate**: Average throughput
- **runts**: Frames smaller than 64 bytes (usually collision fragments)
- **giants**: Frames larger than maximum size
- **CRC**: Frames with cyclic redundancy check errors
- **input errors**: Total of all input error types

**`show interfaces status`** (brief table format):
```
Switch# show interfaces status
Port      Name               Status       Vlan       Duplex  Speed Type
Gi0/1     Uplink             connected    trunk      a-full a-1000 10/100/1000BaseTX
Gi0/2                        connected    1          a-full a-1000 10/100/1000BaseTX
Gi0/3                        notconnect   1            auto   auto 10/100/1000BaseTX
Gi0/4                        disabled     1            auto   auto 10/100/1000BaseTX
Te0/1                        notconnect   1            full  10000 10GBase-CX1
```

Status values:
- **connected**: Link is up and active
- **notconnect**: No link detected (cable unplugged or device off)
- **disabled**: Administratively shut down (`shutdown` command applied)
- **err-disabled**: Port disabled by error-disable mechanism (BPDU guard, port security violation, etc.)
- **notpresent**: No SFP/cable module inserted

**`show interfaces description`**:
```
Switch# show interfaces description
Interface                      Status         Protocol Description
Gi0/1                          up             up       Uplink to Router
Gi0/2                          up             up       Server 1
Gi0/3                          down           down
Gi0/4                          admin down     down
```

**`show interfaces trunk`**:
```
Switch# show interfaces trunk
Port        Mode             Encapsulation  Status        Native vlan
Gi0/1       on               802.1q         trunking      1

Port        Vlans allowed on trunk
Gi0/1       1-4094

Port        Vlans allowed and active in management domain
Gi0/1       1,10,20,30

Port        Vlans in spanning tree forwarding state and not pruned
Gi0/1       1,10,20,30
```

**`show interfaces switchport`** (for a specific interface):
```
Switch# show interfaces GigabitEthernet0/2 switchport
Name: Gi0/2
Switchport: Enabled
Administrative Mode: static access
Operational Mode: static access
Administrative Trunking Encapsulation: dot1q
Operational Trunking Encapsulation: native
Negotiation of Trunking: Off
Access Mode VLAN: 10 (Engineering)
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
Appliance trust: none
```

### `show inventory`
Display the hardware inventory (chassis, modules, SFPs).

```
Switch# show inventory
NAME: "1", DESCR: "WS-C3560CX-12PC-S"
PID: WS-C3560CX-12PC-S , VID: V01  , SN: FCW1234A5BC

NAME: "Power Supply 1", DESCR: "FRU Power Supply"
PID: PWR-C1-1100WAC    , VID: V01  , SN: LIT1234A5BC
```

### `show ip`
Display IP-related information. This has many subcommands.

```
Switch# show ip ?
  access-lists     List IP access lists
  arp              IP ARP table
  bgp              BGP information
  cache            IP fast-switching route cache
  cef              Cisco Express Forwarding
  dhcp             Dynamic Host Configuration Protocol status
  dns              IP domain information
  eigrp            IP-EIGRP show commands
  helper-address   IP helper address
  http             HTTP information
  igmp             IGMP information
  interface        IP interface status and configuration
  irdp             IP IRDP information
  local            IP local options
  mroute           IP multicast routing table
  msdp             MSDP information
  nat              IP NAT information
  nhrp             NHRP information
  ospf             OSPF information
  pim              PIM information
  policy           Policy routing
  protocols        IP routing protocol process parameters and statistics
  rip              IP RIP show commands
  route            IP routing table
  rsvp             RSVP information
  sla              IP SLA information
  socket           IP socket connections
  ssh              Information about SSH
  tcp              TCP/IP Header Compression statistics
  traffic          IP protocol statistics
  wccp             WCCP information
```

**`show ip interface brief`** (most commonly used):
```
Switch# show ip interface brief
Interface              IP-Address      OK? Method Status                Protocol
GigabitEthernet0/1     unassigned      YES unset  up                    up
GigabitEthernet0/2     unassigned      YES unset  up                    up
GigabitEthernet0/3     unassigned      YES unset  down                  down
Vlan1                  192.168.1.10    YES NVRAM  up                    up
Vlan10                 10.0.0.1        YES NVRAM  up                    up
```

Column descriptions:
- **Interface**: Interface name (abbreviated)
- **IP-Address**: IP address assigned, or `unassigned`
- **OK?**: Whether the IP address is valid (`YES`/`NO`)
- **Method**: How the IP was assigned: `NVRAM` (from config), `DHCP`, `manual`, `unset`
- **Status**: Physical layer status (up/down/administratively down)
- **Protocol**: Data link layer status (up/down)

**`show ip route`**:
```
Switch# show ip route
Codes: C - connected, S - static, R - RIP, M - mobile, B - BGP
       D - EIGRP, EX - EIGRP external, O - OSPF, IA - OSPF inter area
       N1 - OSPF NSSA external type 1, N2 - OSPF NSSA external type 2
       E1 - OSPF external type 1, E2 - OSPF external type 2
       i - IS-IS, su - IS-IS summary, L1 - IS-IS level-1, L2 - IS-IS level-2
       ia - IS-IS inter area, * - candidate default, U - per-user static route
       o - ODR, P - periodic downloaded static route, H - NHRP, l - LISP
       + - replicated route, % - next hop override

Gateway of last resort is 192.168.1.1 to network 0.0.0.0

S*    0.0.0.0/0 [1/0] via 192.168.1.1
C     10.0.0.0/24 is directly connected, Vlan10
L     10.0.0.1/32 is directly connected, Vlan10
C     192.168.1.0/24 is directly connected, Vlan1
L     192.168.1.10/32 is directly connected, Vlan1
```

Route entry format: `[administrative distance/metric] via <next-hop>, <age>, <interface>`
- `C` = directly connected
- `L` = local (host route for interface IP, IOS 15.x)
- `S` = static
- `S*` = static default route (candidate default)
- `[1/0]` = [administrative distance / metric]

**`show ip arp`**:
```
Switch# show ip arp
Protocol  Address          Age (min)  Hardware Addr   Type   Interface
Internet  192.168.1.1             5   0011.2233.4455  ARPA   Vlan1
Internet  192.168.1.10            -   aabb.ccdd.eeff  ARPA   Vlan1
```

**`show ip ospf`**, **`show ip ospf neighbor`**:
```
Switch# show ip ospf neighbor
Neighbor ID     Pri   State           Dead Time   Address         Interface
10.0.0.2          1   FULL/DR         00:00:38    192.168.1.2     Vlan1
```

**`show ip protocols`**:
```
Switch# show ip protocols
*** IP Routing is NSF aware ***

Routing Protocol is "ospf 1"
  Outgoing update filter list for all interfaces is not set
  Incoming update filter list for all interfaces is not set
  Router ID 192.168.1.10
  Number of areas in this router is 1. 1 normal 0 stub 0 nssa
  Maximum path: 4
  Routing for Networks:
    192.168.1.0 0.0.0.255 area 0
  Routing Information Sources:
    Gateway         Distance      Last Update
    10.0.0.2             110      00:05:23
  Distance: (default is 110)
```

### `show lacp`
Display LACP (Link Aggregation Control Protocol) information.

```
Switch# show lacp ?
  <1-48>       Channel group number
  counters     Traffic information
  internal     Internal port information
  neighbor     Neighbor information
  sys-id       LACP System ID
```

### `show license`
Display software license information.

```
Switch# show license
Index 1 Feature: ipservices
        Period left: Life time
        License Type: Permanent
        License State: Active, In Use
        License Count: Non-Counted
        License Priority: Medium
```

### `show line`
Display TTY line (console, VTY) information.

```
Switch# show line
   Tty Line Typ     Tx/Rx    A Modem  Roty AccO AccI   Uses  Noise  Overruns   Int
*     0    0 CTY              -    -    -    -    -      0      0     0/0       -
      1    1 VTY              -    -    -    -    -      0      0     0/0       -
      2    2 VTY              -    -    -    -    -      0      0     0/0       -
      3    3 VTY              -    -    -    -    -      0      0     0/0       -
      4    4 VTY              -    -    -    -    -      0      0     0/0       -
      5    5 VTY              -    -    -    -    -      0      0     0/0       -
```

### `show lldp`
Display LLDP (Link Layer Discovery Protocol) information.

```
Switch# show lldp ?
  entry         LLDP neighbor entry information
  errors        LLDP errors
  interface     LLDP interface information
  local-info    LLDP local device information
  neighbors     LLDP neighbor information
  traffic       LLDP traffic statistics
```

Example `show lldp neighbors`:
```
Switch# show lldp neighbors
Capability codes:
    (R) Router, (B) Bridge, (T) Telephone, (C) DOCSIS Cable Device
    (W) WLAN Access Point, (P) Repeater, (S) Station, (O) Other

Device ID           Local Intf     Hold-time  Capability      Port ID
Router1             Gi0/1          120        R               Gi0/0/1
Switch2             Gi0/2          120        B               Gi0/1

Total entries displayed: 2
```

### `show logging`
Display the logging buffer contents.

```
Switch# show logging
Syslog logging: enabled (0 messages dropped, 3 messages rate-limited,
                0 flushes, 0 overruns, xml disabled, filtering disabled)

No Active Message Discriminator.

No Inactive Message Discriminator.

    Console logging: level debugging, 45 messages logged, xml disabled,
                     filtering disabled
    Monitor logging: level debugging, 0 messages logged, xml disabled,
                     filtering disabled
    Buffer logging:  level debugging, 45 messages logged, xml disabled,
                     filtering disabled
    Exception Logging: size (8192 bytes)
    Count and timestamp logging messages: disabled
    Persistent logging: disabled

No active filter modules.

    Trap logging: level informational, 45 message lines logged
        Logging to 192.168.1.100  (udp port 514,  audit disabled,
              link up),
              45 message lines logged,
              0 message lines rate-limited,
              0 message lines dropped-by-MD,
              xml disabled, sequence number disabled
              filtering disabled

Log Buffer (8192 bytes):

Mar 25 14:30:00.000: %SYS-5-CONFIG_I: Configured from console by admin on vty0
Mar 25 14:25:00.000: %LINK-3-UPDOWN: Interface GigabitEthernet0/1, changed state to up
```

### `show mac address-table`
Display the MAC address forwarding table.

```
Switch# show mac address-table ?
  address      Unicast or multicast MAC address
  aging-time   Aging time
  count        MAC entries count
  dynamic      Dynamic MAC addresses
  interface    Interface whose MAC addresses are to be displayed
  learning     Learning status
  move         MAC move information
  multicast    Multicast MAC addresses
  notification MAC notification parameters and history table
  secure       Secure MAC addresses
  static       Static MAC addresses
  vlan         VLAN whose MAC addresses are to be displayed
```

Example output:
```
Switch# show mac address-table
          Mac Address Table
-------------------------------------------

Vlan    Mac Address       Type        Ports
----    -----------       --------    -----
   1    0011.2233.4455    DYNAMIC     Gi0/1
   1    aabb.ccdd.eeff    DYNAMIC     Gi0/2
  10    1122.3344.5566    DYNAMIC     Gi0/3
  10    0000.0000.0001    STATIC      CPU
Total Mac Addresses for this criterion: 4
```

Columns:
- **Vlan**: VLAN the MAC address belongs to
- **Mac Address**: 48-bit MAC address in dotted-hex format (xxxx.xxxx.xxxx)
- **Type**: DYNAMIC (learned automatically), STATIC (manually configured or system)
- **Ports**: Interface the MAC was learned on

Note: The `show mac address-table` command (IOS 12.1(11)EA1 and later) superseded
the older `show mac-address-table` format. Both are accepted but `show mac address-table`
is the standard on 3560-CX.

### `show mls`
Display Multi-Layer Switching information.

```
Switch# show mls ?
  acl         ACL information
  cef         CEF information
  ip          IP switching information
  qos         MLS QoS information
  rate-limit  MLS rate-limiter information
```

### `show monitor`
Display SPAN (Switched Port Analyzer) session information.

```
Switch# show monitor
Session 1
---------
Type                   : Local Session
Source Ports           :
    Both               : Gi0/1
Destination Ports      : Gi0/8
    Encapsulation      : Native
          Ingress      : Disabled
```

### `show ntp`
Display NTP (Network Time Protocol) status and associations.

```
Switch# show ntp ?
  associations  NTP associations
  config        NTP configuration
  packets       NTP packets
  sessions      NTP sessions
  status        NTP module status
```

**`show ntp status`**:
```
Switch# show ntp status
Clock is synchronized, stratum 3, reference is 192.168.1.100
nominal freq is 250.0000 Hz, actual freq is 250.0000 Hz, precision is 2**10
ntp uptime is 120000 (1/100 of seconds), resolution is 4000
reference time is D9B12345.6789ABCD (14:30:00.123 UTC Mon Mar 25 2024)
clock offset is 0.2345 msec, root delay is 5.23 msec
root dispersion is 12.45 msec, peer dispersion is 0.34 msec
loopfilter state is 'CTRL' (Normal Controlled Loop), drift is 0.0000001 s/s
system poll interval is 64, last update was 30 sec ago.
```

**`show ntp associations`**:
```
Switch# show ntp associations
  address         ref clock       st   when   poll reach  delay  offset   disp
*~192.168.1.100   10.0.0.1         2     25     64   377   0.234   0.234  0.345
 + = selected, * = sys.peer, # = selected, x = falseticker, ~ = configured
```

### `show port-security`
Display port security configuration and status.

```
Switch# show port-security ?
  address    Secure MAC address information
  interface  Secure port information
```

**`show port-security`** (summary):
```
Switch# show port-security
Secure Port  MaxSecureAddr  CurrentAddr  SecurityViolation  Security Action
              (Count)       (Count)          (Count)
---------------------------------------------------------------------------
      Gi0/5              2            1                  0         Shutdown
---------------------------------------------------------------------------
Total Addresses in System (excluding one mac per port)     : 0
Max Addresses limit in System (excluding one mac per port) : 4096
```

**`show port-security interface GigabitEthernet0/5`**:
```
Switch# show port-security interface GigabitEthernet0/5
Port Security              : Enabled
Port Status                : Secure-up
Violation Mode             : Shutdown
Aging Time                 : 0 mins
Aging Type                 : Absolute
SecureStatic Address Aging : Disabled
Maximum MAC Addresses      : 2
Total MAC Addresses        : 1
Configured MAC Addresses   : 0
Sticky MAC Addresses       : 1
Last Source Address:Vlan   : aabb.ccdd.eeff:10
Security Violation Count   : 0
```

### `show processes`
Display active process information.

```
Switch# show processes ?
  cpu    Show CPU use per process
  log    Show process log
  memory Show memory use per process
```

**`show processes cpu`**:
```
Switch# show processes cpu
CPU utilization for five seconds: 5%/1%; one minute: 4%; five minutes: 3%
 PID Runtime(ms)     Invoked      uSecs   5Sec   1Min   5Min TTY Process
   1           0           2          0  0.00%  0.00%  0.00%   0 Chunk Manager
   2         308        1234        249  0.00%  0.00%  0.00%   0 Load Meter
 ...
```

### `show running-config`
Display the current running configuration. See running-config-format.md for full format details.

```
Switch# show running-config ?
  <cr>
  all            Show all configurations (including defaults)
  aaa            AAA configurations
  brief          brief version of running-config
  full           full version of running-config
  interface      Interface specific configuration
  map-class      map-class configuration
  partition      Partial configuration
  view           View specific configuration
```

### `show sdm`
Display SDM (Switch Database Manager) template information. Controls the allocation
of resources for Layer 2/3 forwarding tables.

```
Switch# show sdm prefer
 The current template is "default" template.
 The selected template optimizes the resources in
 the switch to support this level of features for
 0 routed interfaces and 255 VLANs.

  number of unicast mac addresses:                  8K
  number of IPv4 IGMP groups + multicast routes:    0.25K
  number of IPv4 unicast routes:                    0
    number of directly-connected IPv4 hosts:        0
    number of indirect IPv4 routes:                 0
  number of IPv4 policy based routing aces:         0
  number of IPv4/MAC qos aces:                      0.5K
  number of IPv4/MAC security aces:                 0.5K
  number of IPv6 unicast routes:                    0
  number of directly-connected IPv6 addresses:      0
  number of indirect IPv6 unicast routes:           0
  number of IPv6 policy based routing aces:         0
  number of IPv6 qos aces:                          0
  number of IPv6 security aces:                     0
```

### `show snmp`
Display SNMP statistics and configuration.

```
Switch# show snmp
Chassis: FCW1234A5BC
0 SNMP packets input
    0 Bad SNMP version errors
    0 Unknown community name
    0 Illegal operation for community name supplied
    0 Encoding errors
    0 Number of requested variables
    0 Number of altered variables
    0 Get-request PDUs
    0 Get-next PDUs
    0 Set-request PDUs
    0 Input queue packet drops (Maximum queue size 1000)
0 SNMP packets output
    0 Too big errors (Maximum packet size 1500)
    0 No such name errors
    0 Bad values errors
    0 General errors
    0 Response PDUs
    0 Trap PDUs
```

### `show spanning-tree`
Display Spanning Tree Protocol (STP) topology information.

```
Switch# show spanning-tree ?
  VLAN<1-4094>    VLAN Switch Spanning Trees
  active          Report on active interfaces only
  backbonefast    Show whether BackboneFast is enabled
  blockedports    Show blocked ports
  bridge          Show bridge spanning tree
  detail          Show detailed information
  inconsistentports  Show inconsistent ports
  interface       Spanning Tree interface status and configuration
  mst             Multiple spanning tree
  pathcost        method to use for computing spanning tree pathcost
  portfast        Show portfast info
  root            Spanning Tree root information
  summary         Summary of port states
  uplinkfast      Show whether UplinkFast is enabled
  vlan            VLAN Switch Spanning Trees
```

**`show spanning-tree`** (or `show spanning-tree vlan 1`):
```
Switch# show spanning-tree vlan 1

VLAN0001
  Spanning tree enabled protocol ieee
  Root ID    Priority    32769
             Address     0011.2233.4455
             This bridge is the root
             Hello Time   2 sec  Max Age 20 sec  Forward Delay 15 sec

  Bridge ID  Priority    32769  (priority 32768 sys-id-ext 1)
             Address     0011.2233.4455
             Hello Time   2 sec  Max Age 20 sec  Forward Delay 15 sec
             Aging Time  300 sec

Interface           Role Sts Cost      Prio.Nbr Type
------------------- ---- --- --------- -------- --------------------------------
Gi0/1               Desg FWD 4         128.1    P2p
Gi0/2               Desg FWD 4         128.2    P2p
Gi0/3               Desg FWD 4         128.3    P2p
```

**When the switch is NOT the root**:
```
VLAN0001
  Spanning tree enabled protocol ieee
  Root ID    Priority    4097
             Address     aabb.ccdd.eeff
             Cost        4
             Port        1 (GigabitEthernet0/1)
             Hello Time   2 sec  Max Age 20 sec  Forward Delay 15 sec

  Bridge ID  Priority    32769  (priority 32768 sys-id-ext 1)
             Address     0011.2233.4455
             Hello Time   2 sec  Max Age 20 sec  Forward Delay 15 sec
             Aging Time  300 sec

Interface           Role Sts Cost      Prio.Nbr Type
------------------- ---- --- --------- -------- --------------------------------
Gi0/1               Root FWD 4         128.1    P2p
Gi0/2               Desg FWD 4         128.2    P2p
Gi0/3               Altn BLK 4         128.3    P2p
```

Field descriptions:
- **Role**:
  - `Root` = root port (toward root bridge)
  - `Desg` = designated port (forwarding, away from root)
  - `Altn` = alternate port (blocked, RSTP)
  - `Back` = backup port (blocked, RSTP)
- **Sts** (Status):
  - `FWD` = forwarding
  - `BLK` = blocking
  - `LIS` = listening
  - `LRN` = learning
  - `DIS` = disabled
- **Cost**: Path cost to root
- **Prio.Nbr**: Port priority.port number
- **Type**:
  - `P2p` = point-to-point (full duplex)
  - `Shr` = shared (half duplex)
  - `P2p Edge` = PortFast enabled

**STP priority formula**: Bridge priority = configured priority (default 32768) + VLAN ID (sys-id-ext)
So VLAN 1 = 32768 + 1 = 32769.

**`show spanning-tree summary`**:
```
Switch# show spanning-tree summary
Switch is in rapid-pvst mode
Root bridge for: VLAN0001, VLAN0010, VLAN0020
EtherChannel misconfig guard          is enabled
Extended system ID                    is enabled
Portfast Default                      is disabled
PortFast BPDU Guard Default           is disabled
Portfast BPDU Filter Default          is disabled
Loopguard Default                     is disabled
UplinkFast                            is disabled
BackboneFast                          is disabled
Configured Pathcost method used is short

Name                   Blocking Listening Learning Forwarding STP Active
---------------------- -------- --------- -------- ---------- ----------
VLAN0001                     0         0        0          3          3
VLAN0010                     0         0        0          2          2
VLAN0020                     1         0        0          2          3
---------------------- -------- --------- -------- ---------- ----------
3 vlans                      1         0        0          7          8
```

### `show ssh`
Display SSH connection information.

```
Switch# show ssh
Connection Version Mode Encryption  Hmac               State                 Username
0          2.0     IN   aes128-cbc  hmac-md5    Session started          admin
0          2.0     OUT  aes128-cbc  hmac-md5    Session started          admin
%No SSHv1 server connections running.
```

### `show startup-config`
Display the configuration stored in NVRAM (will be loaded at next boot).

```
Switch# show startup-config
```
(Output has the same format as `show running-config`)

### `show storm-control`
Display storm control settings.

```
Switch# show storm-control ?
  broadcast  Broadcast address storm control
  interface  Interface to display storm control information
  multicast  Multicast address storm control
  unicast    Unicast address storm control
```

### `show system`
Display system information.

```
Switch# show system mtu
System MTU size is 1500 bytes
System Jumbo MTU size is 1500 bytes
Routing MTU size is 1500 bytes
```

### `show tech-support`
Generate a comprehensive system report for troubleshooting. Output is very lengthy.

```
Switch# show tech-support
```

This automatically runs many `show` commands and collects their output.

### `show terminal`
Display terminal settings.

```
Switch# show terminal
Line 0, Location: "", Type: ""
Length: 24 lines, Width: 80 columns
Baud rate (TX/RX) is 9600/9600, no parity, 2 stopbits, 8 databits
Status: PSI Enabled, Ready, Active, No Exit Banner
Capabilities: none
Modem state: Ready
Group codes:    0
Special Chars: Escape  Hold  Stop  Start  Disconnect  Activation
                 ^^X  none   -     -       none
Timeouts:      Idle EXEC    Idle Session   Modem Answer  Session   Dispatch
               00:10:00     never          none          not set
Session limit is not set.
Time since activation: 00:05:23
Editing is enabled.
History is enabled, history size is 20.
DNS resolution in show commands is enabled
Full user help is disabled
Allowed input transports are none.
Allowed output transports are none.
Preferred transport is none.
No output characters are padded
No special data dispatching characters
```

### `show udld`
Display UDLD (Unidirectional Link Detection) status.

```
Switch# show udld ?
  GigabitEthernet    GigabitEthernet interface
  TenGigabitEthernet TenGigabitEthernet interface
  neighbors          Neighbor cache contents
```

### `show users`
Display users currently logged into the device.

```
Switch# show users
    Line       User       Host(s)              Idle       Location
*  0 con 0     admin      idle                 00:00:00
   1 vty 0     admin      idle                 00:02:35   192.168.1.100

  Interface    User               Mode         Idle     Peer Address
```

### `show version`
Display system hardware and software information. This is typically the first command
run on a switch.

Example output for C3560-CX:
```
Switch# show version
Cisco IOS Software, C3560CX Software (C3560CX-UNIVERSALK9-M), Version 15.2(7)E2, RELEASE SOFTWARE (fc1)
Technical Support: http://www.cisco.com/techsupport
Copyright (c) 1986-2021 by Cisco Systems, Inc.
Compiled Thu 07-Oct-21 11:27 by prod_rel_team

ROM: Bootstrap program is C3560CX boot loader
BOOTLDR: C3560CX Boot Loader (C3560CX-HBOOT-M) Version 15.2(7r)E2, RELEASE SOFTWARE (fc1)

Switch uptime is 2 weeks, 3 days, 4 hours, 5 minutes
System returned to ROM by power-on
System image file is "flash:c3560cx-universalk9-mz.152-7.E2/c3560cx-universalk9-mz.152-7.E2.bin"
Last reload reason: power-on



This product contains cryptographic features and is subject to United
States and local country laws governing import, export, transfer and
use. Delivery of Cisco cryptographic products does not imply
third-party authority to import, export, distribute or use encryption.
Importers, exporters, distributors and users are responsible for
compliance with U.S. and local country laws. By using this product you
agree to comply with applicable laws and regulations. If you are unable
to comply with U.S. and local country laws return this product
immediately.

A summary of U.S. laws governing Cisco cryptographic products may be found at:
http://www.cisco.com/wwl/export/crypto/tool/stqrg.html

If you are a U.S. government agency, export of this product is subject to
United States Government Control. Please contact exportcontrol@cisco.com
for information about our compliance program.

cisco WS-C3560CX-12PC-S (APM86XXX) processor (revision A0) with 524288K bytes of memory.
Processor board ID FCW1234A5BC
Last reset from power-on
3 Virtual Ethernet interfaces
12 Gigabit Ethernet interfaces
2 Ten Gigabit Ethernet interfaces
The password-recovery mechanism is enabled.

512K bytes of flash-simulated non-volatile configuration memory.
Base ethernet MAC Address       : 00:11:22:33:44:55
Motherboard assembly number     : 73-16124-05
Power supply part number        : 341-0569-01
Motherboard serial number       : FOC1234A5BC
Power supply serial number      : LIT1234A5BC
Model revision number           : A0
Motherboard revision number     : A0
Model number                    : WS-C3560CX-12PC-S
Daughterboard assembly number   : 800-40598-01
Daughterboard serial number     : FOC1234A5BC
System serial number            : FCW1234A5BC
Top Assembly Part Number        : 800-40952-01
Top Assembly Revision Number    : A0
Version ID                      : V01
CLEI Code Number                : CMMPB10ARA
Hardware Board Revision Number  : 0x04


Switch Ports Model              SW Version            SW Image
------ ----- -----              ----------            ----------
*    1 14    WS-C3560CX-12PC-S  15.2(7)E2             C3560CX-UNIVERSALK9-M


Configuration register is 0x102
```

Key fields:
- **Cisco IOS Software line**: Image name, version string
- **Switch uptime**: How long since last boot
- **System image file**: Flash path of the running image
- **cisco WS-C3560CX-...**: Model designation and processor
- **Processor board ID**: Serial number of processor board
- **Base ethernet MAC Address**: Management MAC address (used for bridge ID in STP)
- **Model number**: Full hardware model (WS-C3560CX-12PC-S, etc.)
- **System serial number**: Main chassis serial number
- **Configuration register**: Controls boot behavior (0x102 = load from flash, no break)

### `show vlan`
Display VLAN information.

```
Switch# show vlan ?
  brief           VTP all VLAN status in brief
  dot1q-tunnel    dot1q-tunnel parameters
  id              VTP VLAN status by VLAN id
  ifindex         ifIndex of vlans
  internal        VLAN internal usage
  mtu             VLAN MTU
  name            VTP VLAN status by VLAN name
  private-vlan    Private VLAN information
  remote-span     Remote SPAN VLANs
  summary         VLAN summary information
```

**`show vlan brief`**:
```
Switch# show vlan brief

VLAN Name                             Status    Ports
---- -------------------------------- --------- -------------------------------
1    default                          active    Gi0/2, Gi0/3, Gi0/4, Gi0/5
10   Engineering                      active    Gi0/6, Gi0/7
20   Marketing                        active    Gi0/8
30   Servers                          active
1002 fddi-default                     act/unsup
1003 token-ring-default               act/unsup
1004 fddinet-default                  act/unsup
1005 trnet-default                    act/unsup
```

Notes:
- Only access ports are shown in the Ports column; trunk ports appear in `show interfaces trunk`
- VLANs 1002-1005 are legacy VLANs always present (cannot be deleted)
- Status `act/unsup` = active but unsupported on this platform

**`show vlan id 10`**:
```
Switch# show vlan id 10

VLAN Name                             Status    Ports
---- -------------------------------- --------- -------------------------------
10   Engineering                      active    Gi0/6, Gi0/7

VLAN Type  SAID       MTU   Parent RingNo BridgeNo Stp  BrdgMode Trans1 Trans2
---- ----- ---------- ----- ------ ------ -------- ---- -------- ------ ------
10   enet  100010     1500  -      -      -        ieee -        0      0

Primary Secondary Type              Ports
------- --------- ----------------- ------------------------------------------
```

### `show vtp`
Display VTP (VLAN Trunking Protocol) status and statistics.

```
Switch# show vtp ?
  counters  VTP statistics
  devices   VTP devices information
  password  VTP password
  status    VTP domain status
```

**`show vtp status`**:
```
Switch# show vtp status
VTP Version capable             : 1 to 3
VTP version running             : 1
VTP Domain Name                 : CORP
VTP Pruning Mode                : Disabled
VTP Traps Generation            : Disabled
Device ID                       : 0011.2233.4455
Configuration last modified by 192.168.1.100 at 3-25-24 14:30:00
Local updater ID is 192.168.1.10 on interface Vl1 (lowest numbered VLAN interface found)

Feature VLAN:
--------------
VTP Operating Mode                : Server
Maximum VLANs supported locally   : 1005
Number of existing VLANs          : 7
Configuration Revision            : 5
MD5 digest                        : 0x1A 0x2B 0x3C 0x4D ...
```

VTP modes:
- **Server**: Can create, modify, delete VLANs; advertises to clients
- **Client**: Cannot modify VLANs; receives from server; forwards VTP updates
- **Transparent**: Does not participate in VTP; forwards VTP updates but uses local VLAN config
- **Off**: Does not send or forward VTP updates
