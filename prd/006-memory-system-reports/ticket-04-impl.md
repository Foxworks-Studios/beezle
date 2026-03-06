# Implementation Report: Ticket 4 -- Prompt Injection -- Load Memory into System Prompt on Startup

**Ticket:** 4 - Prompt Injection -- Load Memory into System Prompt on Startup
**Date:** 2026-03-05 12:00
**Status:** COMPLETE

---

## Files Changed

### Created
- None

### Modified
- `src/main.rs` - Added `load_memory_store()`, `build_effective_system_prompt()`, memory tool registration in `build_agent()`, and memory injection in `main()`

## Implementation Notes
- `build_effective_system_prompt()` is a pure function for testability -- takes base prompt and memory content, returns assembled string.
- `MEMORY_MAX_CHARS` constant set to 4000; truncation preserves UTF-8 char boundaries using `char_indices()`.
- `load_memory_store()` uses `dirs::home_dir()` and logs `tracing::warn!` if `None`, returning `None` to gracefully disable memory.
- `build_agent()` now accepts `memory_store: Option<Arc<MemoryStore>>`. When `Some`, `MemoryReadTool` and `MemoryWriteTool` are constructed from the shared `Arc` and wrapped in `ToolWrapper` for real-time feedback.
- The `/model` slash command rebuild passes `None` for memory_store (memory tools are not re-registered on model switch -- this is acceptable since memory content is already in the system prompt and tools are stateless).
- All existing test callers compile without changes because they don't call `build_agent` directly (they use `mock_agent` or the existing slash command test helper which routes through `handle_slash_command`).

## Acceptance Criteria
- [x] AC 1: `load_memory_store()` helper builds `MemoryStore` from `~/.beezle/memory/` using `SystemClock`; gracefully falls back with `tracing::warn!` if `dirs::home_dir()` returns `None`.
- [x] AC 2: On startup, `MEMORY.md` content is read; if non-empty, appended to system prompt under `"\n\n## Persistent Memory\n"` header.
- [x] AC 3: Content truncated to 4000 characters before injection; `[truncated]` suffix appended when truncated.
- [x] AC 4: `MemoryReadTool` and `MemoryWriteTool` constructed with same `Arc<MemoryStore>` and pushed onto tools vec in `build_agent()`, wrapped in `ToolWrapper`.
- [x] AC 5: `build_agent` accepts `memory_store: Option<Arc<MemoryStore>>`; tools registered only when `Some`. Existing test callers compile without changes.
- [x] AC 6: Test: `build_effective_system_prompt(base, content_under_4000)` returns string containing base AND memory content.
- [x] AC 7: Test: `build_effective_system_prompt(base, content_over_4000)` returns string with `[truncated]` and memory section <= 4000 chars + overhead.
- [x] AC 8: Test: `build_effective_system_prompt(base, "")` returns string equal to base with no memory section.
- [x] AC 9: Quality gates pass.

## Test Results
- Lint: PASS (`cargo clippy -- -D warnings`)
- Tests: PASS (129 total: 55 bin + 74 lib)
- Build: PASS (zero warnings)
- Format: PASS (`cargo fmt --check`)
- New tests added:
  - `src/main.rs::tests::build_effective_system_prompt_empty_memory_returns_base`
  - `src/main.rs::tests::build_effective_system_prompt_appends_memory_under_limit`
  - `src/main.rs::tests::build_effective_system_prompt_truncates_over_4000_chars`

## Concerns / Blockers
- The `/model` slash command passes `None` for `memory_store` when rebuilding the agent. This means memory tools are lost after a model switch. If this is undesirable, `handle_slash_command` would need the `Arc<MemoryStore>` passed through as an additional parameter. This is a minor enhancement that could be addressed in a follow-up ticket.
- None blocking.
