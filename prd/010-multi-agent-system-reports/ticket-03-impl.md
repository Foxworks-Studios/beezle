# Implementation Report: Ticket 3 -- `tools_for_names()` and `coordinator_agent_prompt()` (TDD red + green)

**Ticket:** 3 - `tools_for_names()` and `coordinator_agent_prompt()` (TDD red + green)
**Date:** 2026-03-07 12:00
**Status:** COMPLETE

---

## Files Changed

### Created
- None

### Modified
- `src/agent/sub_agents.rs` - Replaced `todo!()` stubs with implementations for `tools_for_names()` and `coordinator_agent_prompt()`. Added 12 unit tests covering both functions.

## Implementation Notes
- `tools_for_names()` uses a match statement mapping each of the six known tool name strings to their yoagent constructor via `Arc::new(ToolType::default())`. Unrecognized names emit `tracing::warn!` with the tool name and are filtered out via `filter_map`.
- `coordinator_agent_prompt()` builds a Markdown string with `## Available Sub-Agents` section listing each agent's name, description, and model. The `## Available Models` section is only appended when `model_roster.len() > 1`.
- The file already had `todo!()` stubs with doc comments from a prior ticket, along with the necessary imports (`Arc`, `tracing::warn`, tool types, `ModelEntry`). The prior ticket also added `load_user_sub_agents_from()`, `load_user_sub_agents()`, `anthropic_model_roster()`, and `load_model_roster()` with tests.
- Note: the file also has unused imports `std::path::Path` and `crate::config::AppConfig` -- these are used by functions added by a later ticket (Ticket 4 work already present). Not removing them as they are outside my scope.

## Acceptance Criteria
- [x] `tools_for_names()` maps the six known tool name strings to correct yoagent constructors, each wrapped in `Arc` - Implemented with match on `read_file`, `write_file`, `edit_file`, `list_files`, `search`, `bash`
- [x] `tools_for_names()` emits `tracing::warn!` and skips unrecognized names - `unknown` arm emits `warn!(tool_name = unknown, "unrecognized tool name, skipping")` and returns `None`
- [x] `tools_for_names([])` returns empty vec without panicking - Tested in `tools_for_names_empty_input_returns_empty_vec`
- [x] `coordinator_agent_prompt()` output contains each agent's name, description, and model info - Tested in `coordinator_prompt_contains_agent_names`, `_descriptions`, `_model_info`
- [x] `coordinator_agent_prompt()` includes `## Available Models` section when `model_roster` has more than one entry - Tested in `coordinator_prompt_includes_models_section_when_multiple`
- [x] `coordinator_agent_prompt()` omits `## Available Models` section when zero or one entry - Tested in `coordinator_prompt_omits_models_section_when_empty_roster` and `_when_single_model`
- [x] All tests written before implementation; all pass - RED phase confirmed with `todo!()` panic, then GREEN with implementation
- [x] `cargo clippy -- -D warnings` passes - Clean

## Test Results
- Lint: PASS (`cargo clippy -- -D warnings` clean)
- Tests: PASS (206 library tests, 57 binary tests, 0 failures)
- Build: PASS (no warnings)
- Format: PASS (`cargo fmt --check` clean)
- New tests added:
  - `tools_for_names_empty_input_returns_empty_vec`
  - `tools_for_names_resolves_all_six_known_tools`
  - `tools_for_names_skips_unrecognized_names`
  - `tools_for_names_all_unrecognized_returns_empty`
  - `tools_for_names_single_tool`
  - `coordinator_prompt_contains_agent_names`
  - `coordinator_prompt_contains_agent_descriptions`
  - `coordinator_prompt_contains_model_info`
  - `coordinator_prompt_omits_models_section_when_empty_roster`
  - `coordinator_prompt_omits_models_section_when_single_model`
  - `coordinator_prompt_includes_models_section_when_multiple`
  - `coordinator_prompt_empty_agents_and_empty_roster`
  All in `src/agent/sub_agents.rs`

## Concerns / Blockers
- The file already contains work from Ticket 4 (`load_user_sub_agents_from`, `load_user_sub_agents`, `anthropic_model_roster`, `load_model_roster` and their tests). This was present when I started and I left it untouched.
- Unused imports `std::path::Path` and `crate::config::AppConfig` exist in the file but are used by the Ticket 4 functions already present. Did not remove them.
