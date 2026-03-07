# Code Review: Ticket 5 -- Terminal Permission Prompt Display and Response

**Ticket:** 5 -- Terminal Permission Prompt Display and Response
**Impl Report:** prd/007-permissions-system-reports/ticket-05-impl.md
**Date:** 2026-03-07 12:00
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | Prompt format `? <tool>: <args>\n  [Y]es  [N]o  [A]lways` | Met | `format_permission_prompt()` at line 30-33 produces the correct format. Tests at lines 282-303 verify layout, tool name, args, and response options. |
| 2 | y/Y writes `PermissionResponse::Yes` | Met | `parse_permission_input` handles both cases (line 41). Tests at lines 308-315. Response is inserted into `pending_responses` map in prompt loop (line 238-239). |
| 3 | n/N writes `PermissionResponse::No` | Met | Same mechanism, line 42. Tests at lines 318-325. |
| 4 | a/A writes `PermissionResponse::Always` | Met | Same mechanism, line 43. Tests at lines 329-341. |
| 5 | Unrecognized input re-displays without crashing | Met | Inner loop at line 214 re-iterates when `parse_permission_input` returns `None` (line 237-241). Test at lines 344-349 verifies None return. |
| 6 | Prompt subscriber runs concurrently via tokio::spawn | Met | `tokio::spawn(run_permission_prompt_loop(...))` at line 122. The receiver is taken from the `std::sync::Mutex<Option<...>>` at line 119, ensuring it only spawns once. |
| 7 | `cargo build` succeeds with no warnings | Met | Verified: `cargo clippy --lib -p beezle -- -D warnings` passes clean. 147 tests pass. |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)

1. **Weak integration tests for pending_responses insertion (lines 366-401):** The three async tests (`prompt_response_written_to_pending_map`, `prompt_response_always_written_to_pending_map`, `prompt_response_no_written_to_pending_map`) only test that `HashMap::insert` works on an `Arc<Mutex<HashMap>>`. They do not exercise `run_permission_prompt_loop` or any code path unique to this module. They are effectively testing std/tokio primitives. Given the difficulty of testing stdin-reading code in unit tests, this is understandable, but worth noting that the actual prompt loop integration is untested.

2. **Shared stdin contention (acknowledged in impl report):** Both the REPL loop and the permission prompt loop call `spawn_blocking` to read from stdin. This means permission prompts and REPL input compete for the same stdin stream. The implementer correctly flags this as out of scope, but it will need addressing before the UX is production-ready.

3. **Impl report claims 17 new tests but lists 14:** The report says "55 total, 17 new" but only lists 14 test names. The actual file has 17 tests in the `tests` module (3 pre-existing + 14 new). The count of 17 new appears to be a miscount -- there are 14 new tests. This is cosmetic but worth correcting in the report.

## Suggestions (non-blocking)

- Consider adding a `#[must_use]` attribute to `format_permission_prompt` and `parse_permission_input` since discarding their return values would always be a bug.
- The `PendingResponses` type alias uses `tokio::sync::Mutex`. This is appropriate for use in the async prompt loop, but note that `PermissionGuard` (ticket 4) presumably also uses `tokio::sync::Mutex` for this map -- worth confirming consistency across modules.

## Scope Check
- Files within scope: YES -- only `src/channels/terminal.rs` was modified
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- Changes are additive (new builder method, new free functions, new spawned task). The existing REPL loop behavior is unchanged when `with_permission_prompt` is not called.
- Security concerns: NONE
- Performance concerns: NONE -- The spawned task only runs when permission prompts are configured and blocks on stdin read, which is expected behavior for an interactive terminal.
