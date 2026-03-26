# MockIOS Command Tree: Design & Implementation Plan

## Status: READY FOR IMPLEMENTATION

## Goal

Replace the hardcoded if/else command dispatch in `mockios/src/lib.rs` with a
data-driven command tree that provides:

- Unique prefix (abbreviation) matching — `sh ver` → `show version`
- Ambiguous command detection — `co` → `% Ambiguous command: "co"`
- Correct error classification:
  - `% Incomplete command.` — valid prefix but needs more tokens
  - `% Invalid input detected at '^' marker.` — unrecognized token at a specific position
  - `% Ambiguous command: "..."` — multiple prefix matches
- `?` help support (both `show ?` and `sh?` forms)
- Mode-aware command visibility (user exec, priv exec, config, config sub-modes)
- `do` prefix support from config modes
- Easy extensibility — adding a command = adding a node to the tree
- Foundation for future smart data model, tclsh mode, etc.

## Architecture

### File layout

```
mockios/src/
  lib.rs           — MockIosDevice (uses cmd_tree for dispatch)
  cmd_tree.rs      — CommandTree types, parser, builder API
  cmd_tree_exec.rs — Exec-mode command tree definitions + handlers
  cmd_tree_conf.rs — Config-mode command tree definitions + handlers
```

### Core types (`cmd_tree.rs`)

```rust
/// How a token in the command line is matched.
#[derive(Debug, Clone)]
pub enum TokenMatcher {
    /// A keyword — matched by unique prefix (e.g., "sh" matches "show").
    Keyword(String),
    /// A parameter placeholder — matches a value of the given type.
    Param {
        name: String,           // display name for help, e.g., "<ip-address>"
        param_type: ParamType,
    },
}

/// Types of parameter values.
#[derive(Debug, Clone)]
pub enum ParamType {
    /// Any single word.
    Word,
    /// An integer.
    Number,
    /// Rest of line (greedy — consumes all remaining tokens).
    RestOfLine,
}

/// Handler function signature.
/// Receives the device and the remaining unparsed args after the matched path.
/// The handler mutates device state (queue_output, change mode, set pending, etc.).
pub type CmdHandler = fn(&mut MockIosDevice, args: &str);

/// A node in the command tree.
#[derive(Clone)]
pub struct CommandNode {
    pub matcher: TokenMatcher,
    pub help: String,
    pub children: Vec<CommandNode>,
    pub handler: Option<CmdHandler>,
    pub mode_filter: ModeFilter,
}

/// Which CLI modes a command node is visible in.
#[derive(Debug, Clone)]
pub enum ModeFilter {
    /// Available in all modes where the parent tree applies.
    Any,
    /// Only in these specific mode classes.
    Only(Vec<CliModeClass>),
}

/// Simplified mode classification for filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliModeClass {
    UserExec,
    PrivExec,
    Config,
    ConfigSub,
}
```

### Parse result

```rust
pub enum ParseResult {
    /// Command matched — execute handler with remaining args.
    Execute {
        handler: CmdHandler,
        args: String,
    },
    /// Valid prefix but command is incomplete (node has children, no handler).
    Incomplete,
    /// No match at a specific byte position in the input.
    InvalidInput {
        caret_pos: usize,
    },
    /// Multiple keywords match the given prefix.
    Ambiguous {
        input: String,
        matches: Vec<String>,
    },
    /// The input is empty (just whitespace).
    Empty,
}
```

### Parser algorithm

```rust
pub fn parse(
    input: &str,
    tree: &[CommandNode],
    mode: &CliMode,
) -> ParseResult
```

1. Tokenize `input` by whitespace, tracking byte offsets.
2. For each token, filter `current_children` by `mode_filter` vs current `mode`.
3. Find matches: keyword children where `keyword.starts_with(token_lowercase)`,
   plus param children where `param_type.matches(token)`.
4. Match count:
   - **0** → `InvalidInput { caret_pos }` where caret_pos is the byte offset of this token.
   - **1** → descend into matched node. If this is the last token:
     - Node has handler → `Execute { handler, remaining_args }`
     - Node has no handler but has children → `Incomplete`
     - Node has no handler and no children → `Execute` should not happen (design error)
   - **>1** → `Ambiguous { input, matches }`
5. If a matched node has `ParamType::RestOfLine`, consume all remaining tokens as args.

### `?` help algorithm

```rust
pub fn help(
    input_before_question: &str,
    tree: &[CommandNode],
    mode: &CliMode,
) -> HelpResult
```

Two forms:

- **`show ?`** (input ends with space): Walk tree to resolve `show`, then list its
  visible children with help text, formatted as `  {keyword:<20}{help}`.
- **`sh?`** (no trailing space): Walk tree to resolve tokens before the last partial
  token, then list visible children whose keyword starts with the partial token.
  Format: `{keyword}  {keyword}  ...` (space-separated, then re-show prompt + input).

### Builder API

```rust
pub fn keyword(name: &str, help: &str) -> CommandNode { ... }
pub fn param(name: &str, param_type: ParamType, help: &str) -> CommandNode { ... }

impl CommandNode {
    pub fn handler(mut self, h: CmdHandler) -> Self { ... }
    pub fn children(mut self, c: Vec<CommandNode>) -> Self { ... }
    pub fn child(mut self, c: CommandNode) -> Self { ... }
    pub fn mode(mut self, m: ModeFilter) -> Self { ... }
}
```

### Mode mapping

```rust
impl CliModeClass {
    pub fn from_cli_mode(mode: &CliMode) -> Self {
        match mode {
            CliMode::UserExec => CliModeClass::UserExec,
            CliMode::PrivilegedExec => CliModeClass::PrivExec,
            CliMode::Config => CliModeClass::Config,
            CliMode::ConfigSub(_) => CliModeClass::ConfigSub,
            _ => panic!("not a command mode"),
        }
    }
}

impl ModeFilter {
    pub fn matches(&self, mode: &CliMode) -> bool {
        match self {
            ModeFilter::Any => true,
            ModeFilter::Only(classes) => {
                classes.contains(&CliModeClass::from_cli_mode(mode))
            }
        }
    }
}
```

## `do` prefix handling

Handled as a special case BEFORE tree parsing in `handle_config_mode()` and
`handle_config_sub()`:

```
if cmd.starts_with("do ") {
    strip "do " prefix
    parse against exec tree with mode = PrivilegedExec
    execute result
    restore config mode
    return
}
```

This is simpler than making `do` a tree node whose children reference the exec tree.

## `no` prefix handling

In config mode, `no` is a tree node at the top level whose children mirror the
config tree. This way `no shutdown`, `no ip route ...`, etc. all parse correctly.
The handler receives a flag or the `no` prefix is part of the args.

Simpler alternative for now: `no` is a keyword node with `ParamType::RestOfLine`
handler that stores the negation in running config. This can be refined later.

## Tree definitions

### Exec tree (both user exec and priv exec)

```
show                          — "Show running system information"
  version                     — "System hardware and software status"
  running-config              — "Current operating configuration" [priv only]
  startup-config              — "Contents of startup configuration" [priv only]
  clock                       — "Display the system clock"
  ip                          — "IP information"
    interface                 — "IP interface status and configuration"
      brief                   — "Brief summary of IP status and configuration"
    route                     — "IP routing table"
  boot                        — "Boot and startup information"
  interfaces                  — "Interface status and configuration"
    <name>                    — specific interface
  install                     — "Install information" [priv only]
    summary                   — "Show install summary"
  flash:                      — "Display information about flash: file system"
configure                     — "Enter configuration mode" [priv only]
  terminal                    — "Configure from the terminal"
enable                        — "Turn on privileged commands" [user only]
disable                       — "Turn off privileged commands" [priv only]
terminal                      — "Set terminal line parameters"
  length                      — "Set number of lines on a screen"
    <number>                  — line count
  width                       — "Set width of the terminal"
    <number>                  — column count
copy                          — "Copy from one file to another" [priv only]
  <source>                    — source URL/file
    <dest>                    — destination URL/file
delete                        — "Delete a file" [priv only]
  <filespec>                  — file to delete
dir                           — "List files on a filesystem" [priv only]
  <filesystem>                — filesystem (e.g., flash:)
verify                        — "Verify a file" [priv only]
  /md5                        — "MD5 signature"
    <filespec>                — file to verify
reload                        — "Halt and perform a cold restart" [priv only]
  cancel                      — "Cancel pending reload"
  in                          — "Reload after a time interval"
    <minutes>                 — minutes until reload
write                         — "Write running configuration" [priv only]
  memory                      — "Write to NV memory"
install                       — "Install commands" [priv only]
  add                         — "Add a package"
    file                      — "Add from file"
      <filespec>              — package file
  activate                    — "Activate installed packages"
  commit                      — "Commit activated packages"
  remove                      — "Remove packages"
    inactive                  — "Remove inactive packages"
ping                          — "Send echo messages"
  <target>                    — target address
traceroute                    — "Trace route to destination"
  <target>                    — target address
exit                          — "Exit from the EXEC"
```

### Config tree

```
hostname                      — "Set system's network name"
  <name>                      — hostname string
interface                     — "Select an interface to configure"
  <name>                      — interface name (enters config-if)
router                        — "Enable a routing process"
  ospf                        — "OSPF routing"
    <process-id>              — process ID
  bgp                         — "BGP routing"
    <as-number>               — AS number
  eigrp                       — "EIGRP routing"
    <as-number>               — AS number
ip                            — "Global IP configuration subcommands"
  route                       — "Establish static routes"
    <prefix>                  — destination prefix
      <mask>                  — destination mask
        <nexthop>             — forwarding router's address
  address                     — "Set the IP address of an interface" [config-if]
    <ip>                      — IP address
      <mask>                  — subnet mask
  domain-name                 — "Define the default domain name"
    <name>                    — domain name
  name-server                 — "Specify address of name server"
    <ip>                      — name server address
no                            — "Negate a command or set its defaults"
  (mirrors config tree)       — or RestOfLine for simplicity
line                          — "Configure a terminal line"
  vty                         — "Virtual terminal"
    <first>                   — first line number
      <last>                  — last line number (enters config-line)
  console                     — "Primary terminal line"
    <number>                  — line number (enters config-line)
enable                        — "Modify enable password parameters"
  secret                      — "Assign the privileged level secret"
    <password>                — the secret
  password                    — "Assign the privileged level password"
    <password>                — the password
service                       — "Modify use of network based services"
  <rest>                      — RestOfLine
logging                       — "Modify message logging facilities"
  <rest>                      — RestOfLine
username                      — "Establish User Name Authentication"
  <rest>                      — RestOfLine
shutdown                      — "Shutdown the selected interface" [config-if]
description                   — "Interface specific description" [config-if]
  <rest>                      — RestOfLine
switchport                    — "Set switching mode characteristics" [config-if]
  <rest>                      — RestOfLine
spanning-tree                 — "Spanning Tree Subsystem"
  <rest>                      — RestOfLine
vlan                          — "VLAN commands"
  <rest>                      — RestOfLine
exit                          — "Exit from current mode"
end                           — "Exit to privileged EXEC mode"
do                            — special: dispatches to exec tree
```

## Implementation steps (TDD)

### Step 1: Core types and parser (`cmd_tree.rs`)

Tests first:
1. `test_keyword_exact_match` — "show" matches "show"
2. `test_keyword_prefix_match` — "sh" matches "show"
3. `test_keyword_no_match` — "xyz" matches nothing
4. `test_keyword_ambiguous` — "co" matches "configure" and "copy"
5. `test_parse_incomplete` — "show ip" has children but no handler
6. `test_parse_invalid_input_caret` — "show bogus" → caret at "bogus"
7. `test_parse_execute` — "show version" → handler called
8. `test_param_word_match` — param(Word) matches any token
9. `test_param_number_match` — param(Number) matches "42" but not "abc"
10. `test_parse_rest_of_line` — RestOfLine consumes everything
11. `test_mode_filter` — node only visible in matching mode
12. `test_help_subcommands` — "show ?" lists show's children
13. `test_help_prefix_completion` — "sh?" lists commands starting with "sh"

### Step 2: Builder API

Tests:
1. `test_builder_keyword` — keyword() creates correct node
2. `test_builder_chain` — keyword().child().handler() builds tree
3. `test_builder_mode` — .mode() sets filter

### Step 3: Exec command tree (`cmd_tree_exec.rs`)

Build the exec tree and handler functions. Each handler is a
`fn(&mut MockIosDevice, &str)` that calls `self.queue_output()`.

Tests: The existing 61 tests in lib.rs should all still pass after
replacing the if/else dispatch with tree dispatch. Run them as the
regression suite.

Additional tests:
1. `test_abbreviation_show_ver` — "show ver" → show version output
2. `test_abbreviation_conf_t` — "conf t" → enter config mode
3. `test_abbreviation_sh_run` — "sh run" → running config
4. `test_ambiguous_command` — "co" → ambiguous error
5. `test_caret_position_correct` — "show xyz" → caret under "xyz"

### Step 4: Config command tree (`cmd_tree_conf.rs`)

Build the config tree. Handlers mutate running_config, change modes, etc.

Tests:
1. `test_config_known_command_accepted` — "hostname Foo" accepted
2. `test_config_unknown_command_caret` — "bogusconfigcmd" → caret error
3. `test_config_interface_enters_submode` — "interface Gi0/0" → config-if
4. `test_config_do_prefix` — "do show version" works from config
5. `test_config_exit_and_end` — mode transitions

### Step 5: Integration into MockIosDevice

Replace `handle_user_exec()`, `handle_privileged_exec()`, `handle_config_mode()`,
`handle_config_sub()` to use tree parsing.

The `MockIosDevice` stores the exec and config trees (or references a global).
Decision: Use `once_cell::sync::Lazy` for global trees since they're the same for
all device instances. If per-device customization is needed later, the device can
override specific nodes.

### Step 6: `?` help integration

Wire up `?` processing. Since mockios is line-buffered for automation testing,
`?` help is triggered when the input line ends with `?`. In server/interactive
mode, `?` is processed immediately (character-level).

For now, support `?` in line-buffered mode:
- Input "show ?\n" → list show subcommands, re-display prompt
- Input "sh?\n" → list prefix matches, re-display prompt + "sh"

## Design decisions

1. **Tree ownership**: Global `Lazy<Vec<CommandNode>>` — same tree for all
   instances. Per-device customization can be added later via an overlay mechanism.

2. **Handler signature**: `fn(&mut MockIosDevice, args: &str)` — handlers
   mutate device directly (queue_output, change mode, set pending_interactive).
   This is more flexible than returning a String.

3. **`no` prefix**: For now, `no` is a keyword node with a `RestOfLine` child
   that stores the negation in running config. Can be refined to mirror the
   config tree later.

4. **`do` prefix**: Special-cased before tree parsing, not a tree node.
   Strips "do ", dispatches against exec tree with PrivExec mode, restores mode.

5. **Config sub-mode trees**: Each sub-mode (config-if, config-router, config-line)
   can have its own tree of valid commands. For now, use a shared config-sub tree
   with mode-filtered nodes (`shutdown` only in config-if, etc.).

## Dependencies

No new external crates needed. `once_cell` is likely already available or can be
replaced with `std::sync::LazyLock` (stabilized in Rust 1.80).
