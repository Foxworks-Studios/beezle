# Code Review: Ticket 2 -- PermissionPolicy -- Settings Loading and check()

**Ticket:** 2 -- PermissionPolicy -- Settings Loading and check()
**Impl Report:** /home/travis/Development/beezle/prd/007-permissions-system-reports/ticket-02-impl.md
**Date:** 2026-03-07 14:30
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `load(cwd)` reads all three tiers; missing files silently ignored | Met | `read_settings` returns `None` on I/O error; `load_missing_files_returns_empty_policy` test confirms |
| 2 | Malformed JSON emits `tracing::warn!` and skips tier | Met | `read_settings` lines 161-163 log warning; `load_malformed_json_skips_tier` test confirms |
| 3 | Allow/deny lists from all tiers are unioned | Met | Loop appends rules from each tier; `three_tiers_merge_correctly` test verifies 3 allow + 2 deny |
| 4 | `check("bash", &args)` returns Allow when allow rule matches, no deny | Met | `check_allow_when_allow_rule_matches` test confirms |
| 5 | `check("bash", &args)` returns Deny when deny matches even with allow | Met | Resolution order checks deny before allow (lines 186-189); `check_deny_overrides_allow` test confirms |
| 6 | `check("bash", &args)` returns Ask when no rule matches, category=Ask | Met | Falls through to `category_default` (line 200); `check_ask_when_no_rule_matches` test confirms |
| 7 | `check("read_file", &args)` returns Allow (Read category default) | Met | `category_default` maps Read to Allow (line 227); `check_read_defaults_to_allow` test confirms |
| 8 | `grant_session()` persists across subsequent checks | Met | Pushes to `session_grants` vec; `session_grant_survives_multiple_checks` test confirms 3 consecutive Allows |
| 9 | Unit test: three tiers merge correctly | Met | `three_tiers_merge_correctly` test |
| 10 | Unit test: deny-over-allow precedence | Met | `check_deny_overrides_allow` test |
| 11 | Unit test: session grant survives multiple checks | Met | `session_grant_survives_multiple_checks` test |
| 12 | `cargo test` passes; clippy clean | Met | Verified: 55 tests pass, `cargo clippy --lib -p beezle -- -D warnings` produces no warnings |

## Issues Found

### Critical (must fix before merge)
- None

### Major (should fix, risk of downstream problems)
- None

### Minor (nice to fix, not blocking)
- **Chained `unwrap_or` in `simple_regex_match` (line 388-392):** The pattern `strip_prefix('^').unwrap_or(pattern).strip_suffix('$').unwrap_or(pattern)` has a subtle bug: if `strip_prefix` succeeds but `strip_suffix` fails, `unwrap_or(pattern)` falls back to the original `pattern` (with `^` still present), silently discarding the prefix strip result. This does not manifest currently because `glob_matches` always adds both anchors, but it is a latent bug. The fix is: `let pat = pattern.strip_prefix('^').unwrap_or(pattern); let pat = pat.strip_suffix('$').unwrap_or(pat);` (bind each step to a new variable).
- **Impl report claims `pub mod hooks;` was removed**, but line 8 still contains it. The report is inconsistent with the code. Since `hooks.rs` exists (from Ticket 3) and compilation succeeds, the code is correct -- the report is just inaccurate.

## Suggestions (non-blocking)
- `session_grant` stores the exact primary arg as the pattern, so `grant_session("bash", {"command": "cargo test"})` will only match the exact command `"cargo test"`, not `"cargo test --release"`. This is a reasonable design choice for session grants (exact match = most restrictive), but worth documenting explicitly so downstream ticket implementers (Ticket 4 PermissionGuard) understand the matching semantics.
- The `extract_primary_arg` and `rule_matches` helper functions are `pub(crate)` by default (private to crate). They could benefit from `#[cfg(test)]` visibility if they need testing independently, but for now the indirect coverage through `check()` tests is sufficient.

## Scope Check
- Files within scope: YES -- only `src/permissions/mod.rs` was modified
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- all 55 existing tests continue to pass; new code is additive
- Security concerns: NONE -- permission policy correctly enforces deny-over-allow precedence
- Performance concerns: NONE -- rule matching is linear over small lists, no I/O in hot paths
