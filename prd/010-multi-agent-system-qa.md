# QA Report: PRD 010 -- Multi-Agent System

**Source PRD:** /home/travis/Development/beezle/prd/010-multi-agent-system.md
**Status File:** /home/travis/Development/beezle/prd/010-multi-agent-system-status.md
**Date:** 2026-03-07 16:00
**Overall Status:** CONDITIONAL PASS

---

## Acceptance Criteria Verification

| AC # | Description | Status | Evidence | Test Scenario |
|------|-------------|--------|----------|---------------|
| 1 | `cargo build` zero warnings/errors | PASS | `cargo build` completes with `Finished dev profile`, no warnings | Run `cargo build` |
| 2 | Startup registers 3 built-in sub-agents; DEBUG log lists names | PASS | `src/main.rs:343-351` -- `builtin_sub_agents()` + `load_user_sub_agents()` collected, names logged via `tracing::debug!` | Run with `RUST_LOG=debug` and check for `registered sub-agents` line |
| 3 | Explorer=Haiku, Researcher=Sonnet, Coder=Opus (Anthropic) | PASS | `src/agent/sub_agents.rs:113,125,137` -- hardcoded model IDs match PRD table exactly. Unit tests `builtin_explorer_uses_haiku_model`, `builtin_researcher_uses_sonnet_model`, `builtin_coder_uses_opus_model` pass | Inspect model field in each SubAgentDef |
| 4 | Ollama sub-agents inherit parent model | PARTIAL | `load_model_roster()` correctly returns empty for Ollama (`src/agent/sub_agents.rs:383`). However, `build_sub_agent()` resolves `def.model.as_deref().unwrap_or(parent_model)` (line 75), and built-in defs have `model: Some("claude-...")`. When provider is Ollama, Anthropic model IDs are passed through to the Ollama provider instead of falling back to parent model. PRD explicitly states: "all sub-agents inherit the parent model" for Ollama. | Configure Ollama provider and check what model ID is sent to sub-agents |
| 5 | Valid `.md` in `~/.beezle/agents/` causes agent to appear at startup | PASS | `load_user_sub_agents_from()` scans dir, parses .md files (lines 300-330). Unit test `load_user_sub_agents_returns_valid_def_for_correct_file` passes. Wired in `build_agent()` at line 344. | Place valid .md file, restart, check DEBUG log |
| 6 | Coordinator prompt contains name/description/model for all agents | PASS | `coordinator_agent_prompt()` (lines 200-230) emits name, description, model for each agent. Tests `coordinator_prompt_contains_agent_names/descriptions/model_info` pass. Appended to system prompt at line 354-355. | Inspect prompt output for all agent details |
| 7 | Malformed `.md` produces WARN, does not prevent startup | PASS | `load_user_sub_agents_from()` logs `warn!` at line 316 (read failure) and line 324 (parse failure), continues. Tests `load_user_sub_agents_skips_file_missing_name/description/delimiter` pass. | Place malformed .md file, check WARN log, verify startup completes |
| 8 | Unrecognized tool name produces WARN, agent registered with recognized tools only | PASS | `tools_for_names()` line 176 emits `warn!` for unknown names, skips them. Test `tools_for_names_skips_unrecognized_names` passes with `fly_rocket`. | Create agent file with `fly_rocket` tool, verify WARN and partial tool set |
| 9 | `spawn_agent` removed from tool list | PASS | grep confirms `spawn_agent` absent from `src/main.rs` and `src/agent/`. Only references are in `src/permissions/mod.rs` (policy category mapping, not tool registration). | `grep -r spawn_agent src/main.rs src/agent/` returns nothing |
| 10 | Coordinator has no `default_tools()`, only sub-agents + memory | PASS | `build_raw_tools()` (line 228) returns only memory tools. `default_tools()` only appears in test helpers (lines 1887, 1959, 1975) that test PermissionGuard, not coordinator wiring. | Inspect `build_raw_tools()` and `build_agent()` tool registration path |
| 11 | Model roster section when multiple models available | PASS | `coordinator_agent_prompt()` includes `## Available Models` when `model_roster.len() > 1` (line 217). Test `coordinator_prompt_includes_models_section_when_multiple` passes. | Provide 2+ ModelEntry items, check prompt output |
| 12 | No model roster when single model (e.g. Ollama) | PASS | Guard at line 217 (`model_roster.len() > 1`) and `load_model_roster()` returns empty for non-anthropic. Tests `coordinator_prompt_omits_models_section_when_single_model` and `load_model_roster_returns_empty_for_ollama` pass. | Configure Ollama, verify no `## Available Models` section |
| 13 | Unit tests for `load_user_sub_agents()` | PASS | 7 tests covering: valid file, missing name, missing description, missing delimiter, empty tools (via `parse_agent_file` tests), absent directory, non-.md files, multiple files. All pass. | `cargo test load_user_sub_agents` |
| 14 | Unit tests for `coordinator_agent_prompt()` | PASS | 7 tests asserting name/description/model presence, roster inclusion/omission. All pass. | `cargo test coordinator_prompt` |
| 15 | Unit tests for `tools_for_names()` | PASS | 5 tests: empty input, all 6 known tools, unknown names, all-unknown, single tool. All pass. | `cargo test tools_for_names` |
| 16 | Unit tests for `load_model_roster()` | PASS | 3 tests: anthropic-only (3 entries), ollama (empty), user models merged. All pass. | `cargo test load_model_roster` |
| 17 | `cargo clippy -- -D warnings` passes | PASS | Clippy completes with `Finished dev profile`, no warnings | Run `cargo clippy -- -D warnings` |

## Bugs Found

### Bug 1: Ollama sub-agents receive Anthropic model IDs instead of inheriting parent model
- **Severity:** Major
- **Location:** `src/agent/sub_agents.rs:75` (build_sub_agent) and `src/agent/sub_agents.rs:108-148` (builtin_sub_agents)
- **Description:** The PRD states: "When the active provider is Ollama (or another local provider), the model field falls back to the parent coordinator's model since local providers typically have only one model loaded." However, `builtin_sub_agents()` returns definitions with `model: Some("claude-haiku-4-5-20251001")` etc., and `build_sub_agent()` uses `def.model.as_deref().unwrap_or(parent_model)`, which will use the Anthropic model ID (since it's `Some`), not the parent model. There is no provider-aware clearing of the model field.
- **Reproduction Steps:**
  1. Configure beezle with `default_provider = "ollama"` and `model = "qwen2.5:14b"`
  2. Start beezle with `RUST_LOG=debug`
  3. Observe that sub-agents are built with `claude-haiku-4-5-20251001`, `claude-sonnet-4-6`, `claude-opus-4-6` instead of `qwen2.5:14b`
  4. The Ollama provider will likely fail or behave unpredictably with these unknown model IDs
- **Suggested Fix:** In `build_agent()`, when `is_ollama` is true, override `def.model` to `None` (or directly pass `parent_model`) for each sub-agent def before calling `build_sub_agent()`. Alternatively, `build_sub_agent()` could accept a `provider_name` parameter and force parent model fallback for non-anthropic providers.

### Bug 2: Sub-agent count hardcoded in tool display
- **Severity:** Minor
- **Location:** `src/main.rs:1050`
- **Description:** `let sub_agent_count = 3;` is hardcoded. If a user has agents in `~/.beezle/agents/`, the displayed tool count will be wrong.
- **Reproduction Steps:**
  1. Place a valid agent `.md` file in `~/.beezle/agents/`
  2. Start beezle
  3. Observe the "tools: N loaded" display -- it will show 3 sub-agents instead of 4
- **Suggested Fix:** Compute sub-agent count from the actual `sub_agent_defs.len()` and pass it to the display section.

## Edge Cases Not Covered
- **Duplicate agent names (built-in + user):** If a user creates `~/.beezle/agents/explorer.md`, two sub-agents named `explorer` will be registered. The PRD does not address name conflict resolution. -- Risk: MEDIUM
- **Very large agent definition files:** No size limit on `.md` files read from `~/.beezle/agents/`. A multi-MB file would be loaded entirely into memory. -- Risk: LOW
- **Non-UTF8 file content:** `std::fs::read_to_string` will error on non-UTF8 files, but this is handled by the `warn!` + skip path. -- Risk: LOW
- **Symlinks in `~/.beezle/agents/`:** `read_dir` follows symlinks; a symlink loop or broken symlink could cause issues. -- Risk: LOW
- **`spawn_agent` references in permissions:** `src/permissions/mod.rs` still references `spawn_agent` in tool category and display name mappings (lines 224, 345) and test fixtures (lines 924, 932, 938). This is dead code referencing a tool that no longer exists. -- Risk: LOW

## Integration Issues
- The Ollama model fallback gap (Bug 1) means the multi-agent system is not functional for Ollama users. This is a cross-cutting issue between `builtin_sub_agents()` definitions and `build_agent()` wiring.
- The coordinator prompt format diverges from the PRD's specified format (uses `## Available Sub-Agents` with `### name` headings instead of `## Sub-Agents` with `- 'name': desc (model: ...)` bullets and a Guidelines section). This does not violate AC 6 since the AC only requires name/description/model presence, but it may affect LLM delegation quality since the PRD's format was designed with specific delegation guidelines.

## Regression Results
- Test suite: PASS -- 264 tests (206 lib + 58 binary + 0 doc), 0 failures
- Build: PASS -- zero warnings, zero errors
- Lint: PASS -- `cargo clippy -- -D warnings` clean
- Format: PASS -- `cargo fmt --check` clean
- Shared code impact: `src/config/mod.rs` modified (added `ModelEntry` and `models` field) -- backward compatible via `#[serde(default)]`; existing config tests pass. `src/agent/mod.rs` modified (removed `build_subagent()`) -- old function fully replaced by new `sub_agents` module.

## Recommended Follow-Up Tickets
1. **Fix Ollama model fallback** -- Add provider-aware model resolution in `build_agent()` or `build_sub_agent()` so that Ollama sub-agents inherit the parent model instead of receiving Anthropic model IDs. This is required for Ollama users.
2. **Dynamic sub-agent count in tool display** -- Replace hardcoded `sub_agent_count = 3` at `src/main.rs:1050` with the actual count from loaded sub-agent definitions.
3. **Handle duplicate agent names** -- Add deduplication or name-conflict detection when merging built-in and user-defined agents in `build_agent()`.
4. **Clean up `spawn_agent` references in permissions** -- Remove the dead `spawn_agent` entries from `src/permissions/mod.rs` category mapping (line 224), display name mapping (line 345), and test fixtures (lines 924, 932, 938).
5. **Align coordinator prompt format with PRD spec** -- Consider matching the PRD's bullet-list format with delegation guidelines section for optimal LLM delegation behavior.
