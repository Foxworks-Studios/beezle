# PRD 002: CLI Args (clap) — Ticket Breakdown

## Ticket 01: Define Cli struct and integrate into main.rs

**Scope**: `Cargo.toml`, `src/main.rs`

**Work**:
1. Add `clap` dependency with `derive` feature to `Cargo.toml`.
2. Define a `Cli` struct with clap derive macros containing all flags:
   - `--model <MODEL>` (optional, overrides config)
   - `--resume [KEY]` (optional value, stub for now)
   - `--prompt <TEXT>` (optional, single-shot mode)
   - `--skills <DIR>` (repeatable)
   - `--config <PATH>` (optional, overrides default config path)
   - `--verbose` (flag)
   - `--no-color` (flag)
   - Auto-generated `--help` and `--version`
3. Parse `Cli` at the top of `main()`.
4. Wire flags into existing logic:
   - `cli.config` -> `load_config()`
   - `cli.model` -> override `resolve_model()` result
   - `cli.skills` -> additional skill directories
   - `cli.prompt` -> single-shot: run one prompt, print result, exit
   - `cli.resume` -> print "not yet implemented" stub
   - `cli.no_color` -> gate ANSI output
5. Add unit tests for `Cli` parsing via `clap::Command::try_parse_from`.

**Acceptance Criteria**:
- [ ] `beezle --help` prints usage with all flags
- [ ] `beezle --version` prints version
- [ ] `beezle --model claude-opus-4-6` overrides config model
- [ ] `beezle --prompt "hello"` runs one turn and exits
- [ ] `beezle --resume` prints stub message
- [ ] `beezle --config /tmp/test.toml` uses that config path
- [ ] Invalid flags produce a helpful error
- [ ] `--no-color` disables ANSI codes
- [ ] Unit tests for CLI parsing pass
- [ ] `cargo test && cargo clippy -- -D warnings && cargo fmt --check` all pass

**Dependencies**: None

**Estimated size**: ~80-120 lines changed
