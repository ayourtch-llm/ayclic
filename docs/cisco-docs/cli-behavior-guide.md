# Cisco IOS 15.2 CLI Behavior Guide

This document details the minute behavioral characteristics of the Cisco IOS
command-line interface as observed on IOS Release 15.2(x)E (Catalyst 3560-CX).
These details are important for implementing a faithful simulator/emulator.

---

## 1. Command Modes and Prompts

The IOS CLI is hierarchical. The prompt always indicates the current mode.

| Mode | Prompt | How to Enter | How to Exit |
|------|--------|-------------|-------------|
| User EXEC | `Switch>` | Default after login | `logout` or `exit` to disconnect |
| Privileged EXEC | `Switch#` | `enable` from user EXEC | `disable` or `exit` |
| Global Config | `Switch(config)#` | `configure terminal` from privileged EXEC | `exit` or `end` or Ctrl+Z |
| Interface Config | `Switch(config-if)#` | `interface TYPE N` from global config | `exit` (go back to global config) or `end`/Ctrl+Z (go to privileged EXEC) |
| Line Config | `Switch(config-line)#` | `line console 0` etc. from global config | `exit` or `end`/Ctrl+Z |
| Router Config | `Switch(config-router)#` | `router ospf N` etc. from global config | `exit` or `end`/Ctrl+Z |
| VLAN Config | `Switch(config-vlan)#` | `vlan N` from global config | `exit` or `end`/Ctrl+Z |
| ACL Config | `Switch(config-std-nacl)#` | `ip access-list standard NAME` | `exit` or `end`/Ctrl+Z |
| DHCP Pool | `Switch(config-dhcp)#` | `ip dhcp pool NAME` | `exit` or `end`/Ctrl+Z |

The hostname in the prompt changes immediately when `hostname NAME` is configured.

### Prompt Format Details

- The prompt is always `HOSTNAME(MODE)#` or `HOSTNAME(MODE)>` with no leading spaces.
- Configuration submode names use hyphens: `config-if`, `config-line`, `config-router`,
  `config-vlan`, `config-std-nacl`, `config-ext-nacl`, `config-dhcp`, `config-keychain`,
  `config-pmap`, `config-cmap`, etc.
- After entering `interface range`, the prompt is the same `(config-if)#` as for a single
  interface.

---

## 2. Command Abbreviation (Prefix Matching)

IOS accepts any unambiguous prefix abbreviation of any command keyword or argument.

### Rules

- You may shorten any keyword to its minimum unique prefix in the current mode.
- The parser tries to match the prefix against all valid commands in the current mode.
- If exactly one match: the abbreviation is accepted and executed.
- If more than one match: you get `% Ambiguous command: "XX"`.
- If zero matches: you get `% Invalid input detected at '^' marker`.

### Examples

```
Switch# conf t           ! Accepted: only "configure" starts with "conf"
Switch# sh ip int br     ! Accepted: show ip interface brief
Switch# sh               ! Ambiguous: "show", "shutdown" (context-dependent)
Switch(config)# int g1/0/1   ! Accepted: interface GigabitEthernet1/0/1
Switch(config-if)# sw mo ac  ! Accepted: switchport mode access
Switch(config-if)# shut      ! Accepted: shutdown
Switch(config-if)# no shut   ! Accepted: no shutdown
```

### Important Notes on Abbreviation

- Abbreviation applies to EACH word/keyword in the command independently.
- The `no` prefix form is also abbreviated with the same rules: `no sh` = `no shutdown`.
- Numeric arguments (VLAN IDs, IP addresses, port numbers) cannot be abbreviated.
- Arguments that accept strings (e.g., passwords, descriptions) must be given in full.
- Interface type names can be abbreviated: `GigabitEthernet` -> `Gig` or `G` or `gi`.
  Common interface type abbreviations:
  - `GigabitEthernet` or `Gi` or `GigE`
  - `TenGigabitEthernet` or `Te`
  - `FastEthernet` or `Fa`
  - `Vlan` (case insensitive)
  - `Port-channel` or `Po`
  - `Loopback` or `Lo`

---

## 3. The `?` Help System

The question mark triggers context-sensitive help. Its behavior depends on whether there
is a space before `?`.

### 3a. Inline `?` (no space before `?`)

Used to complete or list matches for what you have typed so far. Do NOT press Enter first;
type `?` directly after the characters.

```
Switch# sh?
show
```

- Lists all commands that begin with the characters typed.
- No newline is consumed; the prompt re-displays your partial input after the list.
- The partial input is preserved on the command line; you can continue typing.

```
Switch# sho?
show
Switch# sho_    (cursor returns here, "sho" still on command line)
```

Another example showing multiple matches:

```
Switch# s?
set    show    ssh    start-chat    systat
Switch# s_     (partial input preserved)
```

### 3b. End-of-line `?` (space before `?`)

Used to show the next expected argument or subcommand. Type the command (or partial
command), then a space, then `?`.

```
Switch# show ?
  aaa                    AAA services
  access-expression      List access expression
  access-lists           List access lists
  adjacency              Adjacent nodes
  alarm-interface        Alarm Interface slot info
  aliases                Display alias commands
  ...
  <cr>                   (shown if the command can be executed as-is)
```

- Lists all valid next tokens (subcommands, keywords, argument types) with one-line
  descriptions.
- The `<cr>` entry at the end indicates the command can be executed with just Enter
  (no more arguments required).
- After displaying the help, the original command is re-displayed for you to continue
  typing.

```
Switch(config-if)# switchport mode ?
  access    Set trunking mode to ACCESS unconditionally
  dynamic   Set trunking mode to dynamically negotiate access or trunk status
  trunk     Set trunking mode to TRUNK unconditionally

Switch(config-if)# switchport mode _   (partial input preserved)
```

### 3c. `?` After a Space at a Prompt

If you type only `?` at a mode prompt (or `?` after a space when you haven't typed anything
else), it lists ALL commands available in the current mode:

```
Switch(config)# ?
Configure commands:
  aaa                    Authentication, Authorization and Accounting.
  access-list            Add an access list entry
  ...
```

### 3d. Important `?` Interaction Detail

When `?` is used inline (no preceding space), the partial text is preserved on the command
line. The terminal shows:

```
Switch# show ip ?
```

After IOS lists completions, it re-prints the prompt and the partial command so the user
can continue.

---

## 4. Tab Completion

Pressing Tab completes the current keyword if the prefix is unique.

### Rules

- If the prefix uniquely identifies one command/keyword: Tab completes it fully and
  adds a trailing space.
- If the prefix is ambiguous: the terminal beeps (or does nothing visible) and the partial
  input remains.
- If there is no match at all: the terminal beeps; the partial input remains.
- Tab only completes command keywords and interface type names, not free-form text
  arguments like passwords or descriptions.

### Examples

```
Switch# sho<TAB>    ->  Switch# show    (space added)
Switch# show ip<TAB>  ->  Switch# show ip    (ambiguous, no completion)
Switch# show ip in<TAB>  ->  Switch# show ip interface    (unique)
Switch(config)# int<TAB>  ->  Switch(config)# interface
Switch(config)# interface gig<TAB>  ->  Switch(config)# interface GigabitEthernet
```

After Tab completion adds a trailing space, pressing `?` immediately shows the next
expected arguments.

---

## 5. Error Message Formats

IOS uses three primary error messages for invalid commands:

### 5a. Invalid Input Detected

```
% Invalid input detected at '^' marker.
```

Displayed when a token in the command is not recognized. The caret `^` appears on a
separate line below the original command, pointing to the first unrecognized character.

Example:
```
Switch# show versin
           ^
% Invalid input detected at '^' marker.
```

The caret points to the start of `versin` because `version` was misspelled. The entire
word starting at that position is invalid.

### 5b. Incomplete Command

```
% Incomplete command.
```

Displayed when the command is syntactically started but requires more arguments that were
not provided. The command itself is valid syntax but missing required parameters.

Example:
```
Switch(config-if)# ip address
% Incomplete command.
```

`ip address` requires an IP address and subnet mask. No caret is shown.

### 5c. Ambiguous Command

```
% Ambiguous command:  "XX"
```

Displayed when the abbreviated prefix you typed matches more than one command. The typed
string appears in quotes after the colon.

Example:
```
Switch# show c
% Ambiguous command:  "show c"
```

(Because `show c` could match `show cdp`, `show clock`, `show controllers`, etc.)

### 5d. Other Notable Error Messages

```
% Command not found
```
Rarely seen; usually the above three cover invalid inputs.

```
% Login invalid
```
Shown when authentication fails (wrong password at enable or login prompt).

```
% Password required, but none set
```
Displayed when `login` is configured on a line but no `password` command was set.

```
% No password set
```
Similar; when you try to enable without an enable password/secret set.

```
% Authorization failed.
```
When AAA authorization denies the command.

```
% Connection timed out; unrelated to your last keystroke
```
When a connection attempt (SSH/Telnet) times out.

```
% Error in authentication.
```
Generic authentication failure in some contexts.

---

## 6. Pipe (`|`) Output Filtering

Any show command (or other commands with pageable output) can be followed by `|` and a
filter keyword. The format is:

```
Switch# show COMMAND | FILTER_KEYWORD [REGEX]
```

### Available Filter Keywords

| Keyword | Syntax | Description |
|---------|--------|-------------|
| `include` | `| include REGEX` | Display only lines matching the regular expression. |
| `exclude` | `| exclude REGEX` | Display all lines EXCEPT those matching the regex. |
| `begin` | `| begin REGEX` | Start displaying from the first line matching the regex; show everything after. |
| `section` | `| section REGEX` | Display each section (block) whose header line matches. A section includes the matching line and all following indented lines until the next non-indented line. |
| `count` | `| count REGEX` | Display the count of lines matching the regex, not the lines themselves. |
| `grep` | `| grep REGEX` | Similar to `include` (available on some IOS versions). |
| `redirect` | `| redirect URL` | Redirect output to a file/URL. |
| `append` | `| append URL` | Append output to a file/URL. |

### Regex Syntax

The regex is a basic regular expression. Common patterns:
- `.` matches any character.
- `*` matches zero or more of the preceding character.
- `^` anchors to the start of a line.
- `$` anchors to the end of a line.
- `[abc]` matches any character in the set.
- `(abc)` grouping.

### Examples

```
Switch# show ip interface brief | include up
Switch# show running-config | include hostname
Switch# show interfaces | include GigabitEthernet|line protocol
Switch# show running-config | begin interface
Switch# show running-config | section interface
Switch# show running-config | section GigabitEthernet1/0/1
Switch# show spanning-tree | section Root
Switch# show interfaces | exclude (Last|input|output|minute)
Switch# show ip route | begin Gateway
Switch# show logging | include %LINK
Switch# show running-config | count interface
```

### Important Limitations

- IOS does not support chaining multiple pipes (no `| include X | exclude Y`).
- Only one pipe filter per command.
- The `section` filter is particularly useful: `show run | section interface` displays all
  interface blocks. The section match is on the header line; indented sub-lines are
  included automatically.

### Discovering Available Filters

At any point, `show COMMAND | ?` lists the available filter keywords:

```
Switch# show version | ?
  append    Append redirected output to URL (URLs supporting append operation only)
  begin     Begin with the line that matches
  count     Count number of lines which match regexp
  exclude   Exclude lines that match
  include   Include lines that match
  redirect  Redirect output to URL
  section   Filter a section of output
  tee       Copy output to URL
```

---

## 7. Terminal Length and Paging

### terminal length

```
Switch# terminal length N
```

Sets the number of lines displayed before the `--More--` prompt appears. This is a
per-session setting (not saved to config).

- Default: 24 lines.
- `terminal length 0`: disable paging entirely (display all output without stopping).
- `terminal length N` where N is any positive integer: pause after N lines.

The command `terminal no length` also resets to the default.

### The `--More--` Prompt

When output exceeds the terminal length, IOS displays:

```
 --More--
```

(with a leading space and a trailing space, displayed without a newline)

At the `--More--` prompt, the following keystrokes are recognized:

| Keystroke | Action |
|-----------|--------|
| Space | Display the next full page (terminal length lines). |
| Enter (Return) | Display one more line. |
| `q` or `Q` | Quit/stop displaying output; return to prompt. |
| Any other key | Typically ignored or treated as continue. |

After pressing `q`, the display returns to the prompt immediately. The `--More--` text
is erased from the screen (replaced by spaces) before showing the prompt.

### Effect on show commands

`terminal length 0` is commonly used in scripts and automation to avoid `--More--`
interruptions. It can also be set persistently in the configuration for specific lines:

```
Switch(config)# line vty 0 15
Switch(config-line)# length 0
```

This permanently disables paging for all VTY sessions without needing `terminal length 0`
each time.

---

## 8. Keyboard Shortcuts and CLI Editing

IOS provides a readline-like editing interface on all lines. These shortcuts are available
at all prompts.

### Cursor Movement

| Key | Action |
|-----|--------|
| Ctrl+A | Move cursor to the beginning of the line. |
| Ctrl+E | Move cursor to the end of the line. |
| Ctrl+F or Right Arrow | Move cursor one character forward. |
| Ctrl+B or Left Arrow | Move cursor one character backward. |
| Esc+F | Move cursor forward one word. |
| Esc+B | Move cursor backward one word. |

### Deletion

| Key | Action |
|-----|--------|
| Backspace or Ctrl+H | Delete character to the left of the cursor. |
| Delete or Ctrl+D | Delete character at the cursor position. |
| Ctrl+U or Ctrl+X | Delete all characters from cursor to beginning of line. |
| Ctrl+K | Delete all characters from cursor to end of line. |
| Ctrl+W | Delete the word to the left of the cursor. |
| Esc+D | Delete the word to the right of the cursor. |

### Recall (Paste Back)

Deleted text is placed in a buffer. The last deletion can be recalled:

| Key | Action |
|-----|--------|
| Ctrl+Y | Recall (yank) the last text deleted with Ctrl+U, Ctrl+X, or Ctrl+K. |

The buffer holds only the most recently deleted text.

### Line Operations

| Key | Action |
|-----|--------|
| Ctrl+R or Ctrl+L | Redisplay the current line (useful when a syslog message has interrupted the display). |
| Ctrl+T | Transpose the character at the cursor with the character before it. |
| Ctrl+V | Insert the next character literally (escape special meaning of next character). |

### History Navigation

| Key | Action |
|-----|--------|
| Up Arrow or Ctrl+P | Recall the previous command in the history buffer. |
| Down Arrow or Ctrl+N | Recall the next command in the history buffer. |
| Ctrl+P repeatedly | Scroll backward through history. |
| Ctrl+N repeatedly | Scroll forward through history. |

The history buffer stores the last 10 commands by default (configurable up to 256 with
`history size N` on the line).

To view the history buffer:
```
Switch# show history
```

### Special Mode Keys

| Key | Action |
|-----|--------|
| Ctrl+Z | Exit from any configuration mode directly to privileged EXEC. If typed at the end of a command line with content, the command is executed AND then the mode exits. |
| Ctrl+C | Interrupt the current operation. In config mode, aborts the current command line (does not exit the mode). In a running operation (like `ping` or `traceroute`), stops it. |
| Ctrl+Shift+6 (or Ctrl+^) | Break sequence. Interrupts currently executing commands such as `ping`, `traceroute`, or a hanging Telnet connection. Also used to escape from a Telnet session back to the originating device. |
| Tab | Complete current keyword (see section 4). |
| `?` | Context-sensitive help (see section 3). |

### Ctrl+Z Behavior in Config Mode

Ctrl+Z executed at the end of a command line in config mode **first executes the command
on that line**, then exits to privileged EXEC. This is an important edge case:

```
Switch(config-if)# ip address 10.0.0.1 255.255.255.0 [Ctrl+Z]
```

This sets the IP address AND exits to `Switch#`. Use `end` (typed as a command and Enter)
if you only want to exit without ambiguity.

---

## 9. The `do` Command

From any configuration mode, you can run privileged EXEC commands (like `show`, `clear`,
`debug`, `ping`, `traceroute`) without exiting the configuration mode by prefixing the
command with `do`:

```
Switch(config)# do show ip interface brief
Switch(config-if)# do show running-config interface GigabitEthernet1/0/1
Switch(config-router)# do show ip ospf neighbor
Switch(config)# do write memory
Switch(config)# do copy running-config startup-config
```

- The `do` command was introduced in IOS 12.1(11b)E.
- After the `do` command completes, the prompt remains in the same configuration mode.
- You cannot abbreviate `do` to something shorter; `do` itself is the command.
- The `do` command only accepts privileged EXEC commands, not user EXEC commands or
  config-mode commands from a different context.

---

## 10. The `no` Form of Commands

Most configuration commands have a `no` form that reverses or removes the configuration.

### General Rules

- `no COMMAND` removes the effect of `COMMAND` from the running configuration.
- The `no` form often does not require all the original arguments (the entire command
  is removed).
- Some commands require repeating arguments in the `no` form.
- Some commands are toggles: `shutdown` / `no shutdown`.

### Examples

```
Switch(config-if)# ip address 10.0.0.1 255.255.255.0
Switch(config-if)# no ip address                    ! Removes all IP addresses

Switch(config-if)# description My Interface
Switch(config-if)# no description                   ! Removes description entirely

Switch(config-if)# shutdown
Switch(config-if)# no shutdown                      ! Brings the interface up

Switch(config)# hostname MySwitch
Switch(config)# no hostname                         ! Reverts to default hostname "Switch"

Switch(config)# access-list 10 permit 192.168.1.0 0.0.0.255
Switch(config)# no access-list 10                   ! Deletes the ENTIRE ACL

Switch(config-if)# switchport trunk allowed vlan 10,20,30
Switch(config-if)# no switchport trunk allowed vlan 20   ! NOT valid; use "remove"
Switch(config-if)# switchport trunk allowed vlan remove 20  ! Correct way to remove one VLAN
```

### `default` Form

Some commands support a `default` prefix that resets the command to its factory default:

```
Switch(config-if)# default spanning-tree portfast   ! Reset portfast to default
Switch(config)# default interface GigabitEthernet1/0/1  ! Reset entire interface to defaults
```

---

## 11. `exit` vs `end` in Various Modes

Understanding the difference between `exit` and `end` is critical for correct mode
navigation.

### `exit`

- Moves up exactly ONE level in the configuration hierarchy.
- From interface config (`config-if`) -> goes to global config (`config`).
- From global config (`config`) -> goes to privileged EXEC (`#`).
- From privileged EXEC (`#`) -> logs out (ends the session or goes to user EXEC).
- From user EXEC (`>`) -> logs out/disconnects.
- From a sub-submode like `config-pmap-c` -> goes up to `config-pmap`.

### `end`

- Returns directly to privileged EXEC mode from ANY configuration mode, regardless
  of nesting depth.
- Equivalent to Ctrl+Z (except Ctrl+Z also executes the current command line if there
  is text typed).
- `end` in privileged EXEC: does nothing (or gives an error in some IOS versions).
- `end` in user EXEC: not available.

### Summary Table

| Current Mode | `exit` Goes To | `end` Goes To |
|-------------|---------------|--------------|
| User EXEC | Disconnect/logout | N/A |
| Privileged EXEC | User EXEC | Privileged EXEC (no-op or error) |
| Global config | Privileged EXEC | Privileged EXEC |
| Interface config | Global config | Privileged EXEC |
| Line config | Global config | Privileged EXEC |
| Router config | Global config | Privileged EXEC |
| VLAN config | Global config | Privileged EXEC |
| Sub-submode | Parent mode | Privileged EXEC |

### `logout` and `disconnect`

- `logout`: Disconnect from the current terminal session. Available in user EXEC and
  privileged EXEC.
- `disconnect N`: Disconnect a remote session by session number.

---

## 12. Banner Delimiters and Multi-Line Input

### How Banner Delimiters Work

The banner commands use a delimiter to mark the start and end of the banner text:

```
banner motd DELIMITER
```

- The first non-space character after `banner motd ` (or `banner login `, etc.) becomes
  the delimiter.
- IOS then accepts multiple lines of text until it sees the delimiter character again on
  any line (the delimiter can appear alone or at the end of the last line of text).
- Commonly used delimiters: `#`, `^`, `!`, `/`, `@`, `$`, `%`.
- The delimiter character must NOT appear anywhere in the banner text itself.
- If you use `banner motd #text#` on one line, the banner text is just `text`.

### Multi-Line Banner Example

```
Switch(config)# banner motd #
Enter TEXT message.  End with the character '#'.
*********************************************
* WARNING: Unauthorized access prohibited. *
* All activity is monitored and logged.    *
*********************************************
#
Switch(config)#
```

The IOS parser accepts input line by line until it finds the delimiter. The banner is
stored with all newlines intact.

### Banner in Running-Config

In the running-config, banners appear as:

```
banner motd ^C
*********************************************
* WARNING: Unauthorized access prohibited. *
^C
```

The delimiter used in the running-config output is typically `^C` (the Ctrl+C character
representation), regardless of what delimiter was used during input. This is important
for scripts that parse `show running-config`.

### Other Multi-Line Inputs

Multi-line input is also used in:
- `alias exec` commands that reference scripts (rare)
- Some certificate/key paste operations

---

## 13. Command History Details

- Default history size: 10 commands per line.
- Maximum history size: 256 commands.
- History is per-line (console, VTY 0, VTY 1, etc. have separate histories).
- To disable history: `terminal no history` or `no history`.
- `show history` displays the commands in the current line's buffer, most recent last.
- The history buffer stores the exact command as entered (with any errors).

---

## 14. Privileged EXEC Password Behavior

When you type `enable` at user EXEC:

```
Switch> enable
Password:
```

- The password is not echoed (no asterisks, no characters shown at all - blank input).
- After 3 failed attempts (or the configured `aaa authentication` retry limit), the
  session typically closes or returns to user EXEC.
- If no enable password/secret is set: `% No password set` and access is denied.
- If `aaa new-model` is active, the AAA authentication method applies.

---

## 15. Login Behavior

When a line has `login` configured with a `password`:

```
User Access Verification

Password:
```

With `login local`:

```
Username: admin
Password:
```

With `aaa new-model` and `aaa authentication login default local`:
Same as `login local` format.

The MOTD banner (if configured) appears BEFORE the login prompt. The login banner appears
just before `Username:` or `Password:`. The exec banner appears AFTER successful login,
before the prompt.

---

## 16. Partial Command Execution with `|` in Help

When using `?` in the middle of a command with a pipe, the `?` applies to the pipe filter:

```
Switch# show version | ?
  append    Append redirected output to URL
  begin     Begin with the line that matches
  ...
```

This is helpful for discovering what filter keywords are available.

---

## 17. The `show` Command in Config Mode

In configuration mode, you cannot run `show` commands directly without `do`. Attempting
to do so gives:

```
Switch(config)# show ip interface brief
           ^
% Invalid input detected at '^' marker.
```

The `show` command is only available in EXEC modes (user and privileged). Use `do show`
from config mode.

---

## 18. Automatic Logout and Session Management

- `exec-timeout 0 0` on a line disables the idle timeout (session never times out
  automatically).
- `exec-timeout 10 0` (default on console) means 10 minutes of idle time before
  disconnection.
- `absolute-timeout` causes disconnection after the specified time regardless of activity.
- When a session times out, the line displays: `[Connection to switch closed by foreign host]`
  or similar, then the session ends.

---

## 19. `write memory` and `copy running-config startup-config`

These two commands are functionally equivalent and save the running configuration to NVRAM:

```
Switch# write memory
Building configuration...
[OK]

Switch# copy running-config startup-config
Destination filename [startup-config]?
Building configuration...
[OK]
```

Both produce `Building configuration...` followed by `[OK]`. The `copy` command prompts
for a destination filename and shows the default in brackets; pressing Enter accepts it.

`write terminal` is equivalent to `show running-config` (legacy command).

---

## 20. Debug Commands

`debug` commands produce output to the console (by default) or to configured logging
destinations.

- `debug all`: Enable all debugging (WARNING: can crash a busy router).
- `undebug all` or `no debug all`: Disable all debugging.
- `debug ip ospf events`: Example of a specific debug.
- Debug output is prefixed with the facility name and severity, e.g., `*Mar  1 00:01:23.456: %OSPF-5-ADJCHG: ...`
- `terminal monitor`: Direct debug/log output to the current VTY session (not sent by
  default to Telnet/SSH sessions).
- `terminal no monitor`: Stop log/debug output to this VTY session.

---

*Sources:*
- *Cisco Consolidated Platform Configuration Guide, IOS Release 15.2(3)E and 15.2(5)E, Catalyst 3560-CX*
- *Configuration Fundamentals Configuration Guide, Cisco IOS Release 15.0S and 15SY*
- *Cisco IOS CLI Abbreviation and Shortcuts - Cisco Community*
- *Cisco IOS Keyboard Shortcuts - omnisecu.com*
- *Network Lessons: Introduction to Cisco IOS CLI*
- *N-Study: Cisco Basic CLI Error Messages*
