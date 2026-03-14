# Tickets for PRD 013: Migrate to prompt_with_sender for Caller-Controlled Streaming

**Source PRD:** prd/013-prompt-with-sender-migration.md
**Created:** 2026-03-14
**Total Tickets:** 4
**Estimated Total Complexity:** 6 (S=1, M=2, L=3: S+S+L+S = 1+1+3+1 = 6)

---

### Ticket 1: TDD Red Step — Write Failing Test for prompt_with_sender Pattern

**Description:**
Add a new failing unit test `prompt_with_sender_channel_owned_by_caller` to the `#[cfg(test)]`
module in `src/main.rs`. The test constructs the `(tx, rx)` channel at the call site, calls
`agent.prompt_with_sender()` directly (not via `run_prompt()`), reads all events from `rx`,
and asserts `agent.messages().len() == 2`. This test codifies the caller-owned channel contract
and must pass once the production migration is done. It should compile and pass already
(since `prompt_with_sender` exists on `Agent`) — its purpose is to nail down the expected
pattern before touching the production call sites.

**Scope:**
- Modify: `src/main.rs` (add one `#[tokio::test]` in the existing `#[cfg(test)]` module)

**Acceptance Criteria:**
- [ ] New test `prompt_with_sender_channel_owned_by_caller` exists in the `#[cfg(test)]` block
      of `src/main.rs`
- [ ] Test creates `(tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>()` at the call site
      (not assigned from a return value of `agent.prompt()`)
- [ ] Test calls `agent.prompt_with_sender("hello", tx).await` directly
- [ ] Test drains `rx` with `while let Some(_) = rx.recv().await {}` after the call
- [ ] Test asserts `agent.messages().len() == 2` (user + assistant messages)
- [ ] Test: construct `mock_agent("response")`, call `prompt_with_sender("hi", tx).await`,
      drain `rx` to exhaustion, assert `messages().len() == 2` -- test compiles and passes
- [ ] Test: verify `rx.recv().await` returns `None` after the drain, confirming channel is
      closed (sender dropped by `prompt_with_sender` on return)
- [ ] All pre-existing tests still pass (`cargo test`)
- [ ] `cargo build` produces zero warnings

**Dependencies:** None
**Complexity:** S
**Maps to PRD AC:** AC 9

---

### Ticket 2: Migrate `fetch_thinking_label()` to `prompt_with_sender`

**Description:**
Replace the `agent.prompt(user_prompt).await` call in `fetch_thinking_label()` (line ~488)
with a caller-owned channel pattern using `agent.prompt_with_sender(user_prompt, tx).await`.
Because `prompt_with_sender` is an `async fn` that returns only after the agent loop completes
(and drops `tx`), the `while let Some(event) = rx.recv().await` drain that follows is
guaranteed to see all events and then return `None` without blocking — no other structural
changes are required to this function.

**Scope:**
- Modify: `src/main.rs` (change ~4 lines in `fetch_thinking_label()`)

**Acceptance Criteria:**
- [ ] `fetch_thinking_label()` contains `let (tx, mut rx) = mpsc::unbounded_channel();`
      before calling the agent
- [ ] `fetch_thinking_label()` calls `agent.prompt_with_sender(user_prompt, tx).await` (no
      return value assigned)
- [ ] The `while let Some(event) = rx.recv().await` drain is unchanged in structure
- [ ] `fetch_thinking_label()` does NOT call `agent.prompt(` anywhere in its body
- [ ] Test: `grep -n "agent\.prompt(" src/main.rs` does not match inside `fetch_thinking_label`
      (use a grep AC to confirm the old pattern is gone from this function)
- [ ] `cargo test` passes with all pre-existing tests green (no regressions from this change)
- [ ] `cargo build` produces zero errors and zero warnings
- [ ] `cargo clippy -- -D warnings` produces no new violations

**Dependencies:** Ticket 1
**Complexity:** S
**Maps to PRD AC:** AC 4, AC 5

---

### Ticket 3: Migrate `run_prompt()` to Concurrent Consumer Pattern with `prompt_with_sender`

**Description:**
Restructure `run_prompt()` (lines ~523-666) to use the concurrent consumer pattern:
(1) create the caller-owned `(tx, mut rx)` channel, (2) spawn the event-consumer/renderer
as a `tokio::spawn` background task that reads from `rx`, (3) use `tokio::select!` on
`agent.prompt_with_sender(prompt, tx)` and `tokio::signal::ctrl_c()` on the calling task
so that Ctrl+C can call `agent.abort()` without crossing thread boundaries, and (4) await
the consumer `JoinHandle` to retrieve `last_usage` and `streaming_text` for post-loop cleanup.
The consumer task moves the full existing event-match loop into its body and returns
`(last_usage, streaming_text)` as a tuple so the calling task can print the trailing newline
and return `Usage`.

**Scope:**
- Modify: `src/main.rs` (restructure `run_prompt()`, ~130 lines changed/moved within the file)

**Acceptance Criteria:**
- [ ] `run_prompt()` contains `let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();`
      before the consumer spawn
- [ ] The consumer is spawned with `let consumer = tokio::spawn(async move { ... });` before
      calling `prompt_with_sender`
- [ ] The agent call site uses `tokio::select!` with two arms: `agent.prompt_with_sender(prompt,
      tx)` and `tokio::signal::ctrl_c()`, with the Ctrl+C arm calling `agent.abort()`
- [ ] `consumer.await.expect(...)` is called after the `tokio::select!` block to join the
      rendering task and extract `(last_usage, streaming_text)`
- [ ] `run_prompt()` does NOT call `agent.prompt(` anywhere
- [ ] The `label_fut` spawn for `fetch_thinking_label` is still spawned BEFORE the consumer
      task and `prompt_with_sender` call, preserving concurrent label fetching behavior
- [ ] The consumer task returns a tuple `(Usage, bool)` (last_usage, streaming_text) via its
      `JoinHandle`, and the calling task uses these values for post-loop cleanup
- [ ] Test: `run_prompt_returns_usage_from_agent_end` still passes — `run_prompt()` returns
      `Usage { input: 0, output: 0 }` from a `mock_agent("hello")` call with two messages
- [ ] Test: `run_prompt_accumulates_usage_across_turns` still passes — two sequential calls
      with `MockProvider::texts(vec!["First", "Second"])` each produce 2 new messages
- [ ] Test: `run_prompt_processes_tool_execution_events_without_panic` still passes — a
      `MockProvider` that emits `ToolCalls` then `Text` completes without panic
- [ ] `cargo test` passes with zero regressions
- [ ] `cargo build` produces zero errors and zero warnings
- [ ] `cargo clippy -- -D warnings` produces no new violations

**Dependencies:** Ticket 2
**Complexity:** L
**Maps to PRD AC:** AC 3, AC 4, AC 6, AC 7, AC 8

---

### Ticket 4: Verification and Integration Check

**Description:**
Run the full PRD 013 acceptance criteria checklist end-to-end. Verify the migration is
complete (no remaining `agent.prompt(` calls outside test blocks), all quality gates pass,
and the new test from Ticket 1 is present and passing. Confirm `cargo fmt --check` is clean.

**Acceptance Criteria:**
- [ ] `grep -n "agent\.prompt(" src/main.rs` matches ONLY lines inside the `#[cfg(test)]`
      block (zero production call sites remain)
- [ ] `cargo build` completes with zero errors and zero warnings
- [ ] `cargo clippy -- -D warnings` passes with no violations
- [ ] `cargo test` passes with all tests green, including:
      - `run_prompt_returns_usage_from_agent_end`
      - `run_prompt_accumulates_usage_across_turns`
      - `run_prompt_processes_tool_execution_events_without_panic`
      - `prompt_with_sender_channel_owned_by_caller` (new from Ticket 1)
- [ ] `cargo fmt --check` passes with no formatting violations
- [ ] All PRD 013 acceptance criteria (AC 1-10) pass

**Dependencies:** Tickets 1, 2, 3
**Complexity:** S
**Maps to PRD AC:** AC 1, AC 2, AC 3, AC 10

---

## AC Coverage Matrix

| PRD AC # | Description                                                                                          | Covered By Ticket(s) | Status  |
|----------|------------------------------------------------------------------------------------------------------|----------------------|---------|
| 1        | `cargo build` completes with zero errors and zero warnings                                           | Ticket 4             | Covered |
| 2        | `cargo clippy -- -D warnings` passes with no new violations                                         | Ticket 4             | Covered |
| 3        | `cargo test` passes; all pre-migration tests continue to pass                                       | Ticket 3, Ticket 4   | Covered |
| 4        | `src/main.rs` has no `agent.prompt(` calls outside `#[cfg(test)]` blocks                           | Ticket 2, Ticket 3   | Covered |
| 5        | `fetch_thinking_label()` creates its own `mpsc::unbounded_channel()` and owns `tx`                  | Ticket 2             | Covered |
| 6        | `run_prompt()` creates its own `mpsc::unbounded_channel()` and spawns consumer before agent call    | Ticket 3             | Covered |
| 7        | `run_prompt()` uses `tokio::select!` on `prompt_with_sender` + `ctrl_c()` for abort                | Ticket 3             | Covered |
| 8        | `run_prompt()` awaits consumer `JoinHandle` and returns `Usage` derived from consumer events         | Ticket 3             | Covered |
| 9        | New test `prompt_with_sender_channel_owned_by_caller` exists and passes                             | Ticket 1             | Covered |
| 10       | `cargo fmt --check` passes                                                                           | Ticket 4             | Covered |
