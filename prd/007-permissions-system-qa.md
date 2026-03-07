# QA Report: PRD 007 -- Permissions System

**Source PRD:** /home/travis/Development/beezle/prd/007-permissions-system.md
**Status File:** /home/travis/Development/beezle/prd/007-permissions-system-status.md
**Date:** 2026-03-07 18:30
**Overall Status:** PASS

---

## Acceptance Criteria Verification

| AC # | Description | Status | Evidence | Test Scenario |
|------|-------------|--------|----------|---------------|
| 1 | `cargo build` produces zero warnings and zero errors | PASS | `cargo build` and `cargo clippy -- -D warnings` both succeed with no output beyond "Finished" | Run `cargo build` and verify clean output |
| 2 | `bash` tool prompts when no allow rule matches | PASS | `PermissionPolicy::categorize("bash")` returns `Execute` (line 218 mod.rs); `category_default` returns `Ask` for Execute (line 229). Guard broadcasts `PermissionPromptRequest` on Ask verdict (guard.rs lines 176-202). Test `check_ask_when_no_rule_matches` confirms. | Create empty policy, call `check("bash", ...)`, verify `Ask` verdict |
| 3 | `read_file` executes without prompting by default | PASS | `categorize("read_file")` returns `Read` (line 216); `category_default` returns `Allow` for Read (line 228). Test `check_read_defaults_to_allow` passes. | Create empty policy, call `check("read_file", ...)`, verify `Allow` |
| 4 | `Bash(cargo test:*)` allows `cargo test` and `cargo test --release` without prompting | PASS | `pattern_matches("cargo test:*", "cargo test --release")` returns true via `:*` prefix match (mod.rs line 318-321). Tests `prefix_match_with_colon_star` and `check_allow_when_allow_rule_matches` pass. | Add allow rule `Bash(cargo test:*)`, check both commands |
| 5 | Deny rule `Bash(rm -rf:*)` blocks even if `Bash(*)` is in allow | PASS | `check()` evaluates deny rules before allow rules (mod.rs lines 187-195). Test `check_deny_overrides_allow` creates policy with `Bash(*)` allow and `Bash(rm -rf:*)` deny, verifies `Deny`. | Set allow `Bash(*)` and deny `Bash(rm -rf:*)`, check `rm -rf /` |
| 6 | Deny takes precedence over allow when both match | PASS | Same as AC 5 -- resolution order in `check()` is: session grants -> deny -> allow -> category defaults. | Covered by `check_deny_overrides_allow` test |
| 7 | Three settings tiers merge correctly | PASS | `load_with_home()` reads global, project, local files in order and unions rules (mod.rs lines 120-143). Test `three_tiers_merge_correctly` creates all three files and verifies 3 allow + 2 deny rules merged. | Create temp dirs with all three settings files, verify merged counts |
| 8 | Missing settings files silently ignored (empty policy) | PASS | `read_settings()` returns `None` on IO error (mod.rs line 158). Test `load_missing_files_returns_empty_policy` passes. | Load policy from empty temp dir, verify empty |
| 9 | Malformed settings files produce WARN-level log and fall back | PASS | `read_settings()` logs `tracing::warn!` on JSON parse error (mod.rs line 163) and returns `None`. Test `load_malformed_json_skips_tier` passes. | Write "NOT JSON!!!" to settings, load policy, verify empty with no panic |
| 10 | "Yes" allows single invocation | PASS | Guard test `ask_verdict_yes_allows_single_invocation` spawns responder that answers `Yes`, verifies tool executes successfully. | Simulate ask flow with Yes response, verify tool runs |
| 11 | "No" returns permission denied error | PASS | Guard test `ask_verdict_no_returns_permission_denied` confirms `ToolError` containing "permission denied". | Simulate ask flow with No response, verify error |
| 12 | "Always" stops prompting for rest of session | PASS | Guard test `ask_verdict_always_grants_session_and_proceeds` verifies session grant is added to policy. `session_grant_survives_multiple_checks` verifies repeated Allow verdicts after grant. | Answer Always, verify grant exists, re-check same tool -- should Allow |
| 13 | `pre_tool_use` hooks receive JSON with `tool_name` and `tool_input` | PASS | `HookInput::PreToolUse` serializes with `#[serde(tag = "hook_event_name")]` (hooks.rs line 43-55). Test `hook_input_pre_tool_use_serialization` verifies `tool_name` and `tool_input` fields present. | Serialize PreToolUse, verify JSON fields |
| 14 | Hook returning `{"permission_decision":"deny"}` blocks execution | PASS | `HookManager::run()` checks `permission_decision == "deny"` and sets `blocked = true` (hooks.rs line 363). Guard test `pre_hook_block_short_circuits_execution` confirms. | Configure hook that outputs deny JSON, verify tool blocked |
| 15 | Hook returning `{"updated_input":{...}}` replaces tool args | PASS | `HookManager::run()` stores `updated_input` (hooks.rs line 369). Guard test `pre_hook_updated_input_replaces_params` verifies EchoTool receives replaced params. | Configure hook with updated_input, verify inner tool gets new params |
| 16 | Hook exiting with code 2 blocks; stderr is reason | PASS | `execute_hook()` on exit code 2 returns `HookOutput` with `permission_decision: "deny"` and `stop_reason` from stderr (hooks.rs lines 460-470). Test `execute_hook_exit_2_blocks` verifies. | Run hook `exit 2` with stderr, verify deny + reason |
| 17 | `post_tool_use` hooks fire after successful execution | PASS | Guard fires `PostToolUse` hook after `Ok` result (guard.rs lines 209-220). Test `post_hook_fires_after_success` passes. | Configure post_tool_use hook, execute succeeding tool, verify no error |
| 18 | `post_tool_use_failure` hooks fire after failed execution | PASS | Guard fires `PostToolUseFailure` hook after `Err` result (guard.rs lines 222-232). Test `post_failure_hook_fires_after_failed_execution` passes. | Configure post_tool_use_failure hook, execute failing tool, verify hook runs |
| 19 | Hook matchers filter by tool name regex | PASS | `HookHandler::matches()` checks event type then applies regex to matcher target (hooks.rs lines 217-226). Tests `handler_with_matcher_filters_by_regex` and `handler_with_pipe_regex_matcher` pass. | Create handler with `^bash$` matcher, verify matches bash but not read_file |
| 20 | Hook timeouts default to 10s; configurable per hook | PASS | `DEFAULT_TIMEOUT_SECS = 10` (hooks.rs line 17). `parse_handler` uses `unwrap_or(DEFAULT_TIMEOUT_SECS)` (hooks.rs line 341). Test `hook_manager_load_from_settings` verifies custom 5s and default 10s. | Load hooks config with and without timeout_secs, verify values |
| 21 | Non-blocking hook errors (exit != 0, != 2) logged, don't block | PASS | `execute_hook()` on other exit codes logs `tracing::warn!` and returns `HookOutput::default()` (hooks.rs lines 472-480). Test `execute_hook_exit_1_non_blocking` passes. | Run hook `exit 1`, verify default output returned |
| 22 | Unit tests for `parse_rule()` | PASS | Tests: `parse_rule_bash_with_pattern`, `parse_rule_empty_pattern`, `parse_rule_missing_parens` -- all pass. | Run `cargo test permissions::tests::parse_rule` |
| 23 | Unit tests for `pattern_matches()` | PASS | Tests: `prefix_match_with_colon_star`, `prefix_match_no_match`, `recursive_glob_matches`, `recursive_glob_no_match`, `single_segment_glob_matches`, `single_segment_glob_no_nested`, `domain_match`, `domain_no_match`, `bare_wildcard_matches_anything` -- all 9 pass. | Run `cargo test permissions::tests` |
| 24 | Unit tests for `PermissionPolicy::check()` | PASS | Tests: `check_allow_when_allow_rule_matches`, `check_deny_overrides_allow`, `check_ask_when_no_rule_matches`, `check_read_defaults_to_allow`, `session_grant_survives_multiple_checks` -- all 5 pass. | Run `cargo test permissions::tests::check` |
| 25 | Unit tests for settings file merging | PASS | Tests: `load_missing_files_returns_empty_policy`, `load_malformed_json_skips_tier`, `three_tiers_merge_correctly` -- all 3 pass. | Run `cargo test permissions::tests::load` |
| 26 | Unit tests for `execute_hook()` | PASS | Tests: `execute_hook_exit_0_with_json`, `execute_hook_exit_0_empty_stdout`, `execute_hook_exit_2_blocks`, `execute_hook_exit_1_non_blocking`, `execute_hook_timeout` -- all 5 pass. | Run `cargo test permissions::hooks::tests::execute_hook` |
| 27 | Unit tests for `HookInput` serialization for each event type | PASS | Tests for all 8 event types: `pre_tool_use`, `post_tool_use`, `post_tool_use_failure`, `user_prompt_submit`, `session_start`, `session_end`, `subagent_start`, `subagent_stop` -- all 8 pass. | Run `cargo test permissions::hooks::tests::hook_input` |
| 28 | `cargo clippy -- -D warnings` passes | PASS | Clippy finishes with no warnings/errors. | Run `cargo clippy -- -D warnings` |

## Bugs Found

No Critical or Major bugs found.

### Bug 1: Non-ASCII byte-level slicing in `match_recursive` could panic
- **Severity:** Minor
- **Location:** `src/permissions/mod.rs`, `match_recursive()` function, lines 432, 436, 443-448
- **Description:** The function uses `pattern.as_bytes()[0]` and `&value[1..]` byte indexing, which will panic on multi-byte UTF-8 characters. For file paths and tool names this is low risk (typically ASCII), but a non-ASCII path could trigger a panic.
- **Reproduction Steps:**
  1. Create a permission rule with a non-ASCII pattern or match against a non-ASCII path
  2. The `match_recursive` function slices at byte boundaries that may split a multi-byte character
  3. Expected: graceful failure; Actual: potential panic
- **Suggested Fix:** Use `chars()` iteration instead of byte indexing, or validate ASCII-only input.

### Bug 2: Hook timeout does not kill child process
- **Severity:** Minor
- **Location:** `src/permissions/hooks.rs`, `execute_hook()` function, lines 428-435
- **Description:** When a hook times out, the `tokio::time::timeout` wrapping `wait_with_output` fires, but the child process handle has already been consumed by `wait_with_output`. The comment at line 432-433 acknowledges this. On Unix, the child process may become orphaned.
- **Reproduction Steps:**
  1. Configure a hook that runs `sleep 30` with a 1-second timeout
  2. The timeout fires, but the `sleep` process continues running
  3. Expected: child killed; Actual: orphaned process
- **Suggested Fix:** Spawn the process, pass a handle for killing, use `tokio::select!` with kill before `wait_with_output`.

## Edge Cases Not Covered

- Non-ASCII file paths in permission rules -- Risk: LOW (paths are typically ASCII, but Unicode paths could trigger panic in `match_recursive`)
- Concurrent permission prompts from multiple tools executing in parallel -- Risk: MEDIUM (the broadcast + polling approach should work, but no tests exercise concurrent prompts from multiple guards simultaneously)
- Very large settings files or extremely long rule patterns -- Risk: LOW (the recursive glob matcher has no depth limit, could stack overflow on pathological patterns like `*` repeated many times)
- Hook commands that produce very large stdout output -- Risk: LOW (`wait_with_output` buffers all output in memory)

## Integration Issues

- **Shared stdin contention:** The terminal REPL loop and the permission prompt loop both read from stdin via `spawn_blocking`. If the agent requests permission while the user is typing a prompt, input may be consumed by the wrong reader. This is noted in the status file as a known issue for when TUI lands.
- No integration issues between tickets were found -- all six implementation tickets plus the verification ticket compose correctly.

## Regression Results

- **Test suite:** PASS -- 204 tests (147 lib + 57 bin), 0 failures
- **Build:** PASS -- zero warnings, zero errors
- **Lint:** PASS -- `cargo clippy -- -D warnings` clean
- **Format:** PASS -- `cargo fmt --check` clean
- **Shared code impact:** `src/lib.rs` gained `pub mod permissions;` (additive only). `src/channels/terminal.rs` extended with permission prompt support (backward-compatible builder pattern). `src/main.rs` refactored to wrap tools -- existing tests updated and all pass. `Cargo.toml` added `regex = "1"` (new dep, no conflicts).

## Recommended Follow-Up Tickets

1. **Harden `match_recursive` for non-ASCII input** -- Replace byte-level slicing with char-based iteration or validate ASCII-only patterns. Minor severity but could panic on edge case paths.
2. **Kill child process on hook timeout** -- Use `tokio::select!` with process kill to avoid orphaned processes when hooks time out.
3. **Address shared stdin contention** -- When TUI integration lands, the permission prompt loop and REPL input loop must be coordinated to avoid input stealing. Consider using the TUI's own input handling.
4. **Add concurrent permission prompt test** -- Verify that two PermissionGuard instances prompting simultaneously don't corrupt the pending_responses map or deadlock.
