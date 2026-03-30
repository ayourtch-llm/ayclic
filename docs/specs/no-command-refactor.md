# Refactor: Handle `no` at dispatch level, not tree level

## Current Problem
The `no` command in config mode is implemented by cloning the entire command tree
and inserting it as children of a `no` keyword node. This is wasteful and doesn't
scale. It also duplicates every command node.

## Proposed Solution
Handle `no` at the dispatch level in `dispatch_config()`:

1. **Remove** `build_no_node()` and all calls to it from conf_tree builders
2. **Keep** a single `no` keyword in the tree that is handled specially at dispatch
3. In `dispatch_config()`, detect `no ` prefix:
   - Strip `no ` from the line
   - Parse the remainder against the SAME tree (no cloning needed)
   - When executing: pass the FULL original line (with `no`) to the handler
   - When the command has `no_handler`: use that for bare invocations
   - When the command has only `handler`: use it (handlers already check for `no` prefix)
4. For `?` help: `no ?` should list the same commands as the main tree (minus no/exit/end/help/do)

## Benefits
- Eliminates 4x tree cloning (conf, config-if, config-line, config-router)
- Makes `no_handler` work naturally without tree manipulation
- Single source of truth for command definitions
- Easier to maintain as more commands are added

## Implementation Notes
- The `no` keyword needs special handling in help() too - when user types `no ?`,
  list the same commands from the parent tree
- Some commands behave differently under `no` (e.g., `no shutdown` vs `shutdown`)
  - These already handle it via `input.starts_with("no")` checks
- The `no_handler` field allows commands where `no X` takes no args but `X <arg>` does
