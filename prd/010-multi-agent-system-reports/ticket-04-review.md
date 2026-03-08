# Code Review: Ticket 4 -- `load_user_sub_agents()` and `load_model_roster()` (TDD red + green)

**Ticket:** 4 -- `load_user_sub_agents()` and `load_model_roster()` (TDD red + green)
**Impl Report:** prd/010-multi-agent-system-reports/ticket-04-impl.md
**Date:** 2026-03-07 15:00
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `load_user_sub_agents()` returns empty vec when dir doesn't exist, without panic | Met | `load_user_sub_agents_from()` at line 253 catches `Err` from `read_dir` and returns empty vec; tested by `load_user_sub_agents_returns_empty_when_dir_missing` |
| 2 | Skips file with missing `name` with WARN log | Met | `parse_agent_file` returns Err, `load_user_sub_agents_from` line 276 logs `warn!` with path; tested by `load_user_sub_agents_skips_file_missing_name` |
| 3 | Skips file with missing `description` with WARN log | Met | Same path as AC2; tested by `load_user_sub_agents_skips_file_missing_description` |
| 4 | Skips file missing `---` delimiter with WARN log | Met | `parse_agent_file` returns Err on missing delimiter; tested by `load_user_sub_agents_skips_file_missing_delimiter` |
| 5 | Returns valid `SubAgentDef` for correctly formatted file | Met | Tested by `load_user_sub_agents_returns_valid_def_for_correct_file` with full field assertions |
| 6 | `load_model_roster()` returns exactly 3 entries for anthropic with empty user models | Met | `anthropic_model_roster()` returns Haiku/Sonnet/Opus; tested by `load_model_roster_returns_three_for_anthropic` with ID assertions |
| 7 | `load_model_roster()` returns empty vec for ollama | Met | Wildcard match at line 334 returns `Vec::new()`; tested by `load_model_roster_returns_empty_for_ollama` |
| 8 | `load_model_roster()` merges user models with anthropic entries (3 + N) | Met | `roster.extend(config.models.iter().cloned())` at line 331; tested with 3+2=5 assertion |
| 9 | All tests use `tempfile::TempDir`; all pass | Met | All filesystem tests use `tempfile::TempDir::new()`; 44 tests pass |
| 10 | `cargo clippy -- -D warnings` passes | Met | `cargo clippy --lib -p beezle -- -D warnings` passes clean |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)
- **Byte-level slicing at line 205**: `&trimmed[3..]` assumes the `---` prefix is exactly 3 bytes. This is safe here because the guard on line 200 guarantees `trimmed` starts with ASCII `---`, but it is a pattern worth noting. Not a bug in this context.
- **WARN log tests are indirect**: Tests verify that bad files are *skipped* (empty result vec) but do not assert on the actual tracing WARN event. The AC says "emits exactly one WARN-level tracing event naming the file path" -- this is technically unverified. However, adding `tracing-test` is out of scope for this ticket, and the code clearly calls `warn!` with `path = %path.display()` at line 268 and 276. Acceptable for now.
- **Impl report test count discrepancy**: Report claims 32 tests (22 pre-existing + 10 new) but actual count is 44. This is because Ticket 3 ran in parallel and added tests. Not an issue with the code, just an inaccuracy in the report.

## Suggestions (non-blocking)
- The `load_user_sub_agents_from` function swallows all `read_dir` errors (line 255), including permission errors. For a user-facing tool, a WARN log on permission denied might be more helpful than silent empty return. However, the AC explicitly says "without panicking or logging an error," so the current behavior matches the spec.

## Scope Check
- Files within scope: YES -- only `src/agent/sub_agents.rs` was modified
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- new functions only, no modifications to existing logic
- Security concerns: NONE
- Performance concerns: NONE -- `read_dir` is bounded by filesystem entries, `flatten()` handles errors gracefully
