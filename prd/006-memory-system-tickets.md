# Tickets for PRD 006: Persistent Memory System

**Source PRD:** prd/006-memory-system.md
**Created:** 2026-03-05
**Total Tickets:** 5
**Estimated Total Complexity:** 11 (S=1, M=2, L=3 → 1+2+3+2+3=11)

---

### Ticket 1: MemoryStore — Core Types, Error, and File I/O

**Description:**
Create `src/memory/mod.rs` with the `MemoryStore` struct, `MemoryError` error type, and
`Clock` trait for injectable time. Implement all file-system operations: create the memory
directory, read long-term (`MEMORY.md`) and daily (`YYYY-MM-DD.md`) files, append timestamped
entries to daily notes, and fully replace long-term memory. This is the bedrock everything else
builds on; no UI or tool wiring yet.

**Scope:**
- Create: `src/memory/mod.rs`

**Acceptance Criteria:**
- [ ] `MemoryError` is defined with `thiserror` and covers at least `Io(#[from] std::io::Error)` and `HomeNotFound` variants; derives `Debug`.
- [ ] `Clock` trait has a single method `now() -> chrono::DateTime<chrono::Local>` (or `time::OffsetDateTime` — pick one and be consistent); a `SystemClock` unit struct implements it; a `FakeClock(DateTime)` implements it for tests.
- [ ] `MemoryStore` holds a `memory_dir: PathBuf` and a `clock: Arc<dyn Clock>` (or generic `C: Clock`); provides a `new(memory_dir, clock)` constructor.
- [ ] `MemoryStore::memory_dir()` returns the root path; creates the directory (and `MEMORY.md` if absent) lazily on first write, NOT on construction.
- [ ] `MemoryStore::read_long_term() -> Result<String, MemoryError>` returns the contents of `MEMORY.md`, or an empty string if the file does not yet exist.
- [ ] `MemoryStore::read_daily(date) -> Result<String, MemoryError>` returns the contents of `YYYY-MM-DD.md` for the given date, or empty string if absent.
- [ ] `MemoryStore::append_daily(text: &str) -> Result<(), MemoryError>` appends `\n## HH:MM\n{text}\n` to today's daily file, creating it if needed.
- [ ] `MemoryStore::write_long_term(content: &str) -> Result<(), MemoryError>` atomically replaces `MEMORY.md` with the given content (write to temp file, then rename).
- [ ] Test: `FakeClock` set to `2026-03-05T14:30:00` → `append_daily("hello")` → file `2026-03-05.md` exists in temp dir and contains `## 14:30` and `hello`.
- [ ] Test: `read_long_term()` on a fresh temp dir → returns `""` (no file yet, no error).
- [ ] Test: `write_long_term("facts")` → `read_long_term()` returns `"facts"`.
- [ ] Test: `append_daily` called twice → file contains both `## HH:MM` sections in order.
- [ ] Test: `read_daily` for a date with no file → returns `""` (no error).
- [ ] Quality gates pass (`cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo test`).

**Dependencies:** None
**Complexity:** M
**Maps to PRD AC:** AC 3 (daily notes), AC 5 (persistence), AC 6 (unit tests with temp dirs and fake clock)

---

### Ticket 2: Module Wiring — Register `memory` and `tools` in `lib.rs`

**Description:**
Add `pub mod memory;` and `pub mod tools;` to `src/lib.rs`, and create the `src/tools/`
directory with a `mod.rs` that will host memory tool re-exports. This ticket is purely
structural scaffolding so that later tickets can reference `beezle::memory` and
`beezle::tools::memory` without circular-dependency issues.

**Scope:**
- Modify: `src/lib.rs`
- Create: `src/tools/mod.rs`

**Acceptance Criteria:**
- [ ] `src/lib.rs` has `pub mod memory;` and `pub mod tools;` added alongside existing module declarations.
- [ ] `src/tools/mod.rs` exists and declares `pub mod memory;` (referencing `src/tools/memory.rs` which will be created in Ticket 3). Use a stub placeholder (`// memory tool implementations live here`) so it compiles before Ticket 3 lands; then update it in Ticket 3.
- [ ] Test: `cargo build` succeeds with the new module declarations and stub file — no unused import warnings.
- [ ] Test: `cargo test` still passes all existing tests after the structural addition.
- [ ] Quality gates pass.

**Dependencies:** Ticket 1
**Complexity:** S
**Maps to PRD AC:** (structural prerequisite — enables AC 1 and AC 2)

---

### Ticket 3: `MemoryReadTool` and `MemoryWriteTool` — yoagent Tool Implementations

**Description:**
Create `src/tools/memory.rs` with two tools that implement `yoagent::AgentTool`:
`MemoryReadTool` (reads long-term or today's daily notes) and `MemoryWriteTool`
(appends to today's daily notes OR replaces long-term memory). Both tools hold an
`Arc<MemoryStore>` so they share a single store instance. Use `serde_json` for
parameter deserialization and `thiserror`-derived errors mapped to `ToolError`.

**Scope:**
- Create: `src/tools/memory.rs`
- Modify: `src/tools/mod.rs` (uncomment/update `pub mod memory;`)

**Acceptance Criteria:**
- [ ] `MemoryReadTool` has JSON schema `{ "target": { "enum": ["long_term", "daily"] } }` with `target` required; name is `"memory_read"`.
- [ ] `MemoryReadTool::execute` with `target = "long_term"` returns the content of `MEMORY.md` as a `ToolResult` with `Content::Text`; with `target = "daily"` returns today's daily note content.
- [ ] `MemoryWriteTool` has JSON schema `{ "target": { "enum": ["long_term", "daily"] }, "content": { "type": "string" } }` both required; name is `"memory_write"`.
- [ ] `MemoryWriteTool::execute` with `target = "daily"` calls `store.append_daily(content)`; with `target = "long_term"` calls `store.write_long_term(content)`.
- [ ] Both tools return a descriptive success message in `ToolResult::text` on success (e.g. `"Appended to daily notes."`, `"Long-term memory updated."`).
- [ ] Both tools map `MemoryError` to `ToolError::ExecutionFailed(msg)` — no panics or unwraps.
- [ ] `MemoryReadTool` returns `ToolError::InvalidArgs` when `target` field is missing or not a valid enum variant.
- [ ] `MemoryWriteTool` returns `ToolError::InvalidArgs` when `target` or `content` field is missing.
- [ ] Test: `MemoryReadTool::execute({"target":"long_term"})` on a store backed by temp dir containing `MEMORY.md` with `"prior facts"` → `ToolResult` text is `"prior facts"`.
- [ ] Test: `MemoryWriteTool::execute({"target":"daily","content":"standup note"})` → subsequent `MemoryReadTool::execute({"target":"daily"})` returns text containing `"standup note"`.
- [ ] Test: `MemoryWriteTool::execute({"target":"long_term","content":"new facts"})` → subsequent `read_long_term()` on the same store returns `"new facts"`.
- [ ] Test: `MemoryReadTool::execute({})` → `Err(ToolError::InvalidArgs(_))`.
- [ ] Test: `MemoryWriteTool::execute({"target":"daily"})` (missing `content`) → `Err(ToolError::InvalidArgs(_))`.
- [ ] Quality gates pass.

**Dependencies:** Ticket 1, Ticket 2
**Complexity:** M
**Maps to PRD AC:** AC 1 (agent can call `memory_write`), AC 2 (`memory_read` returns file contents)

---

### Ticket 4: Prompt Injection — Load Memory into System Prompt on Startup

**Description:**
Modify `src/main.rs` to load `MemoryStore` at startup (using `~/.beezle/memory/` from
`dirs::home_dir()`), read `MEMORY.md`, truncate to 4000 chars if large, and append it
to `SYSTEM_PROMPT` before constructing the agent. Also register `MemoryReadTool` and
`MemoryWriteTool` in the tool list inside `build_agent()`. The `MemoryStore` is wrapped
in `Arc` and shared between both tools.

**Scope:**
- Modify: `src/main.rs`

**Acceptance Criteria:**
- [ ] A `load_memory_store()` helper function (in `main.rs` or `src/memory/mod.rs`) builds a `MemoryStore` from `~/.beezle/memory/`, using `SystemClock`; gracefully falls back to a no-op (logs warning via `tracing::warn!`) if `dirs::home_dir()` returns `None`.
- [ ] On startup, `MEMORY.md` content is read; if non-empty, it is appended to the system prompt under a section header (`"\n\n## Persistent Memory\n"` + content).
- [ ] Content is truncated to 4000 characters before injection; if truncated, a `[truncated]` suffix is appended so the agent knows the memory is incomplete.
- [ ] `MemoryReadTool` and `MemoryWriteTool` are constructed with the same `Arc<MemoryStore>` and pushed onto the tools vec inside `build_agent()` (wrapped in `ToolWrapper` for real-time feedback).
- [ ] The `build_agent` signature accepts a new `memory_store: Option<Arc<MemoryStore>>` parameter and constructs/registers the tools only when `Some`. Existing callers with `None` compile without changes.
- [ ] Test: `build_effective_system_prompt(base, memory_content_under_4000)` → returned string contains the base prompt AND the memory content.
- [ ] Test: `build_effective_system_prompt(base, memory_content_over_4000)` → returned string contains `[truncated]` and is not longer than `base.len() + 4000 + overhead`.
- [ ] Test: `build_effective_system_prompt(base, "")` → returned string equals `base` with no memory section appended.
- [ ] Quality gates pass.

**Dependencies:** Ticket 1, Ticket 2, Ticket 3
**Complexity:** L
**Maps to PRD AC:** AC 3 (`MEMORY.md` appears in system prompt), AC 4 (daily notes auto-created), AC 5 (persistence across sessions)

---

### Ticket 5: Verification and Integration

**Description:**
Run the full PRD-006 acceptance criteria checklist. Verify that all tickets integrate
correctly: the memory store persists to disk, the tools are callable end-to-end through
the yoagent tool contract, and the system prompt injection works in combination with
the existing agent bootstrap. Confirm no regressions.

**Scope:**
- Modify: none (read-only verification; fix any integration issues found)

**Acceptance Criteria:**
- [ ] `cargo test` passes with all new tests in `src/memory/mod.rs` and `src/tools/memory.rs`.
- [ ] `cargo build` succeeds with zero warnings.
- [ ] `cargo clippy -- -D warnings` passes.
- [ ] `cargo fmt --check` passes.
- [ ] All 6 PRD ACs are verified:
  - [ ] AC 1: `MemoryWriteTool` can be called with `target=daily` and writes to `~/.beezle/memory/YYYY-MM-DD.md`.
  - [ ] AC 2: `MemoryReadTool` returns the contents of the requested file.
  - [ ] AC 3: Starting the agent with a pre-existing `MEMORY.md` results in its content appearing in the active system prompt (verified by inspecting `build_effective_system_prompt` return value in tests).
  - [ ] AC 4: First call to `append_daily` auto-creates `~/.beezle/memory/YYYY-MM-DD.md` with today's date.
  - [ ] AC 5: Write a value via `write_long_term`, drop the `MemoryStore`, reconstruct it pointing at the same dir, and `read_long_term` returns the same value.
  - [ ] AC 6: All tests use `tempfile::TempDir` for isolation and `FakeClock` for deterministic timestamps -- no tests write to the real `~/.beezle/` directory.
- [ ] No regressions in existing tests from PRDs 001-005.

**Dependencies:** Tickets 1, 2, 3, 4
**Complexity:** L
**Maps to PRD AC:** All (AC 1-6)

---

## AC Coverage Matrix

| PRD AC # | Description                                               | Covered By Ticket(s)       | Status  |
|----------|-----------------------------------------------------------|----------------------------|---------|
| 1        | Agent can call `memory_write` to persist notes            | Ticket 3, Ticket 4         | Covered |
| 2        | `memory_read` returns memory file contents               | Ticket 3, Ticket 5         | Covered |
| 3        | MEMORY.md content appears in the system prompt           | Ticket 4, Ticket 5         | Covered |
| 4        | Daily notes file is auto-created with today's date       | Ticket 1, Ticket 4         | Covered |
| 5        | Memory persists across sessions (file-based)             | Ticket 1, Ticket 5         | Covered |
| 6        | Unit tests with temp dirs and fake clock                 | Ticket 1, Ticket 3, Ticket 5 | Covered |
