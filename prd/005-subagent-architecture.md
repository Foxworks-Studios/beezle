# PRD 005: Black-Box Sub-Agent Architecture

## Summary

Define beezle's sub-agent spawning model: sub-agents receive a focused
preamble (not the parent's full context), execute independently, and return
only their final result. Optional progress callbacks allow the parent to
track status without inheriting intermediate noise.

## Problem

Naive sub-agent implementations leak the parent's full context into the
child, wasting tokens and polluting the child's reasoning. Worse, returning
all intermediate tool calls and thinking back to the parent bloats the
parent's context with irrelevant details. This is the "context pollution"
problem.

## Design Principles

1. **Black box**: The parent sees the sub-agent as a function call. It sends
   a task description and gets back a result string. Nothing in between.
2. **Clean fork**: The sub-agent starts with a fresh context containing only:
   - A system prompt (either default or task-specific)
   - The task description as the first user message
   - Tools appropriate to the task (subset of parent's tools)
3. **Result only**: The parent receives a single `ToolResult` with the
   sub-agent's final answer. Intermediate messages, tool calls, and thinking
   are discarded from the parent's perspective.
4. **Progress callbacks**: An optional callback channel lets the parent
   (or TUI) display progress indicators ("sub-agent: reading files...",
   "sub-agent: running tests...") without storing them in context.

## Scope

- `src/tools/subagent.rs` — `SubAgentTool` implementing yoagent's `AgentTool`
- `src/agent/mod.rs` — sub-agent builder helper

## Requirements

### Must Have

1. **SubAgentTool**: A yoagent `AgentTool` that:
   - Accepts parameters: `task` (string), `system_prompt` (optional string),
     `model` (optional string, defaults to parent's model).
   - Spawns a new `yoagent::Agent` with a clean message history.
   - Runs the agent loop to completion.
   - Returns only the final assistant message as the tool result.
2. **Tool isolation**: Sub-agent gets `default_tools()` (or a configurable
   subset). It does NOT inherit the parent's message history.
3. **Progress channel**: Accept an optional `tokio::sync::mpsc::Sender<SubAgentProgress>`
   that receives events like:
   - `Started { task_summary: String }`
   - `ToolCall { tool_name: String, summary: String }`
   - `Completed { result_preview: String }`
   - `Failed { error: String }`
4. **Cancellation**: Sub-agent respects the parent's cancellation token.
   If the parent is cancelled, the sub-agent aborts.

### Nice to Have

- Max iterations limit for sub-agents (default lower than parent, e.g. 15).
- Named sub-agents with different system prompts (e.g. "researcher",
  "coder", "reviewer") defined in config or skill files.

## Acceptance Criteria

- [ ] Parent agent can invoke `spawn_agent` tool with a task description
- [ ] Sub-agent runs with a fresh context (no parent messages)
- [ ] Parent receives only the final result text, not intermediate steps
- [ ] Progress events are emitted to the callback channel during execution
- [ ] Sub-agent cancellation works when parent is cancelled
- [ ] Sub-agent errors are returned as tool errors, not panics
- [ ] Unit tests with yoagent's MockProvider for deterministic sub-agent runs
- [ ] Integration test showing parent context size is not affected by
      sub-agent's intermediate work

## Dependencies

- None (can land independently, but benefits from PRD 004 bus for progress
  routing)

## Estimated Size

~2-3 files, ~250-350 lines + tests
