# Response to ayiosupdate Upstream Observations

Source: `../ayiosupdate/docs/upstream-requests/ayclic-observations.md`
Date: 2026-03-21

## Observation 1: CiscoIosConn dual-transport layer

**Status: FIXED** (commit 9627b0d)

The `RawTransportAdapter` has been eliminated. `CiscoIosConn` now holds
a `CiscoIosConnInner` enum that dispatches directly to either:
- `Generic(GenericCliConn)` — for template-driven connections (new path)
- `Legacy(Box<dyn CiscoTransport>)` — for old-style constructors

The template-driven path (`new()`, `with_timeouts()`, `from_path()`,
`from_generic()`) uses `GenericCliConn` directly with zero adapter
indirection. The legacy path (`new_legacy()`, `with_timeouts_legacy()`,
`new_with_key()`) still uses `CiscoTransport` for backward compatibility
but is clearly marked as legacy.

## Observation 2: No built-in reconnect/retry mechanism

**Status: DEFERRED** — by design, handled by consumer

The architecture already supports reconnection:
1. Drop the existing `CiscoIosConn`
2. Re-run `ConnectionPath::connect()` or `CiscoIosConn::new()`
3. Resume operations

For ayiosupdate's upgrade workflow (where reload is expected), the
reconnect loop should live in ayiosupdate since it has the
domain-specific knowledge of:
- When to expect the device to go down
- How long to wait for it to come back
- What validation to perform after reconnection (show version, etc.)
- Whether to retry the full path or just the last hop

If a "reconnectable session" wrapper becomes needed by multiple
consumers, we can add it to ayclic later. The building blocks
(`ConnectionPath`, `GenericCliConn::connect()`) are all in place.

## Observation 3: `run_cmd_chat` prompt patterns are hardcoded

**Status: FIXED** (commit 9627b0d)

Two improvements:

1. `CiscoIosConn::run_cmd_chat()` already accepted custom prompt/response
   pairs via `Option<&[(&str, PromptAction)]>`. This was mentioned in the
   original API but may not have been obvious from the observation.

2. **New: `CiscoIosConn::run_cmd_with_template()`** — accepts a full
   TextFSMPlus template string for maximum flexibility. This is more
   powerful than prompt/response pairs because templates support:
   - State machines (multi-step interactions)
   - Value capture (extract data from prompts)
   - Computed responses (via aycalc expressions)
   - Error detection with diagnostic messages

   Example for upgrade-specific prompts:
   ```rust
   let template = r#"
   Start
     ^.*# -> Done
     ^.*\[y/n\]\s* -> Send "y"
     ^.*proceed\?\s* -> Send "yes"
     ^.*confirm\]\s* -> Send ""
   "#;
   conn.run_cmd_with_template("install add ...", template).await?;
   ```

   Note: `run_cmd_with_template()` is only available on template-driven
   connections (created via `new()`, not `new_legacy()`).

## Observation 4: Timeout on `run_cmd` — partial output recovery

**Status: DEFERRED** — existing behavior is sufficient for now

`CiscoIosError::Timeout { accumulated }` already preserves all data
received before the timeout. This is available for diagnostic logging.

For long-running commands like `install add` (10+ minutes), the
recommended approach is:
- Set a longer `read_timeout` via `with_timeouts()` or
  `GenericCliConn::with_cmd_timeout()`
- Use `run_cmd_with_template()` with a template that matches
  progress indicators (e.g., `[OK]`, `%` lines) to keep the
  interaction alive

A `drain()` method to recover remaining output after a timeout
could be added if needed, but the session state after an unexpected
timeout is inherently uncertain — the safest approach is usually
to reconnect.
