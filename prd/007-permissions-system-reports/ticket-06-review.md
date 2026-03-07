# Code Review: Ticket 6 -- main.rs Wiring -- Load Policy, Wrap Tools, Fire Session Hooks

**Ticket:** 6 -- main.rs Wiring -- Load Policy, Wrap Tools, Fire Session Hooks
**Impl Report:** /home/travis/Development/beezle/prd/007-permissions-system-reports/ticket-06-impl.md
**Date:** 2026-03-07 17:30
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `PermissionPolicy::load(cwd)` and `HookManager::load(cwd)` called before agent is built | Met | Lines 941-942 in `main()`, clearly before `build_agent()` on line 947. |
| 2 | All tools wrapped in `PermissionGuard` | Met | `wrap_tools_in_permission_guard` (line 266) wraps every tool from `build_raw_tools`. Called inside `build_agent` (line 339). Verified by `wrap_tools_wraps_all_including_custom` test. |
| 3 | `prompt_tx` and `pending_responses` threaded to guards and terminal channel | Met | Created on lines 943-944, passed to `build_agent` (which passes to each `PermissionGuard`), and to `TerminalChannel::with_permission_prompt` on line 1061. |
| 4 | `SessionStart` fires before REPL loop | Met | Lines 1049-1055 fire `SessionStart` before the bus consumer loop. Also fires in single-shot mode at line 994 before the prompt. |
| 5 | `SessionEnd` fires on clean exit (including `--prompt` mode) | Met | Lines 1129-1134 fire `SessionEnd` after the REPL loop exits. Lines 1019-1024 fire it in single-shot mode after the prompt completes. |
| 6 | `UserPromptSubmit` fires before each user message dispatch | Met | Lines 1107-1113 fire it in the `NotSlash` branch before `run_prompt`. Lines 1003-1009 fire it in single-shot mode before `run_prompt`. |
| 7 | `cargo build` succeeds with no warnings | Met | Confirmed by running `cargo clippy -- -D warnings` which passed clean. |
| 8 | `cargo clippy -- -D warnings` passes | Met | Confirmed -- ran successfully with no warnings. |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)

1. **Discarded `HookResult` from `UserPromptSubmit` hooks** (lines 1003-1009, 1107-1113): The return value of `hooks.run(&HookInput::UserPromptSubmit { ... }).await` is discarded. If a hook returns `blocked: true`, the prompt would still be dispatched to the agent. For `SessionStart`/`SessionEnd` this is fine (nothing to block), but for `UserPromptSubmit` it could be meaningful. The ticket ACs only require "fires before dispatch" which is met, but a future ticket may need to respect the blocked result. Worth a `// TODO: check HookResult::blocked` comment.

2. **`/model` command passes `None` for `memory_store`** (line 840): When rebuilding the agent after `/model new-model`, the memory store is hardcoded to `None`, meaning memory tools are lost after a model switch. This is a pre-existing design limitation (not introduced by this ticket), but the permission wiring change preserved this behavior rather than fixing it.

3. **`#[allow(clippy::too_many_arguments)]` on two functions** (lines 265, 310): Both `wrap_tools_in_permission_guard` (10 params) and `build_agent` (12 params) suppress the clippy warning. This is pragmatic for now but signals these functions would benefit from a config/context struct in a future refactor.

## Suggestions (non-blocking)

- The `PendingResponses` type alias (line 34) is a good addition for readability. Consider moving it to the permissions module so it's defined once rather than duplicated between `main.rs` and `terminal.rs`.
- The `_prompt_rx` on line 943 is immediately dropped. This is correct (subscribers are created via `prompt_tx.subscribe()`), but a brief comment explaining why it's unused would help future readers.

## Scope Check
- Files within scope: YES -- only `src/main.rs` was modified, which is the sole file in scope.
- Scope creep detected: NO -- changes are tightly focused on permission wiring and hook firing.
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- existing tests were updated to pass the new parameters. All 57 binary tests pass. The refactor from a single `build_agent` to `build_raw_tools` + `wrap_tools_in_permission_guard` + `build_agent` is clean and well-tested.
- Security concerns: NONE -- permission infrastructure is correctly wired; the guard wraps all tools.
- Performance concerns: NONE -- `HookManager::run()` is async and only fires matching hooks. The broadcast channel has a reasonable buffer size (16).
