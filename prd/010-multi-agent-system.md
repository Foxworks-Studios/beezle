# PRD 010: Multi-Agent System

**Status:** TICKETS READY
**Created:** 2026-03-05
**Revised:** 2026-03-06
**Author:** PRD Writer Agent

---

## Problem Statement

The current `spawn_agent` tool (PRD 005) gives the parent a single generic
sub-agent with no fixed identity, no curated toolset, and no stable system
prompt. The LLM has no way to know what a sub-agent is good at, so delegation
decisions are arbitrary. Meanwhile, power users have no way to define custom
agents tailored to their workflow without modifying source code. Additionally,
there is no mechanism for sub-agents to use different models -- an explorer
should use a fast, cheap model (Haiku) while a coder should use the most
capable model (Opus).

## Goals

- Replace the generic `spawn_agent` tool with three role-specific built-in
  sub-agents (`researcher`, `coder`, `explorer`) registered at startup via
  `Agent::with_sub_agent()`.
- Each built-in sub-agent uses a model appropriate to its role (e.g. explorer
  uses Haiku, coder uses Opus, researcher uses Sonnet).
- Enable users to define additional sub-agents as Markdown files in
  `~/.beezle/agents/`, loaded at startup without any code changes.
- Introduce a model roster system: when multiple providers or models are
  configured, the coordinator's system prompt includes a "when to pick this
  model" guide so it can select the best model for one-off sub-agent tasks.
- Generate the coordinator's system prompt dynamically so it always reflects
  the full set of available sub-agents (built-in + user-defined) and the
  available model roster.

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

- **PRD 011 (Event-Driven Rendering)** -- completed. Wrappers (`SubAgentWrapper`,
  `ToolWrapper`, `StreamProviderWrapper`) removed; `build_agent()` simplified
  (no `use_color` param, provider is `Arc<dyn StreamProvider>`); rendering via
  single `run_prompt()` event loop. `Agent::prompt()` now streams events in
  real time and requires `agent.finish().await` after draining the receiver.
- `yoagent` 0.5.3 -- `SubAgentTool`, `Agent::with_sub_agent()`, and the tool
  types (`ReadFileTool`, `WriteFileTool`, `EditFileTool`, `BashTool`,
  `ListFilesTool`, `SearchTool`) must be available and publicly exported.
  **Confirmed** in yoagent 0.5.3 source and `examples/sub_agent.rs`.
  `SubAgentTool::with_model()` supports per-agent model selection. **Confirmed.**
- `serde_yaml` crate -- for parsing YAML front matter in agent definition files.

## User Stories

- As a developer, I want beezle to automatically route research tasks (reading
  files, searching code, exploring the codebase) to a dedicated `researcher`
  sub-agent, so that the coordinator context stays clean and the sub-agent
  uses a balanced model.
- As a developer, I want beezle to automatically route coding tasks to a
  dedicated `coder` sub-agent running the most capable model (Opus) with write
  access, so that complex implementation work gets the best reasoning.
- As a developer, I want quick file lookups routed to an `explorer` sub-agent
  running a fast, cheap model (Haiku), so simple reads don't burn expensive
  tokens.
- As a power user, I want to drop a Markdown file into `~/.beezle/agents/` and
  have a new sub-agent available the next time I launch beezle, so that I can
  extend the agent roster without rebuilding the binary.
- As a developer, I want the coordinator's system prompt to list every available
  sub-agent by name and description, so that the LLM makes informed delegation
  decisions without hallucinating agent names.
- As a developer using multiple providers (e.g. Anthropic + OpenAI), I want the
  coordinator to see a model roster with guidance on when to pick each model,
  so it can choose the right model for one-off delegated tasks.

## Technical Approach

### Existing code to remove

The following PRD-005 artifacts remain and are removed by this PRD:

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
    /// Model to use. If `None`, inherits the parent coordinator's model.
    pub model: Option<String>,
    pub max_turns: Option<usize>,
    /// Tool names as strings (e.g. "read_file"). If empty, gets default_tools().
    pub tools: Vec<String>,
    pub system_prompt: String,
}

/// Returns the hardcoded built-in sub-agent definitions.
/// Model fields use Anthropic model IDs by default; `resolve_sub_agent_model()`
/// maps them to the active provider's equivalent when building.
pub fn builtin_sub_agents() -> Vec<SubAgentDef>;

/// Scans `~/.beezle/agents/*.md`, parses each file, and returns the
/// resulting definitions. Files that fail to parse are logged with
/// `tracing::warn!` and skipped.
pub fn load_user_sub_agents() -> Vec<SubAgentDef>;

/// Resolves tool name strings to yoagent tool instances.
pub fn tools_for_names(names: &[String]) -> Vec<Arc<dyn AgentTool>>;

/// Generates the coordinator system prompt section listing available agents
/// and the model roster.
pub fn coordinator_agent_prompt(
    agents: &[SubAgentDef],
    model_roster: &[ModelEntry],
) -> String;

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

| Name | Description | Model (Anthropic) | Model (Ollama) | Tools | System prompt focus |
|------|-------------|-------------------|----------------|-------|---------------------|
| `explorer` | Searches and lists files to answer quick questions about the codebase. Fast and cheap. | `claude-haiku-4-5-20251001` | (inherit parent) | `read_file`, `search`, `list_files` | Read-only file exploration; return concise answers |
| `researcher` | Researches topics by reading files, searching code, and exploring the codebase in depth. | `claude-sonnet-4-6` | (inherit parent) | `read_file`, `search`, `list_files` | Deep research; return structured summaries with sources |
| `coder` | Writes, edits, and tests code. Use for implementation tasks. | `claude-opus-4-6` | (inherit parent) | `read_file`, `write_file`, `edit_file`, `bash` | Clean, correct, tested code; follow project conventions |

When the active provider is Ollama (or another local provider), the `model`
field falls back to the parent coordinator's model since local providers
typically have only one model loaded.

### Model roster system

The model roster provides the coordinator LLM with guidance on which model to
pick when delegating one-off tasks. This is useful when:

1. Multiple Anthropic models are available (Haiku, Sonnet, Opus)
2. Multiple providers are configured (Anthropic + OpenAI)
3. The coordinator wants to spawn a one-off sub-agent with a specific model

#### Configuration

Model roster entries come from two sources:

**1. Automatic (Anthropic):** When using Anthropic, the three standard tiers
are always available and documented automatically:

```rust
pub struct ModelEntry {
    pub id: String,           // e.g. "claude-haiku-4-5-20251001"
    pub provider: String,     // e.g. "anthropic"
    pub guidance: String,     // e.g. "Fast, cheap. Use for simple lookups..."
}
```

| Model | Guidance |
|-------|----------|
| `claude-haiku-4-5-20251001` | Fast and cheap. Use for simple lookups, formatting, classification, and tasks that don't need deep reasoning. |
| `claude-sonnet-4-6` | Balanced speed and capability. Use for research, summarization, code review, and moderate complexity tasks. |
| `claude-opus-4-6` | Most capable. Use for complex implementation, architecture decisions, subtle bugs, and tasks requiring deep reasoning. |

**2. User-configured:** Additional models from `agent.toml`:

```toml
[[models]]
id = "gpt-4o"
provider = "openai"
guidance = "Strong general-purpose model. Use when you need a second opinion or OpenAI-specific capabilities."

[[models]]
id = "gpt-4o-mini"
provider = "openai"
guidance = "Fast and cheap OpenAI model. Use for simple tasks when Anthropic quota is exhausted."
```

#### Coordinator prompt integration

`coordinator_agent_prompt()` appends a model roster section after the
sub-agent listing:

```
## Available Models

When spawning a one-off sub-agent, you can select the best model for the task:

- `claude-haiku-4-5-20251001` (anthropic): Fast and cheap. Use for simple lookups...
- `claude-sonnet-4-6` (anthropic): Balanced speed and capability...
- `claude-opus-4-6` (anthropic): Most capable. Use for complex implementation...
- `gpt-4o` (openai): Strong general-purpose model...

Choose the cheapest model that can handle the task well.
```

When only a single model is configured (e.g. Ollama with one model), the
roster section is omitted entirely.

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
| `web_search` | (not yet available -- see follow-up PRD) |
| `web_fetch` | (not yet available -- see follow-up PRD) |

Unknown names are logged with `tracing::warn!` and skipped. Each tool is
wrapped in `Arc` as required by `SubAgentTool::with_tools()`.

### Dynamic coordinator system prompt

`coordinator_agent_prompt()` generates a prompt section like:

```
## Sub-Agents

You have the following sub-agents available. Delegate tasks to the most
appropriate agent rather than doing everything yourself:

- 'explorer': Searches and lists files to answer quick questions about the
  codebase. Fast and cheap. (model: haiku)
- 'researcher': Researches topics by reading files, searching code, and
  exploring the codebase in depth. (model: sonnet)
- 'coder': Writes, edits, and tests code. Use for implementation tasks.
  (model: opus)

Guidelines:
- Use 'explorer' for simple file lookups, finding definitions, listing files.
- Use 'researcher' for deep investigation, reading docs, web searches.
- Use 'coder' for writing, editing, or refactoring code and running tests.
- Run multiple agents in parallel when tasks are independent.
- Only handle simple conversational responses directly.

## Available Models
[model roster section, if multiple models available]
```

This is appended to the base system prompt (after project context and memory
injection).

### Registration flow in `build_agent()`

```
startup
  -> builtin_sub_agents()              // hardcoded Vec<SubAgentDef>
  -> load_user_sub_agents()            // scanned from ~/.beezle/agents/
  -> combined = builtin ++ user
  -> load_model_roster(config)         // automatic + user-configured models
  -> coordinator_agent_prompt(&combined, &roster)  // dynamic prompt section
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
| `explorer` | `with_sub_agent()` |
| `researcher` | `with_sub_agent()` |
| `coder` | `with_sub_agent()` |
| (user-defined agents) | `with_sub_agent()` |
| `memory_read` | `MemoryReadTool` (PRD 006) |
| `memory_write` | `MemoryWriteTool` (PRD 006) |

### Files changed

| File | Change |
|------|--------|
| `src/agent/mod.rs` | Remove `build_subagent()` |
| `src/agent/sub_agents.rs` | New file -- `SubAgentDef`, `ModelEntry`, `builtin_sub_agents()`, `load_user_sub_agents()`, `load_model_roster()`, `coordinator_agent_prompt()`, `tools_for_names()`, `build_sub_agent()` |
| `src/main.rs` | Update `build_agent()` to use new registration flow; remove `spawn_agent` wiring; add coordinator prompt section |
| `src/config/mod.rs` | Add `[[models]]` table parsing to `AppConfig` |
| `Cargo.toml` | Add `serde_yaml` dependency |

## Acceptance Criteria

1. `cargo build` produces zero warnings and zero errors.
2. At startup, beezle registers exactly three built-in sub-agents named
   `explorer`, `researcher`, and `coder`; tracing output at `DEBUG` level
   lists all three names.
3. Each built-in sub-agent uses a different model: explorer uses Haiku,
   researcher uses Sonnet, coder uses Opus (when provider is Anthropic).
4. When the provider is Ollama, all sub-agents inherit the parent model.
5. Placing a valid `.md` agent definition file in `~/.beezle/agents/` and
   restarting beezle causes that agent's name to appear in the `DEBUG`-level
   startup log alongside the built-in agents, without recompiling.
6. The coordinator's system prompt contains the name, description, and model
   of every registered sub-agent, including any user-defined ones.
7. A malformed or missing YAML front matter in a `~/.beezle/agents/*.md` file
   produces a `WARN`-level log line identifying the file path and does not
   prevent beezle from starting.
8. A `.md` file in `~/.beezle/agents/` that lists an unrecognized tool name
   (e.g. `"fly_rocket"`) produces a `WARN`-level log line and the agent is
   registered with only the recognized tools; beezle still starts.
9. The old `spawn_agent` tool name no longer appears in the tool list; the
   registered sub-agents are invoked via `Agent::with_sub_agent()`.
10. The coordinator does not have `default_tools()` directly -- only sub-agents
    and memory tools.
11. When multiple models are available (Anthropic tiers or user-configured
    `[[models]]` entries), the coordinator's system prompt includes a model
    roster section with guidance for each model.
12. When only a single model is available (e.g. Ollama), no model roster
    section appears in the coordinator prompt.
13. Unit tests for `load_user_sub_agents()` cover: valid file, missing `name`
    field, missing `description` field, missing `---` delimiter, empty `tools`
    list, and absent `~/.beezle/agents/` directory.
14. Unit tests for `coordinator_agent_prompt()` assert the output contains each
    agent's `name`, `description`, and model info; and the model roster when
    multiple models are present.
15. Unit tests for `tools_for_names()` cover: valid names, unknown names
    (skipped with warning), and empty list.
16. Unit tests for `load_model_roster()` cover: Anthropic-only (3 auto entries),
    Ollama-only (no roster), and user-configured `[[models]]` entries merged
    with auto entries.
17. `cargo clippy -- -D warnings` passes.

## Previously Open Questions -- Now Resolved

- **Does `with_sub_agent()` accept custom tool lists?** Yes. Confirmed:
  `SubAgentTool::with_tools(Vec<Arc<dyn AgentTool>>)` accepts per-agent tool
  lists. The yoagent `examples/sub_agent.rs` demonstrates this exactly.
- **Does `SubAgentTool::with_model()` support per-agent models?** Yes.
  Confirmed: `SubAgentTool::with_model(impl Into<String>)` sets the model
  for a specific sub-agent independently of the parent.
- **Should `~/.beezle/agents/` be created automatically?** No. Absence is
  silently accepted -- `load_user_sub_agents()` returns an empty vec if the
  directory does not exist.
- **Does yoagent export `SearchTool`?** Yes. Confirmed:
  `yoagent::tools::SearchTool::new()` is available.

## Open Questions

None.

## Follow-Up Work

- **Web tools (separate PRD):** `web_search` and `web_fetch` tools need to be
  implemented as beezle-local tools (in `src/tools/`). Once available, add them
  to the researcher's tool set. The tool name mapping in `tools_for_names()`
  already reserves the `web_search` and `web_fetch` strings for when these are
  built.
