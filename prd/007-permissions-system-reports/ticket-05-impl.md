# Implementation Report: Ticket 5 -- Terminal Permission Prompt Display and Response

**Ticket:** 5 - Terminal Permission Prompt Display and Response
**Date:** 2026-03-07 00:00
**Status:** COMPLETE

---

## Files Changed

### Created
- None

### Modified
- `src/channels/terminal.rs` - Added permission prompt subscriber, format/parse helpers, and `with_permission_prompt` builder method

## Implementation Notes
- Extracted `format_permission_prompt()` and `parse_permission_input()` as public pure functions for testability and reuse.
- Added `PendingResponses` type alias (`Arc<Mutex<HashMap<String, PermissionResponse>>>`) for ergonomic use across modules.
- Used `std::sync::Mutex<Option<broadcast::Receiver<...>>>` to allow taking the receiver from `&self` in the `Channel::run` method (which takes `&self`, not `&mut self`).
- The `with_permission_prompt` builder method follows the existing builder pattern (`with_session_id`, `with_cwd`) in `PermissionGuard`.
- `run_permission_prompt_loop` is a standalone async function spawned via `tokio::spawn`, ensuring it runs concurrently without blocking the REPL loop.
- Unrecognized input re-displays the prompt in a loop without crashing or exiting.
- Handles broadcast lag gracefully with a `tracing::warn!` and continues.

## Acceptance Criteria
- [x] AC 1: When a `PermissionPromptRequest` is received, the terminal prints a prompt in the format `? <tool>: <args>\n  [Y]es  [N]o  [A]lways` - Implemented via `format_permission_prompt()` and the `run_permission_prompt_loop` function.
- [x] AC 2: Typing `y` or `Y` (then Enter) writes `PermissionResponse::Yes` into `pending_responses` for the request ID - Handled by `parse_permission_input()` and map insertion in the prompt loop.
- [x] AC 3: Typing `n` or `N` (then Enter) writes `PermissionResponse::No` into `pending_responses` - Same mechanism as AC 2.
- [x] AC 4: Typing `a` or `A` (then Enter) writes `PermissionResponse::Always` into `pending_responses` - Same mechanism as AC 2.
- [x] AC 5: Unrecognized input re-displays the prompt without crashing - Inner loop in `run_permission_prompt_loop` re-iterates on `None` from `parse_permission_input`.
- [x] AC 6: The prompt subscriber task does not block the main REPL input loop (runs as a concurrent `tokio::spawn`) - Spawned via `tokio::spawn(run_permission_prompt_loop(...))` at the start of `Channel::run`.
- [x] AC 7: `cargo build` succeeds with no warnings - Verified.

## Test Results
- Lint: PASS (`cargo clippy --lib -- -D warnings`)
- Tests: PASS (55 total, 17 new in `channels::terminal::tests`)
- Build: PASS (`cargo build` with no warnings)
- Format: PASS (`cargo fmt --check`)
- New tests added:
  - `src/channels/terminal.rs` - 14 new tests:
    - `terminal_channel_with_permission_prompt_stores_pending`
    - `format_prompt_contains_tool_name_and_args`
    - `format_prompt_contains_response_options`
    - `format_prompt_has_correct_layout`
    - `parse_yes_lowercase`, `parse_yes_uppercase`
    - `parse_no_lowercase`, `parse_no_uppercase`
    - `parse_always_lowercase`, `parse_always_uppercase`
    - `parse_unrecognized_returns_none`
    - `parse_trims_whitespace`
    - `prompt_response_written_to_pending_map`
    - `prompt_response_always_written_to_pending_map`
    - `prompt_response_no_written_to_pending_map`

## Concerns / Blockers
- The `PermissionPromptRequest` struct has `tool_input` (not `tool_args` as stated in the prior work summary). Used the actual field name from the code.
- The `PermissionPromptRequest` does not have a `description` field (contrary to the prior work summary which listed `description`). The prompt format uses `tool_name` and `tool_input` directly.
- Both the REPL loop and the permission prompt loop read from stdin via `spawn_blocking`. In practice this means they share a single stdin stream. If a permission prompt arrives while the user is typing a REPL command, the input may be consumed by either reader. This is inherent to sharing stdin and would require a more sophisticated input multiplexing approach to resolve (out of scope for this ticket).
