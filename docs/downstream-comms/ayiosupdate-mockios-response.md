# Response to ayiosupdate mockios Observations

Source: `../ayiosupdate/docs/upstream-requests/mockios-observations.md`
Date: 2026-03-21

## Observation 1: Reload simulation

**Status: IMPLEMENTED** (commit e6ef0e6)

All requested features for the in-process (RawTransport) mode:

- **`Reloading` CliMode**: After reload confirmation, device enters
  `Reloading` state. `send()`/`receive()` return `CiscoIosError::NotConnected`.
- **`derive()`**: Creates a post-reload device carrying flash files, hostname,
  model, commands, config. New device starts in PrivilegedExec.
- **`with_reload_transform()`**: Queue transforms applied on `power_on()`.
  Supports chained reloads (e.g., bundle→install with two transforms).
- **`power_on()`**: Applies next queued transform, resets to PrivilegedExec.
- **`is_reloading()`**: Check if device is in Reloading state.
- **`reload_delay()`**: Getter for configurable reload delay.
- **`reload in N`**: Full interactive flow (Save? → [confirm] → Reloading).
- **`reload cancel`**: Unchanged — aborts shutdown.

Not yet implemented for server mode:
- `DeviceFactory` / shared state for telnet/SSH server
- Connection refusal during simulated downtime
- `reload in N` timer with `AtomicBool` flag

These can be added when server-mode testing is needed. The in-process
mode covers ayiosupdate's primary test scenario.

## Observation 2: Stack simulation (`show switch`)

**Status: DEFERRED**

This requires significant new data structures (StackMemberConfig, per-member
flash filesystems). Recommended approach for now: use `with_command()` to
register custom `show switch` output. Per-member flash can be simulated
with `with_flash_file("flash-2:image.bin", ...)` once we add support for
multi-flash addressing.

## Observation 3: SSO simulation (`show redundancy`)

**Status: DEFERRED**

Can be handled with `with_command("show redundancy", ...)` for now. A typed
builder can be added when the test suite needs dynamic redundancy state.

## Observation 4: Install mode state (`show install summary`)

**Status: DEFERRED**

Needs `InstallState` struct and command handlers. Can be incrementally
built as ayiosupdate develops the install mode upgrade workflow tests.
For initial testing, `with_command()` works.

## Observation 5: `install` command family

**Status: DEFERRED**

Depends on #4. The interactive simulation pattern is already proven
(copy, reload handlers). Adding `install add/activate/commit/rollback`
follows the same approach.

## Observation 6: `show boot` output

**Status: IMPLEMENTED** (commit e6ef0e6)

- `with_boot_variable("flash:packages.conf")`: Set boot variable.
- `show boot` command generates BOOT variable output.
- Default boot variable derived from model + version if not set.

## Observation 7: Flash space simulation

**Status: IMPLEMENTED** (commit e6ef0e6)

- `with_flash_size(8_000_000_000)`: Set total flash size.
- `dir flash:` output now shows individual file sizes, total bytes,
  and free bytes (computed from total minus sum of file sizes).

## Observation 8: Platform variant profiles

**Status: DEFERRED**

Use `with_command("show version", include_str!("captured_output.txt"))`
for now. Platform profiles can be added when needed.

## Observation 9: HTTP copy simulation fidelity

**Status: DEFERRED**

Current behavior is sufficient for most tests. The mock accepts
`copy http://... flash:...` and creates a placeholder file in flash.
For tests that need specific file content, use `with_flash_file()`
to pre-populate before the copy.

## Summary

| # | Feature | Status |
|---|---------|--------|
| 1 | Reload simulation (in-process) | **DONE** |
| 1 | Reload simulation (server) | Deferred |
| 2 | Stack simulation | Deferred (use `with_command()`) |
| 3 | SSO simulation | Deferred (use `with_command()`) |
| 4 | Install mode state | Deferred |
| 5 | Install command family | Deferred |
| 6 | `show boot` output | **DONE** |
| 7 | Flash space simulation | **DONE** |
| 8 | Platform variant profiles | Deferred |
| 9 | HTTP copy fidelity | Deferred |

Items 1/6/7 unblock initial upgrade workflow tests. Items 2-5 can be
incrementally added as the ayiosupdate test suite develops. Items 8-9
are nice-to-haves that can use `with_command()` in the meantime.
