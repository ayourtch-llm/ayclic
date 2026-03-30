# Cisco IOS 15.2 Running Configuration Format Reference (C3560CX)

This document describes the exact format and section ordering of `show running-config`
output on Cisco IOS 15.2 for the Catalyst 3560-CX. Understanding this format is
critical for parsing, generating, or simulating IOS configurations.

---

## 1. Command Invocation and Initial Output

```
Switch# show running-config
Building configuration...

Current configuration : 4821 bytes
!
```

### Header Lines

1. `Building configuration...` - A status message printed immediately while IOS collects
   the running config from memory. This appears before any configuration content.
   - Followed by a blank line.
2. `Current configuration : NNNN bytes` - The size of the running configuration in bytes.
   - Exactly one space before and after the colon.
   - NNNN is a decimal integer (no commas, no padding).
   - Followed by a blank line.
3. `!` - A comment/separator line. Appears after the header.

The `show startup-config` command has a slightly different header:
```
Using NNNN out of MMMM bytes
!
```

Where NNNN is used bytes and MMMM is total NVRAM available.

---

## 2. Section Ordering in `show running-config`

The running-config sections always appear in this order on IOS 15.2 Catalyst switches.
Not all sections appear in every config; only sections with non-default settings are
shown (with some exceptions noted below).

### 2.1 Version Line

```
version 15.2
```

- Always the very first configuration line.
- Specifies the IOS major.minor version.
- No `!` before this line (it follows immediately after the header `!`).

### 2.2 Service Commands

Appear immediately after the version line. Only non-default service settings appear,
but some commonly seen lines:

```
service timestamps debug datetime msec
service timestamps log datetime msec
no service password-encryption
```

Or if password encryption is enabled:
```
service password-encryption
```

Common service commands in this section:
- `service timestamps debug ...`
- `service timestamps log ...`
- `service password-encryption` (only if explicitly configured)
- `no service pad` (if explicitly disabled)
- `service tcp-keepalives-in`

### 2.3 Hostname

```
!
hostname Switch
!
```

- Always appears, even if it is the default `Switch`.
- Preceded by `!`.

### 2.4 Boot Markers

```
!
boot-start-marker
boot-end-marker
!
```

- These two lines ALWAYS appear in every IOS 15.2 running-config on Catalyst switches.
- They are not actual CLI commands; they are markers placed by IOS to delimit boot
  commands.
- If boot commands are configured, they appear between the markers:
  ```
  boot-start-marker
  boot system flash:c3560cx-universalk9-mz.152-7.E2.bin
  boot-end-marker
  ```
- Even with no boot commands configured, both markers still appear with nothing between
  them.

### 2.5 Enable Secret / Password

```
!
enable secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
!
```

Or if only enable password (no secret) is configured:
```
enable password 7 02050D480809
```

- `enable secret` uses type 5 (MD5) and always appears encrypted regardless of
  `service password-encryption`.
- `enable password` appears with type 7 if `service password-encryption` is active,
  or cleartext otherwise.
- If neither is configured, neither line appears.

### 2.6 AAA Configuration

```
!
aaa new-model
!
aaa authentication login default local
aaa authorization exec default local
!
aaa session-id common
!
```

- `aaa new-model` appears as a single line when AAA is enabled.
- If AAA is not configured, the line `no aaa new-model` does NOT appear in the
  running-config (absence of `aaa new-model` implies it is disabled).
- Individual `aaa authentication`, `aaa authorization`, `aaa accounting` lines follow.

### 2.7 System MTU and Switch Specific Settings

```
!
system mtu routing 1500
!
```

On some switch platforms, system-level settings appear here.

### 2.8 IP Settings (Global)

```
!
ip domain-name example.com
ip name-server 8.8.8.8
ip name-server 8.8.4.4
!
```

- `no ip domain-lookup` appears only if DNS lookup has been disabled.
- `ip routing` appears only if explicitly configured (Layer 3 switches).
- `ip classless` typically does NOT appear (it is the default and hidden).

### 2.9 Login Security Settings

```
!
login block-for 120 attempts 3 within 60
!
```

### 2.10 Username Database

```
!
username admin privilege 15 secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
username operator privilege 7 password 7 02050D480809
!
```

- Each username gets its own line.
- Password field: `secret 5` for MD5, `password 7` for type-7, `password` (no number)
  for cleartext.

### 2.11 VTP (VLAN Trunking Protocol)

```
!
vtp domain MyDomain
vtp mode transparent
!
```

- VTP domain and mode only appear if configured.
- Default VTP mode (server) with no domain typically does not generate lines.

### 2.12 Spanning Tree

```
!
spanning-tree mode rapid-pvst
spanning-tree extend system-id
!
spanning-tree vlan 1 priority 4096
!
```

- `spanning-tree extend system-id` always appears on Catalyst switches (part of MST
  and extended system ID feature, enabled by default).
- Spanning-tree mode appears only if changed from default (but `rapid-pvst` is the
  default on IOS 15.2, so it may appear to explicitly document the mode).
- Per-VLAN priority lines appear only if modified from the default (32768).

### 2.13 Interface Configurations

```
!
interface GigabitEthernet1/0/1
 description Uplink to Router
 switchport trunk encapsulation dot1q
 switchport mode trunk
 spanning-tree portfast trunk
!
interface GigabitEthernet1/0/2
 switchport access vlan 10
 switchport mode access
 spanning-tree portfast
!
```

Interface sections appear in this order:
1. Physical switchport interfaces in numerical order (GigabitEthernet1/0/1,
   GigabitEthernet1/0/2, ...).
2. Uplink/SFP ports (GigabitEthernet and TenGigabitEthernet).
3. Port-channel interfaces (Port-channel1, Port-channel2, ...).
4. VLAN interfaces / SVIs (interface Vlan1, Vlan10, ...) in numerical order.
5. Loopback interfaces (if any).

Interface ordering notes:
- Physical interfaces appear even if no commands have been explicitly configured
  (they show with just the interface header and no subcommands if they are at defaults).
- Actually: on IOS 15.2, interfaces that are completely at defaults (no description,
  default VLAN, no manual speed/duplex, etc.) typically appear in the running-config
  with just their interface line and no sub-commands.

#### Default Interface Lines (What Does NOT Appear)

The following are default settings and do NOT generate lines in the running-config:
- `switchport mode dynamic auto` (the default mode on many ports - may not appear)
- `duplex auto` (default - not shown)
- `speed auto` (default - not shown)
- `no shutdown` (default for most ports - not shown; `shutdown` appears when an
  interface IS shut down)
- `spanning-tree portfast disable` (default - not shown)
- `switchport access vlan 1` (VLAN 1 is default - not shown unless explicitly set)

Lines that DO appear even for default/common configurations:
- `shutdown` when the interface is administratively down (this is notable because
  shutdown is the default state for many IOS router interfaces but NOT for Catalyst
  switch access ports).
- On Catalyst switches, physical ports default to `no shutdown` (enabled). Shutdown
  appears in the config only when explicitly configured.

#### Interface Sub-Command Order within an Interface Block

When multiple subcommands are configured on an interface, they appear in this order:

```
interface GigabitEthernet1/0/1
 description LINK TO CORE
 switchport trunk encapsulation dot1q
 switchport trunk native vlan 100
 switchport trunk allowed vlan 1,10,20,100
 switchport mode trunk
 switchport nonegotiate
 ip address 192.168.1.1 255.255.255.0    (only on routed ports)
 ip helper-address 10.0.0.1
 ip ospf 1 area 0
 channel-group 1 mode active
 spanning-tree portfast
 spanning-tree bpduguard enable
 no cdp enable
 storm-control broadcast level 20
 shutdown
```

The ordering IOS uses within an interface block follows a consistent internal priority:
1. `description`
2. `switchport` commands (encapsulation, then native vlan, then allowed vlan, then mode,
   then nonegotiate, then other switchport commands)
3. IP addressing commands (`ip address`, secondary addresses)
4. IP service commands (`ip helper-address`, `ip access-group`, `ip ospf`, etc.)
5. Routing protocol interface commands
6. EtherChannel (`channel-group`)
7. Spanning-tree commands
8. CDP/LLDP commands
9. Storm-control
10. `shutdown` (always last if present)

### 2.14 IP Routing Protocols

```
!
router ospf 1
 log-adjacency-changes
 network 192.168.1.0 0.0.0.255 area 0
!
```

- Each routing protocol (`router ospf N`, `router eigrp N`, `router bgp N`) gets its
  own section.
- The section begins with the `router` line, followed by indented subcommands.
- `log-adjacency-changes` is often shown as it is the default enabled state; it may
  appear by default.

### 2.15 IP Access Lists (ACLs)

Named ACLs appear in their own sections:

```
!
ip access-list standard MGMT-HOSTS
 permit 10.0.0.0 0.255.255.255
 deny   any
!
ip access-list extended DENY-TELNET
 deny   tcp any any eq 23
 permit ip any any
!
```

Numbered ACLs appear as:
```
!
access-list 10 permit 10.0.0.0 0.255.255.255
access-list 10 deny   any
!
```

Note the spacing: numbered ACL lines have variable-width spacing between `permit`/`deny`
and the address (IOS aligns the address arguments).

### 2.16 SNMP Configuration

```
!
snmp-server community public RO
snmp-server community private RW
snmp-server location "Server Room"
snmp-server contact admin@example.com
!
```

### 2.17 Logging Configuration

```
!
logging buffered 16384
logging 192.168.1.100
logging trap informational
!
```

### 2.18 NTP Configuration

```
!
ntp server 192.168.1.1
ntp server 192.168.1.2 prefer
!
```

### 2.19 Banner Configuration

Banners appear as a block:

```
!
banner motd ^C
*************************************
* WARNING: Authorized access only! *
*************************************
^C
!
banner login ^C
Authenticate yourself.
^C
!
```

Notes:
- The delimiter in the saved config is typically `^C` (Control-C character, displayed
  as `^C`).
- The banner text can span multiple lines.
- Each banner type (motd, login, exec, incoming) appears separately.

### 2.20 Line Configuration

Line sections appear in this order: `line con 0`, `line aux 0` (if present), then
`line vty 0 4`, `line vty 5 15` (or whatever ranges are configured).

```
!
line con 0
 exec-timeout 5 0
 logging synchronous
 login local
!
line aux 0
!
line vty 0 4
 access-class MGMT-HOSTS in
 exec-timeout 15 0
 logging synchronous
 login local
 transport input ssh
!
line vty 5 15
 access-class MGMT-HOSTS in
 exec-timeout 15 0
 logging synchronous
 login local
 transport input ssh
!
```

#### Default Line Settings (What Does NOT Appear)

- `transport input telnet` may or may not appear (it is a legacy default that varies
  by IOS version; in hardened configs it is explicit).
- `no exec-timeout` does not appear; to show no timeout it shows as `exec-timeout 0 0`.
- `privilege level 1` (default level for vty) does not appear.
- `history size 10` (default) does not appear.

#### Line Sub-Command Order

Within a line block, commands appear in this typical order:
1. `exec-timeout`
2. `absolute-timeout`
3. `password` (if configured)
4. `login` / `login local`
5. `privilege level`
6. `logging synchronous`
7. `history size`
8. `transport input`
9. `transport output`
10. `access-class`
11. `length` / `width`

### 2.21 End Marker

```
!
end
```

- The very last line of the configuration.
- Preceded by `!`.
- Followed by a newline (the prompt appears on the next line).

---

## 3. Full Annotated Example

Here is a representative `show running-config` output for a minimally configured
C3560CX switch:

```
Building configuration...

Current configuration : 3142 bytes
!
version 15.2
service timestamps debug datetime msec
service timestamps log datetime msec
no service password-encryption
!
hostname SW1
!
boot-start-marker
boot-end-marker
!
!
enable secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
!
username admin privilege 15 secret 5 $1$abc1$Kj0sQbXKH7mPNoS8rVt3m1
!
!
!
ip domain-name example.com
ip name-server 8.8.8.8
!
!
!
spanning-tree mode rapid-pvst
spanning-tree extend system-id
!
!
!
!
!
interface GigabitEthernet1/0/1
 description To-Router
 switchport mode access
 switchport access vlan 10
 spanning-tree portfast
!
interface GigabitEthernet1/0/2
!
interface GigabitEthernet1/0/3
 shutdown
!
interface GigabitEthernet1/0/4
 switchport trunk encapsulation dot1q
 switchport mode trunk
!
interface Vlan1
 ip address 192.168.1.1 255.255.255.0
 no shutdown
!
interface Vlan10
 ip address 10.10.10.1 255.255.255.0
!
!
ip default-gateway 192.168.1.254
!
!
!
!
!
no ip http server
!
!
!
!
!
!
line con 0
 exec-timeout 5 0
 logging synchronous
 login local
!
line aux 0
!
line vty 0 4
 exec-timeout 15 0
 logging synchronous
 login local
 transport input ssh
!
line vty 5 15
 exec-timeout 15 0
 logging synchronous
 login local
 transport input ssh
!
!
end
```

---

## 4. Password Encryption Formats

### Type 5 (MD5 Hash)

Used by `enable secret` and `username NAME secret`. Always hashed, never cleartext.

Format:
```
enable secret 5 $1$SALT$HASHHASHHASHHASHHASHHASH
```

- `5` is the encryption type indicator.
- The hash is in the format `$1$SALT$MD5HASH` (standard Unix MD5 crypt format).
- The salt is 4 characters; the hash is 22 characters (Base64-encoded).
- This cannot be reversed to get the plaintext password.

Example:
```
enable secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
```

### Type 7 (Vigenere Cipher / Weak Encryption)

Used by `enable password`, `line password`, and `username NAME password` when
`service password-encryption` is active.

Format:
```
enable password 7 13061E010803
```

- `7` is the encryption type indicator.
- The encrypted value is a hexadecimal string.
- This is easily reversible and should not be considered secure.

### Cleartext (Type 0)

When `service password-encryption` is NOT configured:

```
enable password MyPassword
```

or explicitly:

```
enable password 0 MyPassword
```

- No type number, or type `0`, means plaintext.

### Type 8 (PBKDF2-SHA256) and Type 9 (SCRYPT)

Available in newer IOS 15.2 releases and IOS-XE:

```
enable secret 9 $9$SALT/HASH
username admin secret 8 $8$salt$hashhash...
```

- Type 8 uses PBKDF2 with SHA-256.
- Type 9 uses SCRYPT.
- These are the recommended strong hashing algorithms.

### How Passwords Appear per Context

| Config Context | `service password-encryption` OFF | `service password-encryption` ON |
|---------------|----------------------------------|----------------------------------|
| `enable secret` | Type 5 hash (always) | Type 5 hash (unchanged) |
| `enable password` | Cleartext | Type 7 |
| `line password` | Cleartext | Type 7 |
| `username ... password` | Cleartext | Type 7 |
| `username ... secret` | Type 5 hash (always) | Type 5 hash (unchanged) |
| Community strings (SNMP) | Cleartext | Type 7 |
| BGP neighbor passwords | Cleartext | Type 7 |

---

## 5. What Appears by Default vs. Only When Changed

### Always Appears (Present Even at Default)

- `version X.X`
- `hostname SWITCH` (even if it is the default `Switch`)
- `boot-start-marker` and `boot-end-marker`
- `spanning-tree extend system-id`
- Interface sections for all physical ports (at minimum the interface header line)
- `line con 0`, `line aux 0` (even if empty)
- `line vty 0 4` and `line vty 5 15` sections
- `end`

### Never Appears at Default (Only Shown When Configured)

- `service password-encryption` - only when explicitly enabled
- `enable secret` / `enable password` - only when set
- `aaa new-model` - only when enabled
- `ip routing` - only when enabled (Layer 3 switch mode)
- `ip domain-name` - only when set
- `ip name-server` - only when set
- `username` entries - only when created
- `interface Vlan N` (except possibly Vlan1) - only when configured
- `router ospf N` section - only when OSPF is configured
- ACL sections - only when ACLs are defined
- `banner motd` etc. - only when banners are set
- `snmp-server` lines - only when SNMP is configured
- `logging` lines (beyond defaults) - only when configured
- `ntp server` - only when NTP is configured
- `vtp domain` / `vtp mode` - only when VTP is explicitly configured

### Appears When Explicitly Disabled (Negated Commands)

Some default-ON features generate `no` lines when disabled:

- `no ip domain-lookup` - when DNS lookup is disabled
- `no cdp run` - when CDP is globally disabled
- `no ip http server` - when HTTP server is disabled (common hardening step)
- `no logging console` - when console logging is disabled
- `no service pad` - when PAD is disabled

---

## 6. Indentation Format

IOS uses a single space (` `) to indent subcommands within a section (interface, line,
router, etc.). This is consistent across all IOS versions.

```
interface GigabitEthernet1/0/1
 description My Interface        <- one space indent
 switchport mode access          <- one space indent
 spanning-tree portfast          <- one space indent
!
```

There are NO tabs in Cisco IOS running-config output; all indentation is a single space.

---

## 7. Comment Lines (`!`)

Exclamation marks (`!`) serve as section separators/comments. They are inserted:
- After the initial header.
- Between most top-level configuration blocks.
- Before and after major sections.
- At the end of each interface/line/router block (the `!` on its own line after the
  block's last command).
- Before the final `end`.

IOS sometimes inserts multiple consecutive `!` lines between sections. The number of
`!` lines can vary; parsers should treat any number of consecutive `!` lines as a
section separator.

Example of multiple `!` lines (common after the boot-end-marker section):
```
boot-end-marker
!
!
enable secret 5 $1$...
```

---

## 8. Interface Section Details

### Physical Ports at Default

A switch port with no explicit configuration appears as:

```
!
interface GigabitEthernet1/0/2
!
```

Just the interface name followed by an exclamation mark (empty section). The port is
in its default state: switchport, VLAN 1, dynamic auto mode, auto speed/duplex, no
description, administratively up.

### Shutdown Port

```
!
interface GigabitEthernet1/0/3
 shutdown
!
```

The word `shutdown` is the only line, indented one space.

### Routed Port (no switchport)

```
!
interface GigabitEthernet1/0/10
 no switchport
 ip address 10.0.0.1 255.255.255.252
 no shutdown
!
```

Note: `no shutdown` appears explicitly on SVIs because they default to shutdown when
first created. Physical ports may not show `no shutdown` since they default to enabled.

### SVI (interface Vlan N)

```
!
interface Vlan1
 ip address 192.168.1.1 255.255.255.0
!
interface Vlan10
 description Management
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
```

SVIs default to shutdown state when created, so `no shutdown` appears when they are
enabled.

---

## 9. VLAN Database and running-config

For VLANs 1-1005, the VLAN configuration (names, states) is stored in `vlan.dat` on
flash, NOT in the running-config. Therefore:

- `show running-config` does NOT show VLAN name assignments for VLANs 1-1005.
- VLAN information for those VLANs is seen with `show vlan brief` or `show vlan`.
- Extended range VLANs (1006-4094) in VTP transparent mode ARE stored in running-config:
  ```
  vlan 2000
   name Extended-Vlan
  !
  ```

---

## 10. `show running-config` Variants

| Command | Description |
|---------|-------------|
| `show running-config` | Display the current running configuration (non-default settings). |
| `show running-config all` | Show all settings including defaults (very long output). |
| `show running-config interface GigabitEthernet1/0/1` | Show only the config for one interface. |
| `show running-config | section interface` | Show all interface sections. |
| `show running-config | include hostname` | Show only lines containing "hostname". |
| `show running-config | begin router` | Show from the first line containing "router". |
| `show startup-config` | Show the saved configuration from NVRAM. |
| `write terminal` | Legacy equivalent of `show running-config`. |

---

## 11. `show startup-config` Header Difference

```
Switch# show startup-config
Using 3142 out of 524288 bytes
!
version 15.2
...
```

- Header is `Using NNNN out of MMMM bytes` (no "Building configuration..." line).
- The rest of the format is identical to running-config.
- If NVRAM is empty (factory default or after `erase startup-config`):
  ```
  startup-config is not present
  ```

---

## 12. Configuration Size Byte Count

The byte count in `Current configuration : NNNN bytes` reflects the exact number of
bytes in the text representation of the running configuration (including newlines).
This count:
- Increases when commands are added.
- Decreases when commands are removed.
- Changes when the configuration is modified.
- Is recalculated each time `show running-config` is run.

The byte count does NOT include the header lines themselves ("Building configuration...",
"Current configuration : N bytes").

---

*Sources:*
- *Cisco Consolidated Platform Configuration Guide, IOS Release 15.2(5)E, Catalyst 3560-CX*
- *Cisco IOS Password Encryption documentation*
- *Cisco Learning Network: boot-start-marker and boot-end-marker explanation*
- *Cisco IOS Configuration Fundamentals Command Reference*
- *show running-config - Cisco E-Learning Command Reference*
