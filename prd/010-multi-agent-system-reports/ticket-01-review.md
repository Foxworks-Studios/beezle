# Code Review: Ticket 1 -- Add `serde_yaml` dep and `[[models]]` config support

**Ticket:** 1 -- Add `serde_yaml` dep and `[[models]]` config support
**Impl Report:** prd/010-multi-agent-system-reports/ticket-01-impl.md
**Date:** 2026-03-07 13:00
**Verdict:** CHANGES REQUESTED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `ModelEntry` is public with `id`, `provider`, `guidance` fields; all `#[serde(default)]`-safe | Met | Struct at `src/config/mod.rs:21`, derives `Default`, each field has `#[serde(default)]` |
| 2 | `AppConfig.models` round-trips through TOML with `#[serde(default)]`; backward compatible | Met | Field at line 43 with `#[serde(default)]`, `Default` impl sets `Vec::new()`, existing `default_config_roundtrips_through_toml` test covers round-trip |
| 3 | Unit test for TOML with `[[models]]` entry | Met | `toml_with_models_entry_deserializes_correctly` at line 461, verifies all three fields |
| 4 | Unit test for TOML without `[[models]]` yields empty vec | Met | `toml_without_models_section_yields_empty_vec` at line 488 |
| 5 | `cargo build` zero warnings; `cargo clippy -- -D warnings` passes | Met | Impl report claims pass; config tests confirmed passing (21/21) |

## Issues Found

### Critical (must fix before merge)

- **Out-of-scope files modified**: `src/channels/terminal.rs` and `src/permissions/mod.rs` have formatting-only changes that are outside the ticket scope (`Cargo.toml` and `src/config/mod.rs` only). The implementer acknowledges this in the report as "side effect of running `cargo fmt`". These changes must be reverted or split into a separate commit. Touching out-of-scope files risks merge conflicts with other in-flight tickets.

### Major (should fix, risk of downstream problems)

- None

### Minor (nice to fix, not blocking)

- The `serde_yaml` dependency is added per ticket spec, but it is not used anywhere in this ticket's code. This is expected since it is a prep for Ticket 2, but note that `cargo clippy` may eventually flag it as an unused dependency if a future linter rule is added.

## Suggestions (non-blocking)

- The two new tests (`toml_with_models_entry_deserializes_correctly` and `toml_without_models_section_yields_empty_vec`) are well-structured and cover the required ACs cleanly. No changes needed.
- Consider adding a test with multiple `[[models]]` entries to verify vec accumulation, though this is not required by the ACs.

## Scope Check
- Files within scope: YES for `Cargo.toml` and `src/config/mod.rs`
- **Out-of-scope files touched**: `src/channels/terminal.rs` (import reorder), `src/permissions/mod.rs` (formatting), `Cargo.lock` (auto-generated, acceptable)
- Scope creep detected: YES -- formatting changes to two files outside ticket scope
- Unauthorized dependencies added: NO (`serde_yaml` is explicitly called for)

## Risk Assessment
- Regression risk: LOW -- `ModelEntry` and the `models` field are additive with `#[serde(default)]`; existing configs remain valid
- Security concerns: NONE
- Performance concerns: NONE
