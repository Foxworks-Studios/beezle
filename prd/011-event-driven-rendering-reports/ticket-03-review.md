# Code Review: Ticket 3 -- Remove wrappers, thinking-label system, and simplify `build_agent()`

**Ticket:** 3 -- Remove wrappers, thinking-label system, and simplify `build_agent()`
**Impl Report:** prd/011-event-driven-rendering-reports/ticket-03-impl.md
**Date:** 2026-03-06 15:30
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `StreamProviderWrapper` struct and impl deleted | Met | grep confirms zero matches in src/main.rs |
| 2 | `ToolWrapper` struct and impl deleted | Met | grep confirms zero matches |
| 3 | `SubAgentWrapper` struct and impl deleted | Met | grep confirms zero matches |
| 4 | `wrap_tools()` deleted | Met | grep confirms zero matches |
| 5 | `run_single_prompt()` deleted | Met | grep confirms zero matches |
| 6 | `render_events()` deleted | Met | grep confirms zero matches |
| 7 | `clear_thinking_line()`, `fetch_thinking_label()`, `thinking_label()` deleted | Met | grep confirms zero matches |
| 8 | `THINKING_LABELS` deleted | Met | grep confirms zero matches |
| 9 | `build_agent()` no longer accepts `use_color`; passes tools unwrapped | Met | Signature at line 216 has 6 params, no `use_color`. Body uses `default_tools()` directly (line 250), pushes unwrapped subagent (line 251) and memory tools (lines 255-256) |
| 10 | All call sites drop `use_color` argument | Met | `build_agent` called at line 652 (main) and line 560 (/model handler) -- neither passes `use_color` |
| 11 | Standalone Ctrl+C handler removed from `main()` | Met | grep for `std::process::exit` returns no matches. Ctrl+C now handled inside `run_prompt()` at line 393 |
| 12 | `thinking_key` variable removed | Met | grep for `thinking_key` returns zero matches |
| 13 | 5 wrapper-specific tests deleted | Met | Impl report lists 9 deleted tests (5 wrapper + 4 from render_events/run_single_prompt). All confirmed absent via grep |
| 14 | `build_agent_tools_include_spawn_agent` updated | Met | Test at line 1279 uses `default_tools()` + unwrapped subagent, no wrapper types referenced |
| 15 | `bus_regular_prompt_processes_through_mock_agent` updated | Met | Test at line 1240 calls `run_prompt` (line 1257), not `run_single_prompt` |
| 16 | Grep assertion: no matches for deleted items | Met | Verified: `grep -c` returns 0 |
| 17 | `cargo test` passes; `cargo build` no warnings; `cargo clippy` passes | Met | 49 binary tests pass, 75 lib tests pass (124 total). Clippy clean. Build clean. |

## Issues Found

### Critical (must fix before merge)
None.

### Major (should fix, risk of downstream problems)
None.

### Minor (nice to fix, not blocking)
- Line 1267: Comment says "streaming happened via wrappers" but wrappers no longer exist. Should say "streaming happened via run_prompt's event loop" or similar. Stale comment from pre-refactor.

## Suggestions (non-blocking)
- The impl report notes `use_color` was also removed from `handle_slash_command()` as a necessary scope extension. Verified: the parameter was replaced by pre-resolved `dim: &str` and `reset: &str` parameters (lines 511-512), which is cleaner. This is a reasonable scope extension to maintain zero-warning compliance.

## Scope Check
- Files within scope: YES -- only `src/main.rs` modified
- Scope creep detected: MINOR -- `use_color` removed from `handle_slash_command()` signature (not listed in ticket scope). Justified: without this change, `cargo build` would emit an unused-parameter warning, violating AC 17. The change is mechanical and correct.
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- all 124 tests pass. Deleted code was obsolete (replaced by `run_prompt()` in Ticket 2). No behavioral changes to remaining code paths.
- Security concerns: NONE
- Performance concerns: NONE
