# Cisco IOS 15.2 CLI Behavior Guide
## Platform: Catalyst 3560-CX Series Switches

This document covers the exact behavior of the Cisco IOS command-line interface
for implementing a faithful mock/simulator. Every nuance matters for correctness.

---

## 1. Command Modes and Prompts

### Mode Hierarchy

```
Switch>                      ← User EXEC mode (privilege level 1)
Switch#                      ← Privileged EXEC mode (privilege level 15)
Switch(config)#              ← Global configuration mode
Switch(config-if)#           ← Interface configuration mode
Switch(config-line)#         ← Line configuration mode
Switch(config-router)#       ← Router protocol configuration mode
Switch(config-vlan)#         ← VLAN configuration mode
Switch(config-mst)#          ← MST configuration mode
Switch(config-dhcp)#         ← DHCP pool configuration mode
Switch(dhcp-config)#         ← DHCP pool configuration (alternate prompt form)
Switch(config-std-nacl)#     ← Standard named ACL mode
Switch(config-ext-nacl)#     ← Extended named ACL mode
Switch(config-pmap)#         ← Policy map configuration mode
Switch(config-pmap-c)#       ← Policy map class configuration mode
Switch(config-cmap)#         ← Class map configuration mode
Switch(vlan)#                ← Legacy VLAN database mode (deprecated)
```

### Mode Transitions

| Current Mode | Command | Next Mode |
|---|---|---|
| `Switch>` | `enable` | `Switch#` |
| `Switch#` | `disable` | `Switch>` |
| `Switch#` | `configure terminal` | `Switch(config)#` |
| `Switch(config)#` | `interface Gi0/1` | `Switch(config-if)#` |
| `Switch(config)#` | `line vty 0 4` | `Switch(config-line)#` |
| `Switch(config)#` | `router ospf 1` | `Switch(config-router)#` |
| `Switch(config)#` | `vlan 10` | `Switch(config-vlan)#` |
| Any config mode | `exit` | One level up |
| Any config mode | `end` | `Switch#` |
| Any config mode | Ctrl+Z | `Switch#` |

### Prompt Format Rules

- The hostname portion of the prompt changes immediately when `hostname` is configured
- In nested configuration modes, the mode identifier appears in parentheses
- Interface mode shows abbreviated interface name: `Switch(config-if)#`
  (not `Switch(config-GigabitEthernet0/1)#`)
- VLAN mode: `Switch(config-vlan)#`
- Sub-interface mode: `Switch(config-subif)#`

---

## 2. Command Abbreviation (Truncation)

### Rules

1. Commands can be abbreviated to the minimum unique prefix within that command mode.
2. The abbreviation must uniquely identify ONE command from all available commands.
3. If the abbreviation matches multiple commands, an "% Ambiguous command" error occurs.
4. Keywords within commands can also be abbreviated independently.

### Examples

```
Switch# conf t                  ! configure terminal
Switch# sh ver                  ! show version
Switch# sh int gi0/1            ! show interfaces GigabitEthernet0/1
Switch# sh ip int br            ! show ip interface brief
Switch# sh run                  ! show running-config
Switch# no sh                   ! Ambiguous - could be no shutdown, etc.
Switch# cop run star            ! copy running-config startup-config
Switch# int gi0/1               ! (in config mode) interface GigabitEthernet0/1
```

### Ambiguity Example

```
Switch# s
% Ambiguous command: "s"
Switch# se
% Ambiguous command: "se"      ! send, set, setup, ...
Switch# sen
send                           ! Now unique
```

### Interface Name Abbreviation

Interface names follow their own abbreviation rules:
- `GigabitEthernet` = `gi`, `gig`, `GigabitE`, etc.
- `TenGigabitEthernet` = `te`, `TenGig`, etc.
- `FastEthernet` = `fa`, `FastE`, etc.
- `Vlan` = `vl`, `Vlan` (case insensitive)
- `Port-channel` = `po`, `Port-c`, etc.

Interface numbering: always required in full (e.g., `0/1` not abbreviated).

---

## 3. The `?` Help System

### Types of `?` Usage

#### 3.1 End-of-Line `?` (standalone on empty or command line)

Typing `?` at the end of a prompt (with a space before it) shows all available
commands or subcommands:

```
Switch# ?
Exec commands:
  <1-99>      Session number to resume
  clear       Reset functions
  ...
```

```
Switch# show ?
  aaa         Show AAA values
  access-lists  List access lists
  ...
```

#### 3.2 Inline `?` (no space before `?`)

Typing `?` immediately after characters (no space) shows completions for what
you've typed so far:

```
Switch# sh?
  show
Switch# sho?
  show
```

This is equivalent to partial-tab-completion display — shows what commands
START WITH the characters typed.

#### 3.3 Argument Help

Typing `?` after a command (with space) shows what can come next:

```
Switch# show interfaces ?
  GigabitEthernet       GigabitEthernet IEEE 802.3z
  TenGigabitEthernet    TenGigabitEthernet IEEE 802.3
  Vlan                  Catalyst Vlans
  counters              Show interface counters
  ...
  <cr>
```

The `<cr>` entry means the command is complete and can be executed as-is.

#### 3.4 `?` Within a Partial Word

```
Switch# show int?
  interfaces
```

Shows completions without executing anything. The `?` is NOT echoed.

### How `?` Works (Implementation Details)

- When you press `?` at the end of a partial word (no space), the system:
  1. Shows matching commands
  2. **Redisplays the prompt and the partial command you typed** so you can continue
  3. Does NOT add any characters to the command line

```
Switch# sh?
show
Switch# sh_        ← cursor returns here (user sees their partial input again)
```

- When you press `?` after a space:
  1. Shows all valid next tokens
  2. **Redisplays the prompt and command so far**
  3. Cursor is positioned at end of what was typed

```
Switch# show ?
  aaa     Show AAA values
  ...
Switch# show _     ← cursor returns here
```

---

## 4. Tab Completion

### Rules

1. Press Tab after typing a partial command to complete it.
2. If the partial string uniquely matches ONE command, it is completed automatically.
3. If multiple commands match, the terminal beeps (no completion) — user must type more.
4. A double-Tab or `?` after partial input shows all matching options.
5. Tab completion works for:
   - Command keywords
   - Subcommand keywords
   - Some argument types (interface names, VLAN names if configured)
6. Tab completion does NOT work for free-form arguments (IP addresses, descriptions, etc.)

### Examples

```
Switch# sh<Tab>         → Switch# show
Switch# show ve<Tab>    → Switch# show version
Switch# show i<Tab>     → (beep - ambiguous: interfaces, ip, ipv6, inventory, isis)
Switch# show in<Tab>    → Switch# show interfaces (if unique prefix)
Switch# show ip <Tab>   → (shows all ip subcommands or completes if unique)
```

### Partial completion display

If multiple matches, pressing Tab may display partial completion up to ambiguity:
```
Switch# show i<Tab>     → Switch# show i    (beep, no display of options)
```
The user must press `?` to see the options.

---

## 5. Error Messages

### 5.1 Invalid Input Detected

Format:
```
% Invalid input detected at '^' marker.
```

The `^` (caret) appears on a separate line below the command, pointing to the
FIRST character of the erroneous token.

Examples:
```
Switch# show versin
             ^
% Invalid input detected at '^' marker.
```

```
Switch(config)# ip adress 10.0.0.1 255.255.255.0
                   ^
% Invalid input detected at '^' marker.
```

```
Switch(config)# show running-config
               ^
% Invalid input detected at '^' marker.
```
(show is not valid in config mode without `do`)

Implementation note for caret position:
- Count characters from start of command line
- Point to the start of the first unrecognized/invalid token
- The `^` must be on the line BELOW the command (no blank line between)
- Spacing: the `^` must align with the start of the bad word

### 5.2 Incomplete Command

Format:
```
% Incomplete command.
```

Occurs when a command requires more arguments but none were given:
```
Switch# show
% Incomplete command.
Switch# copy
% Incomplete command.
Switch(config)# interface
% Incomplete command.
```

Note: No `^` marker for incomplete commands.

### 5.3 Ambiguous Command

Format:
```
% Ambiguous command: "text"
```

The text in quotes is what the user typed:
```
Switch# sh i
% Ambiguous command: "sh i"
Switch# co t
% Ambiguous command: "co t"
```

Note: The quoted string includes all words typed, not just the ambiguous one.

### 5.4 Other Common Error Messages

```
% No such file or directory
% Cannot find filesystem
% Error in authentication.
% Access denied
% Invalid interface type
% Invalid interface number
% VLAN 10 not found in current VLAN database
% Interface not found
% Unknown command or computer name, or unable to find computer address
% Translating "hostname"...domain server (255.255.255.255)   ← DNS lookup attempt
```

For an unknown hostname after a command that expects one:
```
Switch# ping notahost
Translating "notahost"...domain server (255.255.255.255)
% Unrecognized host or address, or protocol not running.
```
(This happens when DNS lookup is enabled - `ip domain-lookup`)

With `no ip domain-lookup`:
```
Switch# ping notahost
Translating "notahost"
% Unknown command or computer name, or unable to find computer address
```

---

## 6. The `no` Form of Commands

### Purpose

`no` is a prefix that negates a command or restores it to its default state.

### Rules

1. `no` followed by a command keyword removes or reverses the configuration.
2. In most cases `no` removes the configuration entirely.
3. In some cases `no` restores the default value.
4. `no` works in all configuration modes.

### Examples

```
Switch(config)# hostname MySwitch      ! Set hostname
Switch(config)# no hostname            ! Restore default hostname ("Switch" or "Router")

Switch(config-if)# shutdown            ! Disable interface
Switch(config-if)# no shutdown         ! Enable interface

Switch(config)# spanning-tree vlan 1 priority 4096
Switch(config)# no spanning-tree vlan 1 priority   ! Restore default priority (32768)

Switch(config)# access-list 10 permit 192.168.1.0 0.0.0.255
Switch(config)# no access-list 10     ! Delete entire ACL 10

Switch(config)# snmp-server community public RO
Switch(config)# no snmp-server community public    ! Remove this community
Switch(config)# no snmp-server                     ! Remove ALL SNMP config
```

### `default` Form

Some commands have a `default` keyword to explicitly restore defaults:
```
Switch(config-if)# default interface GigabitEthernet0/1  ! Reset interface to defaults
Switch(config)# default spanning-tree vlan 1 priority
```

---

## 7. The `do` Command (EXEC Commands in Configuration Mode)

### Purpose

The `do` command allows execution of EXEC-mode commands from within any configuration
mode without having to exit configuration mode.

### Usage

```
Switch(config)# do show running-config
Switch(config)# do show interfaces
Switch(config-if)# do show ip interface brief
Switch(config)# do copy running-config startup-config
Switch(config)# do ping 192.168.1.1
Switch(config)# do write memory
```

### Behavior Notes

- `do` is available in ALL configuration modes (global, interface, line, router, etc.)
- After `do` completes, you remain in the configuration mode you were in
- `do ?` shows all exec commands available
- `do` itself is NOT added to the running configuration
- Tab completion works after `do`: `Switch(config)# do sh<Tab>`
- `do` cannot be used to enter another mode (e.g., `do configure terminal` is invalid)

---

## 8. Pipe Filtering (`|`)

### Available Filter Operators

The pipe character `|` can follow any `show` or `more` command:

```
Switch# show running-config | ?
  append      Append redirected output to URL (URLs supporting append operation only)
  begin       Begin with the line that matches
  count       Count number of lines which match regexp
  exclude     Exclude lines that match
  grep        Linux-style grep
  include     Include lines that match
  no-more     Turn off pagination for command output
  redirect    Redirect output to URL
  section     Filter a section of output
  tee         Copy output to URL
```

### `| include <regex>`

Displays ONLY lines containing the regular expression:
```
Switch# show interfaces | include GigabitEthernet
Switch# show interfaces | include "is up"
Switch# show running-config | include hostname
Switch# show ip route | include 192.168
Switch# show mac address-table | include Gi0/1
```

### `| exclude <regex>`

Displays all lines EXCEPT those containing the regex:
```
Switch# show interfaces | exclude "line protocol"
Switch# show running-config | exclude "^!"  ! Remove comment lines
```

### `| begin <regex>`

Starts display from the FIRST line matching the regex, then shows all remaining lines:
```
Switch# show running-config | begin interface
Switch# show running-config | begin hostname
Switch# show version | begin "System image"
```

### `| section <regex>`

Shows only the sections (configuration blocks) that contain a match. A "section"
is defined as a block of lines that starts with the regex and ends when indentation
returns to the same level (or a new top-level keyword starts).

Very useful for filtering running-config:
```
Switch# show running-config | section interface
Switch# show running-config | section "interface GigabitEthernet"
Switch# show running-config | section ospf
Switch# show running-config | section vlan
```

Example:
```
Switch# show running-config | section "interface Vlan"
interface Vlan1
 ip address 192.168.1.1 255.255.255.0
 no shutdown
interface Vlan10
 ip address 10.0.0.1 255.255.255.0
```

### `| count`

Counts the number of lines that match the pattern:
```
Switch# show ip route | count Connected
3
Switch# show interfaces | count "is up"
5
```

### `| grep`

Linux-style grep with additional options:
```
Switch# show running-config | grep hostname
Switch# show running-config | grep -v "^!"   ! Exclude comment lines
Switch# show running-config | grep -c hostname  ! Count matches
```

Note: `grep` and `include`/`exclude` cannot be chained in standard IOS.

### Regex in Pipe Filters

- Patterns are basic regular expressions (similar to POSIX BRE)
- Case-sensitive by default
- Common regex metacharacters supported: `.`, `*`, `^`, `$`, `[`, `]`, `\`
- The pattern `^` matches start of line
- The pattern `$` matches end of line
- Example: `| include ^interface` matches lines starting with "interface"

### Chaining Pipes

Standard IOS does NOT support chaining multiple pipes:
```
Switch# show run | include interface | include Vlan    ! NOT supported in IOS 15.2
```
Only ONE pipe filter per command in standard IOS.

### `| redirect`

Redirects output to a file:
```
Switch# show running-config | redirect flash:myconfig.txt
Switch# show tech-support | redirect tftp://192.168.1.100/techsupport.txt
```

### `| no-more`

Disables `--More--` paging for this single command:
```
Switch# show running-config | no-more
```
Equivalent to `terminal length 0` but only affects this one command.

---

## 9. Terminal Length and the `--More--` Prompt

### Default Behavior

- Default terminal length: 24 lines
- When output exceeds the terminal length, IOS pauses with `--More--`
- The `--More--` prompt appears at the bottom of the screen

### `--More--` Prompt Behavior

When `--More--` appears:
- Press **Space** to display the next full page (terminal length lines)
- Press **Enter** (Return) to display one more line
- Press **q** to quit and stop the output
- Press **-** (dash) to display the next page minus one line (rare)
- Any other key: behavior is platform-dependent (often advances one line or quits)

The `--More--` prompt itself:
```
 --More--
```
(with a space before and after on some versions)

After `q` is pressed, the `--More--` line is cleared and the prompt returns.

### `terminal length` Command

```
Switch# terminal length ?
  <0-512>  Number of lines on screen (0 for no pausing)

Switch# terminal length 0      ! Disable paging (show all at once)
Switch# terminal length 24     ! Default - page every 24 lines
Switch# terminal length 50     ! Page every 50 lines
```

Setting `terminal length 0` is the standard practice for automated scripts/SSH
sessions where paging is undesirable.

`terminal length` setting is per-session (not saved to configuration).
It resets when the session ends.

To set default length for a line:
```
Switch(config)# line vty 0 15
Switch(config-line)# length 0    ! Save length 0 as default for VTY
```

### `terminal width`

```
Switch# terminal width ?
  <0-512>  Number of characters on a screen line (0 for no wrapping)

Switch# terminal width 80
Switch# terminal width 132
Switch# terminal width 0
```

---

## 10. Keyboard Shortcuts and Editing

### Cursor Movement

| Key | Action |
|-----|--------|
| Ctrl+A | Move cursor to beginning of line |
| Ctrl+E | Move cursor to end of line |
| Ctrl+B or Left Arrow | Move cursor back one character |
| Ctrl+F or Right Arrow | Move cursor forward one character |
| Esc+B | Move cursor back one word |
| Esc+F | Move cursor forward one word |

### Editing

| Key | Action |
|-----|--------|
| Ctrl+D | Delete character at cursor |
| Ctrl+H or Backspace | Delete character before cursor |
| Ctrl+K | Delete from cursor to end of line (cut to buffer) |
| Ctrl+U or Ctrl+X | Delete entire line (cut to buffer) |
| Ctrl+W | Delete word before cursor |
| Esc+D | Delete word after cursor |
| Ctrl+Y | Paste (yank) from cut buffer |
| Ctrl+T | Transpose characters (swap current and previous char) |

### History Navigation

| Key | Action |
|-----|--------|
| Ctrl+P or Up Arrow | Previous command in history |
| Ctrl+N or Down Arrow | Next command in history |
| Ctrl+R | Recall last search/command (refresh display) |

### Session Control

| Key | Context | Action |
|-----|---------|--------|
| Ctrl+Z | Configuration mode | Exit config, return to privileged EXEC |
| Ctrl+Z | User EXEC | No effect (or may log out, platform dependent) |
| Ctrl+C | During command | Abort in-progress operation |
| Ctrl+C | During `--More--` | Stop output |
| Ctrl+C | At CLI prompt | Cancel current input line |
| Ctrl+Shift+6 (or Ctrl+^) | During ping/traceroute | Abort sequence (escape sequence) |
| Ctrl+Shift+6 x | During connected session | Return to local switch |

### Enhanced Editing Mode

Enhanced editing is enabled by default. Can be toggled:
```
Switch# terminal editing        ! Enable (default)
Switch# terminal no editing     ! Disable (use for dumb terminals)
```

With editing disabled, cursor movement keys don't work; only backspace/delete.

---

## 11. Ctrl+Z and Ctrl+C Behaviors in Detail

### Ctrl+Z

- In any configuration mode (global, interface, line, router, etc.): exits ALL config modes and returns to privileged EXEC (`Switch#`)
- Does NOT save any configuration; changes already entered remain in running-config
- Equivalent to typing `end`
- In user EXEC mode (`Switch>`): usually no effect

### Ctrl+C

- At any prompt: cancels the current input (clears the line)
- During a `ping` or `traceroute` in progress: aborts the operation
- During `--More--` prompt: stops displaying more output
- During password entry: may abort the login sequence

### Escape Sequence (Ctrl+Shift+6, then x)

Used to escape from remote connections or running commands back to the local switch:
```
Switch# telnet 192.168.1.1
... (telnet session) ...
<Ctrl+Shift+6, x>           ! Returns to Switch# prompt
Switch#
```

The escape sequence is shown as `^^X` or `Ctrl+^` in some documentation.
The actual sequence is: press and release Ctrl+Shift+6, then press x.

---

## 12. Command History

### Configuration

```
Switch# terminal history size ?
  <0-256>   Number of history entries remembered (0 disables history)

Switch# terminal history size 20    ! Store 20 commands
Switch# terminal history            ! Enable history (default)
Switch# terminal no history         ! Disable history
```

Line-level default:
```
Switch(config-line)# history size 20
```

### Viewing History

```
Switch# show history
  show version
  show interfaces
  copy running-config startup-config
  ...
```

---

## 13. Command Modes - Navigation Summary

### Mode Abbreviations in Help Text

IOS uses these abbreviations in help text:
- `EXEC` = user EXEC mode
- `#` = privileged EXEC mode
- `(config)#` = global configuration mode
- `(config-if)#` = interface configuration mode
- `(config-line)#` = line configuration mode
- `(config-router)#` = router configuration mode

### Common Navigation Commands

```
Switch(config-if)# exit           ! Return to Switch(config)#
Switch(config-if)# end            ! Return to Switch# (privileged EXEC)
Switch(config)# exit              ! Return to Switch# (privileged EXEC)
Switch#  exit                     ! Return to Switch> (user EXEC)
Switch>  exit                     ! Logout / disconnect
Switch#  disable                  ! Return to Switch> (user EXEC)
```

---

## 14. Configuration Mode Specifics

### Global Config Only Commands

Some commands only work in global config mode, not interface mode. Typing a
global config command in interface mode generates an error:
```
Switch(config-if)# hostname test
                      ^
% Invalid input detected at '^' marker.
```

### Interface Mode Specifics

Interface mode is "sticky" for interface selection — entering a new `interface`
command from interface mode switches to the new interface directly:
```
Switch(config)# interface Gi0/1
Switch(config-if)# interface Gi0/2   ! Switches to Gi0/2 without exiting first
Switch(config-if)#
```

### `no` in Configuration Mode

`no` at the start of a command negates it. For some commands, arguments must match:
```
Switch(config)# access-list 10 permit 192.168.1.0 0.0.0.255
Switch(config)# no access-list 10 permit 192.168.1.0 0.0.0.255  ! Remove specific line
Switch(config)# no access-list 10   ! Remove entire ACL
```

---

## 15. Output Formatting

### IOS Output Conventions

- Lines starting with `!` in configuration output are comment lines (ignored when loaded)
- Lines starting with `no` are explicit negations
- Indentation: sub-commands under a parent are indented by one space (1 space, not tab)
- Blank lines separate logical sections in running-config output
- The running-config ends with `end` on its own line

### Show Command Output Alignment

IOS show command output typically:
- Uses fixed-width columns aligned with spaces
- Has header rows separated by dashes (`---`)
- Left-aligns most text
- Right-aligns numerical values in some tables

### `no-more` Behavior

When output is prefixed with `| no-more`, the `--More--` prompt is suppressed and
all output is displayed immediately regardless of terminal length setting.

---

## 16. Abbreviations Reference

### Common Command Abbreviations (Privileged EXEC)

| Full Command | Abbreviation |
|---|---|
| `configure terminal` | `conf t` |
| `show running-config` | `sh run` |
| `show interfaces` | `sh int` |
| `show ip interface brief` | `sh ip int br` |
| `show version` | `sh ver` |
| `copy running-config startup-config` | `cop run star` or `wr` (write mem) |
| `write memory` | `wr` or `wr mem` |
| `erase startup-config` | `er star` |
| `reload` | `rel` |
| `show spanning-tree` | `sh span` |
| `show vlan brief` | `sh vl br` |
| `show mac address-table` | `sh mac add` |
| `clear mac address-table dynamic` | `cl mac add dyn` |
| `ping` | `pi` |
| `traceroute` | `tra` |
| `terminal length 0` | `ter len 0` |

### Common Configuration Abbreviations

| Full Command | Abbreviation |
|---|---|
| `interface GigabitEthernet0/1` | `int gi0/1` |
| `no shutdown` | `no sh` |
| `shutdown` | `sh` |
| `ip address 192.168.1.1 255.255.255.0` | `ip add 192.168.1.1 255.255.255.0` |
| `switchport mode access` | `sw mo ac` |
| `switchport access vlan 10` | `sw ac vl 10` |
| `switchport mode trunk` | `sw mo tr` |
| `spanning-tree portfast` | `sp portf` |
| `spanning-tree bpduguard enable` | `sp bpdug en` |

---

## 17. DNS Resolution Behavior

When DNS lookup is enabled (default) and an unknown string is entered as a command
or hostname, IOS attempts DNS resolution, which can cause a long delay:

```
Switch# notacommand
Translating "notacommand"...domain server (255.255.255.255)
% Unknown command or computer name, or unable to find computer address
```

This is because IOS treats unrecognized commands as potential hostnames for
Telnet connections.

To disable this behavior:
```
Switch(config)# no ip domain-lookup
```

After disabling:
```
Switch# notacommand
               ^
% Invalid input detected at '^' marker.
```

---

## 18. Confirmations and Prompts

### Commands That Require Confirmation

Some destructive commands require the user to press Enter or type `y`/`yes`:

```
Switch# erase startup-config
Erasing the nvram filesystem will remove all configuration files! Continue? [confirm]
```
Press Enter to confirm, or `n` to cancel.

```
Switch# reload
Proceed with reload? [confirm]
```

```
Switch# delete flash:vlan.dat
Delete filename [vlan.dat]?
Delete flash:/vlan.dat? [confirm]
```

```
Switch# write erase
Erasing the nvram filesystem will remove all configuration files! Continue? [confirm]
```

### `[confirm]` Behavior

`[confirm]` prompts accept:
- Enter key: confirms the action
- `y` or `yes`: confirms
- `n` or `no`: cancels
- Ctrl+C: cancels

### `copy` Command Prompts

```
Switch# copy running-config startup-config
Destination filename [startup-config]?     ← Press Enter to accept default
Building configuration...
[OK]
```

```
Switch# copy running-config tftp:
Address or name of remote host []? 192.168.1.100
Destination filename [switch-confg]? myswitch.cfg
!!
1234 bytes copied in 0.123 secs (10034 bytes/sec)
```

---

## 19. Enable Password Behavior

```
Switch> enable
Password:           ← Password not echoed (no characters shown)
Switch#
```

If wrong password:
```
Switch> enable
Password:
% Access denied
```
After 3 failures, the connection may be locked out depending on configuration.

---

## 20. Session Timeout Messages

When exec-timeout expires:
```

[Connection to 192.168.1.1 closed by foreign host]
```
or for console:
```
Switch con0 is now available

Press RETURN to get started.
```

---

## 21. Syslog Messages at CLI

When syslog messages are sent to the terminal (e.g., link status changes), they
appear inline in the CLI even during command entry:

```
Switch#
*Mar 25 14:30:00.000: %LINK-3-UPDOWN: Interface GigabitEthernet0/1, changed state to up
*Mar 25 14:30:01.000: %LINEPROTO-5-UPDOWN: Line protocol on Interface GigabitEthernet0/1, changed state to up
```

The `logging synchronous` line configuration command causes the current command
line to be re-displayed after a log message interrupts typing:

```
Switch# show ver
*Mar 25 14:30:00.000: %LINK-3-UPDOWN: Interface GigabitEthernet0/2, changed state to down

Switch# show ver    ← re-displayed so user can see what they typed
```

Without `logging synchronous`, the log message appears in the middle of whatever
you're typing, which can be confusing.

---

## 22. Syslog Message Format

Syslog messages follow this format:
```
*timestamp: %FACILITY-SEVERITY-MNEMONIC: message text
```

Where:
- `*` = unsynced/unconfirmed timestamp (before NTP sync)
- timestamp = date/time (format depends on `service timestamps` config)
- FACILITY = subsystem name (e.g., LINK, LINEPROTO, SYS, CDP, DOT1X)
- SEVERITY = 0-7 (0=emergencies, 1=alerts, 2=critical, 3=errors, 4=warnings, 5=notifications, 6=informational, 7=debugging)
- MNEMONIC = specific message code
- message text = human-readable description

Example messages:
```
*Mar  1 00:00:05.003: %SYS-5-CONFIG_I: Configured from console by console
*Mar  1 00:00:10.000: %LINK-3-UPDOWN: Interface GigabitEthernet0/1, changed state to up
*Mar  1 00:00:11.000: %LINEPROTO-5-UPDOWN: Line protocol on Interface GigabitEthernet0/1, changed state to up
*Mar  1 00:00:15.000: %CDP-4-NATIVE_VLAN_MISMATCH: Native VLAN mismatch discovered on GigabitEthernet0/1 (1), with Switch2 GigabitEthernet0/1 (10).
```

---

## 23. Command Return Values / Status Messages

### Successful Operations

```
Switch# copy running-config startup-config
Building configuration...
[OK]
```

```
Switch# write memory
Building configuration...
[OK]
```

### Flash Write Progress

When writing large files to flash, IOS shows progress:
```
Switch# copy tftp: flash:
!!!!!!!!!!
(each ! = 1KB transferred, . = timeout/retry)
```

### Ping Output Format

```
Type escape sequence to abort.
Sending 5, 100-byte ICMP Echos to 192.168.1.1, timeout is 2 seconds:
!!!!!
Success rate is 100 percent (5/5), round-trip min/avg/max = 1/2/4 ms
```

### Traceroute Output Format

```
Type escape sequence to abort.
Tracing the route to 192.168.1.1
VRF info: (vrf in name/id, vrf out name/id)
  1 192.168.1.1 1 msec 1 msec 0 msec
```
