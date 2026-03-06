# Implementation Report: Ticket 02 -- Wire sub-agent into build_agent and main.rs

**Ticket:** 02 - Wire sub-agent into build_agent and main.rs
**Date:** 2026-03-05 15:30
**Status:** COMPLETE

---

## Files Changed

### Modified
- `src/main.rs` - Added `build_subagent` import, modified `build_agent()` to create and include `spawn_agent` tool, modified `wrap_tools()` to skip wrapping tools named `"spawn_agent"`, added 3 new tests

## Implementation Notes
- `wrap_tools()` now checks `inner.name() == "spawn_agent"` and passes the tool through unwrapped. This prevents duplicate output since `SubAgentTool` manages its own progress via `on_update` events.
- In `build_agent()`, the sub-agent is created via `build_subagent()` and appended to the tools list AFTER `wrap_tools()` processes default tools. This is clearer than relying solely on the name-based skip in `wrap_tools()`, though the skip acts as a safety net.
- The sub-agent system prompt is generic: "You are a helpful sub-agent. Complete the task you are given thoroughly and return the result."
- The sub-agent description explains its purpose to the LLM: "Spawn a sub-agent to handle a focused task independently. The sub-agent runs with a fresh context and returns only its final result."
- The sub-agent inherits the parent's model and API key from `build_agent()` parameters.

## Acceptance Criteria
- [x] AC 1: `build_agent()` calls `build_subagent()` to create a `SubAgentTool` named `"spawn_agent"` - implemented with suitable description and system prompt
- [x] AC 2: Sub-agent tool converted to `Box<dyn AgentTool>` and included in tools vec alongside `default_tools()` - appended after wrapping default tools
- [x] AC 3: Sub-agent tool NOT wrapped in `ToolWrapper` - added after `wrap_tools()` call, and `wrap_tools()` has name-based skip as safety net
- [x] AC 4: `wrap_tools()` skips wrapping tools whose name is `"spawn_agent"` - conditional check added
- [x] AC 5: Existing CLI tests, slash command tests, and format tests still pass - all 108 tests pass
- [x] AC 6: Integration test verifying spawn_agent in tool list - `build_agent_tools_include_spawn_agent` test
- [x] AC 7: Test verifying `wrap_tools` skip behavior - `wrap_tools_skips_spawn_agent` and `wrap_tools_still_wraps_regular_tools` tests

## Test Results
- Lint: PASS (cargo clippy -- -D warnings)
- Tests: PASS (108 total: 59 lib + 49 main)
- Build: PASS (zero warnings)
- Format: PASS (cargo fmt --check)
- New tests added:
  - `src/main.rs::tests::wrap_tools_skips_spawn_agent`
  - `src/main.rs::tests::wrap_tools_still_wraps_regular_tools`
  - `src/main.rs::tests::build_agent_tools_include_spawn_agent`

## Concerns / Blockers
- The `ToolWrapper` delegates all trait methods (`name()`, `label()`, `description()`, `parameters_schema()`) to the inner tool, making it impossible to distinguish wrapped vs unwrapped tools purely through the `AgentTool` trait interface. The tests verify the code path exists and that spawn_agent is present in the tool list, but cannot directly observe the absence of wrapping at the type level. Runtime behavior (no duplicate stdout output) is the true verification.
