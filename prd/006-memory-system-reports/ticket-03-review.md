# Code Review: Ticket 3 -- MemoryReadTool and MemoryWriteTool

**Ticket:** 3 -- MemoryReadTool and MemoryWriteTool -- yoagent Tool Implementations
**Impl Report:** prd/006-memory-system-reports/ticket-03-impl.md
**Date:** 2026-03-05 15:00
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | MemoryReadTool JSON schema with target enum, name "memory_read" | Met | `parameters_schema()` at line 49 returns correct schema; `name()` returns `"memory_read"` at line 37. Verified by `read_tool_name_and_schema` test. |
| 2 | MemoryReadTool::execute reads long_term or daily | Met | Match on target at line 78: `"long_term"` calls `store.read_long_term()`, `"daily"` calls `store.today()` then `store.read_daily(date)`. Both return `Content::Text`. |
| 3 | MemoryWriteTool JSON schema with target and content, name "memory_write" | Met | `parameters_schema()` at line 138; both fields required. `name()` returns `"memory_write"` at line 126. Verified by `write_tool_name_and_schema` test. |
| 4 | MemoryWriteTool::execute dispatches daily/long_term correctly | Met | Match at line 180: `"daily"` calls `store.append_daily(content)`, `"long_term"` calls `store.write_long_term(content)`. |
| 5 | Both tools return descriptive success messages | Met | Write tool returns `"Appended to daily notes."` (line 185) and `"Long-term memory updated."` (line 191). Read tool returns raw content (correct -- success is the content itself). |
| 6 | Map MemoryError to ToolError::Failed (ticket says "ExecutionFailed") | Met | All `.map_err(\|e\| ToolError::Failed(e.to_string()))` calls at lines 82, 87, 184, 190. Ticket spec says `ToolError::ExecutionFailed` but `ToolError::Failed` is the actual yoagent variant -- correct adaptation. |
| 7 | MemoryReadTool returns InvalidArgs for missing/invalid target | Met | Missing: line 71 `ok_or_else`. Invalid variant: line 89 match arm. Both verified by tests. |
| 8 | MemoryWriteTool returns InvalidArgs for missing target/content | Met | Missing target: line 164. Missing content: line 174. Both verified by tests. |
| 9 | Test: read long_term with seeded MEMORY.md returns "prior facts" | Met | `read_long_term_returns_file_content` test at line 269. |
| 10 | Test: write daily then read daily contains "standup note" | Met | `write_daily_then_read_contains_text` test at line 337. |
| 11 | Test: write long_term then read returns "new facts" | Met | `write_long_term_replaces_content` test at line 364. |
| 12 | Test: read with empty params returns InvalidArgs | Met | `read_missing_target_returns_invalid_args` test at line 302. |
| 13 | Test: write with missing content returns InvalidArgs | Met | `write_missing_content_returns_invalid_args` test at line 383. |
| 14 | Quality gates pass | Met | `cargo test` (10/10 pass), `cargo clippy -- -D warnings` (clean), `cargo fmt --check` (clean). |

## Issues Found

### Critical (must fix before merge)
- None.

### Major (should fix, risk of downstream problems)
- None.

### Minor (nice to fix, not blocking)
- **Duplicated FakeClock**: `src/tools/memory.rs` test module defines its own `FakeClock` (line 221) that is identical to the one in `src/memory/mod.rs` tests (line 191). The impl report acknowledges this was intentional to avoid modifying `mod.rs` exports. Acceptable since the memory module's `FakeClock` is inside `#[cfg(test)]` and not easily re-exported without a `test_utils` feature or `pub(crate)` test helper module. Low priority.

## Suggestions (non-blocking)
- Consider a shared `test_utils` module (e.g., `src/test_utils.rs` behind `#[cfg(test)]`) to deduplicate `FakeClock`, `fixed_time()`, and `test_store()` if more tool modules need them.
- The `text_of` helper (line 259) indexes `result.content[0]` without bounds checking. Safe in tests but could use `.first().expect("empty content")` for a clearer panic message.

## Scope Check
- Files within scope: YES -- `src/tools/memory.rs` (created/replaced), `src/tools/mod.rs` (already had `pub mod memory;` from Ticket 2, not re-modified).
- Out-of-scope modification: `src/memory/mod.rs` gained `pub fn today()` (3 lines). The implementer documented this as a necessary minimal addition since `clock` is private. The method is a thin accessor, non-breaking, and will be used by production code in Ticket 4. Acceptable.
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- all 10 new tests pass; existing 64 lib tests unaffected.
- Security concerns: NONE
- Performance concerns: NONE
