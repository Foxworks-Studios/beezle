# Code Review: Ticket 03 -- Sub-agent progress display

**Ticket:** 03 -- Sub-agent progress display
**Impl Report:** prd/005-subagent-architecture-reports/ticket-03-impl.md
**Date:** 2026-03-05 18:30
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | When `spawn_agent` is invoked, ToolWrapper prints start line | Met | `SubAgentWrapper::execute()` prints `> spawn_agent: <task preview>` at line 353-357, matching ToolWrapper's pattern but with task preview |
| 2 | Sub-agent intermediate progress via `on_update`/`on_progress` callbacks in dim text | Met | Lines 362-388 (`on_update`) and 392-403 (`on_progress`) print dim-text progress to stdout |
| 3 | Progress output prefixed to distinguish from parent | Met | `[sub] >` prefix for tool calls (line 373), `[sub]` for progress messages (line 394) |
| 4 | Progress output is ephemeral (not stored in parent context) | Met | Callbacks only print to stdout; the parent's ToolResult (line 414) contains only the final sub-agent output |
| 5 | Specialized wrapper for sub-agent tools | Met | `SubAgentWrapper` struct (lines 313-424) is a dedicated wrapper distinct from `ToolWrapper` |
| 6 | Test: `on_update` callback receives events during execution | Met | `subagent_wrapper_on_update_receives_events` (line 1739) verifies updates are collected and non-empty |
| 7 | Test: Progress event types match expected patterns | Met | `subagent_wrapper_progress_events_contain_text_deltas` (line 1795) verifies text content in progress events |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)
- **Misleading test name**: `wrap_tools_skips_spawn_agent` (line 1640) no longer tests skip behavior -- `wrap_tools` now wraps ALL tools uniformly. The test passes `spawn_agent` through `wrap_tools` and verifies it's still present, but it's wrapped in `ToolWrapper` now. The test name should be updated to reflect the current behavior (e.g., `wrap_tools_wraps_all_tools_uniformly`). This is a Ticket 02 artifact that was modified by Ticket 03.
- **`SubAgentWrapper` missing doc comments on struct fields**: CLAUDE.md requires doc comments on public items. `SubAgentWrapper` is private, so this is not a strict violation, but `ToolWrapper` (which it mirrors) also lacks field docs, so this is consistent with existing patterns.

## Suggestions (non-blocking)
- The `execute` method prints `spawn_agent` as a hardcoded string (line 354) rather than using `self.inner.name()`. This is fine since `SubAgentWrapper` is always used for `spawn_agent`, but using `self.inner.name()` would be more robust if the wrapper is ever reused.
- The `truncate` helper is called with `60` chars for the task preview (line 355). This is reasonable but could be extracted as a named constant for clarity.
- The `on_update` callback (line 362-388) only detects tool call notifications via string prefix matching (`text.starts_with("[sub-agent calling tool:")`). If yoagent changes this string format, the detection silently breaks. This is a pragmatic choice given yoagent's current API, but worth noting as a fragility point.

## Scope Check
- Files within scope: YES -- only `src/main.rs` was modified
- Scope creep detected: NO -- the changes to `wrap_tools()` (removing the skip logic) and `build_agent()` (using `SubAgentWrapper` instead of raw tool) are necessary refactors to implement the specialized wrapper pattern described in the ticket
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- all 52 tests pass, clippy clean, fmt clean. The refactoring of `wrap_tools` is a simplification (removing special-case logic), not a behavior change, since `spawn_agent` was already added after `wrap_tools` in Ticket 02.
- Security concerns: NONE
- Performance concerns: NONE -- the wrapper adds two Arc-wrapped closures per sub-agent invocation, negligible overhead
