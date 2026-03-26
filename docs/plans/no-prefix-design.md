# Design: `no` as a Prefix Modifier

## Problem

Currently `no` is a keyword with `<rest>` RestOfLine child. This means:
- `no` eats everything as one blob — no abbreviation matching, no tab completion
- `no shut<TAB>` doesn't complete to `no shutdown`
- `no ip ro<TAB>` doesn't complete
- Each `no X` case must be special-cased in `handle_no()`

Real IOS treats `no` as a prefix that re-enters the same command tree.
`no ip route` goes through the same `ip` → `route` tree as `ip route`,
and the handler knows it's negated because the raw input starts with `no `.

## Solution: `no` children mirror the parent tree

Instead of `no <rest>`, make `no` a keyword whose children ARE the same
command nodes (minus `no` itself to prevent `no no ...`).

### In conf_tree:
```rust
// Build main tree
let mut main_commands = vec![
    keyword("hostname", ...).handler(handle_hostname),
    keyword("interface", ...).children(..),
    keyword("ip", ...).children(..),
    keyword("shutdown", ...).handler(handle_shutdown),
    // ... etc
];

// no — children are the same tree (handlers check input for "no" prefix)
let no_children = main_commands.clone(); // Clone the tree
main_commands.push(
    keyword("no", "Negate a command or set its defaults")
        .children(no_children)
);
```

### Handler behavior
Handlers already receive the full input line. They check for negation:
```rust
pub fn handle_shutdown(d: &mut MockIosDevice, input: &str) {
    let negated = input.trim().starts_with("no");
    if let Some(ref iface_name) = d.current_interface.clone() {
        if let Some(iface) = d.state.get_interface_mut(iface_name) {
            iface.admin_up = negated; // "no shutdown" = admin_up=true
        }
    }
    // ...
}
```

### Benefits
- Tab completion works: `no shut<TAB>` → `no shutdown`
- `?` help works: `no ?` shows the same commands as top-level `?`
- No code duplication — handlers just check for the `no` prefix
- Abbreviation matching works: `no sh` matches `no shutdown`

### Considerations
- Must exclude `no` from its own children (prevent `no no ...`)
- Must exclude `exit`, `end`, `help` from `no` children (those can't be negated)
- Some commands have different behavior under `no` (e.g., `no ip address` removes
  the address entirely, not sets a different one)
- The clone is a one-time cost at tree initialization (OnceLock)

### Commands that need `no` handling updated in their handlers:
- `hostname` → `no hostname` resets to default "Router"
- `ip address` → `no ip address` removes IP from interface
- `ip route` → `no ip route` removes static route (already works)
- `shutdown` → `no shutdown` enables interface (already works)
- `description` → `no description` clears description
- `switchport` → `no switchport mode` etc.
- `access-list` → `no access-list <num>` removes the ACL
- `banner motd` → `no banner motd` clears the banner
- `ip domain-name` → `no ip domain-name` clears domain name
- `service timestamps` → `no service timestamps` disables

## Implementation Plan
1. Refactor tree building to collect commands in a Vec first
2. Clone the Vec (minus no/exit/end/help) as `no` children
3. Update handlers to check for negation prefix
4. Fix P0 bugs first (description, enable secret writing to wrong field)
5. Add tests for `no` tab completion and execution
