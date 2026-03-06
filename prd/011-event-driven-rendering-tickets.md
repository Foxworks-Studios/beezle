# Tickets for PRD 011: Event-Driven Rendering

**Source PRD:** prd/011-event-driven-rendering.md
**Created:** 2026-03-06
**Total Tickets:** 4
**Estimated Total Complexity:** 9 (S=1, M=2, L=3: S + M + L + S = 1+2+3+1)

---

### Ticket 1: Failing tests for `run_prompt()` (TDD red step)

**Description:**
Write the full test suite for the new `run_prompt()` function *before* the function
exists. Tests must be written to the exact signature the PRD specifies:
`async fn run_prompt(agent: &mut Agent, prompt: &str, use_color: bool) -> Usage`.
All new tests are expected to fail to compile or fail at runtime — that is the red step.
Also add a `#[test] fn run_prompt_exists()` compile-time sentinel that will gate all
subsequent work. Existing tests must continue to compile and pass throughout.

**Scope:**
- Modify: `src/main.rs` (add test cases only — no implementation changes)

**Acceptance Criteria:**
- [ ] A new `#[tokio::test] async fn run_prompt_returns_usage_from_agent_end()` test is added in the `#[cfg(test)]` block. Setup: create `mock_agent("hello")`. Action: call `run_prompt(&mut agent, "hi", false).await`. Assertion: the returned `Usage` has `input == 0` and `output == 0` (MockProvider returns zero usage), and `agent.messages().len() == 2` after the call.
- [ ] A new `#[tokio::test] async fn run_prompt_accumulates_usage_across_turns()` test is added. Setup: `MockProvider::texts(vec!["First", "Second"])`. Action: call `run_prompt` twice sequentially. Assertion: `agent.messages().len() == 4` after the second call.
- [ ] A new `#[tokio::test] async fn run_prompt_processes_tool_execution_events_without_panic()` test is added. Setup: manually send `AgentEvent::ToolExecutionStart` then `AgentEvent::ToolExecutionEnd { is_error: false, .. }` through a channel; wire them to a mock provider sequence. Action: call `run_prompt`. Assertion: function returns without panicking.
- [ ] The new test functions reference `run_prompt` by name (causing a compile error at this stage, which is expected and intentional as the TDD red step).
- [ ] All pre-existing tests (`render_events_*`, `run_single_prompt_*`, etc.) still compile and pass — no existing test is modified in this ticket.
- [ ] `cargo test` output shows the new tests failing (compile error or runtime failure) while all prior tests pass: `cargo build 2>&1 | grep -c "cannot find function \`run_prompt\`"` returns `>0`.

**Dependencies:** None
**Complexity:** S
**Maps to PRD AC:** AC 2, AC 5, AC 7

---

### Ticket 2: Implement `run_prompt()` — event-driven loop with Ctrl+C

**Description:**
Add the new `run_prompt()` function to `src/main.rs` exactly as specified in the PRD's
technical approach section. The function handles all `AgentEvent` variants via
`tokio::select!`, tracks tool durations with `HashMap<String, Instant>`, calls
`agent.abort()` on Ctrl+C, and calls `agent.finish().await` after the loop.
Replace the two call sites of `run_single_prompt` in `main()` with `run_prompt`.
`run_single_prompt` and `render_events` are NOT deleted in this ticket — they remain
until Ticket 3 removes the wrappers. The old and new functions coexist temporarily.

**Scope:**
- Modify: `src/main.rs` (add `run_prompt()`, update two call sites in `main()`)

**Acceptance Criteria:**
- [ ] `run_prompt` is added with signature `async fn run_prompt(agent: &mut Agent, prompt: &str, use_color: bool) -> Usage`.
- [ ] The function body opens with `let mut rx = agent.prompt(prompt).await;` and a `HashMap<String, Instant>` for tool timing.
- [ ] The `tokio::select!` loop handles all event arms: `ToolExecutionStart` (print yellow summary + flush, insert timer), `ToolExecutionEnd` (remove timer, compute elapsed, print ok/error in green/red with duration), `MessageUpdate { delta: StreamDelta::Text { .. } }` (print delta + flush), `MessageUpdate { delta: StreamDelta::Thinking { .. } }` (print dim delta + flush), `MessageEnd` with `StopReason::Error` (log + print red error), `AgentEnd` (extract usage from last assistant message), `ProgressMessage` (print dim text), `InputRejected` (print red rejection), `_ => {}` catch-all.
- [ ] The Ctrl+C arm calls `agent.abort()` then `break`.
- [ ] `agent.finish().await` is called unconditionally after the loop.
- [ ] Both `main()` call sites (`--prompt` mode and the REPL loop) are updated to call `run_prompt(&mut agent, ..., use_color)` instead of `run_single_prompt`.
- [ ] The `thinking_key: Option<&str>` parameter is no longer passed at the two call sites (it is removed from the call, not yet from `run_single_prompt` definition — that happens in Ticket 3).
- [ ] Test: `run_prompt_returns_usage_from_agent_end` now passes (green). Test: `run_prompt_accumulates_usage_across_turns` now passes (green).
- [ ] Test: `run_prompt_processes_tool_execution_events_without_panic` passes (green).
- [ ] `cargo test` passes with all tests green; `cargo build` has no warnings.

**Dependencies:** Ticket 1
**Complexity:** M
**Maps to PRD AC:** AC 2, AC 3, AC 4, AC 5, AC 6, AC 7, AC 8, AC 9

---

### Ticket 3: Remove wrappers, thinking-label system, and simplify `build_agent()`

**Description:**
Delete all obsolete code: `StreamProviderWrapper`, `ToolWrapper`, `SubAgentWrapper`,
`wrap_tools()`, `run_single_prompt()`, `render_events()`, `clear_thinking_line()`,
`fetch_thinking_label()`, `thinking_label()`, and `THINKING_LABELS`. Remove the
`use_color` parameter from `build_agent()` and pass tools unwrapped. Remove the old
Ctrl+C `tokio::spawn` + `std::process::exit(0)` handler from `main()` (Ctrl+C is
now handled inside `run_prompt()`). Delete all wrapper-specific tests. Update
`build_agent_tools_include_spawn_agent` and `subagent_wrapper_delegates_name` tests
to use unwrapped construction.

**Scope:**
- Modify: `src/main.rs` (deletions throughout; `build_agent` signature change; `main()` cleanup)

**Acceptance Criteria:**
- [ ] `StreamProviderWrapper` struct and its `StreamProvider` impl are deleted.
- [ ] `ToolWrapper` struct and its `AgentTool` impl are deleted.
- [ ] `SubAgentWrapper` struct and its `AgentTool` impl are deleted.
- [ ] `wrap_tools()` function is deleted.
- [ ] `run_single_prompt()` function is deleted.
- [ ] `render_events()` function is deleted.
- [ ] `clear_thinking_line()`, `fetch_thinking_label()`, `thinking_label()` functions are deleted.
- [ ] `THINKING_LABELS` constant (inside `thinking_label`) is deleted along with the function.
- [ ] `build_agent()` no longer accepts a `use_color: bool` parameter. Its body passes `default_tools()` directly (no `wrap_tools()` call), passes the provider directly to `Agent::new` (no `StreamProviderWrapper`), and pushes the subagent and memory tools unwrapped (no `ToolWrapper` or `SubAgentWrapper` wrapping).
- [ ] All call sites of `build_agent()` (in `main()` and in `handle_slash_command` via `/model`) drop the `use_color` argument.
- [ ] The standalone `tokio::spawn` Ctrl+C handler (`std::process::exit(0)`) is removed from `main()`.
- [ ] The `thinking_key` local variable and its derivation in `main()` are removed.
- [ ] Test: `wrap_tools_skips_spawn_agent`, `wrap_tools_still_wraps_regular_tools`, `subagent_wrapper_on_update_receives_events`, `subagent_wrapper_progress_events_contain_text_deltas` are deleted from the test module.
- [ ] Test: `subagent_wrapper_delegates_name` is deleted (type no longer exists).
- [ ] Test: `build_agent_tools_include_spawn_agent` is updated to verify that `default_tools()` plus the unwrapped subagent tool yields a list containing `"spawn_agent"` — no `SubAgentWrapper` or `wrap_tools` call.
- [ ] Test: `bus_regular_prompt_processes_through_mock_agent` is updated: it calls `run_prompt` instead of `run_single_prompt`.
- [ ] Grep assertion: `grep -r "StreamProviderWrapper\|ToolWrapper\|SubAgentWrapper\|wrap_tools\|run_single_prompt\|render_events\|clear_thinking_line\|fetch_thinking_label\|thinking_label\|THINKING_LABELS" src/main.rs` returns no matches.
- [ ] `cargo test` passes; `cargo build` has no warnings; `cargo clippy -- -D warnings` passes.

**Dependencies:** Ticket 2
**Complexity:** L
**Maps to PRD AC:** AC 1, AC 10, AC 11

---

### Ticket 4: Verification

**Description:**
Run the full PRD acceptance criteria checklist end-to-end. Verify all deletions are
complete, all new behavior is present, and all quality gates pass. This is a read-only
verification ticket — no new code is written unless a gap is found, in which case a
minimal fix is applied.

**Acceptance Criteria:**
- [ ] `grep -r "StreamProviderWrapper\|ToolWrapper\|SubAgentWrapper\|wrap_tools\|run_single_prompt\|render_events\|clear_thinking_line\|fetch_thinking_label\|thinking_label\|THINKING_LABELS" src/main.rs` returns empty (AC 1 confirmed).
- [ ] `grep -n "fn run_prompt" src/main.rs` returns exactly one match (AC 2 confirmed).
- [ ] `grep -n "ToolExecutionStart" src/main.rs` matches inside `run_prompt` body (AC 3 confirmed).
- [ ] `grep -n "ToolExecutionEnd" src/main.rs` matches inside `run_prompt` body with duration formatting (AC 4 confirmed).
- [ ] `grep -n "StreamDelta::Text" src/main.rs` matches inside `run_prompt` body with `print!` (AC 5 confirmed).
- [ ] `grep -n "StreamDelta::Thinking" src/main.rs` matches inside `run_prompt` body with dim color (AC 6 confirmed).
- [ ] `grep -n "AgentEnd" src/main.rs` matches inside `run_prompt` body with usage extraction (AC 7 confirmed).
- [ ] `grep -n "agent\.abort" src/main.rs` returns a match inside the `tokio::select!` Ctrl+C arm (AC 8 confirmed).
- [ ] `grep -n "agent\.finish" src/main.rs` returns a match after the event loop (AC 9 confirmed).
- [ ] `grep -n "fn build_agent" src/main.rs` shows the signature does NOT include `use_color` (AC 10 confirmed).
- [ ] `cargo test -- --nocapture 2>&1 | tail -5` shows `test result: ok`.
- [ ] `cargo build 2>&1 | grep -c "^error"` returns 0.
- [ ] `cargo clippy -- -D warnings 2>&1 | grep -c "^error"` returns 0.
- [ ] `cargo fmt --check` exits with status 0.

**Dependencies:** Ticket 3
**Complexity:** S
**Maps to PRD AC:** AC 1, AC 2, AC 3, AC 4, AC 5, AC 6, AC 7, AC 8, AC 9, AC 10, AC 11, AC 12

---

## AC Coverage Matrix

| PRD AC # | Description | Covered By Ticket(s) | Status |
|----------|-------------|----------------------|--------|
| 1 | `StreamProviderWrapper`, `ToolWrapper`, `SubAgentWrapper`, `wrap_tools()`, `render_events()`, `run_single_prompt()`, `clear_thinking_line()`, `fetch_thinking_label()`, `thinking_label()`, `THINKING_LABELS` removed | Ticket 3, Ticket 4 | Covered |
| 2 | New `run_prompt()` handles all agent output via `AgentEvent` matching in `tokio::select!` loop | Ticket 1, Ticket 2, Ticket 4 | Covered |
| 3 | `ToolExecutionStart` prints yellow tool summary | Ticket 2, Ticket 4 | Covered |
| 4 | `ToolExecutionEnd` prints success/error with elapsed duration | Ticket 2, Ticket 4 | Covered |
| 5 | `MessageUpdate::Text` events print deltas to stdout as they arrive | Ticket 1, Ticket 2, Ticket 4 | Covered |
| 6 | `MessageUpdate::Thinking` events print in dim text | Ticket 2, Ticket 4 | Covered |
| 7 | `AgentEnd` accumulates usage from last assistant message | Ticket 1, Ticket 2, Ticket 4 | Covered |
| 8 | Ctrl+C calls `agent.abort()` and returns cleanly (no process exit) | Ticket 2, Ticket 4 | Covered |
| 9 | `agent.finish().await` called after event loop | Ticket 2, Ticket 4 | Covered |
| 10 | `build_agent()` has no `use_color` param; tools are unwrapped | Ticket 3, Ticket 4 | Covered |
| 11 | All existing tests pass or updated; no wrapper-specific tests remain | Ticket 3, Ticket 4 | Covered |
| 12 | `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo test` all pass | Ticket 2 (partial), Ticket 3 (partial), Ticket 4 (full) | Covered |
