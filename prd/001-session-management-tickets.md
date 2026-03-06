# PRD 001: Session Management -- Ticket Breakdown

## Ticket 01: SessionManager with save/load/list

**Scope**: `src/session/mod.rs` (new), `src/lib.rs`

**Work**:
1. Create `src/session/mod.rs` with `SessionManager` struct wrapping the
   sessions directory path.
2. Implement:
   - `generate_key() -> String`: timestamp-based key (YYYY-MM-DD_HH-MM-SS)
   - `save(key, json) -> Result<PathBuf>`: write JSON string to `<key>.json`
   - `load(key) -> Result<String>`: read JSON from `<key>.json`
   - `list() -> Result<Vec<SessionInfo>>`: list sessions sorted by mtime,
     returning key + modified time + file size
   - `most_recent() -> Result<Option<String>>`: key of the newest session
   - `delete(key) -> Result<()>`: remove a session file
3. `SessionInfo` struct: key, modified timestamp, size_bytes.
4. Unit tests for all operations using temp directories.

## Ticket 02: Wire sessions into main.rs REPL

**Scope**: `src/main.rs`

**Work**:
1. Replace `--resume` stub with actual session loading via `SessionManager`.
2. Auto-save on `/quit` and `/exit`.
3. Add `/save [name]` command to save with a user-chosen key.
4. Add `/sessions` command to list saved sessions.
5. Wire Ctrl+C handler to save before exit (best-effort).

**Dependencies**: Ticket 01
