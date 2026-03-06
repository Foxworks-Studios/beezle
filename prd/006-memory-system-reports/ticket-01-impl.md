# Implementation Report: Ticket 1 -- MemoryStore -- Core Types, Clock Trait, and File I/O

**Ticket:** 1 - MemoryStore -- Core Types, Clock Trait, and File I/O
**Date:** 2026-03-05 14:30
**Status:** COMPLETE

---

## Files Changed

### Created
- `src/memory/mod.rs` - Core memory module with `MemoryStore`, `MemoryError`, `Clock` trait, `SystemClock`, and `FakeClock` (test-only)

### Modified
- `Cargo.toml` - Added `chrono = "0.4"` dependency
- `src/lib.rs` - Added `#[cfg(test)] #[allow(dead_code)] mod memory;` to compile module during tests only

## Implementation Notes
- The module is **not** publicly registered in `lib.rs` per ticket instructions. A `#[cfg(test)]` declaration was added so tests can compile and run. Ticket 2 will replace this with `pub mod memory;`.
- `#[allow(dead_code)]` on the module declaration suppresses warnings for items like `SystemClock` and `memory_dir()` that are defined but not yet consumed in tests.
- `FakeClock` is defined inside `#[cfg(test)] mod tests` since it's only needed for testing. If future tickets need it externally, it can be moved.
- `write_long_term` uses write-to-temp-then-rename for atomicity, with the temp file in the same directory to ensure same-filesystem rename.
- `append_daily` uses `OpenOptions` with `create(true).append(true)` for safe concurrent appends.
- `read_long_term` and `read_daily` return empty string for `NotFound` errors, propagating all other I/O errors.
- Directory creation is lazy -- `ensure_dir()` is called only by write methods, not by the constructor or read methods.

## Acceptance Criteria
- [x] AC 1: `MemoryError` defined with `thiserror`, has `Io(#[from] std::io::Error)` and `HomeNotFound` variants, derives `Debug`
- [x] AC 2: `Clock` trait with `now() -> DateTime<Local>`; `SystemClock` implements it; `FakeClock(DateTime)` implements it in tests
- [x] AC 3: `MemoryStore` holds `memory_dir: PathBuf` and `clock: Arc<dyn Clock>`; provides `new(memory_dir, clock)` constructor
- [x] AC 4: `memory_dir()` returns the root path; directory created lazily on first write, not on construction
- [x] AC 5: `read_long_term()` returns contents of `MEMORY.md` or empty string if absent
- [x] AC 6: `read_daily(date)` returns contents of `YYYY-MM-DD.md` or empty string if absent
- [x] AC 7: `append_daily(text)` appends `\n## HH:MM\n{text}\n` to today's daily file, creating it if needed
- [x] AC 8: `write_long_term(content)` atomically replaces `MEMORY.md` (temp file + rename)
- [x] AC 9: Test: `FakeClock` at `2026-03-05T14:30:00` -> `append_daily("hello")` -> file contains `## 14:30` and `hello`
- [x] AC 10: Test: `read_long_term()` on fresh dir -> returns `""`
- [x] AC 11: Test: `write_long_term("facts")` -> `read_long_term()` returns `"facts"`
- [x] AC 12: Test: `append_daily` called twice -> file contains both `## HH:MM` sections in order
- [x] AC 13: Test: `read_daily` for missing date -> returns `""`
- [x] AC 14: Quality gates pass

## Test Results
- Lint: PASS (`cargo clippy -- -D warnings` -- zero warnings)
- Tests: PASS (52 total, 5 new memory tests)
- Build: PASS (`cargo build` -- zero warnings)
- Format: PASS (`cargo fmt --check`)
- New tests added:
  - `src/memory/mod.rs::tests::read_long_term_returns_empty_on_fresh_dir`
  - `src/memory/mod.rs::tests::write_then_read_long_term`
  - `src/memory/mod.rs::tests::append_daily_creates_file_with_timestamp`
  - `src/memory/mod.rs::tests::append_daily_twice_contains_both_sections`
  - `src/memory/mod.rs::tests::read_daily_returns_empty_for_missing_date`

## Concerns / Blockers
- `src/lib.rs` was modified with a `#[cfg(test)]` module declaration to enable test compilation. This is a minimal, test-only change. Ticket 2 should replace it with `pub mod memory;` and remove the `#[allow(dead_code)]`.
- `FakeClock` lives inside the test module. If Ticket 2 or other tickets need it for integration tests, it should be extracted to a shared test utility.
