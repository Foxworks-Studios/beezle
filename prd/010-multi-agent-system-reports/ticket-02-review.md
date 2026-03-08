# Code Review: Ticket 2 -- SubAgentDef, builtin_sub_agents(), and YAML front-matter parsing

**Ticket:** 2 -- SubAgentDef, builtin_sub_agents(), and YAML front-matter parsing (TDD red + green)
**Impl Report:** prd/010-multi-agent-system-reports/ticket-02-impl.md
**Date:** 2026-03-07 14:30
**Verdict:** APPROVED

---

## AC Coverage

| AC # | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | `SubAgentDef` has public fields `name`, `description`, `model: Option<String>`, `max_turns: Option<usize>`, `tools: Vec<String>`, `system_prompt: String` | Met | Verified in `sub_agents.rs` lines 14-28. All fields are `pub`, types match exactly. |
| 2 | `builtin_sub_agents()` returns exactly three entries named `"explorer"`, `"researcher"`, `"coder"` with models, tool lists, and descriptions per PRD | Met | Verified against PRD table (line 187-191): explorer=haiku (`claude-haiku-4-5-20251001`), researcher=sonnet (`claude-sonnet-4-6`), coder=opus (`claude-opus-4-6`). Tool lists match exactly. Descriptions match PRD table verbatim. |
| 3 | `parse_agent_file()` correctly deserializes well-formed YAML front-matter + Markdown body into SubAgentDef | Met | `parse_valid_agent_file` test exercises all fields. Logic at lines 105-149 correctly splits on `---` delimiters, deserializes YAML via `FrontMatter`, and extracts body as `system_prompt`. |
| 4 | `parse_agent_file()` returns `Err` for missing/empty name, missing/empty description, absent `---` delimiter, and malformed YAML | Met | Six error tests cover all cases: `error_missing_name`, `error_empty_name`, `error_missing_description`, `error_empty_description`, `error_missing_opening_delimiter`, `error_missing_closing_delimiter`, `error_malformed_yaml`. Logic uses `.filter(|s| !s.trim().is_empty())` for empty-string detection. |
| 5 | `parse_agent_file()` returns `Ok` with empty `tools` vec when `tools` key absent | Met | Test `parse_agent_file_empty_tools_vec_when_tools_absent` verifies. `FrontMatter` uses `#[serde(default)]` on `tools: Vec<String>` (line 38). |
| 6 | All tests pass | Met | 22/22 tests pass (`cargo test --lib sub_agents`). |
| 7 | `cargo clippy -- -D warnings` passes; all public items have doc comments | Met | Clippy passes clean. Doc comments present on `SubAgentDef` (struct + all fields), `builtin_sub_agents()`, and `parse_agent_file()`. `FrontMatter` is private and does not require doc comments per convention. |

## Issues Found

### Critical (must fix before merge)

None.

### Major (should fix, risk of downstream problems)

None.

### Minor (nice to fix, not blocking)

- **Visibility of `parse_agent_file()`**: The ticket scope says "private helper" but the implementer made it `pub`. This is a reasonable deviation documented in the impl report -- `load_user_sub_agents()` in Ticket 4 will need to call it. `pub(crate)` would be more precise, but `pub` is acceptable since the module is internal to the crate. No action required.

## Suggestions (non-blocking)

- The `parse_agent_file` function handles `\r\n` line endings implicitly through `find("\n---")`, which would fail on files with `\r\n` endings (the `\r` would be part of the YAML content). Since agent files are expected to be authored by the user and this is a CLI tool, Windows line endings may appear. Consider normalizing to `\n` at the start of the function if cross-platform support is desired. Not blocking for this ticket.

## Scope Check
- Files within scope: YES -- only `src/agent/sub_agents.rs` (created) and `src/agent/mod.rs` (modified) were touched, matching the ticket scope exactly.
- Scope creep detected: NO
- Unauthorized dependencies added: NO -- `serde_yaml` was added by Ticket 1 (a dependency of this ticket).

## Risk Assessment
- Regression risk: LOW -- new module with no modifications to existing logic. The only change to existing code is two lines added to `src/agent/mod.rs` (`pub mod sub_agents;` and `pub use sub_agents::SubAgentDef;`).
- Security concerns: NONE
- Performance concerns: NONE
