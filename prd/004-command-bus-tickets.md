# PRD 004: Command Bus -- Tickets

## Ticket 01: Bus types and CommandBus

**Depends on**: None

**Scope**: Create `src/bus/mod.rs` with `Command`, `Response`, `ChannelKind`, and `CommandBus`.

**Requirements**:
- `ChannelKind` enum: `Terminal`, `Discord`, `Telegram` (serde-serializable)
- `Response` struct: `content: String` (serde-serializable)
- `Command` struct: `source: ChannelKind`, `content: String`, `response_tx: oneshot::Sender<Response>`
- `CommandBus` wrapping `tokio::sync::mpsc` with:
  - `new(capacity: usize) -> (CommandBus, CommandBusReceiver)` — split sender/receiver
  - `CommandBus::send(cmd: Command)` — async send
  - `CommandBusReceiver::recv() -> Option<Command>` — async recv
  - `CommandBus` must be `Clone` (mpsc::Sender is Clone)
- Register `pub mod bus` in `src/lib.rs`
- Add `tokio` channel dependencies (already in Cargo.toml)

**Tests**:
- Send a command through the bus and receive it
- Verify response routing via oneshot
- Verify `ChannelKind` serialization roundtrips
- Bus recv returns None when all senders are dropped

**Files**: `src/bus/mod.rs`, `src/lib.rs`

---

## Ticket 02: Channel trait and TerminalChannel

**Depends on**: Ticket 01

**Scope**: Create `src/channels/mod.rs` with the `Channel` trait and `src/channels/terminal.rs` with `TerminalChannel`.

**Requirements**:
- `Channel` trait with `async fn run(&self, bus: CommandBus) -> Result<(), anyhow::Error>`
  - Each channel runs as a loop feeding commands into the bus
  - Receives responses via the oneshot in each Command
- `TerminalChannel` struct with fields for display configuration (use_color, etc.)
- `TerminalChannel::run()` reads stdin lines, wraps them as `Command`s with `ChannelKind::Terminal`, sends to bus, awaits response via oneshot, prints response
- Slash commands (`/quit`, `/clear`, `/save`, `/sessions`, `/model`) are NOT handled in the channel — they pass through as regular commands. The consumer decides what's a command vs a prompt.
- The channel should handle the prompt display (`> `) and basic input loop
- Register `pub mod channels` in `src/lib.rs`

**Tests**:
- TerminalChannel can be constructed
- Channel trait is object-safe (can be used as `Box<dyn Channel>`)

**Files**: `src/channels/mod.rs`, `src/channels/terminal.rs`, `src/lib.rs`

---

## Ticket 03: Wire bus into main.rs

**Depends on**: Ticket 01, Ticket 02

**Scope**: Refactor `main.rs` to create a `CommandBus`, spawn `TerminalChannel` on a tokio task, and consume commands from the bus receiver instead of reading stdin directly.

**Requirements**:
- Create `CommandBus` + `CommandBusReceiver` in main
- Spawn `TerminalChannel::run(bus.clone())` as a tokio task
- Replace the stdin REPL loop with a `bus_rx.recv()` loop
- Slash command handling stays in main (the consumer side), operating on the `Command.content`
- Send `Response` back via `command.response_tx` after processing (agent response text, command acknowledgment, etc.)
- Single-shot mode (`--prompt`) bypasses the bus (sends directly to agent as before)
- Session auto-save on exit still works
- Ctrl+C handling still works

**Tests**:
- Existing CLI arg parsing tests still pass
- Existing format/color tests still pass
- Manual verification: REPL works identically to before

**Files**: `src/main.rs`
