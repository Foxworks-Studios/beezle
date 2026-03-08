# Code Review: Ticket 5 -- `build_sub_agent()` constructor (TDD red + green)

**Ticket:** 5 -- `build_sub_agent()` constructor (TDD red + green)
**Impl Report:** prd/010-multi-agent-system-reports/ticket-05-impl.md
**Date:** 2026-03-07 14:30
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `build_sub_agent()` with `model: Some(...)` calls `SubAgentTool::with_model(...)` -- confirmed by `tool.name()` matching `def.name` | Met | Line 88: `.with_model(resolved_model)` always called. Test at line 916 confirms `tool.name() == "explorer"` with explicit model. |
| 2 | `build_sub_agent()` with `model: None` uses `parent_model` | Met | Line 75: `def.model.as_deref().unwrap_or(parent_model)`. Test at line 931 passes `model: None` and `parent_model: "qwen2.5:14b"`. |
| 3 | `build_sub_agent()` calls `tools_for_names()` and passes result to `with_tools()`; empty tools don't panic | Met | Line 83-90: `tools_for_names(&def.tools)` result passed to `.with_tools(tools)`. Test at line 946 uses empty tools vec. |
| 4 | `build_sub_agent()` sets `max_turns` when `Some`, omits when `None` | Met | Lines 92-94: conditional `with_max_turns()` call. Tests at lines 961 and 976 cover both branches. |
| 5 | `tracing::debug!` emitted with agent name and resolved model | Met | Lines 77-81: `debug!(agent_name = %def.name, model = %resolved_model, "building sub-agent")` |
| 6 | All tests pass; clippy clean | Met | Verified: 50 tests pass in sub_agents module, `cargo clippy --lib -p beezle -- -D warnings` clean. |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)
- **Test depth is shallow**: All 6 `build_sub_agent()` tests only assert `tool.name()` equals the expected name. They cannot verify that the correct model string was passed to `with_model()`, that `with_tools()` received the right tools, or that `max_turns` was actually set. This is a limitation of the `SubAgentTool` API (no getters/introspection), so it's understandable, but worth noting. If `yoagent` ever adds inspection methods, these tests should be strengthened.
- **No test for the `tracing::debug!` emission (AC 5)**: The debug log is present in code but no test captures/asserts on it. This could be done with `tracing-test` or `tracing_subscriber`'s test layer, but is not blocking since the log line is visually confirmed in the source.

## Suggestions (non-blocking)
- Consider adding a comment in the test module noting the `SubAgentTool` API limitation that prevents deeper assertions, so future reviewers understand why tests are smoke-level.

## Scope Check
- Files within scope: YES -- only `src/agent/sub_agents.rs` was modified for this ticket (the `src/agent/mod.rs` change adding `pub mod sub_agents` is from ticket 2, not this ticket).
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- the function is additive (new public API), does not modify any existing code paths, and all existing tests continue to pass.
- Security concerns: NONE -- `api_key` is passed through to `SubAgentTool`, no logging of secrets.
- Performance concerns: NONE -- `tools_for_names()` allocates per-call but this is a setup path, not a hot loop.
