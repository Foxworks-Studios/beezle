# PRD 002: CLI Argument Parsing with clap

## Summary

Replace the ad-hoc `std::env::args()` parsing in `main.rs` with `clap`
derive-based argument parsing for correctness, help text, and extensibility.

## Problem

The current main.rs manually iterates `args` looking for `--model`,
`--api-url`, `--skills`. This doesn't generate `--help`, has no validation,
and is fragile to extend.

## Solution

Add `clap` as a dependency and define a `Cli` struct with derive macros.

## Scope

- `Cargo.toml` — add `clap` dependency
- `src/main.rs` — replace manual arg parsing with `Cli` struct

## Requirements

### Must Have

1. `--model <MODEL>` — override the model from config.
2. `--resume [KEY]` — resume a session (optional key, ties into PRD 001).
3. `--prompt <TEXT>` — single-shot mode: run one prompt and exit.
4. `--skills <DIR>` — additional skill directories (repeatable).
5. `--help` — auto-generated help text.
6. `--version` — print version.
7. `--config <PATH>` — override config file path.

### Nice to Have

- `--verbose` flag for debug logging.
- `--no-color` flag to disable ANSI output.

## Acceptance Criteria

- [ ] `beezle --help` prints usage with all flags documented
- [ ] `beezle --version` prints the version from Cargo.toml
- [ ] `beezle --model claude-opus-4-6` overrides the config model
- [ ] `beezle --prompt "hello"` runs one turn and exits
- [ ] `beezle --resume` works (requires PRD 001, can stub initially)
- [ ] Invalid flags produce a helpful error message
- [ ] Unit tests for CLI struct parsing

## Dependencies

- Soft dependency on PRD 001 for `--resume` (can accept the flag and print
  "not yet implemented" until sessions land)

## Estimated Size

~1 file touched, ~50-80 lines of new code
