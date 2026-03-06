# PRD 010: Multi-Agent System

**Status:** DRAFT
**Created:** 2026-03-05
**Author:** PRD Writer Agent

---

## Problem Statement

The current `spawn_agent` tool (PRD 005) gives the parent a single generic sub-agent with no fixed identity, no curated toolset, and no stable system prompt. The LLM has no way to know what a sub-agent is good at, so delegation decisions are arbitrary. Meanwhile, power users have no way to define custom agents tailored to their workflow without modifying source code.

## Goals

- Replace the generic `spawn_agent` tool with role-specific sub-agents (`researcher` and `coder`) that are registered at startup via `Agent::with_sub_agent()`.
- Enable users to define additional sub-agents as Markdown files in `~/.beezle/agents/`, loaded at startup without any code changes.
- Generate the coordinator's system prompt dynamically so it always reflects the full set of available sub-agents (built-in + user-defined).

## Non-Goals

- Does not implement inter-agent communication beyond the existing parent-delegates-to-child model already established by `yoagent`.
- Does not support nested sub-agents (a sub-agent spawning its own sub-agents).
- Does not provide a UI for creating or editing agent definition files.
- Does not validate that tools named in user-defined agent files are compatible with the running model or API tier.
- Does not hot-reload agent definition files while the process is running; the directory is scanned once at startup.
- Does not deprecate or gate the progress-callback infrastructure from PRD 005; that remains in place.

## User Stories

- As a developer, I want beezle to automatically route research tasks (reading files, searching) to a dedicated `researcher` sub-agent, so that the coordinator context stays clean and the sub-agent's toolset is appropriately read-only.
- As a developer, I want beezle to automatically route coding tasks to a dedicated `coder` sub-agent with write access, so that I do not have to manually specify a system prompt each time I delegate implementation work.
- As a power user, I want to drop a Markdown file into `~/.beezle/agents/` and have a new sub-agent available the next time I launch beezle, so that I can extend the agent roster without rebuilding the binary.
- As a developer, I want the coordinator's system prompt to list every available sub-agent by name and description, so that the LLM makes informed delegation decisions without hallucinating agent names.

## Technical Approach

### Existing code to remove / replace

`build_subagent()` in `src/agent/mod.rs` and `SubAgentWrapper` in `src/main.rs` (introduced by PRD 005) are replaced by the new registration flow below. The `spawn_agent` `AgentTool` implementation in `src/tools/subagent.rs` is also removed; `yoagent`'s native `SubAgentTool` (exposed via `with_sub_agent()`) replaces it.

### New module: `src/agent/sub_agents.rs`

Owns all sub-agent construction logic. Exports:

```rust
pub struct SubAgentDef {
    pub name: String,
    pub description: String,
    /// If `None`, the sub-agent inherits the parent coordinator's model.
    /// Users can override per-agent (e.g. use Haiku for fast cheap tasks).
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub tools: Vec<String>,      // tool names as strings, e.g. "read_file"
    pub system_prompt: String,
}
```

And two public functions:

```rust
/// Returns the hardcoded built-in sub-agent definitions.
pub fn builtin_sub_agents() -> Vec<SubAgentDef>;

/// Scans `~/.beezle/agents/*.md`, parses each file, and returns the
/// resulting definitions. Files that fail to parse are logged with
/// `tracing::warn!` and skipped — they do not abort startup.
pub fn load_user_sub_agents() -> Vec<SubAgentDef>;
```

### Agent definition file format

Files in `~/.beezle/agents/` use YAML front matter delimited by `---` lines, with a Markdown body that becomes the system prompt. Required front-matter fields: `name`, `description`. Optional: `model`, `max_turns`, `tools` (list of tool name strings). If `model` is absent, the agent inherits the parent coordinator's model (e.g. if the parent uses `claude-opus-4-6`, sub-agents without an explicit `model` also use it). If `tools` is absent, the agent inherits the full default tool set.

Parsing steps:
1. Split file contents on the first and second `---` delimiters.
2. Deserialize the YAML block into a front-matter struct using `serde_yaml`.
3. Trim the remainder as the system prompt string.
4. Validate that `name` and `description` are non-empty; warn and skip on failure.

Add `serde_yaml` to `Cargo.toml`.

### Built-in agent definitions (hardcoded in `builtin_sub_agents()`)

| Name | Description | Tools | System prompt focus |
|---|---|---|---|
| `researcher` | Gathers information, reads files, and summarizes findings | `read_file`, `search`, `list_files` | Read-only investigation; return structured summaries |
| `coder` | Writes, edits, and verifies code | `read_file`, `write_file`, `edit_file`, `bash` | Clean, correct, tested code; follow project conventions |

### Tool name -> yoagent type mapping

The `SubAgentDef.tools` field holds string names. When constructing a `SubAgentTool` via `with_sub_agent()`, a helper function `tools_for_names(names: &[String]) -> Vec<Box<dyn AgentTool>>` resolves each string to the corresponding yoagent tool type:

| String name | yoagent type |
|---|---|
| `read_file` | `ReadFileTool` |
| `write_file` | `WriteFileTool` |
| `edit_file` | `EditFileTool` |
| `list_files` | `ListFilesTool` |
| `search` | `SearchTool` |
| `bash` | `BashTool` |

Unknown names are logged with `tracing::warn!` and skipped.

### Dynamic coordinator system prompt

A new function `coordinator_system_prompt(agents: &[SubAgentDef]) -> String` in `src/agent/sub_agents.rs` generates the coordinator prompt. It renders a bullet list of `name: description` entries for every agent in the slice, then appends fixed guidance text about when to delegate vs. handle tasks directly.

### Registration flow in `src/agent/mod.rs`

```
startup
  -> builtin_sub_agents()          // hardcoded Vec<SubAgentDef>
  -> load_user_sub_agents()        // scanned from ~/.beezle/agents/
  -> combined = builtin + user
  -> coordinator_system_prompt(&combined)   // dynamic prompt
  -> for each SubAgentDef:
       model = def.model.unwrap_or(parent_model)  // inherit parent's model
       agent.with_sub_agent(...)
```

`src/main.rs` calls the updated `build_agent()` (or equivalent) function; it no longer needs `SubAgentWrapper`.

### Files changed

| File | Change |
|---|---|
| `src/agent/mod.rs` | Remove `build_subagent()`, update agent construction to use new registration flow |
| `src/agent/sub_agents.rs` | New file — `SubAgentDef`, `builtin_sub_agents()`, `load_user_sub_agents()`, `coordinator_system_prompt()`, `tools_for_names()` |
| `src/tools/subagent.rs` | Remove (or gut) the old `spawn_agent` `AgentTool` |
| `src/main.rs` | Remove `SubAgentWrapper`; call updated agent builder |
| `Cargo.toml` | Add `serde_yaml` dependency |

## Acceptance Criteria

1. Running `cargo build` produces zero warnings and zero errors after this change is applied.
2. At startup, beezle registers exactly two built-in sub-agents named `researcher` and `coder`; tracing output at `DEBUG` level lists both names.
3. Placing a valid `.md` agent definition file in `~/.beezle/agents/` and restarting beezle causes that agent's name to appear in the `DEBUG`-level startup log alongside the built-in agents, without recompiling.
4. The coordinator's system prompt (logged at `TRACE` level during agent construction) contains the name and description of every registered sub-agent, including any user-defined ones loaded from disk.
5. A malformed or missing YAML front matter in a `~/.beezle/agents/*.md` file produces a `WARN`-level log line identifying the file path and does not prevent beezle from starting.
6. A `.md` file in `~/.beezle/agents/` that lists an unrecognized tool name (e.g. `"fly_rocket"`) produces a `WARN`-level log line and the agent is registered with only the recognized tools; beezle still starts.
7. The old `spawn_agent` tool name no longer appears in the list of tools the coordinator exposes to the LLM; the registered sub-agents are invoked via `yoagent`'s native `with_sub_agent()` mechanism.
8. Unit tests for `load_user_sub_agents()` cover: valid file, missing `name` field, missing `description` field, missing `---` delimiter, and empty `tools` list; all tests pass under `cargo test`.
9. Unit tests for `coordinator_system_prompt()` assert that the output string contains each agent's `name` and `description` substring when given a slice of two or more `SubAgentDef` values.
10. `cargo clippy -- -D warnings` passes with no suppressed lints.

## Open Questions

- Does `yoagent` 0.5.3's `with_sub_agent()` accept a fully custom tool list per sub-agent, or does it only accept the default tool set? If tool customization is not supported, the `tools` field in `SubAgentDef` becomes advisory only and the tool-name mapping table is deferred.
- Should `~/.beezle/agents/` be created automatically on first launch if it does not exist, or should absence be silently accepted? (Default assumption: absence is silently accepted; no directory creation.)
- The `search` tool name maps to `SearchTool` — this needs confirmation that `yoagent` 0.5.3 exports a type by that exact name.

## Dependencies

- PRD 005 (sub-agent black-box architecture) — this PRD replaces the `spawn_agent` tool built there; the progress-callback infrastructure from PRD 005 may be retained or removed based on whether `yoagent`'s native `with_sub_agent()` provides equivalent observability.
- `yoagent` 0.5.3 — `SubAgentTool`, `Agent::with_sub_agent()`, and the tool types (`ReadFileTool`, `WriteFileTool`, `EditFileTool`, `BashTool`, `ListFilesTool`, `SearchTool`) must be available and publicly exported.
- `serde_yaml` crate — for parsing YAML front matter in agent definition files.
