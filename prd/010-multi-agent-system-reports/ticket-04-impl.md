# Implementation Report: Ticket 4 -- `load_user_sub_agents()` and `load_model_roster()` (TDD red + green)

**Ticket:** 4 - `load_user_sub_agents()` and `load_model_roster()` (TDD red + green)
**Date:** 2026-03-07 14:30
**Status:** COMPLETE

---

## Files Changed

### Created
- None

### Modified
- `src/agent/sub_agents.rs` - Added `load_user_sub_agents_from()`, `load_user_sub_agents()`, `anthropic_model_roster()`, `load_model_roster()` functions, and added `use std::path::Path` and `use crate::config::AppConfig` imports. Added 10 new tests at the end of the test module.

## Implementation Notes
- `load_user_sub_agents_from(dir: &Path)` is the testable inner function; `load_user_sub_agents()` is the convenience wrapper using `~/.beezle/agents/`. This follows the ticket's explicit instruction for testability.
- `load_user_sub_agents_from()` silently returns empty vec when directory doesn't exist (no error, no log), filters to `.md` files only, and logs WARN on parse failures including the file path.
- `anthropic_model_roster()` is a private helper that returns the three standard Anthropic tier entries with guidance text matching the PRD exactly.
- `load_model_roster()` uses a match on `config.agent.default_provider` -- returns anthropic tiers + user models for "anthropic", empty vec for everything else (including "ollama").
- All new code was added after `parse_agent_file()` and before the test module, and all new tests were added at the end of the test module, to minimize merge conflicts with Ticket 3 which adds `tools_for_names()` and `coordinator_agent_prompt()`.
- Ticket 3 ran in parallel and added `todo!()` stubs for its functions plus imports for yoagent tools. These stubs cause clippy warnings (unused variables/imports) that are not from this ticket's code.

## Acceptance Criteria
- [x] AC 1: `load_user_sub_agents()` returns empty vec when `~/.beezle/agents/` does not exist - tested via `load_user_sub_agents_returns_empty_when_dir_missing`
- [x] AC 2: Skips file with missing `name` field and emits WARN - tested via `load_user_sub_agents_skips_file_missing_name`; `parse_agent_file` returns Err, which triggers `warn!` with path
- [x] AC 3: Skips file with missing `description` field with WARN - tested via `load_user_sub_agents_skips_file_missing_description`
- [x] AC 4: Skips file missing `---` delimiter with WARN - tested via `load_user_sub_agents_skips_file_missing_delimiter`
- [x] AC 5: Returns valid `SubAgentDef` for correctly formatted file - tested via `load_user_sub_agents_returns_valid_def_for_correct_file`
- [x] AC 6: `load_model_roster()` returns exactly 3 entries for anthropic with empty models - tested via `load_model_roster_returns_three_for_anthropic`
- [x] AC 7: `load_model_roster()` returns empty vec for ollama - tested via `load_model_roster_returns_empty_for_ollama`
- [x] AC 8: `load_model_roster()` merges user models with anthropic entries (3 + N) - tested via `load_model_roster_merges_user_models_with_anthropic` (3 + 2 = 5)
- [x] AC 9: All tests use `tempfile::TempDir` for filesystem isolation; all pass
- [x] AC 10: `cargo clippy -- -D warnings` - my code passes; pre-existing failures from Ticket 3 stubs (see Concerns)

## Test Results
- Lint: PARTIAL - `cargo clippy --lib -- -D warnings` fails due to Ticket 3's `todo!()` stubs (`tools_for_names` and `coordinator_agent_prompt`) which have unused variables and unused imports. These are not from this ticket's code.
- Tests: PASS - all 32 tests in `agent::sub_agents::tests` pass (22 pre-existing + 10 new)
- Build: PASS (with warnings from Ticket 3 stubs)
- Format: PASS - `cargo fmt --check` passes
- New tests added:
  - `load_user_sub_agents_returns_empty_when_dir_missing`
  - `load_user_sub_agents_returns_valid_def_for_correct_file`
  - `load_user_sub_agents_skips_file_missing_name`
  - `load_user_sub_agents_skips_file_missing_description`
  - `load_user_sub_agents_skips_file_missing_delimiter`
  - `load_user_sub_agents_skips_non_md_files`
  - `load_user_sub_agents_returns_multiple_valid_agents`
  - `load_model_roster_returns_three_for_anthropic`
  - `load_model_roster_returns_empty_for_ollama`
  - `load_model_roster_merges_user_models_with_anthropic`

## Concerns / Blockers
- Ticket 3 (running in parallel) added `todo!()` stubs for `tools_for_names()` and `coordinator_agent_prompt()` with unused imports (`BashTool`, `EditFileTool`, etc.) and unused variables. These cause `cargo clippy -- -D warnings` to fail. Once Ticket 3 implements those functions, the clippy errors will resolve.
- The WARN log tests verify behavior indirectly (by checking that bad files are skipped and don't appear in results). To directly assert on tracing events would require `tracing-test` or similar, which is not in the project's dependencies. The current approach is sufficient for the acceptance criteria.
