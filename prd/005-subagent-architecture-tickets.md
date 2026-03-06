# PRD 005: Black-Box Sub-Agent Architecture -- Tickets

## Important Context

yoagent (0.5.3) already provides `SubAgentTool` in `yoagent::sub_agent` with:
- Context isolation (fresh `AgentContext` with empty messages)
- Black-box result extraction (`extract_final_text`)
- Cancellation propagation (forwards parent's cancel token)
- Event forwarding via `on_update` and `on_progress` callbacks
- Max turns limiting (`with_max_turns`)
- Tool isolation (configurable tool set via `with_tools`)
- Builder pattern (`with_description`, `with_system_prompt`, `with_model`, `with_api_key`, `with_tools`, etc.)

Therefore, beezle does NOT need to reimplement `SubAgentTool`. Instead, this PRD's
scope reduces to:

1. A builder helper in `src/agent/mod.rs` that constructs pre-configured
   `SubAgentTool` instances from beezle's `AppConfig` (provider, model, API key)
2. Wiring the sub-agent tool into `build_agent()` in `main.rs` so the parent
   agent can invoke it
3. Real-time progress display via the existing `ToolWrapper` pattern
4. Tests proving context isolation, progress forwarding, and config-driven construction

---

## Ticket 01: Agent module with sub-agent builder helper

**Depends on**: None

**Scope**: Create `src/agent/mod.rs` with a helper that builds pre-configured
`SubAgentTool` instances from beezle's config, and register `pub mod agent` in
`src/lib.rs`.

**Requirements**:
- `build_subagent(name, description, system_prompt, config, model, api_key) -> SubAgentTool`
  - Reads `config.agent.default_provider` to pick the right `StreamProvider`
    (`AnthropicProvider` or `OpenAiCompatProvider` wrapped in `Arc`)
  - Sets `with_model`, `with_api_key`, `with_system_prompt`, `with_description`
  - Sets `with_tools(default_tools())` — sub-agents get the standard tool set
  - Sets `with_max_turns(15)` as the default sub-agent turn limit
  - Does NOT wrap tools in `ToolWrapper` (sub-agent tool output is forwarded
    via the parent's event stream, not printed directly)
- Register `pub mod agent` in `src/lib.rs`

**Tests** (using `MockProvider`):
- `build_subagent` returns a `SubAgentTool` that can be executed with a task
- Sub-agent runs with fresh context (no parent messages)
- Sub-agent returns only final text result
- Sub-agent errors on missing `task` parameter

**Files**: `src/agent/mod.rs`, `src/lib.rs`

---

## Ticket 02: Wire sub-agent into build_agent and main.rs

**Depends on**: Ticket 01

**Scope**: Add the sub-agent tool to the parent agent's tool set in `build_agent()`,
so the LLM can invoke `spawn_agent` during a conversation.

**Requirements**:
- In `build_agent()`, call `build_subagent()` to create a `SubAgentTool` named
  `"spawn_agent"` with a suitable description and system prompt
- Convert it to `Box<dyn AgentTool>` and include it in the tools vec passed to
  the agent (alongside existing `default_tools()`)
- The sub-agent tool should NOT be wrapped in `ToolWrapper` (its `on_update`
  events handle progress; wrapping would duplicate output)
- `wrap_tools()` should skip wrapping tools whose name is `"spawn_agent"`
  (or accept an exclusion list), since the sub-agent manages its own output
- Existing CLI tests, slash command tests, and format tests still pass

**Tests**:
- `build_agent()` includes `spawn_agent` in the agent's tool list (verify tool
  names contain `"spawn_agent"`)
- Integration test: mock agent calls `spawn_agent` tool, sub-agent runs and
  returns result to parent (using `MockProvider` for both parent and sub-agent)

**Files**: `src/main.rs`, `src/agent/mod.rs` (minor additions if needed)

---

## Ticket 03: Sub-agent progress display

**Depends on**: Ticket 02

**Scope**: Display real-time sub-agent progress in the terminal using the existing
`ToolWrapper` pattern and yoagent's `on_update`/`on_progress` callbacks.

**Requirements**:
- When `spawn_agent` is invoked, the `ToolWrapper` already prints the tool start
  line (e.g., `> spawn_agent`). The sub-agent's intermediate progress should
  also appear.
- Use `on_progress` callback on the `ToolContext` to print sub-agent status
  updates (tool calls, text deltas) to stdout in dim text while the sub-agent runs
- Progress output should be prefixed to distinguish from parent output
  (e.g., `  [sub] > read_file src/main.rs`)
- Progress output is ephemeral (printed to stdout but not stored in parent context)
- This may require adjusting how `ToolWrapper` handles sub-agent tools — instead
  of skipping the wrapper entirely, we may want a specialized wrapper that
  shows sub-agent progress

**Tests**:
- Sub-agent `on_update` callback receives events during execution
  (using `MockProvider` with tool calls)
- Verify progress event types match expected patterns (Started, ToolCall, etc.)

**Files**: `src/main.rs`
