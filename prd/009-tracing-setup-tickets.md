# PRD 009: Tracing Setup -- Ticket Breakdown

## Ticket 01: Initialize tracing subscriber with env-filter

**Scope**: `Cargo.toml`, `src/main.rs`

**Work**:
1. Add `tracing-subscriber` with `env-filter` feature to Cargo.toml.
2. At the top of `main()`, initialize `tracing_subscriber::fmt` with:
   - `EnvFilter` reading from `BEEZLE_LOG` env var
   - Default level: `warn`
   - `--verbose` flag overrides to `debug`
3. Add `tracing::debug!` call at startup to log config path and model for
   verifying the subscriber works.

**Acceptance Criteria**:
- [ ] `BEEZLE_LOG=debug` shows debug-level output
- [ ] Default log level is `warn` (quiet by default)
- [ ] `--verbose` enables debug logging
- [ ] `BEEZLE_LOG=beezle::config=trace` filters by module
- [ ] No `println!` diagnostics in library code
- [ ] `cargo test && cargo clippy -- -D warnings && cargo fmt --check` pass
