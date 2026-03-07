# Code Review: Ticket 1 -- Core Permission Types, `parse_rule()`, and `pattern_matches()`

**Ticket:** 1 -- Core Permission Types, `parse_rule()`, and `pattern_matches()`
**Impl Report:** prd/007-permissions-system-reports/ticket-01-impl.md
**Date:** 2026-03-07 13:00
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `src/permissions/mod.rs` compiles with all public types and `PermissionError` using `thiserror` | Met | All five types present (`PermissionRule`, `ToolCategory`, `PermissionVerdict`, `PermissionResponse`, `PermissionError`). `PermissionError` derives `thiserror::Error`. Confirmed via `cargo clippy --lib`. |
| 2 | `parse_rule("Bash(cargo test:*)")` returns correct `PermissionRule` | Met | Test `parse_rule_bash_with_pattern` at line 244 verifies. |
| 3 | `parse_rule("Read()")` returns Ok with empty pattern | Met | Test `parse_rule_empty_pattern` at line 251 verifies. |
| 4 | `parse_rule("NoParen")` returns `Err(PermissionError::InvalidRule(...))` | Met | Test `parse_rule_missing_parens` at line 258 verifies. |
| 5 | `pattern_matches("cargo test:*", "cargo test --release")` returns `true` | Met | Test `prefix_match_with_colon_star` at line 268 verifies. |
| 6 | `pattern_matches("cargo test:*", "cargo fmt")` returns `false` | Met | Test `prefix_match_no_match` at line 273 verifies. |
| 7 | `pattern_matches("/src/**", "/src/main.rs")` returns `true` | Met | Test `recursive_glob_matches` at line 278 verifies. |
| 8 | `pattern_matches("/src/**", "/tests/foo.rs")` returns `false` | Met | Test `recursive_glob_no_match` at line 283 verifies. |
| 9 | `pattern_matches("/src/*.rs", "/src/main.rs")` returns `true` | Met | Test `single_segment_glob_matches` at line 288 verifies. |
| 10 | `pattern_matches("/src/*.rs", "/src/nested/main.rs")` returns `false` | Met | Test `single_segment_glob_no_nested` at line 293 verifies. |
| 11 | `pattern_matches("domain:docs.rs", "https://docs.rs/tokio")` returns `true` | Met | Test `domain_match` at line 298 verifies. |
| 12 | `pattern_matches("domain:docs.rs", "https://crates.io/tokio")` returns `false` | Met | Test `domain_no_match` at line 303 verifies. |
| 13 | `pattern_matches("*", "anything")` returns `true` | Met | Test `bare_wildcard_matches_anything` at line 311 verifies. |
| 14 | Unit tests pass, clippy clean | Met | 12/12 tests pass. `cargo clippy --lib -p beezle -- -D warnings` clean. |

## Issues Found

### Critical (must fix before merge)
None.

### Major (should fix, risk of downstream problems)
None.

### Minor (nice to fix, not blocking)

1. **Latent `unwrap_or` bug in `simple_regex_match`** (`src/permissions/mod.rs:175-179`). The chained strip logic uses `unwrap_or(pattern)` where `pattern` is the function parameter. If `strip_prefix('^')` succeeds but `strip_suffix('$')` fails, `unwrap_or` returns the original `pattern` (with `^` still present), discarding the prefix strip result. This is currently unreachable because `glob_matches` always adds both anchors, but it would be a real bug if this function were ever called with a pattern that has `^` but not `$`. Fix: bind the intermediate result to a variable (`let pat = pattern.strip_prefix('^').unwrap_or(pattern); let pat = pat.strip_suffix('$').unwrap_or(pat);`).

2. **Non-ASCII input could panic in `match_recursive`** (`src/permissions/mod.rs:218-234`). The function uses `as_bytes()[0]` and `&pattern[1..]` / `&value[1..]` for character-by-character matching. Slicing at byte index 1 on a multi-byte UTF-8 character would panic. In practice, file paths and tool names are overwhelmingly ASCII, so this is low risk, but it violates Rust safety expectations. Using `.chars().next()` and `char_indices` would be more robust.

## Suggestions (non-blocking)

- The `parse_rule` function doesn't validate that the tool name contains only reasonable characters (e.g., alphanumeric). A rule like `"()()"` would parse as tool=`"("`, pattern=`""`. This may be fine if downstream validation exists, but worth considering.
- `ToolCategory` is defined but unused in this ticket. This is expected since Ticket 2 will use it for default category policies -- no concern here.

## Scope Check
- Files within scope: YES -- only `src/permissions/mod.rs` (created) and `src/lib.rs` (modified) were touched.
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- new module with no changes to existing functionality. The `pub mod permissions;` addition to `lib.rs` is purely additive.
- Security concerns: NONE
- Performance concerns: NONE -- the recursive matcher is O(n*m) worst case but patterns and values are short strings in practice.
