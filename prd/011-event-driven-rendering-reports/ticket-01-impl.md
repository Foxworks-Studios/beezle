# Implementation Report: Ticket 1 -- Failing tests for `run_prompt()` (TDD red step)

**Ticket:** 1 - Failing tests for `run_prompt()` (TDD red step)
**Date:** 2026-03-06 12:00
**Status:** COMPLETE

---

## Files Changed

### Created
- None

### Modified
- `src/main.rs` - Added three new `#[tokio::test]` functions in the `#[cfg(test)] mod tests` block that call the not-yet-implemented `run_prompt()` function

## Implementation Notes
- The three new tests follow the exact same patterns as the existing `run_single_prompt` tests (same `mock_agent()` helper, same assertion style)
- Test 3 (`run_prompt_processes_tool_execution_events_without_panic`) uses `MockProvider::new(vec![MockResponse::ToolCalls(...), MockResponse::Text(...)])` to trigger real tool execution events through the agent loop, which will produce `ToolExecutionStart` and `ToolExecutionEnd` events naturally
- All new tests reference `run_prompt` by name, causing the expected compile error `cannot find function 'run_prompt' in this scope`
- No existing tests were modified

## Acceptance Criteria
- [x] AC 1: `run_prompt_returns_usage_from_agent_end` test added - creates `mock_agent("hello")`, calls `run_prompt(&mut agent, "hi", false).await`, asserts `Usage` has `input == 0` and `output == 0`, and `agent.messages().len() == 2`
- [x] AC 2: `run_prompt_accumulates_usage_across_turns` test added - uses `MockProvider::texts(vec!["First", "Second"])`, calls `run_prompt` twice, asserts `agent.messages().len() == 4` after second call
- [x] AC 3: `run_prompt_processes_tool_execution_events_without_panic` test added - uses `MockProvider::new(vec![MockResponse::ToolCalls(...), MockResponse::Text(...)])` with `default_tools()` to generate `ToolExecutionStart`/`ToolExecutionEnd` events through the agent loop; asserts function returns without panicking
- [x] AC 4: All new test functions reference `run_prompt` by name, causing compile error as expected for TDD red step
- [x] AC 5: All pre-existing tests still compile and pass - verified with `cargo test --lib` (75 pass) and confirmed only `run_prompt`-related errors from the binary crate
- [x] AC 6: `cargo build 2>&1 | grep -c "cannot find function 'run_prompt'"` returns 4 (>0) - all compile errors are exclusively about the missing `run_prompt` function

## Test Results
- Lint: N/A (binary crate won't compile due to intentional TDD red step; `cargo fmt --check` PASS)
- Tests: `cargo test --lib` PASS (75/75); binary tests fail to compile as expected (4 errors, all `cannot find function 'run_prompt'`)
- Build: Binary intentionally fails to compile (TDD red step); library builds fine
- New tests added: 3 tests in `src/main.rs` (`run_prompt_returns_usage_from_agent_end`, `run_prompt_accumulates_usage_across_turns`, `run_prompt_processes_tool_execution_events_without_panic`)

## Concerns / Blockers
- None. The compile failure is intentional and expected -- Ticket 2 will implement `run_prompt()` to make these tests pass (TDD green step).
