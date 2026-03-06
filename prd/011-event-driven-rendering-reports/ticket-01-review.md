# Code Review: Ticket 1 -- Failing tests for `run_prompt()` (TDD red step)

**Ticket:** 1 -- Failing tests for `run_prompt()` (TDD red step)
**Impl Report:** prd/011-event-driven-rendering-reports/ticket-01-impl.md
**Date:** 2026-03-06 13:15
**Verdict:** CHANGES REQUESTED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `run_prompt_returns_usage_from_agent_end` test added | Met | Lines 1946-1958. Correct setup with `mock_agent("hello")`, calls `run_prompt(&mut agent, "hi", false).await`, asserts `usage.input == 0`, `usage.output == 0`, and `agent.messages().len() == 2`. |
| 2 | `run_prompt_accumulates_usage_across_turns` test added | Met | Lines 1960-1973. Uses `MockProvider::texts(vec!["First", "Second"])`, calls `run_prompt` twice, asserts `agent.messages().len() == 2` then `== 4`. |
| 3 | `run_prompt_processes_tool_execution_events_without_panic` test added | Partial | Lines 1975-1998. Test exists and will exercise tool execution events, but the approach deviates from the AC. AC specifies "manually send `AgentEvent::ToolExecutionStart` then `AgentEvent::ToolExecutionEnd` through a channel; wire them to a mock provider sequence." Implementation instead uses `MockProvider::new(vec![MockResponse::ToolCalls(...), MockResponse::Text(...)])` with `default_tools()` to trigger events naturally through the agent loop. Functionally this is arguably better (tests the real event flow), but it does not match the specified setup. |
| 4 | New tests reference `run_prompt` by name (compile error expected) | Met | `cargo test` produces exactly 4 `cannot find function 'run_prompt'` errors, all from the three new test functions. |
| 5 | All pre-existing tests compile and pass, no existing test modified | Met | `cargo test --lib` passes 75/75. No existing test function was modified (the 3 new tests are appended before the last existing test). |

## Issues Found

### Critical (must fix before merge)

- **Scope violation: non-test changes in `src/main.rs`**. The ticket scope says "add test cases only -- no implementation changes." The diff includes three non-test modifications:
  1. **Line 621**: `agent.finish().await;` added inside `fetch_thinking_label` (or the function containing it -- `thinking_label()`'s helper).
  2. **Lines 675-676**: Comment rewritten in `run_single_prompt` from "agent.prompt() awaits the full agent loop -- all events are buffered" to "agent.prompt() spawns the loop on a background task and returns immediately -- events stream in real time through the receiver."
  3. **Lines 687-693**: `run_single_prompt` restructured -- `render_events` return value captured, `agent.finish().await` added, return changed from direct `render_events(rx, use_color).await` to separate variable + finish + return.

  These are implementation changes that belong to Ticket 2 or later. They must be reverted in this ticket's scope. The impl report does not mention these changes at all.

### Major (should fix, risk of downstream problems)

None.

### Minor (nice to fix, not blocking)

- **AC 3 approach deviation**: The ticket AC specifies manually constructing `AgentEvent::ToolExecutionStart`/`ToolExecutionEnd` and sending them through a channel. The implementation instead uses `MockProvider` with tool calls to trigger events naturally through the agent loop. The implemented approach is arguably superior (tests real behavior rather than synthetic events), and the test does exercise tool execution events. Since `run_prompt` doesn't exist yet, this deviation has no functional impact at the red step -- the test will compile-fail either way. The Ticket 2 implementer should be aware of this design choice when making the test pass.

## Suggestions (non-blocking)

- None.

## Scope Check
- Files within scope: NO -- `src/main.rs` test block additions are in scope, but the non-test implementation changes (lines 621, 675-676, 687-693) are out of scope.
- Scope creep detected: YES -- `agent.finish().await` additions and `run_single_prompt` comment/logic changes are Ticket 2 work.
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- The non-test changes (`agent.finish().await`) may actually be necessary for correctness with a yoagent API change, but they belong in a different ticket. The test additions themselves carry zero regression risk.
- Security concerns: NONE
- Performance concerns: NONE
