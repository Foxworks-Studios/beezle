# Code Review: Ticket 6 -- Wire multi-agent system into `build_agent()` and remove `spawn_agent`

**Ticket:** 6 -- Wire multi-agent system into `build_agent()` and remove `spawn_agent`
**Impl Report:** prd/010-multi-agent-system-reports/ticket-06-impl.md
**Date:** 2026-03-07 15:00
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `cargo build` succeeds with zero warnings and zero errors | Met | Verified: `cargo build` completes cleanly |
| 2 | `spawn_agent` no longer appears in `build_raw_tools()` or `build_agent()` | Met | Grep for `spawn_agent` in `src/main.rs` returns zero matches |
| 3 | `tracing::debug!` log lists all registered sub-agent names at startup | Met | Line 351: `tracing::debug!(sub_agents = ?sub_agent_names, "registered sub-agents")` |
| 4 | Coordinator's `with_tools()` no longer includes `default_tools()` -- only memory tools | Met | `build_raw_tools()` (line 228) returns only memory tools. `default_tools()` only appears in two standalone test functions that test PermissionGuard behavior independently |
| 5 | Coordinator built with `agent.with_sub_agent()` for each definition | Met | Lines 365-368: loop iterates `sub_agent_defs` and calls `agent.with_sub_agent(sub)` |
| 6 | `coordinator_agent_prompt()` section appended to system prompt | Met | Lines 354-358: `coordinator_agent_prompt()` output is formatted into `full_system_prompt` and passed to `with_system_prompt()` |
| 7 | All tests pass; `cargo clippy -- -D warnings` passes | Met | 264 tests pass (206 lib + 58 bin). Clippy clean. |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)
- **Hardcoded `sub_agent_count = 3` in display** (line 1050 of `src/main.rs`): The tool count display hardcodes `3` for builtin sub-agents, ignoring any user-defined agents loaded from `~/.beezle/agents/`. The implementer noted this in their report. Since this is display-only (not functional), it is minor, but a dynamic count from `sub_agent_defs.len()` would be more accurate. The tool count tests (lines 1722-1735) similarly hardcode `3` and would need updating if this is fixed.
- **Tool count tests are arithmetic tautologies** (lines 1722-1735): `tool_count_calculation_without_memory` and `tool_count_calculation_with_memory` test that `0 + 3 == 3` and `2 + 3 == 5`. They do not exercise any actual code path -- they just verify arithmetic. They could be made more meaningful by calling `build_raw_tools()` and checking `.len()`, or removed in favor of the existing `build_raw_tools_*` tests which already do this properly.

## Suggestions (non-blocking)
- The `build_agent_logs_tool_count_and_names` test (line 1686) is a smoke test that only verifies no panic. Consider adding an assertion on the returned agent state if the API supports it, or add a comment making the "no-panic" intent explicit in the test name.

## Scope Check
- Files within scope: YES -- only `src/main.rs` and `src/agent/mod.rs` were modified, matching the ticket scope
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- the `spawn_agent` removal is clean; all existing tests pass; the new sub-agent wiring delegates to functions already tested in tickets 2-5
- Security concerns: NONE
- Performance concerns: NONE
