# PRD 013: Migrate to prompt_with_sender for Caller-Controlled Streaming

**Status:** TICKETS READY
**Created:** 2026-03-14
**Author:** PRD Writer Agent

---

## Problem Statement

`run_prompt()` and `fetch_thinking_label()` in `src/main.rs` call `agent.prompt()`, which
internally creates an `mpsc::unbounded_channel`, spawns the agent loop, and returns the
`rx` half. This hides channel ownership from the caller and is the convenience wrapper — not
the intended extension point. `prompt_with_sender(text, tx)` is the primary API: the caller
creates the channel, passes the `tx` to the agent, and reads events from `rx` while the agent
loop runs. Adopting it makes the event pipeline explicit, aligns with the command bus
architecture (which already manages its own channels), and documents the channel ownership
contract clearly in code.

## Goals

- Replace every call to `agent.prompt()` in `src/main.rs` with `prompt_with_sender()` so the
  caller owns and creates the channel in all agent-prompting paths.
- Restructure `run_prompt()` so that the consumer task drives the event loop concurrently
  while `prompt_with_sender()` runs the agent loop on the calling task, consistent with
  yoagent 0.6.1's design (the method is `async fn`, not fire-and-forget).
- Restructure `fetch_thinking_label()` to use `prompt_with_sender()` the same way.
- Preserve all existing observable behavior: real-time token streaming, tool execution
  display, Ctrl+C abort, `Usage` return value, and test correctness.

## Non-Goals

- Does not change behavior for the command bus, permission prompts, or any other subsystem
  beyond the two call sites in `src/main.rs`.
- Does not extract `run_prompt()` or `fetch_thinking_label()` into separate modules; they
  remain in `src/main.rs`.
- Does not change the `Agent` API or any yoagent code.
- Does not add cancellation tokens, timeouts, or other new control features.
- Does not migrate `continue_loop()` calls (none currently exist in `main.rs`).
- Does not change the `MockProvider` usage in existing tests; tests may need to adapt their
  channel construction but must not change tested behavior.

## User Stories

- As a developer reading `src/main.rs`, I want `run_prompt()` to explicitly create its own
  `mpsc::unbounded_channel()` so I can see the event pipeline's ownership without tracing
  through a convenience wrapper.
- As a developer extending beezle with new input channels, I want the channel-creation
  pattern for agent prompting to be consistent with the command bus pattern so I can reuse
  it without learning a different idiom.
- As a user of beezle, I want streaming output, tool progress display, and Ctrl+C
  cancellation to continue working exactly as before after this refactor.

## Technical Approach

### Affected call sites

There are two call sites in `src/main.rs` where `agent.prompt()` is called directly:

| Location | Line (approx.) | Current call | After migration |
|---|---|---|---|
| `fetch_thinking_label()` | ~488 | `agent.prompt(user_prompt).await` | `agent.prompt_with_sender(user_prompt, tx).await` |
| `run_prompt()` | ~537 | `agent.prompt(prompt).await` | `agent.prompt_with_sender(prompt, tx).await` |

### `fetch_thinking_label()` migration

Currently the function calls `agent.prompt(user_prompt).await` which returns `rx`, then
drains `rx` to collect text deltas. After migration:

```rust
let (tx, mut rx) = mpsc::unbounded_channel();
agent.prompt_with_sender(user_prompt, tx).await;
// rx is already fully populated; drain synchronously
while let Some(event) = rx.recv().await { ... }
```

Because `prompt_with_sender` is an `async fn` that returns only after the agent loop
completes (and drops `tx`), the `while let` drain on `rx` is guaranteed to see all events
and then return `None` without blocking. No other changes to this function are required.

### `run_prompt()` migration — concurrent consumer pattern

`prompt_with_sender` runs the agent loop on the calling task and returns only when the loop
is complete. The event consumer (the `tokio::select!` loop that renders output) must run
concurrently on a separate task. The correct pattern, as demonstrated in
`yoagent-0.6.1/tests/agent_test.rs` `test_prompt_with_sender_real_time_streaming`, is:

```rust
let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

// Spawn the consumer task BEFORE calling prompt_with_sender so events
// are consumed in real time while the agent loop drives tx.
let consumer = tokio::spawn(async move {
    // ... event loop reading from rx ...
});

// Run the agent loop on the current task; tx is moved in and dropped on return.
agent.prompt_with_sender(prompt, tx).await;

// Join the consumer to get its result (e.g. last_usage, streaming_text cleanup).
let (last_usage, streaming_text) = consumer.await.expect("consumer panicked");
```

The current `run_prompt()` concurrency model is:
1. `agent.prompt()` internally spawns the loop (background).
2. The `tokio::select!` on `rx.recv()` and `ctrl_c()` runs on the calling task.

After migration the model is:
1. `tokio::spawn(consumer reading rx)` — output rendering runs on a background task.
2. `agent.prompt_with_sender(prompt, tx).await` — agent loop runs on the calling task.
3. `consumer.await` — join the rendering task after the loop finishes.

The `agent.abort()` call in the Ctrl+C branch must move into the consumer task so it can
still cancel the in-progress loop. Because `agent` is `&mut Agent` (not `Send`), abort
requires a `CancellationToken` or `Arc`-shared abort handle. The simplest approach is to
spawn a small watcher task that calls `abort_token.cancel()` on Ctrl+C and pass the same
token to a `before_turn` callback or an `Arc<AtomicBool>` checked by the consumer — but
the existing `agent.abort()` API operates on an internal `CancellationToken` inside the
`Agent`. A cleaner approach: move the Ctrl+C signal into the calling-task context using
`tokio::select!`:

```rust
let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
let consumer = tokio::spawn(async move { /* drain rx */ });

tokio::select! {
    _ = agent.prompt_with_sender(prompt, tx) => {}
    _ = tokio::signal::ctrl_c() => { agent.abort(); }
}

consumer.await.expect("consumer panicked");
```

This keeps `agent.abort()` on the calling task (which still holds `&mut Agent`) and
eliminates the need to share the abort handle. The `tx` sender is dropped when
`prompt_with_sender` returns, causing the consumer to drain remaining buffered events and
exit its `while let` loop naturally.

The existing behavior — printing output in real time, updating the status line, and
returning `last_usage` — is preserved. The consumer task returns `(last_usage, streaming_text)`
via its `JoinHandle` return value.

### Haiku label concurrency

The `label_fut` spawn in `run_prompt()` currently fires before `agent.prompt()` so the label
call and agent loop run concurrently. After migration, `label_fut` is still spawned before
the consumer task and `prompt_with_sender`, so the concurrency is unchanged.

### TDD requirement

Per CLAUDE.md, red/green TDD is mandatory. The implementation must:
1. First write tests that fail because `run_prompt()` and `fetch_thinking_label()` still use
   `agent.prompt()` (e.g., tests that assert channel creation happens at the call site, or
   tests that verify `prompt_with_sender` is called on a mock that only implements
   `prompt_with_sender`). In practice, since `MockProvider` supports both, the test evidence
   is behavioral: ensure the existing tests in `src/main.rs` that call `run_prompt()` with a
   `MockProvider` still pass without modification.
2. Add one new unit test that explicitly constructs the `(tx, rx)` pair and calls
   `agent.prompt_with_sender()` directly to confirm the pattern compiles and events flow.

### File change table

| File | Change |
|---|---|
| `src/main.rs` | Replace `agent.prompt(...)` with caller-owned channel + `agent.prompt_with_sender(...)` in `fetch_thinking_label()` and `run_prompt()`. Restructure `run_prompt()` to spawn the consumer task before calling `prompt_with_sender`, then `select!` on `prompt_with_sender` and Ctrl+C. |

No other files require changes.

## Acceptance Criteria

1. `cargo build` completes with zero errors and zero warnings after the migration.
2. `cargo clippy -- -D warnings` passes with no new violations.
3. `cargo test` passes with no regressions; all tests that exercised `run_prompt()` before
   migration continue to pass after it.
4. `src/main.rs` contains no calls to `agent.prompt(` (the convenience wrapper) outside of
   `#[cfg(test)]` blocks; every production call to the agent uses `prompt_with_sender`.
5. `fetch_thinking_label()` creates its own `mpsc::unbounded_channel()` before calling
   `prompt_with_sender`, and the `tx` is owned by the call site, not returned by the agent.
6. `run_prompt()` creates its own `mpsc::unbounded_channel()` before spawning the consumer
   task, and the consumer task is spawned before `prompt_with_sender` is called.
7. `run_prompt()` uses `tokio::select!` on `agent.prompt_with_sender(...)` and
   `tokio::signal::ctrl_c()` so that a Ctrl+C signal triggers `agent.abort()` on the
   calling task without requiring the abort handle to be sent across threads.
8. `run_prompt()` awaits the consumer `JoinHandle` after the `tokio::select!` block and
   returns `last_usage` derived from events received by the consumer task — the `Usage`
   return value is identical to what the pre-migration version returned.
9. A new unit test named `prompt_with_sender_channel_owned_by_caller` (or equivalent)
   exists in `src/main.rs`'s `#[cfg(test)]` module; it passes a `MockProvider` agent
   through the `prompt_with_sender` API directly, asserts events are received on the
   caller-created `rx`, and asserts `agent.messages().len() == 2` after the call.
10. `cargo fmt --check` passes with no formatting violations.

## Open Questions

- **Consumer task return type**: The consumer task must return both `last_usage` and a
  `streaming_text` boolean so the post-loop cleanup (`println!()` if streaming was active)
  is correct. The implementer should define a small private struct or tuple to carry this
  state out of the spawned task.
- **`tokio::select!` biased behavior**: If events arrive simultaneously with Ctrl+C,
  `select!` may non-deterministically pick the Ctrl+C branch and drop buffered events. This
  matches the pre-migration behavior (Ctrl+C already aborts mid-stream) and is acceptable.

## Dependencies

- **yoagent 0.6.1** — `prompt_with_sender` is available in the current dependency version
  (confirmed in `~/.cargo/registry/src/.../yoagent-0.6.1/src/agent.rs` line 429). No
  `Cargo.toml` change is required.
- **PRD 011 (Event-Driven Rendering)** — implemented. `run_prompt()` is the function this
  PRD targets; PRD 011's implementation is the baseline code being refactored here.
