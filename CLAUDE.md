# Project: Beezle

## What This Is

A fully-featured AI coding agent CLI (like Claude Code) built on the `yoagent` agent loop crate, with planned multi-channel input support (Discord, Slack, Telegram, WhatsApp). Iteratively re-implementing features from `beezle-rs` while leveraging `yoagent`'s agent loop.

## GitHub

- **Org**: [Foxworks-Studios](https://github.com/Foxworks-Studios)
- **This repo**: `Foxworks-Studios/beezle`
- **Reference project**: `Foxworks-Studios/beezle-rs` — use as inspiration for feature parity targets, but adapt to idiomatic `yoagent` patterns
- **yoagent fork**: `Foxworks-Studios/yoagent` (branch `streaming-prompt`) — our fork adds true streamed events instead of buffered. A PR has been submitted upstream but hasn't received feedback yet. Our fork is the source of truth for the yoagent API.

## Tech Stack

- Language: Rust (edition 2024)
- Agent Loop: `yoagent` (forked — provides agent loop, tools, skills, streaming)
- Async Runtime: `tokio` (full features)
- CLI: `clap` (derive)
- LLM Providers: Anthropic Claude (cloud), Ollama (local)
- Logging: `tracing` + `tracing-subscriber`
- Serialization: `serde` + `serde_json`, `toml` (config)
- Error Handling: `thiserror` (library errors), `anyhow` (application errors)
- Testing: `cargo test` (built-in), `tempfile` (test fixtures)

## Setup

1. Set `ANTHROPIC_API_KEY` in your environment (or use Ollama for local-only)
2. `cargo run` — on first run, interactive onboarding walks you through provider/model selection
3. Config is saved to `~/.beezle/config.toml`

## Architecture

The codebase is organized into domain modules:

```
src/
  main.rs           # Entry point, CLI arg parsing, REPL loop, streaming output
  lib.rs            # Public API re-exports
  agent/            # Agent setup, system prompts, subagent construction
  bus/              # Command bus for multi-channel input (terminal, Discord, etc.)
  channels/         # Input channel adapters (terminal implemented)
  config/           # Configuration loading, validation, and interactive onboarding
  context/          # Project context loading (CLAUDE.md, git info, etc.)
  memory/           # Persistent memory system (long-term + daily notes)
  session/          # Session management, conversation state persistence
  tools/            # Custom tool implementations (memory read/write)
```

**Planned / aspirational modules** (not yet created):
```
  tui/              # Terminal UI (ratatui) — planned
  skills/           # Custom skill definitions — planned
```

Key architectural decisions:
- **Command bus**: All input channels (terminal REPL, Discord, Telegram, etc.) feed into a unified command bus. The agent consumes commands from the bus, not directly from stdin.
- **Idiomatic yoagent API**: All tools, skills, and agents MUST be defined using `yoagent`'s traits and APIs. Custom functionality must conform to `yoagent`'s extension points.
- **Feature flags**: Channel integrations (Discord, Telegram, etc.) are behind Cargo feature flags to keep the default build lean.
- **Multi-provider**: Supports both Anthropic (cloud) and Ollama (local) via onboarding config.

## Commands

- `cargo build` — build the project
- `cargo test` — run all tests
- `cargo clippy -- -D warnings` — lint with all warnings as errors
- `cargo fmt --check` — check formatting
- `cargo run` — run the agent CLI
- `cargo run -- --model claude-opus-4-6` — run with a specific model override
- `cargo run -- --resume` — resume most recent session
- `cargo run -- --prompt "do something"` — non-interactive single-prompt mode

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

- All tools, skills, and agents MUST use `yoagent`'s traits and APIs — no custom abstractions that bypass `yoagent`'s extension model
- Use feature flags for optional channel integrations (e.g., `telegram`, `discord`)
- All input sources go through the command bus — never read directly from stdin in agent code
- Use `tracing` for all logging, never `println!` for diagnostics
- Use `thiserror` for error types in library modules, `anyhow` in `main.rs`/binary code
- Bring in crates for solved problems rather than rolling custom solutions
- Follow `Foxworks-Studios/beezle-rs` for feature parity targets, but adapt to idiomatic `yoagent` patterns
- Testable I/O: functions that interact with users take generic `Read`/`Write` params (see `config/onboard.rs` for the pattern)

## Landmines / Gotchas

- **yoagent fork**: We depend on our fork (`Foxworks-Studios/yoagent`, branch `streaming-prompt`), NOT the upstream crate. Always check our fork's source for the current trait API before implementing tools or skills.
- **Edition 2024 Rust**: Be aware of edition-specific behavior changes (e.g., `gen` is a reserved keyword, lifetime capture rules in opaque types).
- **main.rs is large (~60KB)**: It evolved from yoagent's CLI example and contains the full REPL loop, streaming output, and display logic. This is a known tech debt target for extraction into modules.
- **beezle-rs has a fully custom agent loop**: Not everything from it needs to be ported since `yoagent` handles the core loop. Check what `yoagent` already provides before porting a feature.
- **Config path**: Default config lives at `~/.beezle/config.toml`. The onboarding flow creates this on first run.

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
