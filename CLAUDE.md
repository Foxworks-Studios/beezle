# Project: Beezle

## What This Is

A fully-featured AI coding agent CLI (like Claude Code) built on the `yoagent` agent loop crate, with planned multi-channel input support (Discord, Slack, Telegram, WhatsApp). Iteratively re-implementing features from `../beezle-rs` while leveraging `yoagent`'s agent loop. Because `yoagent` provides the core agent loop, some features from `beezle-rs` may already be covered and won't need reimplementation.

## Tech Stack

- Language: Rust (edition 2024)
- Agent Loop: `yoagent` (provides agent loop, tools, skills, streaming)
- Async Runtime: `tokio` (full features)
- CLI: `clap` (derive)
- Logging: `tracing` + `tracing-subscriber`
- TUI: `ratatui` + `crossterm`
- Serialization: `serde` + `serde_json`
- Error Handling: `thiserror` (library errors), `anyhow` (application errors)
- Testing: `cargo test` (built-in)

## Architecture

The codebase is organized into domain modules:

```
src/
  main.rs           # Entry point, CLI arg parsing, app bootstrap
  lib.rs            # Public API re-exports
  bus/              # Command bus for multi-channel input (terminal, Discord, etc.)
  config/           # Configuration loading and validation
  agent/            # Agent setup, system prompts, yoagent wiring
  channels/         # Input channel adapters (terminal, future: Discord, Slack, etc.)
  tools/            # Custom tool implementations (using yoagent tool traits)
  skills/           # Custom skill definitions (using yoagent skill traits)
  tui/              # Terminal UI (ratatui)
  session/          # Session management, conversation state
```

Key architectural decisions:
- **Command bus**: All input channels (terminal REPL, Discord, Telegram, etc.) feed into a unified command bus. The agent consumes commands from the bus, not directly from stdin.
- **Idiomatic yoagent API**: All tools, skills, and agents MUST be defined using `yoagent`'s traits and APIs. This keeps the project compatible with `yoagent` and prevents API drift. Custom functionality is welcome, but it must conform to `yoagent`'s extension points.
- **Feature flags**: Channel integrations (Discord, Telegram, etc.) are behind Cargo feature flags to keep the default build lean.

## Commands

- `cargo build` -- build the project
- `cargo test` -- run all tests
- `cargo clippy -- -D warnings` -- lint with all warnings as errors
- `cargo fmt --check` -- check formatting
- `cargo run` -- run the agent CLI

## Development Workflow

### Red/Green TDD (MANDATORY)

**Every change in this project follows strict red/green TDD. No exceptions.**

1. **Red**: Write a failing test FIRST that describes the desired behavior.
2. **Green**: Write the minimum code to make the test pass.
3. **Refactor**: Clean up while keeping tests green.

Do NOT write implementation code without a failing test. Do NOT skip the red step. If you catch yourself writing code first, stop, delete it, and write the test first.

This applies to:
- New features (write acceptance/unit tests first)
- Bug fixes (write a test that reproduces the bug first)
- Refactors (ensure existing tests cover the behavior, add tests if they don't)

### Before Committing

- [ ] All tests pass (`cargo test`)
- [ ] No compiler warnings (`cargo build`)
- [ ] Clippy passes (`cargo clippy -- -D warnings`)
- [ ] Code is formatted (`cargo fmt --check`)
- [ ] All public items have doc comments
- [ ] No commented-out code or debug statements
- [ ] No hardcoded credentials

## Conventions

- All tools, skills, and agents MUST use `yoagent`'s traits and APIs -- no custom abstractions that bypass `yoagent`'s extension model
- Use feature flags for optional channel integrations (e.g., `telegram`, `discord`)
- All input sources go through the command bus -- never read directly from stdin in agent code
- Use `tracing` for all logging, never `println!` for diagnostics
- Use `thiserror` for error types in library modules, `anyhow` in `main.rs`/binary code
- Bring in crates for solved problems rather than rolling custom solutions
- Follow the reference project `../beezle-rs` for feature parity targets, but adapt to idiomatic `yoagent` patterns

## Landmines / Gotchas

- `yoagent` is a relatively new crate -- always check its docs/source for the current trait API before implementing tools or skills
- The current `main.rs` is a copy of `yoagent`'s CLI example -- it will be restructured into the module architecture above
- `beezle-rs` has a fully custom agent loop; not everything from it needs to be ported since `yoagent` handles the core loop
- Edition 2024 Rust -- be aware of edition-specific behavior changes (e.g., `gen` is a reserved keyword, lifetime capture rules in opaque types)

## CI

Automated checks (GitHub Actions or equivalent):

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- `cargo build`

## Agent Workflow File Structure

All agent artifacts live under `/prd/`. The expected structure for a feature:

```
/prd/
  NNN-feature-name.md                    # PRD (source of truth)
  NNN-feature-name-tickets.md            # Ticket breakdown
  NNN-feature-name-status.md             # Orchestrator's live status tracker
  NNN-feature-name-qa.md                 # Final QA report
  NNN-feature-name-reports/              # Per-ticket reports
    ticket-01-impl.md                    # Implementer completion report
    ticket-01-review.md                  # Code review result
    ticket-02-impl.md
    ticket-02-review.md
    ...
```
