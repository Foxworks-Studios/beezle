# Tickets for PRD 010: Multi-Agent System

**Source PRD:** /home/travis/Development/beezle/prd/010-multi-agent-system.md
**Created:** 2026-03-07
**Total Tickets:** 7
**Estimated Total Complexity:** 16 (S=1, M=2, L=3)

---

### Ticket 1: Add `serde_yaml` dep and `[[models]]` config support

**Description:**
Add `serde_yaml` to `Cargo.toml` and extend `AppConfig` with a `models` field that
holds a `Vec<ModelEntry>` sourced from `[[models]]` TOML table entries. This is the
foundational dependency for the model roster system -- every subsequent ticket reads
from this field or the `serde_yaml` crate.

**Scope:**
- Modify: `Cargo.toml` — add `serde_yaml = "0.9"` under `[dependencies]`
- Modify: `src/config/mod.rs` — add `ModelEntry` struct, add `models: Vec<ModelEntry>` field to `AppConfig` with `#[serde(default)]`

**Acceptance Criteria:**
- [ ] `ModelEntry` is a public struct with `id: String`, `provider: String`, and `guidance: String` fields; all fields `#[serde(default)]`-safe
- [ ] `AppConfig` has a `models: Vec<ModelEntry>` field that round-trips through TOML with `#[serde(default)]` so existing config files without `[[models]]` still parse without error
- [ ] A unit test asserts that a TOML string containing one `[[models]]` entry deserializes correctly into `AppConfig::models`
- [ ] A unit test asserts that a TOML string with no `[[models]]` section yields an empty `models` vec (backward compatibility)
- [ ] `cargo build` produces zero warnings; `cargo clippy -- -D warnings` passes

**Dependencies:** None
**Complexity:** S
**Maps to PRD AC:** AC 11, AC 16

---

### Ticket 2: `SubAgentDef`, `builtin_sub_agents()`, and YAML front-matter parsing (TDD red + green)

**Description:**
Create `src/agent/sub_agents.rs` with the `SubAgentDef` struct and `builtin_sub_agents()`
returning the three hardcoded definitions (explorer/researcher/coder). Also implement
the pure YAML front-matter parsing logic used by `load_user_sub_agents()`, along with
its complete test suite (written red-first per TDD mandate). This ticket contains no
filesystem I/O — parsing operates on `&str` inputs only.

**Scope:**
- Create: `src/agent/sub_agents.rs` — `SubAgentDef` struct, `builtin_sub_agents()`, `parse_agent_file(content: &str) -> Result<SubAgentDef, String>` (private helper)
- Modify: `src/agent/mod.rs` — add `pub mod sub_agents;` and `pub use sub_agents::SubAgentDef;`
- Modify: `src/lib.rs` — no change needed (agent is already `pub mod`)

**Acceptance Criteria:**
- [ ] `SubAgentDef` has public fields `name`, `description`, `model: Option<String>`, `max_turns: Option<usize>`, `tools: Vec<String>`, `system_prompt: String`
- [ ] `builtin_sub_agents()` returns exactly three entries named `"explorer"`, `"researcher"`, `"coder"` with the models, tool lists, and descriptions specified in the PRD
- [ ] `parse_agent_file()` correctly deserializes a well-formed YAML front-matter + Markdown body string into a `SubAgentDef`
- [ ] `parse_agent_file()` returns `Err` when `name` or `description` is missing or empty, when the `---` delimiter is absent, and when the YAML block is malformed
- [ ] `parse_agent_file()` returns `Ok` with an empty `tools` vec when `tools` key is absent from front matter
- [ ] All tests were written before the implementation (TDD red step demonstrated by commit history or test-first structure); all pass
- [ ] `cargo clippy -- -D warnings` passes; all public items have doc comments

**Dependencies:** Ticket 1 (for `serde_yaml` crate availability)
**Complexity:** M
**Maps to PRD AC:** AC 2, AC 3, AC 13

---

### Ticket 3: `tools_for_names()` and `coordinator_agent_prompt()` (TDD red + green)

**Description:**
Add two pure functions to `src/agent/sub_agents.rs`: `tools_for_names()` resolves
tool-name strings to yoagent `Arc<dyn AgentTool>` instances (with `tracing::warn!` for
unknown names), and `coordinator_agent_prompt()` generates the Markdown section that
will be appended to the coordinator's system prompt. Both functions have no I/O and
must be fully covered by unit tests written red-first.

**Scope:**
- Modify: `src/agent/sub_agents.rs` — add `tools_for_names(names: &[String]) -> Vec<Arc<dyn AgentTool>>` and `coordinator_agent_prompt(agents: &[SubAgentDef], model_roster: &[ModelEntry]) -> String`

**Acceptance Criteria:**
- [ ] `tools_for_names()` maps the six known tool name strings (`read_file`, `write_file`, `edit_file`, `list_files`, `search`, `bash`) to the correct yoagent constructors, each wrapped in `Arc`
- [ ] `tools_for_names()` emits a `tracing::warn!` log line and skips any unrecognized name (e.g. `"fly_rocket"`), returning only recognized tools
- [ ] `tools_for_names([])` returns an empty vec without panicking
- [ ] `coordinator_agent_prompt()` output contains each agent's `name`, `description`, and model info for every entry in `agents`
- [ ] `coordinator_agent_prompt()` includes a `## Available Models` section when `model_roster` has more than one entry
- [ ] `coordinator_agent_prompt()` omits the `## Available Models` section entirely when `model_roster` has zero or one entry
- [ ] All tests were written before the implementation; all pass
- [ ] `cargo clippy -- -D warnings` passes

**Dependencies:** Ticket 2 (for `SubAgentDef`), Ticket 1 (for `ModelEntry`)
**Complexity:** M
**Maps to PRD AC:** AC 6, AC 11, AC 12, AC 14, AC 15

---

### Ticket 4: `load_user_sub_agents()` and `load_model_roster()` (TDD red + green)

**Description:**
Implement the two I/O-bound loading functions in `src/agent/sub_agents.rs`.
`load_user_sub_agents()` scans `~/.beezle/agents/*.md`, calls `parse_agent_file()` per
file, and returns valid definitions (logging `WARN` on failures). `load_model_roster()`
returns automatic Anthropic tier entries when the provider is Anthropic, user-configured
`[[models]]` entries from config, and an empty vec for Ollama-only setups. Use
`tempfile::TempDir` for all filesystem tests.

**Scope:**
- Modify: `src/agent/sub_agents.rs` — add `pub fn load_user_sub_agents() -> Vec<SubAgentDef>` and `pub fn load_model_roster(config: &AppConfig) -> Vec<ModelEntry>`

**Acceptance Criteria:**
- [ ] `load_user_sub_agents()` returns an empty vec when `~/.beezle/agents/` does not exist, without panicking or logging an error
- [ ] `load_user_sub_agents()` skips a file with missing `name` field and emits exactly one `WARN`-level tracing event naming the file path
- [ ] `load_user_sub_agents()` skips a file with missing `description` field with a `WARN` log
- [ ] `load_user_sub_agents()` skips a file missing the `---` delimiter with a `WARN` log
- [ ] `load_user_sub_agents()` returns a valid `SubAgentDef` for a correctly formatted file
- [ ] `load_model_roster()` returns exactly three entries (Haiku, Sonnet, Opus) when `config.agent.default_provider == "anthropic"` and `config.models` is empty
- [ ] `load_model_roster()` returns an empty vec when `config.agent.default_provider == "ollama"`
- [ ] `load_model_roster()` merges user-configured `[[models]]` entries with the auto-generated Anthropic entries, resulting in 3 + N total entries
- [ ] All tests use `tempfile::TempDir` for filesystem isolation; all were written red-first; all pass
- [ ] `cargo clippy -- -D warnings` passes

**Dependencies:** Ticket 2 (for `parse_agent_file`, `SubAgentDef`), Ticket 1 (for `AppConfig::models`, `ModelEntry`)
**Complexity:** M
**Maps to PRD AC:** AC 4, AC 5, AC 7, AC 13, AC 16

---

### Ticket 5: `build_sub_agent()` constructor (TDD red + green)

**Description:**
Add `pub fn build_sub_agent(def: &SubAgentDef, provider: Arc<dyn StreamProvider>, parent_model: &str, api_key: &str) -> SubAgentTool` to `src/agent/sub_agents.rs`. This function uses `tools_for_names()` to resolve the tool list, calls `SubAgentTool::with_model()` when `def.model` is `Some`, falls back to `parent_model` for Ollama (i.e. when `def.model` is `None`), and emits a `DEBUG` log with the agent name and resolved model. Tests use `MockProvider`.

**Scope:**
- Modify: `src/agent/sub_agents.rs` — add `pub fn build_sub_agent(...)` and its unit tests

**Acceptance Criteria:**
- [ ] `build_sub_agent()` with a `def` that has `model: Some("claude-haiku-...")` calls `SubAgentTool::with_model("claude-haiku-...")` — confirmed by asserting `tool.name()` equals `def.name` (structural smoke test with `MockProvider`)
- [ ] `build_sub_agent()` with `model: None` uses `parent_model` as the model string
- [ ] `build_sub_agent()` calls `tools_for_names(&def.tools)` and passes the result to `SubAgentTool::with_tools()`; when `def.tools` is empty the tool still builds without panicking
- [ ] `build_sub_agent()` sets `max_turns` when `def.max_turns` is `Some`, and omits the call (using yoagent's default) when `None`
- [ ] A `tracing::debug!` event is emitted with the agent name and resolved model
- [ ] All tests were written red-first; all pass; `cargo clippy -- -D warnings` passes

**Dependencies:** Ticket 2 (for `SubAgentDef`), Ticket 3 (for `tools_for_names`)
**Complexity:** M
**Maps to PRD AC:** AC 3, AC 4, AC 8

---

### Ticket 6: Wire multi-agent system into `build_agent()` and remove `spawn_agent`

**Description:**
Update `src/main.rs` to use the new sub-agent infrastructure. Remove `build_raw_tools()`'s
`spawn_agent` wiring and `default_tools()` from the coordinator. Add the `build_agent()`
call sequence: `builtin_sub_agents()` + `load_user_sub_agents()`, `load_model_roster()`,
`coordinator_agent_prompt()` appended to the system prompt, and `agent.with_sub_agent()`
for each definition. Update or delete tests that reference `spawn_agent`. Emit `DEBUG`
logs listing all registered sub-agent names at startup.

**Scope:**
- Modify: `src/main.rs` — update `build_raw_tools()` / `build_agent()`: remove `spawn_agent` wiring, remove `default_tools()` from coordinator, add `builtin_sub_agents()`/`load_user_sub_agents()` calls, add `load_model_roster()`, append `coordinator_agent_prompt()` to system prompt, loop `agent.with_sub_agent()` for each def; update/remove outdated tests
- Modify: `src/agent/mod.rs` — mark old `build_subagent()` function as `#[deprecated]` or remove it; keep file compiling

**Acceptance Criteria:**
- [ ] `cargo build` succeeds with zero warnings and zero errors after all changes
- [ ] The `spawn_agent` tool name no longer appears anywhere in `build_raw_tools()` or `build_agent()` -- confirmed by `grep`-based test or manual code review
- [ ] At startup, a `tracing::debug!` log lists all registered sub-agent names (at minimum `explorer`, `researcher`, `coder`)
- [ ] The coordinator's `with_tools()` call no longer includes `default_tools()` -- the coordinator only receives memory tools; `default_tools()` only appear inside sub-agent tool lists
- [ ] The coordinator is built with `agent.with_sub_agent()` for each definition returned by `builtin_sub_agents()` and `load_user_sub_agents()`
- [ ] `build_agent()` appends the `coordinator_agent_prompt()` section to the system prompt string before passing it to `Agent::with_system_prompt()`
- [ ] All previously-passing tests that referenced `spawn_agent` are updated or removed; `cargo test` passes
- [ ] `cargo clippy -- -D warnings` passes

**Dependencies:** Tickets 3, 4, 5 (all `sub_agents.rs` public functions must exist before wiring)
**Complexity:** L
**Maps to PRD AC:** AC 1, AC 2, AC 6, AC 9, AC 10, AC 11, AC 12

---

### Ticket 7: Verification and Integration Check

**Description:**
Run the full PRD 010 acceptance criteria checklist end-to-end. Verify that all tickets
integrate correctly, the coordinator has the expected tool set, user-defined agents load
from `~/.beezle/agents/`, malformed files are gracefully skipped, and the model roster
appears (or is omitted) as specified.

**Acceptance Criteria:**
- [ ] AC 1: `cargo build` produces zero warnings and zero errors
- [ ] AC 2: Startup `DEBUG` log lists exactly `explorer`, `researcher`, `coder` (plus any user-defined agents present)
- [ ] AC 3: Built-in sub-agents use Haiku (explorer), Sonnet (researcher), Opus (coder) when provider is Anthropic -- confirmed by inspecting `builtin_sub_agents()` definitions in code
- [ ] AC 4: When `config.agent.default_provider == "ollama"`, `load_model_roster()` returns empty and built-in agents use `parent_model` -- confirmed by unit test in Ticket 4 + 5
- [ ] AC 5: Placing a valid `.md` file in a `~/.beezle/agents/`-like directory (via `tempfile` test) causes that agent's name to appear in `load_user_sub_agents()` output
- [ ] AC 6: `coordinator_agent_prompt()` output contains name, description, and model for every registered agent -- confirmed by unit tests in Ticket 3
- [ ] AC 7: A malformed `.md` file in the agents directory produces a `WARN` log and does not prevent startup -- confirmed by unit test in Ticket 4
- [ ] AC 8: An agent file listing `"fly_rocket"` produces a `WARN` log; agent is registered with only recognized tools -- confirmed by unit tests in Tickets 3 and 4
- [ ] AC 9: `spawn_agent` is absent from all tool registration paths -- confirmed by `grep` of `src/main.rs` and `src/agent/mod.rs`
- [ ] AC 10: The coordinator's tool list contains no `default_tools()` entries -- only sub-agent tools and memory tools
- [ ] AC 11: `coordinator_agent_prompt()` includes `## Available Models` section when multiple models are available -- confirmed by unit tests in Ticket 3
- [ ] AC 12: `coordinator_agent_prompt()` omits the model roster when only one model is available -- confirmed by unit tests in Ticket 3
- [ ] AC 13: All `load_user_sub_agents()` unit tests pass (valid file, missing `name`, missing `description`, missing `---`, empty `tools`, absent directory)
- [ ] AC 14: All `coordinator_agent_prompt()` unit tests pass (agent listing, model roster present/absent)
- [ ] AC 15: All `tools_for_names()` unit tests pass (valid names, unknown names skipped, empty list)
- [ ] AC 16: All `load_model_roster()` unit tests pass (Anthropic-only, Ollama-only, user-configured entries merged)
- [ ] AC 17: `cargo clippy -- -D warnings` passes with zero warnings
- [ ] `cargo test` passes with no regressions in pre-existing tests
- [ ] `cargo fmt --check` passes

**Dependencies:** All previous tickets
**Complexity:** S
**Maps to PRD AC:** AC 1-17 (all)

---

## AC Coverage Matrix

| PRD AC # | Description | Covered By Ticket(s) | Status |
|----------|-------------|----------------------|--------|
| 1 | `cargo build` zero warnings and errors | Ticket 6, Ticket 7 | Covered |
| 2 | Three built-in sub-agents registered at startup with DEBUG log | Ticket 2, Ticket 6, Ticket 7 | Covered |
| 3 | Each built-in uses different model (Anthropic): Haiku/Sonnet/Opus | Ticket 2, Ticket 5, Ticket 7 | Covered |
| 4 | Ollama provider: all sub-agents inherit parent model | Ticket 4, Ticket 5, Ticket 7 | Covered |
| 5 | Valid user `.md` agent file causes name to appear in startup DEBUG log | Ticket 4, Ticket 6, Ticket 7 | Covered |
| 6 | Coordinator system prompt contains name/description/model of every registered agent | Ticket 3, Ticket 6, Ticket 7 | Covered |
| 7 | Malformed YAML front matter produces WARN log and does not prevent startup | Ticket 4, Ticket 7 | Covered |
| 8 | Unrecognized tool name produces WARN log; agent registered with recognized tools only | Ticket 3, Ticket 4, Ticket 7 | Covered |
| 9 | `spawn_agent` tool no longer appears in tool list; sub-agents via `with_sub_agent()` | Ticket 6, Ticket 7 | Covered |
| 10 | Coordinator does not have `default_tools()` directly; only sub-agent and memory tools | Ticket 6, Ticket 7 | Covered |
| 11 | Model roster section in coordinator prompt when multiple models available | Ticket 3, Ticket 6, Ticket 7 | Covered |
| 12 | No model roster section when only a single model available (e.g. Ollama) | Ticket 3, Ticket 4, Ticket 7 | Covered |
| 13 | Unit tests for `load_user_sub_agents()` (valid, missing name, missing desc, missing ---, empty tools, absent dir) | Ticket 4, Ticket 7 | Covered |
| 14 | Unit tests for `coordinator_agent_prompt()` (agent listing, model roster present/absent) | Ticket 3, Ticket 7 | Covered |
| 15 | Unit tests for `tools_for_names()` (valid, unknown skipped, empty list) | Ticket 3, Ticket 7 | Covered |
| 16 | Unit tests for `load_model_roster()` (Anthropic-only, Ollama-only, user-configured merged) | Ticket 1, Ticket 4, Ticket 7 | Covered |
| 17 | `cargo clippy -- -D warnings` passes | Ticket 7 | Covered |
