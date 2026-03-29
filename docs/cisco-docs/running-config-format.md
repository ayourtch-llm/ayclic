# Cisco IOS 15.2 Running Configuration Format Reference
## Platform: Catalyst 3560-CX Series Switches

This document describes the exact format and section ordering of `show running-config`
output on Cisco IOS 15.2 for the Catalyst 3560-CX. Understanding this format is
critical for implementing a faithful mock CLI.

---

## How `show running-config` Works

The running configuration reflects the current active configuration in RAM.
It is NOT the saved (startup) configuration unless explicitly copied.

```
Switch# show running-config
Building configuration...

Current configuration : 3456 bytes
!
...
end
```

The output begins with:
1. `Building configuration...` (on its own line)
2. Blank line
3. `Current configuration : <N> bytes` (where N is the size in bytes)
4. `!` (comment/separator line)
5. Configuration body
6. `end` (final line, always present)

---

## Section Ordering

The running-config sections appear in a specific order that IOS enforces. This
order is FIXED and does not depend on the order you entered commands.

### Complete Section Order

```
version <X.Y>
service ...
service ...
!
hostname <name>
!
boot-start-marker
boot system flash:<image>
boot-end-marker
!
enable secret 5 <hash>
enable password <password>
!
username <name> privilege <level> secret 5 <hash>
!
no aaa new-model        (or aaa configuration)
!
clock timezone ...
clock summer-time ...
!
switch 1 provision ...  (switch stacking if applicable)
!
ip domain-name ...
ip name-server ...
ip cef
no ip domain-lookup
!
ipv6 unicast-routing    (if enabled)
!
spanning-tree mode ...
spanning-tree extend system-id
spanning-tree vlan ... priority ...
!
vlan internal allocation policy ...
!
vlan <id>
 name <name>
!
ip dhcp excluded-address ...
!
ip dhcp pool <name>
 ...
!
ip access-list standard <name>
ip access-list extended <name>
!
interface <first physical interface>
 ...
!
interface <next physical interface>
 ...
!
interface Vlan<id>
 ...
!
ip default-gateway <address>   (or)
ip route ...
!
ip http server    / no ip http server
ip http secure-server    / no ip http secure-server
!
snmp-server ...
!
radius-server ...
tacacs-server ...
!
logging ...
!
ntp ...
!
crypto ...
!
line con 0
 ...
line aux 0          (routers only, not on 3560CX)
line vty 0 4
 ...
line vty 5 15
 ...
!
end
```

---

## Section Details

### 1. Version Line

**Always the first line.**

```
version 15.2
```

Format: `version <major>.<minor>`

Common values for C3560-CX:
- `version 15.2` (all 15.2.x releases use just `15.2`)

### 2. Service Commands

Service configuration lines immediately follow the version. These configure
fundamental IOS behaviors.

Default (factory reset) lines shown. Lines with `no` prefix mean the feature is
off by default:

```
no service pad
service timestamps debug datetime msec
service timestamps log datetime msec
no service password-encryption
```

Common variants when configured:
```
service password-encryption
service timestamps debug datetime msec localtime show-timezone year
service timestamps log datetime msec localtime show-timezone year
service compress-config
no service tcp-small-servers
no service udp-small-servers
```

**Key `service` commands and their defaults:**

| Command | Default | Appears in config when |
|---|---|---|
| `service pad` | OFF | Rarely shown (`no service pad` shown if explicitly disabled) |
| `service timestamps debug datetime msec` | varies | Always shown if timestamps enabled |
| `service timestamps log datetime msec` | varies | Always shown |
| `service password-encryption` | OFF | Shown when enabled |
| `service compress-config` | OFF | Shown when enabled |

**Note:** `no service pad` only appears in config if it was explicitly set.
On factory-fresh C3560CX it IS shown (pad is disabled by default on most switches).

### 3. Hostname

```
!
hostname Switch
!
```

Default hostname for a C3560-CX is `Switch`.

### 4. Boot Markers

```
!
boot-start-marker
boot-end-marker
!
```

These are always present. If boot system commands exist:
```
!
boot-start-marker
boot system flash:c3560cx-universalk9-mz.152-7.E2/c3560cx-universalk9-mz.152-7.E2.bin
boot-end-marker
!
```

On factory default, there may be no `boot system` commands between the markers
(system boots the first image found in flash automatically).

### 5. Enable Password / Secret

Appears only if configured:
```
!
enable secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
!
```

Or if using plaintext (not recommended):
```
!
enable password cleartext-password
!
```

With `service password-encryption` enabled, cleartext passwords are type-7 encrypted:
```
!
enable password 7 0822455D0A16
!
```

Both can appear if different levels are configured:
```
enable secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
enable password 7 0822455D0A16
```

### 6. Username Accounts

```
!
username admin privilege 15 secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
username readonly privilege 1 secret 5 $1$mERr$abcdefghijk1234567890/
!
```

If `service password-encryption` is enabled but username uses cleartext password:
```
username admin privilege 15 password 7 0822455D0A16
```

### 7. AAA Configuration

Default (no AAA):
```
!
no aaa new-model
!
```

With AAA enabled:
```
!
aaa new-model
!
!
aaa authentication login default local
aaa authentication login CONSOLE none
aaa authorization exec default local
aaa accounting exec default start-stop group tacacs+
!
```

### 8. System Settings (clock, memory, etc.)

```
!
clock timezone EST -5 0
clock summer-time EDT recurring
!
system mtu routing 1500
!
```

`system mtu` may appear showing the MTU. On switch-only configs, `system mtu routing`
is relevant.

### 9. IP Settings (Global)

```
!
ip domain-name corp.example.com
ip name-server 8.8.8.8
ip name-server 8.8.4.4
ip cef
!
```

No IP routing (L2-only switch, default):
```
no ip routing
ip default-gateway 192.168.1.1
```

With IP routing enabled:
```
ip routing
```

Other common global IP commands:
```
no ip domain-lookup
ip ssh version 2
ip ssh time-out 120
ip ssh authentication-retries 5
```

### 10. IPv6 Settings

If not configured (default), nothing appears. If enabled:
```
!
ipv6 unicast-routing
!
```

### 11. Spanning Tree Configuration

```
!
spanning-tree mode pvst
spanning-tree extend system-id
!
```

Or with RSTP:
```
!
spanning-tree mode rapid-pvst
spanning-tree extend system-id
!
```

With custom priority:
```
spanning-tree mode rapid-pvst
spanning-tree extend system-id
spanning-tree vlan 1 priority 4096
spanning-tree vlan 10,20 priority 8192
```

Additional global STP settings:
```
spanning-tree portfast default
spanning-tree portfast bpduguard default
spanning-tree loopguard default
```

### 12. VLAN Internal Allocation Policy

Appears on Layer 3 switches:
```
!
vlan internal allocation policy ascending
!
```

### 13. VLAN Definitions

VLANs appear in numerical order:
```
!
vlan 10
 name Engineering
!
vlan 20
 name Marketing
!
vlan 30
 name Servers
!
```

Notes:
- VLAN 1 (default) NEVER appears in running-config (it's always active)
- Only VLANs with non-default names appear here
- VLANs 1002-1005 (legacy) do not appear in running-config unless modified
- A VLAN with default name but custom settings may appear

### 14. DHCP Configuration

```
!
ip dhcp excluded-address 192.168.1.1 192.168.1.10
ip dhcp excluded-address 10.0.0.1
!
ip dhcp pool VLAN1_POOL
 network 192.168.1.0 255.255.255.0
 default-router 192.168.1.1
 dns-server 8.8.8.8 8.8.4.4
 domain-name corp.example.com
 lease 7
!
```

DHCP pool sub-commands are indented with ONE space.

### 15. Access Lists

Named ACLs:
```
!
ip access-list standard MGMT_HOSTS
 permit 192.168.1.0 0.0.0.255
 deny   any
!
ip access-list extended VLAN10_IN
 permit tcp 10.0.0.0 0.0.0.255 any eq www
 permit tcp 10.0.0.0 0.0.0.255 any eq 443
 deny   ip any any log
!
```

Numbered ACLs appear inline with `access-list` commands:
```
access-list 10 permit 192.168.1.0 0.0.0.255
access-list 10 deny   any
```

Note: Numbered ACLs do NOT get their own section header; they appear as flat commands.

### 16. Interface Configuration

**This is the largest section.** Interfaces appear in a specific order:

1. Physical interfaces (GigabitEthernet, TenGigabitEthernet, FastEthernet) in order
2. Virtual interfaces (Port-channel, Tunnel, Loopback)
3. VLAN interfaces (SVI) in order

**C3560-CX interface numbering:**
- `interface GigabitEthernet0/1` through `GigabitEthernet0/12` (for 12-port model)
- `interface TenGigabitEthernet0/1` through `TenGigabitEthernet0/2` (uplink ports)
- `interface Vlan1` (management VLAN SVI, always present)
- Additional VLANs: `interface Vlan10`, `interface Vlan20`, etc.

**Default (unconfigured) interface NOT shown:**
- Interfaces with only default settings do not appear at all

OR they appear as empty (just the interface declaration):
```
!
interface GigabitEthernet0/3
!
```

**This behavior varies:** In some IOS versions, truly-default interfaces are
completely omitted. In others, they appear with no sub-commands.
The C3560-CX typically shows interfaces that have ANY non-default configuration.

#### Access Port Interface Example

```
!
interface GigabitEthernet0/2
 description Server-1-eth0
 switchport access vlan 10
 switchport mode access
 spanning-tree portfast
!
```

#### Trunk Port Interface Example

```
!
interface GigabitEthernet0/1
 description Uplink-to-CoreSwitch
 switchport trunk native vlan 1
 switchport trunk allowed vlan 1,10,20,30
 switchport mode trunk
!
```

#### Port With Port Security

```
!
interface GigabitEthernet0/5
 description Secure-Port
 switchport access vlan 20
 switchport mode access
 switchport port-security maximum 2
 switchport port-security
 switchport port-security violation restrict
 spanning-tree portfast
!
```

#### Port in EtherChannel

```
!
interface GigabitEthernet0/7
 channel-group 1 mode active
!
interface GigabitEthernet0/8
 channel-group 1 mode active
!
interface Port-channel1
 switchport trunk native vlan 1
 switchport trunk allowed vlan 1,10,20
 switchport mode trunk
!
```

#### Shutdown Interface

```
!
interface GigabitEthernet0/4
 shutdown
!
```

OR (if default state is already shutdown, only non-defaults shown):
On switches, physical interfaces default to `no shutdown` state but many
internal/management interfaces default to `shutdown`. Typically `shutdown` appears
explicitly in the config when set.

#### SVI (VLAN Interface) Example

```
!
interface Vlan1
 ip address 192.168.1.10 255.255.255.0
 no shutdown
!
interface Vlan10
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
```

Notes:
- `no shutdown` appears explicitly for SVIs that are up (because their default is shutdown)
- An SVI with no IP address and shutdown might appear as:
  ```
  interface Vlan1
   no ip address
   shutdown
  ```
- The management SVI (Vlan1 by default) is always present in config

#### Interface Sub-commands Order (within an interface block)

Sub-commands within an interface section follow this approximate order:
1. `description`
2. `shutdown` / `no shutdown` (usually omitted if default)
3. Layer 2:
   - `switchport mode`
   - `switchport access vlan`
   - `switchport trunk ...` (if trunk)
   - `switchport voice vlan`
   - `switchport port-security ...`
4. Layer 3:
   - `ip address`
   - `ip helper-address`
   - `ipv6 address`
5. `duplex` (if non-default)
6. `speed` (if non-default)
7. `mtu` (if non-default)
8. `storm-control`
9. `spanning-tree ...`
10. `channel-group` (for EtherChannel members)
11. `ip access-group`
12. `service-policy`
13. `ip ospf ...`

**Indentation:** All sub-commands are indented with exactly ONE space.

### 17. IP Routing Configuration

Static routes:
```
!
ip classless
ip route 0.0.0.0 0.0.0.0 192.168.1.1
ip route 10.0.0.0 255.0.0.0 192.168.1.2
!
```

Default gateway (no routing mode):
```
!
ip default-gateway 192.168.1.1
!
```

### 18. OSPF / Routing Protocol Configuration

```
!
router ospf 1
 router-id 1.1.1.1
 log-adjacency-changes
 network 192.168.1.0 0.0.0.255 area 0
 network 10.0.0.0 0.0.0.255 area 0
 default-information originate
!
```

### 19. HTTP Server

```
!
no ip http server
no ip http secure-server
!
```

Or if enabled:
```
!
ip http server
ip http secure-server
ip http authentication local
!
```

### 20. SNMP Server

```
!
snmp-server community public RO
snmp-server community private RW
snmp-server location "Building A"
snmp-server contact admin@example.com
snmp-server host 192.168.1.100 version 2c public
snmp-server enable traps
!
```

### 21. Logging

```
!
logging buffered 8192
logging trap informational
logging host 192.168.1.100
!
```

### 22. NTP

```
!
ntp server 192.168.1.100 prefer
ntp server pool.ntp.org
!
```

### 23. RADIUS / TACACS+

```
!
radius-server host 192.168.1.200 auth-port 1812 acct-port 1813
radius-server key 7 0822455D0A16
!
```

Or new-style (IOS 15.x preferred):
```
!
radius server RADIUS_SERVER
 address ipv4 192.168.1.200 auth-port 1812 acct-port 1813
 key 7 0822455D0A16
!
```

### 24. Line Configuration

Lines appear at the end of the configuration. Order is:
1. `line con 0` (console)
2. `line aux 0` (auxiliary, not present on 3560-CX switches typically)
3. `line vty 0 4`
4. `line vty 5 15` (if configured separately)

**Console (line con 0):**
```
!
line con 0
 exec-timeout 0 0
 password 7 0822455D0A16
 login
 logging synchronous
 stopbits 1
!
```

Note: `stopbits 1` may appear on some versions as a default. On others it's omitted.

**VTY Lines:**
```
!
line vty 0 4
 access-class 10 in
 exec-timeout 10 0
 password 7 0822455D0A16
 login local
 transport input ssh
 transport output ssh
!
line vty 5 15
 exec-timeout 10 0
 login local
 transport input ssh
!
```

If VTY lines 5-15 have identical config to 0-4, they may share the same block:
```
line vty 0 15
 exec-timeout 10 0
 login local
 transport input ssh
```

**Default VTY (no configuration):**
```
line vty 0 4
 login
```
The `login` command is ALWAYS present (can be `login`, `login local`, or `no login`).
The default is `no login` on most IOS versions (no password required), but best
practice requires at minimum `login` with a password, so a fresh switch may show:
```
line vty 0 4
 login
```

### 25. Final `end` Statement

**The last line of running-config is always `end`.**

```
!
end
```

---

## Complete Example: Factory Default C3560-CX Configuration

A factory-reset C3560-CX running IOS 15.2 would show something like this:

```
Building configuration...

Current configuration : 1743 bytes
!
! Last configuration change at 00:01:23 UTC Mon Mar 1 1993
!
version 15.2
no service pad
service timestamps debug datetime msec
service timestamps log datetime msec
no service password-encryption
!
hostname Switch
!
boot-start-marker
boot-end-marker
!
!
no aaa new-model
!
!
!
!
!
!
!
no ip domain-lookup
ip cef
no ipv6 cef
!
!
!
spanning-tree mode pvst
spanning-tree extend system-id
!
vlan internal allocation policy ascending
!
!
!
!
!
!
!
!
!
!
!
!
!
interface GigabitEthernet0/1
!
interface GigabitEthernet0/2
!
interface GigabitEthernet0/3
!
interface GigabitEthernet0/4
!
interface GigabitEthernet0/5
!
interface GigabitEthernet0/6
!
interface GigabitEthernet0/7
!
interface GigabitEthernet0/8
!
interface GigabitEthernet0/9
!
interface GigabitEthernet0/10
!
interface GigabitEthernet0/11
!
interface GigabitEthernet0/12
!
interface TenGigabitEthernet0/1
!
interface TenGigabitEthernet0/2
!
interface Vlan1
 no ip address
 shutdown
!
ip default-gateway 192.168.1.1
ip classless
!
ip http server
ip http secure-server
!
!
!
!
!
!
line con 0
line vty 0 4
 login
line vty 5 15
 login
!
end
```

Note: The multiple `!` lines (blank sections) represent empty sections that
IOS outputs. The exact number of blank `!` lines varies by version and configured features.

---

## Realistic Configured Example

```
Building configuration...

Current configuration : 4218 bytes
!
version 15.2
no service pad
service timestamps debug datetime msec
service timestamps log datetime msec
service password-encryption
!
hostname Corp-Access-01
!
boot-start-marker
boot-end-marker
!
enable secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
!
username admin privilege 15 secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
!
no aaa new-model
!
clock timezone EST -5 0
clock summer-time EDT recurring
!
no ip domain-lookup
ip domain-name corp.example.com
ip name-server 8.8.8.8
ip cef
no ipv6 cef
!
spanning-tree mode rapid-pvst
spanning-tree extend system-id
spanning-tree portfast default
spanning-tree portfast bpduguard default
!
vlan internal allocation policy ascending
!
vlan 10
 name Engineering
!
vlan 20
 name Marketing
!
vlan 30
 name Servers
!
!
!
interface GigabitEthernet0/1
 description Uplink-to-Distribution
 switchport trunk native vlan 1
 switchport trunk allowed vlan 1,10,20,30
 switchport mode trunk
 spanning-tree portfast trunk
!
interface GigabitEthernet0/2
 description Server-1
 switchport access vlan 30
 switchport mode access
!
interface GigabitEthernet0/3
 description Engineer-Desk
 switchport access vlan 10
 switchport mode access
 spanning-tree portfast
!
interface GigabitEthernet0/4
 description Marketing-Desk
 switchport access vlan 20
 switchport mode access
 spanning-tree portfast
!
interface GigabitEthernet0/5
 description Phone+PC
 switchport access vlan 10
 switchport mode access
 switchport voice vlan 100
 spanning-tree portfast
!
interface GigabitEthernet0/6
!
interface GigabitEthernet0/7
!
interface GigabitEthernet0/8
!
interface GigabitEthernet0/9
!
interface GigabitEthernet0/10
!
interface GigabitEthernet0/11
!
interface GigabitEthernet0/12
!
interface TenGigabitEthernet0/1
!
interface TenGigabitEthernet0/2
!
interface Vlan1
 ip address 192.168.1.10 255.255.255.0
 no shutdown
!
interface Vlan10
 ip address 10.10.0.1 255.255.255.0
!
interface Vlan20
 ip address 10.20.0.1 255.255.255.0
!
interface Vlan30
 ip address 10.30.0.1 255.255.255.0
!
ip default-gateway 192.168.1.1
ip classless
!
no ip http server
no ip http secure-server
!
snmp-server community public RO 10
snmp-server location "Server Room A"
snmp-server contact admin@corp.example.com
!
logging buffered 8192
logging trap informational
logging host 192.168.1.100
!
ntp server 192.168.1.100 prefer
!
!
!
line con 0
 exec-timeout 0 0
 password 7 0822455D0A16
 login
 logging synchronous
!
line vty 0 4
 access-class 10 in
 exec-timeout 10 0
 login local
 transport input ssh
!
line vty 5 15
 access-class 10 in
 exec-timeout 10 0
 login local
 transport input ssh
!
end
```

---

## Rules: What Appears vs. What Doesn't

### Commands That ALWAYS Appear (even at default)

| Command | Notes |
|---|---|
| `version 15.2` | Always first line |
| `service timestamps debug ...` | If timestamps configured |
| `service timestamps log ...` | If timestamps configured |
| `hostname <name>` | Always present |
| `boot-start-marker` | Always present |
| `boot-end-marker` | Always present |
| `no aaa new-model` | When AAA not configured |
| `spanning-tree mode pvst/rapid-pvst` | The mode is always shown |
| `spanning-tree extend system-id` | Always shown on switches |
| `vlan internal allocation policy ascending` | Always shown |
| `interface Vlan1` | The management VLAN SVI is always present |
| `line con 0` | Always present |
| `line vty 0 4` | Always present |
| `end` | Always last line |

### Commands That Only Appear When Changed From Default

| Command | Default | Appears when |
|---|---|---|
| `enable secret` | No password | Configured |
| `enable password` | No password | Configured |
| `username` | None | Configured |
| `no ip domain-lookup` | lookup enabled | disabled |
| `ip domain-name` | None | Configured |
| `ip name-server` | None | Configured |
| `clock timezone` | UTC | Configured |
| `vlan <id>` with `name` | VLANs 2-4094 have numeric default names | When name changed |
| `spanning-tree portfast default` | Disabled | Enabled |
| `no ip routing` | ON for L3 switches | When disabled |
| `ip routing` | Depends on SDM template | When enabled on L2 switch |
| `ip default-gateway` | None | Configured (L2-only switch) |
| `ip route` | None | Configured |
| `no ip http server` | Usually on | When disabled |
| `ntp server` | None | Configured |
| `snmp-server` | None | Configured |
| `logging` | Console only | When changed |
| `shutdown` (interface) | Up | When disabled |
| `description` | None | Configured |
| `switchport mode` | `access` or `dynamic auto` | Configured |
| `spanning-tree portfast` | Off | Enabled |
| `spanning-tree bpduguard enable` | Off (unless global default) | Enabled |

### The `!` Separator Lines

`!` lines in running-config are NOT stored configuration - they are generated output.
They serve as visual separators. IOS generates them:
- After the version line
- After service commands
- After hostname
- After boot markers
- After each major section (aaa, crypto, interface, line, etc.)
- Between interface sections

An empty section (no commands in that area) may generate multiple consecutive `!` lines.

### `no` Commands in Running-Config

Commands explicitly set to their "off" state appear as `no ...` in the config:
- `no service pad`
- `no aaa new-model`
- `no ip domain-lookup`
- `no ip routing`
- `no ip http server`
- `no ip http secure-server`
- `no ipv6 cef`
- `no shutdown` (on SVIs that are admin up)
- `no ip address` (interface with no IP configured)

These represent commands where the feature was explicitly disabled (the opposite
of the compiled-in default). IOS uses "no" commands to show explicitly-set negative states.

---

## `show running-config all` vs `show running-config`

`show running-config` shows only non-default configuration.
`show running-config all` shows EVERY parameter including defaults.

For the mock, standard `show running-config` behavior is appropriate.

`show running-config all` would show thousands of additional lines with all the
compiled-in defaults that are normally hidden.

---

## Interface Sections: Default vs. Configured

### Interface with NO configuration at all

On older IOS, an interface with absolutely no configuration may not appear.
On C3560-CX with IOS 15.2, all physical interfaces appear but may be empty:

```
interface GigabitEthernet0/6
!
```

The `!` after the interface name means no sub-commands (the interface section is empty).

### Interface with only `shutdown`

```
interface GigabitEthernet0/6
 shutdown
!
```

### Typical Access Port (minimally configured)

```
interface GigabitEthernet0/3
 switchport access vlan 10
 switchport mode access
!
```

Note: `no shutdown` is NOT shown for physical interfaces that are up (that's the default).
`no shutdown` IS shown for SVIs (VLAN interfaces) that are up (because their default is shutdown).

---

## Encryption in Running-Config

### Type 5 (MD5 hash) - enable secret, username secret
```
enable secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
username admin privilege 15 secret 5 $1$mERr$hx5rVt7rPNoS4wqbXKX7m0
```

The format is: `5 $1$<salt>$<hash>`
- `$1$` = MD5 algorithm indicator
- `<salt>` = 4-character random salt
- `<hash>` = 22-character base64 hash

### Type 7 (weak obfuscation) - password with encryption
When `service password-encryption` is enabled:
```
enable password 7 0822455D0A16
username admin password 7 045802150C2E
line vty 0 4
 password 7 070C285F4D06
```

Type 7 can be decoded - it is NOT secure.

### Type 0 (cleartext)
```
enable password cleartext
username admin password cleartext
```

### Key ordering in config
`enable secret` takes precedence over `enable password`. Both can coexist
in the configuration, but the secret is always used if present.

---

## The `Current configuration` Header

The first line after `Building configuration...` shows config size:
```
Current configuration : 4218 bytes
```

This byte count changes with every `show running-config` call if the config
was changed. The size includes all characters in the configuration including
newlines.

---

## Comment Lines (`!`) and Structure

Comments (`!`) in running-config:
- Are NOT actual stored commands
- Are generated dynamically during `show running-config`
- Serve as section separators
- Cannot be entered by the user as "comments" in standard IOS 15.2

Some IOS versions do support `remark` commands in ACLs and route-maps, which
appear as `remark <text>` in config, not as `!` lines.
