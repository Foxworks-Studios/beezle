# PRD 004: Command Bus

## Summary

Decouple input sources from the agent loop via a unified async command bus.
All input (terminal, future Discord/Telegram) flows through a single channel
the agent consumes from.

## Problem

The current REPL reads directly from stdin. Adding any other input source
(Discord bot, Telegram, cron jobs) would require threading input handling
into main.rs. The architecture needs a clean abstraction boundary.

## Solution

Introduce a `bus` module with an async mpsc channel. The terminal REPL
becomes one "channel adapter" that writes to the bus. The agent loop
consumes from the bus instead of stdin.

## Scope

- `src/bus/mod.rs` — `CommandBus`, `Command`, `Response` types
- `src/channels/mod.rs` — `Channel` trait
- `src/channels/terminal.rs` — terminal channel (current REPL logic extracted)
- `src/main.rs` — wire bus between channel and agent loop

## Requirements

### Must Have

1. **Command type**: `Command` struct with `source: ChannelKind`, `content: String`,
   `response_tx: oneshot::Sender<Response>` for reply routing.
2. **ChannelKind enum**: `Terminal`, `Discord`, `Telegram` (only Terminal
   implemented now, others are variants for future use).
3. **CommandBus**: Wraps `tokio::sync::mpsc` with `send()` and `recv()` methods.
4. **Channel trait**: `async fn start(bus: CommandBus) -> Result<()>` — each
   channel adapter runs as a tokio task feeding commands into the bus.
5. **Terminal channel**: Extract the current stdin REPL into a `TerminalChannel`
   implementing the `Channel` trait.
6. **Agent consumer**: The main loop reads from the bus instead of stdin.

### Nice to Have

- Broadcast channel for agent responses (multiple listeners).
- Channel-specific metadata on commands (e.g. Discord user ID).

## Acceptance Criteria

- [ ] Terminal REPL works identically to before but goes through the bus
- [ ] `Command` and `Response` types are defined with serde support
- [ ] `Channel` trait is defined and `TerminalChannel` implements it
- [ ] Agent loop consumes from `CommandBus`, not stdin directly
- [ ] Adding a new channel requires only implementing `Channel` trait
- [ ] Unit tests for bus send/recv, command routing

## Dependencies

- None (but PRD 001 sessions and PRD 002 clap should land first for clean
  integration)

## Estimated Size

~4 files, ~300-400 lines + tests
