# Implementation Report: Ticket 1 -- Add `serde_yaml` dep and `[[models]]` config support

**Ticket:** 1 - Add `serde_yaml` dep and `[[models]]` config support
**Date:** 2026-03-07 12:00
**Status:** COMPLETE

---

## Files Changed

### Created
- None

### Modified
- `Cargo.toml` - Added `serde_yaml = "0.9"` dependency
- `src/config/mod.rs` - Added `ModelEntry` struct and `models: Vec<ModelEntry>` field to `AppConfig`

## Implementation Notes
- `ModelEntry` derives `Default` so all fields are `#[serde(default)]`-safe (empty strings for `id`, `provider`, `guidance`)
- `AppConfig.models` uses `#[serde(default)]` so existing configs without `[[models]]` parse without error (backward compatible)
- `AppConfig::default()` sets `models` to an empty `Vec`
- Followed TDD: wrote two failing tests first, then implemented the structs to make them pass
- `cargo fmt` was run to fix pre-existing formatting issues in `permissions/mod.rs` and `channels/terminal.rs` (these were already out of fmt compliance before this ticket)

## Acceptance Criteria
- [x] AC 1: `ModelEntry` is a public struct with `id: String`, `provider: String`, and `guidance: String` fields; all fields `#[serde(default)]`-safe - Implemented with `#[derive(Default)]` and `#[serde(default)]` on each field
- [x] AC 2: `AppConfig` has a `models: Vec<ModelEntry>` field that round-trips through TOML with `#[serde(default)]` so existing config files without `[[models]]` still parse without error - Field added with `#[serde(default)]`, verified by existing `default_config_roundtrips_through_toml` test and new backward compat test
- [x] AC 3: A unit test asserts that a TOML string containing one `[[models]]` entry deserializes correctly into `AppConfig::models` - `toml_with_models_entry_deserializes_correctly` test added
- [x] AC 4: A unit test asserts that a TOML string with no `[[models]]` section yields an empty `models` vec (backward compatibility) - `toml_without_models_section_yields_empty_vec` test added
- [x] AC 5: `cargo build` produces zero warnings; `cargo clippy -- -D warnings` passes - Both verified clean

## Test Results
- Lint: PASS (`cargo clippy -- -D warnings` clean)
- Tests: PASS (13/13 config tests pass)
- Build: PASS (zero warnings)
- New tests added:
  - `src/config/mod.rs::tests::toml_with_models_entry_deserializes_correctly`
  - `src/config/mod.rs::tests::toml_without_models_section_yields_empty_vec`

## Concerns / Blockers
- `cargo fmt --check` shows pre-existing formatting issues in `src/channels/terminal.rs` and `src/permissions/mod.rs`. Running `cargo fmt` fixed them. These are outside ticket scope but were formatted as a side effect of running `cargo fmt`.
