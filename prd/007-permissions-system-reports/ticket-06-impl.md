# Implementation Report: Ticket 6 -- main.rs Wiring -- Load Policy, Wrap Tools, Fire Session Hooks

**Ticket:** 6 - main.rs Wiring -- Load Policy, Wrap Tools, Fire Session Hooks
**Date:** 2026-03-07 12:00
**Status:** COMPLETE

---

## Files Changed

### Modified
- `src/main.rs` - Added permission system wiring: policy/hook loading, tool wrapping in PermissionGuard, broadcast channel creation, terminal channel permission prompt support, and SessionStart/SessionEnd/UserPromptSubmit hook firing at appropriate lifecycle points.

## Implementation Notes
- **Refactored `build_agent`**: Split tool construction into `build_raw_tools` (creates unwrapped tools) and `wrap_tools_in_permission_guard` (wraps all tools). `build_agent` now accepts permission infrastructure parameters and delegates to these helpers.
- **Broadcast channel**: Created `prompt_tx` broadcast channel of type `PermissionPromptRequest` and `pending` (`PendingResponses`) map in `main()` before building the agent. These are shared between all `PermissionGuard` instances and the terminal channel.
- **Terminal channel**: The `TerminalChannel` now receives `prompt_rx` (subscriber) and `pending` via `with_permission_prompt()`, enabling interactive permission prompts during tool execution.
- **Hook firing**: `SessionStart` fires before the REPL loop (and in single-shot mode before the prompt). `SessionEnd` fires after the REPL loop exits (and in single-shot mode after the prompt). `UserPromptSubmit` fires before each user message is dispatched to the agent.
- **`handle_slash_command` updated**: The `/model` command rebuilds the agent with the new permission infrastructure so tool wrapping is preserved after model switches.
- **Type alias**: Added `PendingResponses` type alias at the top of main.rs for consistency with the terminal module's usage.
- **TDD**: Added `wrap_tools_in_permission_guard_preserves_tool_names` and `wrap_tools_wraps_all_including_custom` tests before implementing the wrapping function. Updated all existing tests to use the new `build_agent` and `handle_slash_command` signatures.

## Acceptance Criteria
- [x] AC 1: `PermissionPolicy::load(cwd)` and `HookManager::load(cwd)` are called before the agent is built -- Both are called in `main()` before `build_agent()`.
- [x] AC 2: All tools passed to the agent are wrapped in `PermissionGuard` -- `wrap_tools_in_permission_guard` wraps every tool from `build_raw_tools` (default_tools + subagent + optional memory tools).
- [x] AC 3: The `prompt_tx` broadcast sender and `pending_responses` map are created and threaded through to both `PermissionGuard` instances and the terminal channel -- Created in `main()`, passed to `build_agent` (which passes to each `PermissionGuard`), and to `TerminalChannel::with_permission_prompt()`.
- [x] AC 4: `HookInput::SessionStart` fires before the REPL loop begins -- Fires after banner/info display but before the command bus loop starts, and in single-shot mode before the prompt.
- [x] AC 5: `HookInput::SessionEnd` fires on clean exit (including `--prompt` single-shot mode) -- Fires after the REPL loop exits (before auto-save) and in single-shot mode after the prompt completes.
- [x] AC 6: `HookInput::UserPromptSubmit` fires before each user message is dispatched to the agent -- Fires in the `SlashResult::NotSlash` branch before `run_prompt`, and in single-shot mode before `run_prompt`.
- [x] AC 7: `cargo build` succeeds with no warnings -- Confirmed.
- [x] AC 8: `cargo clippy -- -D warnings` passes -- Confirmed.

## Test Results
- Lint: PASS (clippy clean)
- Tests: PASS (147 lib tests + 57 binary tests = 204 total, 0 failures)
- Build: PASS (no warnings)
- Format: PASS (cargo fmt --check clean)
- New tests added:
  - `tests::wrap_tools_in_permission_guard_preserves_tool_names` in `src/main.rs`
  - `tests::wrap_tools_wraps_all_including_custom` in `src/main.rs`

## Concerns / Blockers
- None
