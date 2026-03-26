# MockIOS CLI Editing Keys

## Status: IN PROGRESS

## Cisco IOS CLI Editing Keys (Emacs-style)

### Currently implemented in mockios
- `?` — context-sensitive help (immediate)
- Tab — command completion
- Backspace/DEL (`0x7f`, `0x08`) — erase char left
- Ctrl+C (`0x03`) — cancel input, new prompt
- Ctrl+Z (`0x1A`) — exit config mode to priv exec
- `\r`, `\n` — submit line

### To implement (priority order)

#### High priority — basic editing
- **Ctrl+A** (`0x01`) — move cursor to beginning of line
- **Ctrl+E** (`0x05`) — move cursor to end of line
- **Ctrl+D** (`0x04`) — delete char under cursor; if line empty, disconnect (like logout)
- **Ctrl+U** (`0x15`) — erase from cursor to beginning of line
- **Ctrl+K** (`0x0B`) — erase from cursor to end of line
- **Ctrl+W** (`0x17`) — erase word before cursor
- **Left/Right arrows** — move cursor (ESC `[D` / ESC `[C`)
- **Up/Down arrows** — command history (ESC `[A` / ESC `[B`)

#### Medium priority — nice to have
- **Ctrl+F** (`0x06`) — forward one char (same as right arrow)
- **Ctrl+B** (`0x02`) — backward one char (same as left arrow)
- **Ctrl+P** (`0x10`) — previous command (same as up arrow)
- **Ctrl+N** (`0x0E`) — next command (same as down arrow)
- **Ctrl+R** (`0x12`) — redisplay current line
- **Ctrl+Y** (`0x19`) — paste last deleted text (yank)
- **Ctrl+T** (`0x14`) — transpose chars

#### Low priority
- **Esc+F** — forward one word
- **Esc+B** — backward one word
- **Esc+D** — delete word forward

## Implementation design

### Cursor tracking

Add to MockIosDevice:
```rust
cursor_pos: usize,          // byte position within input_buffer
command_history: Vec<String>,
history_index: Option<usize>, // None = typing new command
delete_buffer: String,       // for Ctrl+Y yank
```

### Escape sequence parsing

Arrow keys and escape sequences arrive as multi-byte sequences:
- ESC `[A` = Up arrow (3 bytes: 0x1B, 0x5B, 0x41)
- ESC `[B` = Down arrow
- ESC `[C` = Right arrow
- ESC `[D` = Left arrow

Need an escape sequence state machine in `send()`:
```rust
enum EscState {
    Normal,
    GotEsc,      // received 0x1B
    GotBracket,  // received 0x1B 0x5B
}
```

### Terminal cursor movement output

To move the cursor on the terminal, send ANSI escape sequences:
- Move left: `\x1B[D`
- Move right: `\x1B[C`
- Erase to end of line: `\x1B[K`
- Move to column N: `\x1B[{N}G`

For Ctrl+A (move to start): send enough `\x1B[D` to go back, or
use `\r` then re-output the prompt.

### Command history

On Enter, push completed command to `command_history` (if non-empty).
Up arrow: replace current input_buffer with previous command from history.
Down arrow: replace with next command (or clear if at end).

When recalling history:
1. Erase current displayed line (send `\r`, output prompt, erase to end)
2. Output the history command
3. Update input_buffer and cursor_pos
