# Code Review: Ticket 4 -- Prompt Injection -- Load Memory into System Prompt on Startup

**Ticket:** 4 -- Prompt Injection -- Load Memory into System Prompt on Startup
**Impl Report:** prd/006-memory-system-reports/ticket-04-impl.md
**Date:** 2026-03-05 13:00
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `load_memory_store()` builds MemoryStore from ~/.beezle/memory/ with SystemClock; warns if home_dir is None | Met | Lines 437-450: constructs path via `dirs::home_dir().join(".beezle").join("memory")`, uses `Arc::new(SystemClock)`, logs `tracing::warn!` and returns `None` on failure. |
| 2 | MEMORY.md content appended under `## Persistent Memory` section header | Met | Lines 752-771: `build_effective_system_prompt` appends `\n\n## Persistent Memory\n{content}` when non-empty. Called at line 1011. |
| 3 | Content truncated to 4000 chars with `[truncated]` suffix | Met | Lines 757-765: truncation logic fires when content exceeds limit, appends `[truncated]`. See Minor issue about byte-vs-char semantics. |
| 4 | MemoryReadTool and MemoryWriteTool registered in build_agent when memory_store is Some | Met | Lines 510-518: both tools constructed from shared `Arc<MemoryStore>`, wrapped in `ToolWrapper`. |
| 5 | build_agent accepts `memory_store: Option<Arc<MemoryStore>>` parameter | Met | Line 463: parameter added. `/model` caller passes `None` (line 921), main() caller passes the loaded store (line 1020). |
| 6 | Test: empty memory returns base | Met | Lines 1891-1896: `build_effective_system_prompt(base, "")` asserts `result == base`. |
| 7 | Test: over-limit content produces `[truncated]` and bounded length | Met | Lines 1908-1922: uses 5000-char input, asserts `[truncated]` present and content <= 4011 bytes. |
| 8 | Test: under-limit content appended with header | Met | Lines 1898-1906: asserts result starts with base, contains header, contains memory content. |
| 9 | Quality gates pass | Met | Verified: `cargo test` 55 bin + 74 lib tests pass, `cargo clippy -- -D warnings` clean, build zero warnings. |

## Issues Found

### Critical (must fix before merge)
- None.

### Major (should fix, risk of downstream problems)
- None.

### Minor (nice to fix, not blocking)

1. **Byte-vs-character truncation mismatch** (`src/main.rs:757-764`): The gate condition `memory_content.len() > MEMORY_MAX_CHARS` uses byte length, and the `take_while(|(i, _)| *i < MEMORY_MAX_CHARS)` loop compares byte indices. This means the 4000 limit is on bytes, not characters. For multi-byte UTF-8 content, the truncated output could be fewer than 4000 characters. The existing `truncate()` helper at line 532 correctly counts characters via `char_indices().nth(max)`. Consider either reusing that helper or documenting that the limit is intentionally byte-based.

2. **`unwrap_or(MEMORY_MAX_CHARS)` fallback** (`src/main.rs:764`): The `.unwrap_or(MEMORY_MAX_CHARS)` on the `last()` call would only fire if `char_indices().take_while(...)` yields zero elements, which means the first character's byte index is already >= 4000. In that case, slicing at byte offset 4000 would be a panic if it falls inside a multi-byte character. In practice this is unreachable for valid UTF-8 (a single char can be at most 4 bytes), but the fallback is logically suspect. The `unwrap_or(0)` would be safer for the impossible case.

3. **`/model` command loses memory tools** (`src/main.rs:921`): Passing `None` for `memory_store` when rebuilding the agent on `/model` switch drops `MemoryReadTool` and `MemoryWriteTool`. The impl report acknowledges this as a known limitation. A follow-up ticket should thread the `Arc<MemoryStore>` through to `handle_slash_command`.

## Suggestions (non-blocking)

- The `build_effective_system_prompt` function duplicates truncation logic that already exists in `truncate()` at line 532. Consider reusing that helper, adding the `[truncated]` suffix afterward. This would also fix Minor #1 (byte-vs-char) since `truncate` counts characters correctly.
- The truncation test (line 1911) uses ASCII-only input (`"x".repeat(5000)`). Adding a multi-byte test case (e.g., repeating a 2-byte char) would catch the byte/char discrepancy if the behavior is ever relied upon.

## Scope Check
- Files within scope: YES -- only `src/main.rs` modified, as specified.
- Scope creep detected: NO
- Unauthorized dependencies added: NO

## Risk Assessment
- Regression risk: LOW -- the only structural change is the new `memory_store` parameter on `build_agent`, and all callers are updated. Existing tests pass.
- Security concerns: NONE -- memory content comes from the user's own filesystem.
- Performance concerns: NONE -- `read_long_term()` is a single file read at startup; `char_indices()` iteration is O(n) but bounded by 4000.
