# Code Review: Ticket 3 -- Hooks Module (`src/permissions/hooks.rs`)

**Ticket:** 3 -- Hooks Module (`src/permissions/hooks.rs`)
**Impl Report:** prd/007-permissions-system-reports/ticket-03-impl.md
**Date:** 2026-03-07 15:00
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `HookInput` variants for all 8 events serialize with `hook_event_name` | Met | 8 serialization tests verify each variant; `#[serde(tag = "hook_event_name", rename_all = "snake_case")]` is correct |
| 2 | `execute_hook()` exit 0 + valid JSON returns `Ok(HookOutput)` | Met | Test `execute_hook_exit_0_with_json` verifies; line 442 parses trimmed stdout |
| 3 | `execute_hook()` exit 0 + empty stdout returns `Ok(HookOutput::default())` | Met | Test `execute_hook_exit_0_empty_stdout`; line 440 checks trimmed empty |
| 4 | `execute_hook()` exit 2 returns deny with stderr reason | Met | Test `execute_hook_exit_2_blocks`; lines 455-465 construct deny output |
| 5 | `execute_hook()` exit 1 is non-blocking warn | Met | Test `execute_hook_exit_1_non_blocking`; lines 467-475 log warn and return default |
| 6 | `execute_hook()` timeout returns error | Met | Test `execute_hook_timeout`; `tokio::time::timeout` at line 423 |
| 7 | `HookManager::load(cwd)` reads hooks, missing key = empty | Met | Tests `hook_manager_load_from_settings` and `hook_manager_load_missing_hooks_key` |
| 8 | `HookHandler` with `matcher: Some(regex)` filters by regex | Met | Test `handler_with_matcher_filters_by_regex` and `handler_with_pipe_regex_matcher` |
| 9 | `HookHandler` with `matcher: None` fires for all events of type | Met | Test `handler_without_matcher_fires_for_all` |
| 10 | `HookManager::run()` short-circuits on first blocked | Met | Test `hook_manager_run_aggregates_and_short_circuits`; line 361 returns early |
| 11 | Unit tests for HookInput serialization (8 types) | Met | 8 tests present |
| 12 | Unit tests for execute_hook (5 scenarios) | Met | 5 tests present |
| 13 | cargo test passes; clippy clean | Met | Verified: 172 tests pass, clippy clean, fmt clean |

## Issues Found

### Critical (must fix before merge)

None.

### Major (should fix, risk of downstream problems)

- **Timeout does not kill the child process.** In `execute_hook()` (line 423-430), when `tokio::time::timeout` fires, the `child.wait_with_output()` future is dropped, but on Unix this does NOT send SIGKILL to the child process. The child becomes an orphan and continues running. A hook that sleeps or hangs will leave a zombie/orphan process. The fix would be to hold the `Child` handle separately, use `timeout` on `child.wait()`, and call `child.kill()` in the timeout branch. This matters because hooks are user-defined commands that may genuinely hang.

### Minor (nice to fix, not blocking)

- **Stdin write is not covered by timeout.** In `execute_hook()` (lines 416-420), the `stdin.write_all()` happens before the timeout-wrapped `wait_with_output()`. If a hook process has a small stdin buffer and doesn't read it, the write could block indefinitely. In practice this is unlikely for typical JSON payloads, but wrapping the entire spawn+write+wait sequence in the timeout would be more robust.

- **Malformed JSON on exit 0 is silently swallowed.** Lines 442-452: if a hook exits 0 with invalid JSON stdout, the error is logged at WARN but execution continues with `HookOutput::default()`. This is arguably correct for robustness, but it means hook authors get no feedback that their output is being ignored. The current behavior is reasonable but worth documenting explicitly in user-facing docs when those are written.

- **Impl report claims 172 tests but math is off.** The report says "all 172 tests pass, including 22 new hooks tests" but the actual count is 117 (lib) + 55 (bin) = 172. The 22 hooks tests are a subset of the 117 lib tests. This is cosmetically misleading but not a real issue.

## Suggestions (non-blocking)

- Consider adding `#[must_use]` to `HookManager::empty()` and `HookManager::load()` since discarding a `HookManager` is almost certainly a bug.
- The `HookHandler` struct derives `Clone` but `Regex` clone is a full recompilation. If handlers are cloned frequently, consider wrapping in `Arc<Regex>`. Not a concern at current scale.
- The `matcher_target()` method on `HookInput` is well-designed -- returning the appropriate field per variant is clean and extensible.

## Scope Check
- Files within scope: YES -- `src/permissions/hooks.rs` (created), `src/permissions/mod.rs` (modified to add `pub mod hooks;`), `Cargo.toml` (added `regex = "1"`)
- Scope creep detected: NO
- Unauthorized dependencies added: NO -- `regex = "1"` is explicitly required by the ticket

## Risk Assessment
- Regression risk: LOW -- New module with no changes to existing logic. The only modification to `mod.rs` is adding `pub mod hooks;`.
- Security concerns: NONE -- Hook commands are user-configured in settings files, not from untrusted input. The `sh -c` invocation is intentional per the hook protocol.
- Performance concerns: NONE -- Hooks are invoked per-event with configurable timeouts. Regex compilation happens once at load time.
