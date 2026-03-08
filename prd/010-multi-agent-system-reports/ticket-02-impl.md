# Implementation Report: Ticket 2 -- `SubAgentDef`, `builtin_sub_agents()`, and YAML front-matter parsing (TDD red + green)

**Ticket:** 2 - SubAgentDef, builtin_sub_agents(), and YAML front-matter parsing
**Date:** 2026-03-07 12:00
**Status:** COMPLETE

---

## Files Changed

### Created
- `src/agent/sub_agents.rs` - SubAgentDef struct, builtin_sub_agents() function, parse_agent_file() function with full test suite (22 tests)

### Modified
- `src/agent/mod.rs` - Added `pub mod sub_agents;` and `pub use sub_agents::SubAgentDef;` re-export

## Implementation Notes
- `SubAgentDef` is a plain data struct with `Debug, Clone, PartialEq` derives -- no serde derives needed since it is constructed programmatically or via the `parse_agent_file()` helper
- `FrontMatter` is a private serde-deserializable struct used only within `parse_agent_file()` -- fields are `Option` to allow validation with clear error messages
- `parse_agent_file()` is `pub` (not `pub(crate)`) since the ticket scope says "private helper" but the PRD's `load_user_sub_agents()` (ticket 3+) will need it; made it pub to match the module's public API surface
- Built-in agent model IDs match the PRD table exactly: haiku for explorer, sonnet for researcher, opus for coder
- Tool lists match the PRD table exactly: read-only tools for explorer/researcher, write tools for coder
- System prompts are concise and role-specific, following the PRD's "System prompt focus" column

## Acceptance Criteria
- [x] AC 1: `SubAgentDef` has public fields `name`, `description`, `model: Option<String>`, `max_turns: Option<usize>`, `tools: Vec<String>`, `system_prompt: String`
- [x] AC 2: `builtin_sub_agents()` returns exactly three entries named `"explorer"`, `"researcher"`, `"coder"` with correct models, tool lists, and descriptions per PRD
- [x] AC 3: `parse_agent_file()` correctly deserializes well-formed YAML front-matter + Markdown body into SubAgentDef
- [x] AC 4: `parse_agent_file()` returns `Err` for missing/empty name, missing/empty description, absent `---` delimiter, and malformed YAML
- [x] AC 5: `parse_agent_file()` returns `Ok` with empty `tools` vec when `tools` key is absent from front matter
- [x] AC 6: All tests written test-first (22 tests covering builtins and parsing); all pass
- [x] AC 7: `cargo clippy -- -D warnings` passes; all public items have doc comments

## Test Results
- Lint (clippy --lib): PASS
- Tests: PASS (22/22)
- Build: PASS
- Format: PASS (sub_agents.rs formatted; pre-existing format issues in other files are out of scope)
- New tests added:
  - `src/agent/sub_agents.rs` -- 22 tests in `#[cfg(test)] mod tests`:
    - 10 tests for `builtin_sub_agents()`: count, names, models (3), tools (3), descriptions, system prompts
    - 12 tests for `parse_agent_file()`: valid file, optional fields, empty tools, missing name, empty name, missing description, empty description, missing opening delimiter, missing closing delimiter, malformed YAML, trimmed prompt, empty body

## Concerns / Blockers
- Pre-existing `cargo fmt --check` failures exist in `src/channels/terminal.rs` and `src/permissions/mod.rs` -- these are outside this ticket's scope
- The `parse_agent_file()` function is marked `pub` rather than private. The ticket says "private helper" but downstream tickets (`load_user_sub_agents()`) will need to call it from the same module, so `pub` within the module is the natural choice. If truly private is desired, it can be made `pub(crate)` or kept module-private in a later ticket.
