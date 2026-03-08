# Code Review: Ticket 3 -- `tools_for_names()` and `coordinator_agent_prompt()` (TDD red + green)

**Ticket:** 3 -- `tools_for_names()` and `coordinator_agent_prompt()` (TDD red + green)
**Impl Report:** prd/010-multi-agent-system-reports/ticket-03-impl.md
**Date:** 2026-03-07 14:30
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `tools_for_names()` maps six known tool name strings to correct yoagent constructors, each wrapped in `Arc` | Met | Lines 116-135: match on all six names (`read_file`, `write_file`, `edit_file`, `list_files`, `search`, `bash`), each returning `Arc::new(ToolType::...)`. Test `tools_for_names_resolves_all_six_known_tools` verifies all six resolve and produce correct `name()` values. |
| 2 | `tools_for_names()` emits `tracing::warn!` and skips unrecognized names | Met | Line 128: `warn!(tool_name = unknown, "unrecognized tool name, skipping")` in the catch-all arm, returns `None` which `filter_map` drops. Tests `tools_for_names_skips_unrecognized_names` and `tools_for_names_all_unrecognized_returns_empty` cover this. |
| 3 | `tools_for_names([])` returns empty vec without panicking | Met | Line 116 iterates over empty slice, produces empty vec. Test `tools_for_names_empty_input_returns_empty_vec` confirms. |
| 4 | `coordinator_agent_prompt()` output contains each agent's name, description, and model info | Met | Lines 152-181: iterates agents, formats `### name`, description, and `**Model:**` line. Tests `coordinator_prompt_contains_agent_names`, `_descriptions`, `_model_info` verify with two sample agents. |
| 5 | `coordinator_agent_prompt()` includes `## Available Models` when `model_roster` has more than one entry | Met | Line 169: `if model_roster.len() > 1`. Test `coordinator_prompt_includes_models_section_when_multiple` verifies with two entries. |
| 6 | `coordinator_agent_prompt()` omits `## Available Models` when zero or one entry | Met | Same conditional. Tests `coordinator_prompt_omits_models_section_when_empty_roster` and `_when_single_model` verify both cases. |
| 7 | All tests pass | Met | Verified: 44 sub_agents tests pass (includes pre-existing + 12 new). |
| 8 | `cargo clippy -- -D warnings` passes | Met | Verified: clippy clean on library code. |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)
- The `coordinator_prompt_empty_agents_and_empty_roster` test only asserts `## Available Models` is absent but does not assert the output is empty (or at least that `## Available Sub-Agents` is also absent). This is a weak test -- it would pass even if the function returned garbage text. Low risk since the empty-agents path is covered implicitly by the conditional on line 155.

## Suggestions (non-blocking)
- The `coordinator_agent_prompt` function handles the `model: None` case by outputting "inherits coordinator model" (line 163), but no test covers an agent with `model: None` being passed to this function. Consider adding one in a future ticket.

## Scope Check
- Files within scope: YES -- only `src/agent/sub_agents.rs` was modified, which is the sole file listed in the ticket scope.
- Scope creep detected: NO -- the implementer correctly left pre-existing Ticket 4 code untouched and noted its presence.
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- two new pure functions added; no existing code modified. All 44 module tests pass.
- Security concerns: NONE
- Performance concerns: NONE -- `tools_for_names` allocates new tool instances per call, which is the expected pattern for yoagent tool construction.
