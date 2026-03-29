# Cisco IOS 15.2 Configuration Mode Commands Reference (C3560CX)

This document covers configuration mode commands for the Cisco Catalyst 3560-CX running
IOS Release 15.2(x)E. It is organized by configuration mode/submode.

---

## Entering Configuration Modes

| Command (from privileged EXEC) | Effect |
|-------------------------------|--------|
| `configure terminal` | Enter global configuration mode; prompt becomes `Switch(config)#` |
| `configure` | Same as above (abbreviation accepted) |
| `configure memory` | Load config from NVRAM into running-config (rarely used) |
| `configure network` | Load config from a TFTP server |

Within global config, entering a submode command changes the prompt:

| Submode Entry Command | Resulting Prompt |
|----------------------|-----------------|
| `interface GigabitEthernet1/0/1` | `Switch(config-if)#` |
| `interface vlan 10` | `Switch(config-if)#` |
| `line console 0` | `Switch(config-line)#` |
| `line vty 0 15` | `Switch(config-line)#` |
| `router ospf 1` | `Switch(config-router)#` |
| `vlan 10` | `Switch(config-vlan)#` |
| `ip access-list standard MYLIST` | `Switch(config-std-nacl)#` |
| `ip access-list extended MYLIST` | `Switch(config-ext-nacl)#` |

---

## Global Configuration Commands

These commands are entered at the `Switch(config)#` prompt. The list below covers the 60-80
most commonly used commands, grouped thematically.

### Identity and System

| Command | Description |
|---------|-------------|
| `hostname NAME` | Set the device hostname. Changes the CLI prompt immediately. Example: `hostname Core-SW-1` |
| `ip domain-name DOMAIN` | Set the default DNS domain. Example: `ip domain-name example.com`. Used for DNS lookups and SSH key generation. |
| `ip domain lookup` | Enable DNS lookups (default enabled). Use `no ip domain lookup` to prevent CLI delays when mistyping commands. |
| `ip name-server A.B.C.D [A.B.C.D]` | Configure up to 6 DNS server addresses. Example: `ip name-server 8.8.8.8 8.8.4.4` |
| `clock timezone ZONE OFFSET` | Set the timezone. Example: `clock timezone EST -5` |
| `clock summer-time ZONE recurring` | Configure daylight saving time. Example: `clock summer-time EDT recurring` |

### Passwords and Security

| Command | Description |
|---------|-------------|
| `enable secret PASSWORD` | Set the privileged EXEC password using MD5 (type 5) hash. Preferred over `enable password`. Example: `enable secret MySecret123` |
| `enable password PASSWORD` | Set the privileged EXEC password in cleartext (or type 7 if `service password-encryption` is active). Superseded by `enable secret`. |
| `service password-encryption` | Encrypts all cleartext passwords in the config with type 7 (Vigenere cipher). Does not affect `enable secret` which is always MD5. |
| `username NAME privilege LEVEL secret PASSWORD` | Create a local user account. Privilege level 1-15; level 15 = full access. Example: `username admin privilege 15 secret P@ssword` |
| `username NAME privilege LEVEL password PASSWORD` | Create a local user with cleartext/type7 password. |
| `no enable password` | Remove the enable password. |
| `security passwords min-length N` | Enforce minimum password length (IOS 12.3+). |
| `login block-for SECONDS attempts N within SECONDS` | Lock login after N failures. |

### AAA (Authentication, Authorization, Accounting)

| Command | Description |
|---------|-------------|
| `aaa new-model` | Enable AAA. Once enabled, AAA replaces the `login` command on lines. Cannot be undone without care. |
| `aaa authentication login default local` | Use local user database for login authentication. |
| `aaa authentication login default group tacacs+ local` | Try TACACS+ first, fall back to local. |
| `aaa authentication enable default enable` | Use enable password for privilege escalation. |
| `aaa authorization exec default local` | Authorize EXEC sessions using local database. |
| `aaa accounting exec default start-stop group tacacs+` | Send accounting records to TACACS+. |
| `tacacs-server host A.B.C.D` | Specify TACACS+ server (legacy syntax). |
| `tacacs server NAME` | Define a TACACS+ server (modern syntax). |
| `radius-server host A.B.C.D` | Specify RADIUS server (legacy). |

### Banners

| Command | Description |
|---------|-------------|
| `banner motd DELIMITER text DELIMITER` | Set the Message-Of-The-Day banner shown to all connecting users before login. |
| `banner login DELIMITER text DELIMITER` | Set banner shown immediately before the login prompt (after MOTD on remote connections). |
| `banner exec DELIMITER text DELIMITER` | Set banner shown after successful login, just before the EXEC prompt. |
| `banner incoming DELIMITER text DELIMITER` | Shown for reverse telnet connections. |
| `no banner motd` | Remove the MOTD banner. |

Banner delimiter usage: any character can be the delimiter as long as it does not appear in the
banner text. The banner can span multiple lines. Example:

```
Switch(config)# banner motd ^
Enter TEXT message.  End with the character '^'.
WARNING: Authorized access only.
Unauthorized use is prohibited and subject to prosecution.
^
```

### Interfaces (Entering Submode)

| Command | Description |
|---------|-------------|
| `interface GigabitEthernet1/0/N` | Enter config for a GE copper port (1/0/1 through 1/0/12 on C3560CX-12). |
| `interface TenGigabitEthernet1/0/N` | Enter config for 10GE uplink SFP+ port. |
| `interface Vlan N` | Enter config for a Layer 3 SVI (Switched Virtual Interface). |
| `interface range GigabitEthernet1/0/1 - 8` | Enter config for a range of interfaces simultaneously. |
| `interface range GigabitEthernet1/0/1 - 4 , GigabitEthernet1/0/9 - 12` | Non-contiguous range. |
| `default interface GigabitEthernet1/0/1` | Reset interface to factory defaults. |

### VLANs (Global Config)

| Command | Description |
|---------|-------------|
| `vlan N` | Enter VLAN configuration submode to create/modify VLAN N (1-4094). |
| `vlan N,M` | Create multiple VLANs at once. |
| `no vlan N` | Delete VLAN N. Cannot delete VLAN 1 or reserved VLANs. |

### IP Routing and Forwarding

| Command | Description |
|---------|-------------|
| `ip routing` | Enable Layer 3 IP routing on the switch (required for SVIs and routed ports to route). |
| `ip route A.B.C.D MASK NEXTHOP` | Add a static route. Example: `ip route 0.0.0.0 0.0.0.0 192.168.1.1` |
| `ip route A.B.C.D MASK INTERFACE` | Static route via interface. |
| `ip default-gateway A.B.C.D` | Set the default gateway for management traffic when `ip routing` is NOT enabled. |
| `ip classless` | Enable classless routing behavior (default in IOS 12.x+). |
| `ipv6 unicast-routing` | Enable IPv6 routing on the switch. |
| `ipv6 route PREFIX/LEN NEXTHOP` | Add a static IPv6 route. |

### NTP

| Command | Description |
|---------|-------------|
| `ntp server A.B.C.D` | Configure an NTP server. The switch synchronizes to this server. |
| `ntp server A.B.C.D prefer` | Set as preferred NTP server. |
| `ntp master STRATUM` | Make this switch an NTP master (stratum 1-15). |
| `ntp authenticate` | Enable NTP authentication. |
| `ntp authentication-key N md5 KEY` | Define an NTP authentication key. |
| `ntp trusted-key N` | Mark an NTP key as trusted. |
| `ntp source INTERFACE` | Set source interface for NTP packets. |

### Logging

| Command | Description |
|---------|-------------|
| `logging on` | Enable logging (default enabled). |
| `logging A.B.C.D` | Send syslog messages to a remote syslog server. |
| `logging host A.B.C.D` | Same as above (explicit syntax). |
| `logging trap LEVEL` | Set minimum severity level for remote syslog (0=emergencies to 7=debugging). |
| `logging console LEVEL` | Set severity for console logging. |
| `logging buffered SIZE` | Enable internal log buffer; SIZE is in bytes. Example: `logging buffered 16384` |
| `logging buffered LEVEL` | Set severity for buffered logging. |
| `no logging console` | Disable console logging (useful to prevent output interrupting CLI). |
| `service timestamps log datetime msec` | Add timestamps to syslog messages. Common best practice. |
| `service timestamps debug datetime msec` | Add timestamps to debug output. |

### SNMP

| Command | Description |
|---------|-------------|
| `snmp-server community STRING ro` | Configure a read-only SNMP community string. |
| `snmp-server community STRING rw` | Configure a read-write SNMP community string. |
| `snmp-server community STRING ro ACL` | Restrict SNMP access with an ACL. |
| `snmp-server location STRING` | Set the SNMP system location (sysLocation OID). |
| `snmp-server contact STRING` | Set the SNMP contact string (sysContact OID). |
| `snmp-server host A.B.C.D version 2c COMMUNITY` | Configure SNMP trap destination. |
| `snmp-server enable traps` | Enable all SNMP traps. |
| `no snmp-server` | Disable SNMP. |

### Spanning Tree

| Command | Description |
|---------|-------------|
| `spanning-tree mode pvst` | Enable Per-VLAN Spanning Tree (classic STP, 802.1D per VLAN). Default on older IOS. |
| `spanning-tree mode rapid-pvst` | Enable Rapid PVST+ (802.1w per VLAN). Recommended. Default on IOS 15.2. |
| `spanning-tree vlan N priority PRIORITY` | Set STP bridge priority for a VLAN (multiple of 4096; 0=highest priority). Example: `spanning-tree vlan 1 priority 4096` |
| `spanning-tree vlan N root primary` | Macro to set priority low enough to become root (sets to 24576 or 4096 below current root). |
| `spanning-tree vlan N root secondary` | Macro to set priority to 28672. |
| `spanning-tree portfast default` | Enable PortFast on all non-trunk access ports by default. |
| `spanning-tree portfast bpduguard default` | Enable BPDU guard globally on all PortFast-enabled ports. |
| `spanning-tree portfast bpdufilter default` | Enable BPDU filter globally on all PortFast-enabled ports. |
| `spanning-tree loopguard default` | Enable Loop Guard globally. |
| `no spanning-tree vlan N` | Disable STP for a specific VLAN. |

### SSH and Remote Access

| Command | Description |
|---------|-------------|
| `ip ssh version 2` | Force SSHv2 only (recommended). |
| `ip ssh time-out SECONDS` | SSH negotiation timeout in seconds (default 120). |
| `ip ssh authentication-retries N` | Maximum SSH login attempts before disconnect (default 3). |
| `crypto key generate rsa modulus 2048` | Generate RSA key pair for SSH. Requires `ip domain-name` to be set. |
| `crypto key zeroize rsa` | Delete the RSA key pair. |
| `ip telnet source-interface INTERFACE` | Set source interface for outgoing Telnet. |

### Access Control Lists

| Command | Description |
|---------|-------------|
| `access-list N permit A.B.C.D WILDCARD` | Standard numbered ACL permit. N=1-99 or 1300-1999. |
| `access-list N deny A.B.C.D WILDCARD` | Standard numbered ACL deny. |
| `access-list N permit A.B.C.D WILDCARD PROTOCOL D.E.F.G WILDCARD` | Extended numbered ACL. N=100-199 or 2000-2699. |
| `ip access-list standard NAME` | Create/enter a named standard ACL. |
| `ip access-list extended NAME` | Create/enter a named extended ACL. |
| `no access-list N` | Delete a numbered ACL entirely. |

### CDP and LLDP

| Command | Description |
|---------|-------------|
| `cdp run` | Enable Cisco Discovery Protocol globally (default enabled). |
| `no cdp run` | Disable CDP globally. |
| `lldp run` | Enable LLDP globally (default disabled on IOS). |
| `no lldp run` | Disable LLDP globally. |
| `cdp timer SECONDS` | Set CDP advertisement interval (default 60). |
| `cdp holdtime SECONDS` | Set CDP holdtime (default 180). |

### Port Channels / EtherChannel

| Command | Description |
|---------|-------------|
| `interface Port-channel N` | Create/enter config for a port channel interface (N=1-48). |

### SVI Management Interface

| Command | Description |
|---------|-------------|
| `interface Vlan1` | Enter SVI for management VLAN (VLAN 1 exists by default). |
| `ip http server` | Enable HTTP server (web management). |
| `no ip http server` | Disable HTTP server. |
| `ip http secure-server` | Enable HTTPS server. |

### DHCP Server

| Command | Description |
|---------|-------------|
| `ip dhcp pool NAME` | Create/enter a DHCP pool configuration submode. |
| `ip dhcp excluded-address A.B.C.D [A.B.C.D]` | Exclude addresses from DHCP assignment. |
| `no ip dhcp pool NAME` | Delete a DHCP pool. |
| `service dhcp` | Enable DHCP service (default enabled). |
| `no service dhcp` | Disable DHCP service. |

### Service Commands

| Command | Description |
|---------|-------------|
| `service password-encryption` | Apply type 7 encryption to cleartext passwords. |
| `service timestamps log datetime msec localtime show-timezone` | Full timestamp format for logs. |
| `service timestamps debug datetime msec` | Timestamps for debug messages. |
| `service tcp-keepalives-in` | Enable TCP keepalives for incoming connections. |
| `no service pad` | Disable X.25 PAD service (commonly done as hardening). |
| `no service tcp-small-servers` | Disable minor TCP services (echo, discard, etc.) - default disabled in IOS 12.x. |
| `no service udp-small-servers` | Disable minor UDP services. |

### Boot and System

| Command | Description |
|---------|-------------|
| `boot system flash:FILENAME` | Specify the IOS image to boot from. |
| `boot-start-marker` | Marks the beginning of boot commands in the config (generated automatically). |
| `boot-end-marker` | Marks the end of boot commands in the config (generated automatically). |

### DHCP Snooping, DAI, IP Source Guard

| Command | Description |
|---------|-------------|
| `ip dhcp snooping` | Enable DHCP snooping globally. |
| `ip dhcp snooping vlan N` | Enable DHCP snooping for specific VLANs. |
| `ip dhcp snooping verify mac-address` | Verify source MAC in DHCP packets (default enabled). |
| `ip arp inspection vlan N` | Enable Dynamic ARP Inspection for a VLAN. |

---

## Interface Configuration Subcommands (`config-if`)

These commands are entered at the `Switch(config-if)#` prompt after entering an interface
with `interface TYPE SLOT/MOD/PORT`.

### General Interface Commands

| Command | Description |
|---------|-------------|
| `description TEXT` | Set a human-readable description for the interface. Example: `description Uplink to Core Router`. Appears in `show interfaces` and `show running-config`. |
| `shutdown` | Administratively disable the interface. The interface state becomes "administratively down". |
| `no shutdown` | Administratively enable the interface (bring it up). |
| `speed {10|100|1000|auto}` | Set interface speed in Mbps. `auto` enables autonegotiation (default). Not applicable to 10GE ports. |
| `duplex {auto|full|half}` | Set duplex mode. `auto` is default. `half` is only valid at 10/100. Gigabit ports cannot run half-duplex. |
| `mtu BYTES` | Set the interface MTU (68-9000 bytes depending on platform). Default is 1500. |
| `bandwidth KBPS` | Set logical bandwidth for routing protocol calculations (does not affect physical speed). |
| `delay TENS-OF-MICROSECONDS` | Set delay for IGRP/EIGRP metric calculations. |
| `no ip address` | Remove all IP addresses from the interface. |

### Layer 2 (Switchport) Commands

| Command | Description |
|---------|-------------|
| `switchport` | Put the port into Layer 2 switching mode (default for physical switch ports). |
| `no switchport` | Convert to a Layer 3 routed port. Requires `ip routing` to be enabled. |
| `switchport mode access` | Set the port as an access (non-trunking) port. |
| `switchport mode trunk` | Force the port into trunk mode, carrying multiple VLANs. |
| `switchport mode dynamic auto` | Port will become trunk if the other end initiates (default on many ports). |
| `switchport mode dynamic desirable` | Port actively tries to become a trunk. |
| `switchport nonegotiate` | Disable DTP negotiation on the port. Use with `switchport mode trunk` to prevent DTP frames. |
| `switchport access vlan N` | Assign the access port to VLAN N. Default is VLAN 1. |
| `switchport trunk native vlan N` | Set the native (untagged) VLAN on a trunk. Default is VLAN 1. |
| `switchport trunk allowed vlan {all|N|add N|remove N|except N}` | Specify which VLANs are allowed on a trunk. Default is all VLANs. |
| `switchport trunk encapsulation dot1q` | Set trunk encapsulation to 802.1Q (only option on most modern switches; may be required before `switchport mode trunk`). |
| `switchport voice vlan N` | Configure a voice VLAN for IP phones. Places phone traffic in VLAN N, PC traffic in access VLAN. |
| `switchport host` | Shortcut macro: sets `switchport mode access`, `spanning-tree portfast`, and disables channeling. |
| `switchport protected` | Prevent the port from communicating with other protected ports on the same switch (private VLAN lite). |
| `switchport port-security` | Enable port security on the port. |
| `switchport port-security maximum N` | Maximum number of MAC addresses allowed (default 1). |
| `switchport port-security violation {protect|restrict|shutdown}` | Action when violation occurs. `shutdown` (default) puts port in err-disabled state. |
| `switchport port-security mac-address XXXX.XXXX.XXXX` | Manually configure a secure MAC address. |
| `switchport port-security mac-address sticky` | Learn MAC addresses dynamically and add them to the running config. |

### Layer 3 (Routed Port) Commands

| Command | Description |
|---------|-------------|
| `ip address A.B.C.D MASK` | Assign a primary IPv4 address to the interface. Example: `ip address 192.168.1.1 255.255.255.0` |
| `ip address A.B.C.D MASK secondary` | Add a secondary IPv4 address. |
| `ipv6 address PREFIX/LEN` | Assign an IPv6 address. Example: `ipv6 address 2001:db8::1/64` |
| `ipv6 address PREFIX/LEN eui-64` | Assign IPv6 address using EUI-64 from MAC address. |
| `ipv6 enable` | Enable IPv6 on the interface and generate a link-local address. |
| `ip helper-address A.B.C.D` | Forward broadcast UDP packets (DHCP relay) to the specified server. |
| `ip access-group NAME in` | Apply an ACL to inbound traffic on this interface. |
| `ip access-group NAME out` | Apply an ACL to outbound traffic. |
| `ip ospf N area AREA` | Enable OSPF on this interface (interface-level OSPF config, IOS 15.x). |
| `ip ospf cost N` | Set the OSPF interface cost. |
| `ip ospf priority N` | Set the OSPF DR/BDR election priority (0 = never DR). |

### Spanning Tree Interface Commands

| Command | Description |
|---------|-------------|
| `spanning-tree portfast` | Enable PortFast on this access port (skip Listening and Learning states). Do NOT use on ports connected to switches. |
| `spanning-tree portfast disable` | Explicitly disable PortFast on this port. |
| `spanning-tree portfast trunk` | Enable PortFast on a trunk port (use with care). |
| `spanning-tree bpduguard enable` | Enable BPDU guard: if a BPDU is received, put port in err-disabled state. |
| `spanning-tree bpduguard disable` | Explicitly disable BPDU guard. |
| `spanning-tree bpdufilter enable` | Filter all BPDUs on this port (neither sends nor processes them). |
| `spanning-tree guard root` | Enable Root Guard: prevents this port from becoming a root port. |
| `spanning-tree guard loop` | Enable Loop Guard on this port. |
| `spanning-tree cost N` | Set per-interface STP cost (1-200000000). |
| `spanning-tree port-priority N` | Set STP port priority (0-240, multiple of 16; default 128). |
| `spanning-tree vlan N cost N` | Set per-VLAN STP cost on this interface. |
| `spanning-tree vlan N port-priority N` | Set per-VLAN STP port priority. |

### EtherChannel Interface Commands

| Command | Description |
|---------|-------------|
| `channel-group N mode {active|passive|on|auto|desirable}` | Assign port to an EtherChannel. `active`/`passive` = LACP; `auto`/`desirable` = PAgP; `on` = force without negotiation. |
| `channel-protocol {lacp|pagp}` | Specify the channel protocol (usually implicit from channel-group mode). |

### Other Interface Commands

| Command | Description |
|---------|-------------|
| `cdp enable` | Enable CDP on this interface (default). |
| `no cdp enable` | Disable CDP on this interface. |
| `lldp transmit` | Enable sending LLDP advertisements on this interface. |
| `lldp receive` | Enable receiving LLDP advertisements on this interface. |
| `no lldp transmit` | Disable LLDP transmission on this interface. |
| `storm-control broadcast level PCT` | Enable broadcast storm control at PCT% threshold. |
| `storm-control action {shutdown|trap}` | Action when storm control threshold is exceeded. |
| `carrier-delay MSEC` | Delay before reporting a link-down state. |
| `load-interval SECONDS` | Interface load calculation interval (30, 60, or 300 seconds). Default 300. |

---

## Line Configuration Subcommands (`config-line`)

Lines are entered with `line console 0`, `line aux 0`, or `line vty 0 15`.

| Command | Description |
|---------|-------------|
| `exec-timeout MINUTES [SECONDS]` | Set the EXEC session timeout. `exec-timeout 0 0` disables timeout. Default is 10 minutes. Example: `exec-timeout 5 0` = 5 minutes. |
| `password PASSWORD` | Set the line password (used with `login`). |
| `login` | Enable password checking at login using the line password. Requires `password` to be set. |
| `login local` | Require username/password from local user database. Used when `aaa new-model` is not configured. |
| `no login` | Disable login requirement (no authentication). |
| `privilege level N` | Set the default privilege level for users on this line (0-15). Default is 1 for vty, 15 for console on some configs. |
| `transport input {all|none|telnet|ssh|telnet ssh}` | Specify allowed protocols for incoming connections. Example: `transport input ssh` allows SSH only. Default varies. |
| `transport output {all|none|telnet|ssh}` | Specify allowed protocols for outgoing connections from this line. |
| `transport preferred none` | Disable the preferred protocol (prevents accidental Telnet when typing a hostname). |
| `logging synchronous` | Synchronize log messages with CLI output so messages don't interrupt command entry. |
| `history size N` | Set the command history size for this line (default 10, max 256). |
| `length N` | Set the number of lines per screen for this line (0 = no paging). |
| `width N` | Set the terminal width in characters for this line. |
| `speed BAUD` | Set console port baud rate (console line only). Common: 9600 (default), 19200, 115200. |
| `flowcontrol {none|software|hardware}` | Set flow control on the console port. |
| `stopbits {1|1.5|2}` | Set stop bits for console port. Default is 1. |
| `databits {5|6|7|8}` | Set data bits for console port. Default is 8. |
| `parity {none|even|odd|space|mark}` | Set parity. Default is none. |
| `session-timeout MINUTES` | Set the session timeout (different from exec-timeout). |
| `absolute-timeout MINUTES` | Set an absolute timeout after which the session is always disconnected regardless of activity. |
| `access-class ACL {in|out}` | Apply an ACL to restrict which IP addresses can connect to this line (vty lines). Example: `access-class 10 in` |
| `ipv6 access-class ACL in` | Apply an IPv6 ACL to restrict VTY access. |
| `rotary N` | Assign a rotary group number to this line (for reverse telnet). |
| `no exec` | Disable the EXEC process on this line (useful for lines used only for outgoing connections). |
| `autocommand COMMAND` | Automatically execute a command when someone connects to this line. |

---

## Router Configuration Subcommands (`config-router`)

These commands apply after entering `router ospf PROCESS-ID` or other routing protocols.

### OSPF (Open Shortest Path First)

| Command | Description |
|---------|-------------|
| `network A.B.C.D WILDCARD area AREA` | Enable OSPF on interfaces whose addresses match the network/wildcard. AREA can be a number (0-4294967295) or dotted decimal. Example: `network 192.168.1.0 0.0.0.255 area 0` |
| `router-id A.B.C.D` | Manually set the OSPF router ID. Recommended to avoid unexpected ID changes. |
| `passive-interface INTERFACE` | Suppress OSPF hellos on the named interface while still advertising the network. |
| `passive-interface default` | Make all interfaces passive by default. Then use `no passive-interface INTERFACE` to activate specific ones. |
| `no passive-interface INTERFACE` | Re-enable OSPF hellos on an interface when `passive-interface default` is set. |
| `redistribute connected subnets` | Redistribute directly connected networks into OSPF. |
| `redistribute static subnets` | Redistribute static routes into OSPF. |
| `default-information originate` | Originate a Type 5 default route LSA (requires a default route in the routing table). |
| `default-information originate always` | Always originate a default route even without one in the routing table. |
| `auto-cost reference-bandwidth MBPS` | Adjust OSPF auto-cost calculation. Use `100000` (100 Gbps) on networks with 10GE links to avoid all links having cost 1. |
| `area N stub` | Define an OSPF stub area. |
| `area N totally-stub` | Define a totally stubby area (only summary default route). |
| `area N nssa` | Define a Not-So-Stubby Area. |
| `area N authentication` | Enable plain-text authentication for an area. |
| `area N authentication message-digest` | Enable MD5 authentication for an area. |
| `area N range A.B.C.D MASK` | Summarize routes for an area (on ABR). |
| `summary-address A.B.C.D MASK` | Summarize external routes (on ASBR). |
| `timers spf DELAY HOLDTIME` | Adjust SPF calculation delay and hold time. |
| `max-metric router-lsa on-startup wait-period` | Advertise max metric at startup to prevent premature use as transit. |
| `log-adjacency-changes` | Log OSPF neighbor state changes (useful for troubleshooting). |

### EIGRP

| Command | Description |
|---------|-------------|
| `network A.B.C.D` | Enable EIGRP on interfaces in this classful network. |
| `network A.B.C.D WILDCARD` | Enable EIGRP with specific wildcard. |
| `no auto-summary` | Disable automatic summarization at classful boundaries (required in discontiguous networks). |
| `passive-interface INTERFACE` | Suppress EIGRP hellos on the interface. |
| `variance N` | Allow unequal-cost load balancing. |
| `maximum-paths N` | Set maximum number of parallel routes (default 4). |

### BGP

| Command | Description |
|---------|-------------|
| `bgp router-id A.B.C.D` | Set BGP router ID. |
| `neighbor A.B.C.D remote-as N` | Define a BGP neighbor. |
| `neighbor A.B.C.D description TEXT` | Add description to a BGP neighbor. |
| `network A.B.C.D mask MASK` | Advertise a network into BGP. |

---

## VLAN Configuration Subcommands (`config-vlan`)

Entered with `vlan N` from global config mode.

| Command | Description |
|---------|-------------|
| `name STRING` | Assign a name to the VLAN (1-32 characters). Example: `name Sales`. Default name is `VLAN0010` for VLAN 10. |
| `state {active|suspend}` | Set the VLAN state. `active` (default) allows the VLAN to pass traffic; `suspend` disables the VLAN while keeping it in the database. |
| `no shutdown` | Ensure the VLAN is active. |
| `shutdown` | Shut down the VLAN (same as `state suspend` in some IOS versions). |
| `remote-span` | Designate this VLAN as an RSPAN VLAN (used for remote port mirroring). |
| `private-vlan {primary|isolated|community}` | Configure private VLAN type. |
| `private-vlan association N` | Associate secondary private VLANs with a primary. |

**Notes on VLAN storage:**
- VLAN 1 exists by default and cannot be deleted or renamed in most IOS versions.
- VLANs 1002-1005 are reserved for legacy Token Ring and FDDI.
- VLAN configuration for IDs 1-1005 is stored in the `vlan.dat` file on flash, not in the
  running-config.
- VLANs 1006-4094 (extended range) are stored in running-config when the switch is in
  VTP transparent mode.

---

## IP Access-List Configuration Subcommands (`config-std-nacl` / `config-ext-nacl`)

After `ip access-list standard NAME` or `ip access-list extended NAME`:

| Command | Description |
|---------|-------------|
| `permit A.B.C.D WILDCARD` | (Standard) Permit traffic from a host/network. |
| `deny A.B.C.D WILDCARD` | (Standard) Deny traffic from a host/network. |
| `permit ip A.B.C.D WILDCARD A.B.C.D WILDCARD` | (Extended) Permit IP traffic between source and destination. |
| `deny tcp A.B.C.D WILDCARD A.B.C.D WILDCARD eq PORT` | (Extended) Deny TCP to a specific port. |
| `permit any` | Permit all. Equivalent to `permit 0.0.0.0 255.255.255.255`. |
| `deny any` | Deny all (implicit at end, but can be explicit for logging). |
| `10 permit host A.B.C.D` | Numbered ACE (Access Control Entry) with sequence number 10. Named ACLs support sequence numbers for insertion/deletion. |
| `no 10` | Delete ACE with sequence number 10. |

---

## DHCP Pool Subcommands (`config-dhcp`)

After `ip dhcp pool NAME`:

| Command | Description |
|---------|-------------|
| `network A.B.C.D MASK` | Define the DHCP pool network and mask. |
| `default-router A.B.C.D` | Set the default gateway for DHCP clients. |
| `dns-server A.B.C.D [A.B.C.D]` | Specify DNS servers for clients. |
| `domain-name DOMAIN` | Set the domain name for clients. |
| `lease {DAYS [HOURS [MINUTES]] | infinite}` | Set the DHCP lease duration. Default is 1 day. |

---

## SPAN (Port Mirror) Commands (Global Config)

| Command | Description |
|---------|-------------|
| `monitor session N source interface INTERFACE {rx|tx|both}` | Configure SPAN source port. |
| `monitor session N destination interface INTERFACE` | Configure SPAN destination port. |
| `no monitor session N` | Remove a SPAN session. |

---

## Key Configuration Relationships

- `ip routing` must be enabled before SVIs can route traffic between VLANs.
- `crypto key generate rsa` requires `ip domain-name` to be configured.
- `aaa new-model` changes authentication behavior on ALL lines immediately; configure
  fallback (`local`) before enabling.
- `switchport trunk encapsulation dot1q` may be required before `switchport mode trunk`
  on some switch models.
- VLAN 1-1005 data is in `vlan.dat`; VLANs 1006-4094 (extended range) are stored in
  running-config on VTP transparent mode.
- `spanning-tree portfast` should only be enabled on ports connected to end hosts, never
  on inter-switch links.

---

*Sources:*
- *Cisco Consolidated Platform Configuration Guide, IOS Release 15.2(5)E, Catalyst 3560-CX and 2960-CX*
- *Cisco IOS Security Command Reference*
- *Cisco IOS Interface and Hardware Components Configuration Guide, Release 15.2(2)E*
- *Cisco VLAN Configuration Guide, IOS Release 15.2(2)E*
