# PRD 009: Structured Logging with tracing

## Summary

Set up `tracing` + `tracing-subscriber` for structured, leveled logging
throughout the codebase. Replace any remaining `println!` diagnostics with
proper log macros.

## Problem

There is no logging infrastructure. Debugging requires adding `println!`
statements. There's no way to control log verbosity or filter by module.

## Solution

Initialize a `tracing_subscriber` in main.rs with env-filter support.
Use `tracing::{info, warn, error, debug, trace}` throughout the codebase.

## Scope

- `Cargo.toml` — add `tracing-subscriber` with `env-filter` feature
- `src/main.rs` — initialize subscriber early in main
- All modules — replace `println!` diagnostics with tracing macros

## Requirements

### Must Have

1. **Subscriber init**: Set up `tracing_subscriber::fmt` with `EnvFilter`
   in main.rs, defaulting to `warn` level.
2. **`BEEZLE_LOG` env var**: Controls log level (e.g. `BEEZLE_LOG=debug`).
3. **`--verbose` flag**: Sets log level to `debug` (ties into PRD 002 clap).
4. **Module filtering**: `BEEZLE_LOG=beezle::config=trace` works.
5. **No user-facing println for errors**: All error reporting uses
   `tracing::error!` (display to user is separate from logging).

### Nice to Have

- JSON log output option for machine parsing.
- Log file output to `~/.beezle/logs/`.

## Acceptance Criteria

- [ ] Running with `BEEZLE_LOG=debug` shows debug-level output
- [ ] Default log level is `warn` (quiet by default)
- [ ] `--verbose` flag enables debug logging
- [ ] No `println!` used for diagnostic/error output in library code
- [ ] Unit test verifying subscriber initializes without panic

## Dependencies

- PRD 002 (clap) for `--verbose` flag

## Estimated Size

~2 files touched, ~30-50 lines of new code
