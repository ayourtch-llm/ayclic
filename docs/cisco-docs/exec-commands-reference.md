# Cisco IOS 15.2 Privileged EXEC Mode Commands Reference
## Platform: Catalyst 3560-CX Series Switches

This document covers the privileged EXEC (enable) mode commands as shown by `?` on a
Cisco Catalyst 3560-CX running IOS 15.2. The prompt in privileged EXEC mode appears as:

```
Switch#
```

---

## How to Access Privileged EXEC Mode

From user EXEC mode (`Switch>`):
```
Switch> enable
Password: <enable password or secret>
Switch#
```

---

## Complete Command List (as shown by `Switch# ?`)

The following is the list of commands available in privileged EXEC mode on IOS 15.2
for the C3560-CX platform. Each entry shows the keyword and its one-line help text.

```
Switch# ?
Exec commands:
  <1-99>      Session number to resume
  <300-399>   Session number to resume
  access-enable Create a temporary Access-List entry
  access-profile Apply user-profile to interface
  access-template Create a temporary Access-List entry
  archive     manage archive files
  auto        Exec level Automation
  bfe         For manual emergency modes setting
  cd          Change current directory
  clear       Reset functions
  clock       Manage the system clock
  configure   Enter configuration mode
  connect     Open a terminal connection
  copy        Copy from one file to another
  crypto      Encryption related commands
  debug       Debugging functions (see also 'undebug')
  delete      Delete a file
  deny        Deny user entry to network
  dir         List files on a filesystem
  disable     Turn off privileged commands
  disconnect  Disconnect an existing network connection
  dot1x       IEEE 802.1X Exec Commands
  enable      Turn on privileged commands
  erase       Erase a filesystem
  event       Event related commands
  exit        Exit from the EXEC
  format      Format a filesystem
  fsck        Fsck a filesystem
  help        Description of the interactive help system
  history     Display the session command history
  ip          Global IP commands
  isdn        Run an ISDN EXEC command
  l2protocol-tunnel Configure a Protocol Tunneling interface
  lat         Open a lat connection
  license     Make license-related queries and modifications
  logout      Exit from the EXEC
  loop        Loopback operation commands
  mac         MAC configuration
  mbranch     Trace multicast tree downwards using DVMRP
  minfo       Request neighbor and version information from a multicast router
  mkdir       Create new directory
  mls         Show MultiLayer Switching information
  mrinfo      Request neighbor and version information from a multicast router
  mrm         IP Multicast Routing Monitor Test
  mstat       Show statistics after multiple multicast traceroutes
  mtrace      Trace reverse multicast path from destination to source
  name-connection Name an existing network connection
  no          Disable debugging informations
  nslookup    Find IP address corresponding to name
  onep        ONEP commands
  pad         Open a X.29 PAD connection
  ping        Send echo messages
  ppp         Start IETF Point-to-Point Protocol (PPP)
  pwd         Display current working directory
  radius      RADIUS module
  reload      Halt and perform a cold restart
  remote      Execute remote commands
  rename      Rename a file
  repeat      Repeat a command
  resume      Resume an active network connection
  rsh         Execute a remote command
  sdm         Show SDM (Switch Database Manager) information
  send        Send a message to other tty lines
  set         Set system parameter (not config)
  setup       Run the SETUP command facility
  show        Show running system information
  slip        Start Serial-line IP (SLIP)
  ssh         Open a secure shell client connection
  start-chat  Start a X.25 chat
  systat      Display information about terminal lines
  tclquit     Quit Tool Command Language shell
  tclsh       Invoke Tool Command Language shell
  telnet      Open a telnet connection
  terminal    Set terminal line parameters
  test        Test subsystems, memory, and interfaces
  traceroute  Trace route to destination
  tunnel      Open a tunnel connection
  udld        UDLD protocol commands
  undebug     Disable debugging functions (see also 'debug')
  undelete    Undelete a file
  verify      Verify a file
  vlan        Vlan commands
  vmps        VLAN Membership Policy Server commands
  write       Write running configuration to memory, network, or terminal
  xconnect    X-connect commands
```

---

## Commonly Used Commands - Detailed Reference

### `clear`
Reset counters, tables, and other runtime state.

Key subcommands:
```
Switch# clear ?
  access-list          Clear access list statistical information
  arp-cache            Clear the entire ARP cache
  bpdu-guard           Clear BPDU guard statistics
  cdp                  Reset CDP information
  clock                Clear clock
  counters             Clear counters on one or all interfaces
  crypto               Clear cryptographic information
  dhcp                 Clear DHCP (Dynamic Host Configuration Protocol) information
  dot1x                Clear 802.1X statistics
  errdisable           Clear error disabled information
  hw-module            Clear module info
  igmp                 Clear IGMP group cache
  interface            Clear the hardware logic on an interface
  ip                   Clear IP information
  ipv6                 Clear IPv6 information
  lacp                 Clear LACP informational counters
  line                 Reset a terminal line
  logging              Clear logging buffer
  mac                  MAC forwarding table
  mac address-table    Clear MAC address table entries
  pagp                 Clear PAgP informational counters
  port-security        Clear port security information
  privilege            Clear privilege levels of users connected via Telnet
  qos                  Clear QoS counters
  spanning-tree        Clear spanning-tree counters
  udld                 Reset UDLD state
  vtp                  Clear VTP counters
```

### `clock`
Manage the system clock.

```
Switch# clock ?
  read-calendar  Read the hardware calendar into the clock
  set            Set the time and date
  update-calendar Update the hardware calendar from the clock
```

Example:
```
Switch# clock set 14:30:00 25 March 2024
```

### `configure`
Enter configuration mode.

```
Switch# configure ?
  memory     Configure from NV memory
  network    Configure from a TFTP network host
  overwrite-network   Overwrite NV memory from TFTP network host
  replace    Replace the running config with a saved Cisco config file
  revert     Revert the running config to the saved Cisco config file
  terminal   Configure from the terminal
```

Most commonly used form:
```
Switch# configure terminal
Switch(config)#
```

### `copy`
Copy configuration or image files.

```
Switch# copy ?
  /erase           Erase destination file system
  /noverify        Don't verify copied image
  flash:           Copy from flash: file system
  ftp:             Copy from ftp: file system
  null:            Copy from null: file system
  nvram:           Copy from nvram: file system
  rcp:             Copy from rcp: file system
  running-config   Copy from current system configuration
  scp:             Copy from scp: file system
  startup-config   Copy from startup configuration
  system:          Copy from system: file system
  tftp:            Copy from tftp: file system
  tmpsys:          Copy from tmpsys: file system
  xmodem:          Copy from xmodem: file system
  ymodem:          Copy from ymodem: file system
```

Common uses:
```
Switch# copy running-config startup-config
Switch# copy running-config tftp:
Switch# copy tftp: flash:
Switch# copy flash: tftp:
```

### `debug`
Enable debugging output for various subsystems. NOTE: Use with caution in production.

```
Switch# debug ?
  aaa           AAA Authentication, Authorization and Accounting
  access-expression  named access-list compilation/execution
  arp           IP ARP and proxy ARP transactions
  cdp           CDP information
  condition     Condition debugging
  crypto        Cryptographic subsystem
  dhcp          DHCP protocol activity
  dot1x         IEEE 802.1X events
  etherchannel  EtherChannel/PagP/LACP
  flow          NetFlow/IPFIX
  igmp          IGMP protocol activity
  ip            IP information
  ipv6          IPv6 information
  lacp          LACP protocol events
  lldp          LLDP events
  mac-notification  MAC notification events
  mstp          MSTP events
  ntp           NTP information
  pagp          PAgP protocol events
  platform      platform specific information
  port-security  Port Security events
  ppp           PPP (Point to Point Protocol) information
  radius        RADIUS protocol events
  spanning-tree  Spanning Tree Protocol
  sw-vlan       VLAN Manager operations
  tacacs        TACACS authentication and authorization
  udld          UDLD events
  vtp           VTP protocol activity
  vlan          Vlan operations
```

### `delete`
Delete a file from a filesystem.

```
Switch# delete ?
  /force    Don't ask for confirmation
  /recursive  Recursively delete directories
  flash:    Delete from flash: file system
  nvram:    Delete from nvram: file system
```

Example:
```
Switch# delete flash:vlan.dat
Delete filename [vlan.dat]?
Delete flash:vlan.dat? [confirm]
```

### `dir`
List files on a filesystem.

```
Switch# dir ?
  /all         List all files
  /recursive   List files recursively
  flash:       Directory or file name
  nvram:       Directory or file name
  system:      Directory or file name
```

Example output:
```
Switch# dir flash:
Directory of flash:/

    2  -rwx        2523   Mar 1 1993 00:02:47 +00:00  config.text
    3  -rwx           5   Mar 1 1993 00:02:47 +00:00  private-config.text
    4  -rwx        4128   Mar 1 1993 00:02:47 +00:00  multiple-fs
    5  drwx         512   Mar 1 1993 00:03:08 +00:00  c3560cx-universalk9-mz.150-2.SE11

57671680 bytes total (48893952 bytes free)
```

### `disable`
Exit privileged EXEC mode, return to user EXEC mode.

```
Switch# disable
Switch>
```

### `erase`
Erase a filesystem.

```
Switch# erase ?
  /all             Erase all files in filesystem (including hidden)
  flash:           Erase flash: file system
  nvram:           Erase nvram: file system
  startup-config   Erase contents of configuration memory
```

### `exit`
Exit from privileged EXEC (logs out of session).

```
Switch# exit
```

### `ping`
Send ICMP echo messages to test connectivity.

```
Switch# ping ?
  WORD      Ping destination address or hostname
  clns      CLNS echo
  ip        IP echo
  ipv6      IPv6 echo
  srb       srb echo
  tag       Tag encapsulated IP echo
  vrf       specify vrf name
```

Simple ping:
```
Switch# ping 192.168.1.1

Type escape sequence to abort.
Sending 5, 100-byte ICMP Echos to 192.168.1.1, timeout is 2 seconds:
!!!!!
Success rate is 100 percent (5/5), round-trip min/avg/max = 1/2/4 ms
```

Extended ping (press Enter at `ping` prompt):
```
Switch# ping
Protocol [ip]:
Target IP address: 192.168.1.1
Repeat count [5]:
Datagram size [100]:
Timeout in seconds [2]:
Extended commands [n]:
Sweep range of sizes [n]:
```

Ping success/failure characters:
- `!` = successful reply
- `.` = timeout
- `U` = unreachable (destination host or network unreachable)
- `N` = unreachable (network unreachable)
- `P` = protocol unreachable
- `Q` = source quench
- `M` = could not fragment
- `?` = unknown packet type

### `reload`
Reload the switch (performs cold restart).

```
Switch# reload ?
  at     Reload at a specific time/date
  cancel  Cancel pending reload
  in     Reload after a time interval
  WORD   Reload reason
  <cr>
```

```
Switch# reload
System configuration has been modified. Save? [yes/no]: no
Proceed with reload? [confirm]
```

### `show`
Show running system information. This is the most extensive command with dozens of subcommands. See show-commands-reference.md for full details.

```
Switch# show ?
```
(See separate show-commands-reference.md)

### `ssh`
Open an SSH client connection.

```
Switch# ssh ?
  -c  Encryption type
  -l  Log in using this user name
  -m  HMAC type
  -o  Specify options
  -p  Connect to this port
  -v  SSH protocol version
  WORD  IP address or hostname of remote host
```

### `telnet`
Open a Telnet connection.

```
Switch# telnet ?
  WORD     IP address or hostname of remote system
  /debug   Enable telnet debugging
  /encrypt Enable telnet encryption
  /line    Enable sending CR as CR+LF
  /noecho  Disable local echo
  /source-interface  Set source interface
  /stream  Enable stream processing
```

### `terminal`
Set terminal line parameters.

```
Switch# terminal ?
  data-character-bits  Size of characters being sent and received
  default              Set a command to its defaults
  dispatch-character   Define the dispatch character
  dispatch-timeout     Set the dispatch timer
  download             Put into 'download' mode
  editing              Enable command line editing
  escape-character     Change the current line's escape character
  exec-character-bits  Size of exec characters being sent and received
  flowcontrol          Set the flow control
  full-help            Provide help to unprivileged user
  history              Enable and control the command history function
  international        Enable international 8-bit character support
  ip                   IP options
  keymap-type          Specify a keymap entry to use
  latitude             DEC latitude commands
  length               Set number of lines on a screen
  monitor              Copy debug output to the current terminal line
  notify               Inform users of output from concurrent sessions
  no                   Negate a command or set its defaults
  padding              Set padding for a specified output character
  parity               Set terminal parity
  rxspeed              Set the receive speed
  special-character-bits  Size of the escape (and other special) characters
  speed                Set the terminal speed
  start-character      Define the flow control start character
  stop-character       Define the flow control stop character
  stopbits             Set async line stop bits
  telnet               Set telnet options
  terminal-type        Set the terminal type
  txspeed              Set the transmit speed
  type                 Set the terminal type
  width                Set number of characters on a screen
```

Key `terminal` subcommands:
```
Switch# terminal length 0          ! Disable paging (show all output at once)
Switch# terminal length 24         ! Set screen length to 24 lines
Switch# terminal width 132         ! Set terminal width
Switch# terminal monitor           ! Enable syslog output to this terminal
Switch# terminal no monitor        ! Disable syslog output
Switch# terminal editing           ! Enable enhanced editing mode
Switch# terminal no editing        ! Disable enhanced editing mode
```

### `traceroute`
Trace the route to a destination.

```
Switch# traceroute ?
  WORD      Trace route to destination address or hostname
  clns      ISO CLNS Trace
  ip        IP Trace
  ipv6      IPv6 Trace
  vrf       specify vrf name
```

Example output:
```
Switch# traceroute 8.8.8.8

Type escape sequence to abort.
Tracing the route to 8.8.8.8
VRF info: (vrf in name/id, vrf out name/id)
  1 192.168.1.1 4 msec 1 msec 1 msec
  2 10.0.0.1 8 msec 6 msec 7 msec
  3 8.8.8.8 12 msec 11 msec 10 msec
```

Traceroute probe characters:
- `*` = probe timed out
- `!` = successful probe
- `!H` = host unreachable
- `!N` = network unreachable
- `!P` = protocol unreachable
- `!Q` = source quench
- `!A` = administratively prohibited
- `!X` = communication prohibited
- `!F` = fragmentation needed

### `undebug`
Disable debugging. `undebug all` is most commonly used.

```
Switch# undebug all
All possible debugging has been turned off
```

### `vlan`
VLAN database mode (deprecated in newer IOS but still present).

```
Switch# vlan ?
  WORD  ISL VLAN IDs 1-4094
  database  Enter vlan database mode
```

```
Switch# vlan database
Switch(vlan)#
```

### `write`
Write running configuration to memory, network, or terminal.

```
Switch# write ?
  erase    Erase the startup-config file
  memory   Write to NV memory
  network  Write to TFTP network server
  terminal Write to terminal
```

`write memory` is equivalent to `copy running-config startup-config`.

---

## Additional Notes

### Privilege Levels
Commands are assigned to privilege levels 0-15. Level 15 is full privileged EXEC (all commands). Level 1 is user EXEC. Levels 2-14 can be customized.

```
Switch# show privilege
Current privilege level is 15
```

### Command History
The CLI keeps a history of entered commands (default 10 commands).
```
Switch# show history
  show version
  show running-config
  show interfaces
```

Navigating history: Up arrow / Ctrl+P for previous, Down arrow / Ctrl+N for next.

To increase history size:
```
Switch# terminal history size 20
```
