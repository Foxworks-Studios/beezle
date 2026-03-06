# PRD 010: Multi-Agent System

**Status:** DRAFT (revised 2026-03-06)
**Created:** 2026-03-05
**Revised:** 2026-03-06
**Author:** PRD Writer Agent

---

## Problem Statement

The current `spawn_agent` tool (PRD 005) gives the parent a single generic
sub-agent with no fixed identity, no curated toolset, and no stable system
prompt. The LLM has no way to know what a sub-agent is good at, so delegation
decisions are arbitrary. Meanwhile, power users have no way to define custom
agents tailored to their workflow without modifying source code.

## Goals

- Replace the generic `spawn_agent` tool with role-specific sub-agents
  (`researcher` and `coder`) registered at startup via
  `Agent::with_sub_agent()`.
- Enable users to define additional sub-agents as Markdown files in
  `~/.beezle/agents/`, loaded at startup without any code changes.
- Generate the coordinator's system prompt dynamically so it always reflects
  the full set of available sub-agents (built-in + user-defined).

## Non-Goals

- Does not implement inter-agent communication beyond the existing
  parent-delegates-to-child model established by `yoagent`.
- Does not support nested sub-agents (a sub-agent spawning its own sub-agents).
- Does not provide a UI for creating or editing agent definition files.
- Does not validate that tools named in user-defined agent files are compatible
  with the running model or API tier.
- Does not hot-reload agent definition files while the process is running; the
  directory is scanned once at startup.
- Does not change the rendering/display architecture (that is PRD 011).

## Dependencies

- **PRD 011 (Event-Driven Rendering)** must be completed first. PRD 011 removes
  `SubAgentWrapper`, `ToolWrapper`, and `StreamProviderWrapper`, which this PRD
  also needs removed. Implementing in this order avoids conflicting changes.
- `yoagent` 0.5.3 -- `SubAgentTool`, `Agent::with_sub_agent()`, and the tool
  types (`ReadFileTool`, `WriteFileTool`, `EditFileTool`, `BashTool`,
  `ListFilesTool`, `SearchTool`) must be available and publicly exported.
  **Confirmed** in yoagent 0.5.3 source and `examples/sub_agent.rs`.
- `serde_yaml` crate -- for parsing YAML front matter in agent definition files.

## User Stories

- As a developer, I want beezle to automatically route research tasks (reading
  files, searching) to a dedicated `researcher` sub-agent, so that the
  coordinator context stays clean and the sub-agent's toolset is read-only.
- As a developer, I want beezle to automatically route coding tasks to a
  dedicated `coder` sub-agent with write access, so that I do not have to
  manually specify a system prompt each time I delegate implementation work.
- As a power user, I want to drop a Markdown file into `~/.beezle/agents/` and
  have a new sub-agent available the next time I launch beezle, so that I can
  extend the agent roster without rebuilding the binary.
- As a developer, I want the coordinator's system prompt to list every available
  sub-agent by name and description, so that the LLM makes informed delegation
  decisions without hallucinating agent names.

## Technical Approach

### Existing code to remove

After PRD 011 is complete, the following PRD-005 artifacts remain and are
removed by this PRD:

| Item | Location | Replacement |
|------|----------|-------------|
| `build_subagent()` | `src/agent/mod.rs` | `build_sub_agents()` in `src/agent/sub_agents.rs` |
| `spawn_agent` tool registration | `src/main.rs` `build_agent()` | `Agent::with_sub_agent()` calls |

### New module: `src/agent/sub_agents.rs`

Owns all sub-agent definition, loading, and construction logic. Exports:

```rust
/// A declarative sub-agent definition (built-in or user-defined).
pub struct SubAgentDef {
    pub name: String,
    pub description: String,
    /// If `None`, the sub-agent inherits the parent coordinator's model.
    pub model: Option<String>,
    pub max_turns: Option<usize>,
    /// Tool names as strings (e.g. "read_file"). If empty, gets default_tools().
    pub tools: Vec<String>,
    pub system_prompt: String,
}

/// Returns the hardcoded built-in sub-agent definitions.
pub fn builtin_sub_agents() -> Vec<SubAgentDef>;

/// Scans `~/.beezle/agents/*.md`, parses each file, and returns the
/// resulting definitions. Files that fail to parse are logged with
/// `tracing::warn!` and skipped.
pub fn load_user_sub_agents() -> Vec<SubAgentDef>;

/// Resolves tool name strings to yoagent tool instances.
pub fn tools_for_names(names: &[String]) -> Vec<Arc<dyn AgentTool>>;

/// Generates the coordinator system prompt section listing available agents.
pub fn coordinator_agent_prompt(agents: &[SubAgentDef]) -> String;

/// Constructs a SubAgentTool from a definition.
pub fn build_sub_agent(
    def: &SubAgentDef,
    provider: Arc<dyn StreamProvider>,
    parent_model: &str,
    api_key: &str,
) -> SubAgentTool;
```

### Agent definition file format

Files in `~/.beezle/agents/` use YAML front matter delimited by `---` lines,
with a Markdown body that becomes the system prompt.

```markdown
---
name: reviewer
description: Reviews code for bugs, style issues, and security vulnerabilities
model: claude-haiku-4-5-20251001
max_turns: 10
tools:
  - read_file
  - search
  - list_files
  - bash
---
You are a code reviewer. Analyze the code for:
- Bugs and logic errors
- Style and convention violations
- Security vulnerabilities
...
```

Required front-matter fields: `name`, `description`. Optional: `model`,
`max_turns`, `tools`.

- If `model` is absent, the agent inherits the parent coordinator's model.
- If `tools` is absent or empty, the agent gets `default_tools()`.
- If `max_turns` is absent, yoagent's default (10) applies.

Parsing steps:
1. Split file contents on the first and second `---` delimiters.
2. Deserialize the YAML block into a front-matter struct using `serde_yaml`.
3. Trim the remainder as the system prompt string.
4. Validate that `name` and `description` are non-empty; warn and skip on
   failure.

Add `serde_yaml` to `Cargo.toml`.

### Built-in agent definitions

Hardcoded in `builtin_sub_agents()`, following the yoagent `examples/sub_agent.rs`
pattern exactly:

| Name | Description | Tools | System prompt focus |
|------|-------------|-------|---------------------|
| `researcher` | Searches and reads files to gather information. Delegate research tasks here. | `read_file`, `search`, `list_files` | Read-only investigation; return structured summaries |
| `coder` | Writes and edits code files. Delegate coding tasks here. | `read_file`, `write_file`, `edit_file`, `bash` | Clean, correct, tested code; follow project conventions |

### Tool name -> yoagent type mapping

`tools_for_names()` resolves each string to the corresponding yoagent tool
constructor (all confirmed exported in yoagent 0.5.3):

| String name | Constructor |
|-------------|-------------|
| `read_file` | `tools::ReadFileTool::new()` |
| `write_file` | `tools::WriteFileTool::new()` |
| `edit_file` | `tools::EditFileTool::new()` |
| `list_files` | `tools::ListFilesTool::new()` |
| `search` | `tools::SearchTool::new()` |
| `bash` | `tools::BashTool::new()` |

Unknown names are logged with `tracing::warn!` and skipped. Each tool is
wrapped in `Arc` as required by `SubAgentTool::with_tools()`.

### Dynamic coordinator system prompt

`coordinator_agent_prompt()` generates a prompt section like:

```
You have the following sub-agents available:
- 'researcher': Searches and reads files to gather information.
- 'coder': Writes and edits code files.

Delegate tasks to the appropriate sub-agent. You can run both in parallel
when the tasks are independent. Only handle simple questions directly.
```

This is appended to the base system prompt (after project context and memory
injection).

### Registration flow in `build_agent()`

```
startup
  -> builtin_sub_agents()              // hardcoded Vec<SubAgentDef>
  -> load_user_sub_agents()            // scanned from ~/.beezle/agents/
  -> combined = builtin ++ user
  -> coordinator_agent_prompt(&combined)  // dynamic prompt section
  -> append to system_prompt
  -> for each SubAgentDef:
       sub = build_sub_agent(def, provider, parent_model, api_key)
       agent = agent.with_sub_agent(sub)   // yoagent's native API
```

The coordinator agent does NOT get `default_tools()` directly -- it delegates
all file/shell work to sub-agents. It only has sub-agent tools plus memory
tools. This keeps the coordinator's context clean and focused on delegation.

### Coordinator tool set

| Tool | Source |
|------|--------|
| `researcher` | `with_sub_agent()` |
| `coder` | `with_sub_agent()` |
| (user-defined agents) | `with_sub_agent()` |
| `memory_read` | `MemoryReadTool` (PRD 006) |
| `memory_write` | `MemoryWriteTool` (PRD 006) |

### Files changed

| File | Change |
|------|--------|
| `src/agent/mod.rs` | Remove `build_subagent()` |
| `src/agent/sub_agents.rs` | New file -- `SubAgentDef`, `builtin_sub_agents()`, `load_user_sub_agents()`, `coordinator_agent_prompt()`, `tools_for_names()`, `build_sub_agent()` |
| `src/main.rs` | Update `build_agent()` to use new registration flow; remove `spawn_agent` wiring; add coordinator prompt section |
| `Cargo.toml` | Add `serde_yaml` dependency |

## Acceptance Criteria

1. `cargo build` produces zero warnings and zero errors.
2. At startup, beezle registers exactly two built-in sub-agents named
   `researcher` and `coder`; tracing output at `DEBUG` level lists both names.
3. Placing a valid `.md` agent definition file in `~/.beezle/agents/` and
   restarting beezle causes that agent's name to appear in the `DEBUG`-level
   startup log alongside the built-in agents, without recompiling.
4. The coordinator's system prompt contains the name and description of every
   registered sub-agent, including any user-defined ones loaded from disk.
5. A malformed or missing YAML front matter in a `~/.beezle/agents/*.md` file
   produces a `WARN`-level log line identifying the file path and does not
   prevent beezle from starting.
6. A `.md` file in `~/.beezle/agents/` that lists an unrecognized tool name
   (e.g. `"fly_rocket"`) produces a `WARN`-level log line and the agent is
   registered with only the recognized tools; beezle still starts.
7. The old `spawn_agent` tool name no longer appears in the tool list; the
   registered sub-agents are invoked via `Agent::with_sub_agent()`.
8. The coordinator does not have `default_tools()` directly -- only sub-agents
   and memory tools.
9. Unit tests for `load_user_sub_agents()` cover: valid file, missing `name`
   field, missing `description` field, missing `---` delimiter, empty `tools`
   list, and absent `~/.beezle/agents/` directory.
10. Unit tests for `coordinator_agent_prompt()` assert the output contains each
    agent's `name` and `description` substring.
11. Unit tests for `tools_for_names()` cover: valid names, unknown names
    (skipped with warning), and empty list.
12. `cargo clippy -- -D warnings` passes.

## Previously Open Questions -- Now Resolved

- **Does `with_sub_agent()` accept custom tool lists?** Yes. Confirmed:
  `SubAgentTool::with_tools(Vec<Arc<dyn AgentTool>>)` accepts per-agent tool
  lists. The yoagent `examples/sub_agent.rs` demonstrates this exactly.
- **Should `~/.beezle/agents/` be created automatically?** No. Absence is
  silently accepted -- `load_user_sub_agents()` returns an empty vec if the
  directory does not exist.
- **Does yoagent export `SearchTool`?** Yes. Confirmed:
  `yoagent::tools::SearchTool::new()` is available.
