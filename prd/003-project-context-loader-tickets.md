# PRD 003: Project Context Loader -- Ticket Breakdown

## Ticket 01: Context discovery, loading, and prompt injection

**Scope**: `src/context/mod.rs` (new), `src/lib.rs`, `src/main.rs`

**Work**:
1. Create `src/context/mod.rs` with:
   - `CONTEXT_FILES` const: ordered list `["CLAUDE.md", "BEEZLE.md", ".beezle/instructions.md"]`
   - `DEFAULT_MAX_CHARS` const: `8000`
   - `discover_context_files(start_dir) -> Vec<PathBuf>`: walk up from `start_dir`
     to filesystem root, collect all matching files in priority order.
   - `load_project_context(start_dir, max_chars) -> String`: discover files, read
     contents, concatenate with source headers, truncate if over limit, wrap in
     `<project-context>` delimiters.
2. Add `pub mod context;` to `src/lib.rs`.
3. In `src/main.rs`, call `load_project_context(cwd, 8000)` and prepend result
   to the system prompt passed to `build_agent`.
4. Unit tests for:
   - Discovery finds files in CWD
   - Discovery walks up to parent directories
   - Discovery returns empty vec when no files exist
   - Multiple files are concatenated in priority order
   - Content is truncated at max_chars with notice
   - Empty content produces empty string (no delimiters)

**Acceptance Criteria**:
- [ ] `CLAUDE.md` in CWD is injected into system prompt
- [ ] `CLAUDE.md` in parent dir is found from subdirectory
- [ ] Content over 8000 chars is truncated with notice
- [ ] Multiple files concatenated in priority order
- [ ] No context files = normal operation, no error
- [ ] Unit tests pass
- [ ] `cargo test && cargo clippy -- -D warnings && cargo fmt --check` all pass
