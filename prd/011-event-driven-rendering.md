# PRD 011: Event-Driven Rendering

**Status:** TICKETS READY (revised 2026-03-06)
**Created:** 2026-03-06
**Revised:** 2026-03-06
**Author:** PRD Writer Agent

---

## Problem Statement

The current rendering architecture uses three wrapper structs to show real-time
output during the agent loop:

- **`StreamProviderWrapper`** intercepts `TextDelta` events from the LLM provider
  channel and prints them to stdout before forwarding.
- **`ToolWrapper`** wraps every tool to print start/end lines during `execute()`.
- **`SubAgentWrapper`** wraps the sub-agent tool to print `[sub]` progress via
  `on_update`/`on_progress` callback interception.

This works but has serious drawbacks:

1. **Fragile**: Wrappers must delegate every `AgentTool` trait method; adding a
   new trait method in yoagent silently breaks all wrappers.
2. **Duplicated logic**: `render_events()` must skip `ToolExecutionStart`,
   `ToolExecutionEnd`, and `TextDelta` events to avoid double-printing, since
   the wrappers already printed them. This coupling is invisible and error-prone.
3. **Inflexible**: Adding new event types (e.g. `ToolExecutionUpdate` for MCP
   servers, `ProgressMessage` for skills) requires modifying wrappers rather
   than just handling new events in one place.
4. **Blocks sub-agent improvements**: PRD 010 needs to register multiple
   sub-agents via `Agent::with_sub_agent()`, which bypasses the wrapper pattern
   entirely. The wrappers cannot survive the transition.

**This is now fully solvable.** The yoagent fork (`streaming-prompt` branch)
changed `Agent::prompt()` to spawn the agent loop on a background task,
returning the event receiver immediately. Events stream in real time. The
wrappers that existed solely to work around the buffered-events limitation
are now unnecessary.

## Goals

- Remove `StreamProviderWrapper`, `ToolWrapper`, `SubAgentWrapper`, and
  `wrap_tools()`. Replace with a single event-driven `run_prompt()` function
  that handles all display via `AgentEvent` pattern matching.
- Handle all `AgentEvent` variants in `run_prompt()`:
  `ToolExecutionStart`, `ToolExecutionEnd`, `ToolExecutionUpdate`,
  `MessageUpdate` (Text + Thinking), `ProgressMessage`, `AgentEnd`,
  `InputRejected`.
- Remove the thinking-indicator-then-drain pattern (`run_single_prompt` +
  `render_events`) since events now stream in real time.
- Add Ctrl+C cancellation via `agent.abort()` using `tokio::select!` in the
  event loop.
- Call `agent.finish().await` after draining events to restore agent state.
- Preserve all existing user-visible behavior: tool summaries, color gating,
  usage stats, error display.

## Non-Goals

- Does not change the agent construction or tool registration pattern (that is
  PRD 010).
- Does not add cost estimation or context bar (those are enhancements, not
  part of this refactor).
- Does not change the command bus or channel architecture.
- Does not add MCP server support (but the event loop will be ready for it).

## Prerequisites

- **yoagent `streaming-prompt` branch**: `Agent::prompt()` spawns the loop on a
  background task and returns the event receiver immediately. Events stream in
  real time. `Agent::finish()` restores state after draining.
  **Status: DONE** -- beezle's `Cargo.toml` already points to this branch.

## Technical Approach

### Event loop pattern

With real-time streaming, the pattern is straightforward:

```rust
async fn run_prompt(agent: &mut Agent, prompt: &str, use_color: bool) -> Usage {
    let mut rx = agent.prompt(prompt).await;
    let mut last_usage = Usage::default();
    let mut tool_starts: HashMap<String, Instant> = HashMap::new();

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    AgentEvent::ToolExecutionStart { tool_call_id, tool_name, args, .. } => {
                        tool_starts.insert(tool_call_id.clone(), Instant::now());
                        let summary = format_tool_summary(&tool_name, &args);
                        print!("{yellow}  > {summary}{reset}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, .. } => {
                        let elapsed = tool_starts.remove(&tool_call_id)
                            .map(|t| t.elapsed());
                        let duration = elapsed.map(|d| format!(" ({:.1}s)", d.as_secs_f64()))
                            .unwrap_or_default();
                        if is_error {
                            println!(" {red}x{duration}{reset}");
                        } else {
                            println!(" {green}ok{duration}{reset}");
                        }
                    }
                    AgentEvent::ToolExecutionUpdate { partial_result, .. } => {
                        // Stream partial results (sub-agents, MCP) in dim
                    }
                    AgentEvent::MessageUpdate { delta: StreamDelta::Text { delta }, .. } => {
                        print!("{delta}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageUpdate { delta: StreamDelta::Thinking { delta }, .. } => {
                        print!("{dim}{delta}{reset}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageEnd { message: AgentMessage::Llm(Message::Assistant {
                        stop_reason: StopReason::Error, error_message, ..
                    }) } => {
                        let msg = error_message.as_deref().unwrap_or("unknown error");
                        tracing::error!("{msg}");
                        println!("{red}  error: {msg}{reset}");
                    }
                    AgentEvent::AgentEnd { messages } => {
                        for msg in messages.iter().rev() {
                            if let AgentMessage::Llm(Message::Assistant { usage, .. }) = msg {
                                last_usage = usage.clone();
                                break;
                            }
                        }
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        println!("{dim}  {text}{reset}");
                    }
                    AgentEvent::InputRejected { reason } => {
                        println!("{red}  rejected: {reason}{reset}");
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                agent.abort();
                break;
            }
        }
    }

    // Restore agent state (messages, tools) from the completed background task.
    agent.finish().await;

    last_usage
}
```

### Structs and functions to remove

| Item | Location | Reason |
|------|----------|--------|
| `StreamProviderWrapper` | `src/main.rs` | Replaced by `MessageUpdate` event handling |
| `ToolWrapper` | `src/main.rs` | Replaced by `ToolExecutionStart`/`End` event handling |
| `SubAgentWrapper` | `src/main.rs` | Replaced by `ToolExecutionUpdate` + `ProgressMessage` |
| `wrap_tools()` | `src/main.rs` | No longer needed -- tools are not wrapped |
| `render_events()` | `src/main.rs` | Merged into `run_prompt()` |
| `run_single_prompt()` | `src/main.rs` | Merged into `run_prompt()` |
| `clear_thinking_line()` | `src/main.rs` | No longer needed -- text streams immediately |
| `fetch_thinking_label()` | `src/main.rs` | No longer needed -- no blocking wait to label |
| `thinking_label()` | `src/main.rs` | No longer needed -- no thinking indicator |
| `THINKING_LABELS` | `src/main.rs` | No longer needed |

### Functions that stay

| Item | Reason |
|------|--------|
| `format_tool_summary()` | Reused in `ToolExecutionStart` handler |
| `truncate()` | Reused by `format_tool_summary()` |
| Color constants/helpers | Reused throughout `run_prompt()` |
| `build_agent()` | Simplified (see below) |

### `build_agent()` simplification

Since tools are no longer wrapped:

1. Remove `use_color` parameter -- it no longer affects tool construction.
2. Pass `default_tools()` directly (no `wrap_tools()` call).
3. Remove `StreamProviderWrapper` -- pass the provider directly.
4. Add sub-agent tools directly (no `SubAgentWrapper`).
5. Add memory tools directly (no `ToolWrapper` wrapping).

```rust
fn build_agent(
    config: &AppConfig,
    model: &str,
    api_key: &str,
    skills: SkillSet,
    system_prompt: &str,
    memory_store: Option<Arc<MemoryStore>>,
) -> Agent {
    let (provider, model_cfg) = make_provider(config, model);

    let mut tools: Vec<Box<dyn AgentTool>> = default_tools();

    // Sub-agent tool (unwrapped)
    let subagent = build_subagent(...);
    tools.push(Box::new(subagent));

    // Memory tools (unwrapped)
    if let Some(store) = memory_store {
        tools.push(Box::new(MemoryReadTool::new(Arc::clone(&store))));
        tools.push(Box::new(MemoryWriteTool::new(store)));
    }

    let mut agent = Agent::new(provider);
    // ... model config, tools, skills, etc.
    agent
}
```

### Tool duration tracking

Use a `HashMap<String, Instant>` keyed by `tool_call_id` to track tool
durations (insert on `ToolExecutionStart`, remove and compute elapsed on
`ToolExecutionEnd`). This is an improvement over the current approach which
shows no duration.

### Ctrl+C handling

Replace the current `tokio::signal::ctrl_c()` handler (which calls
`std::process::exit(0)`) with `agent.abort()` inside the event loop's
`tokio::select!`. This allows clean cancellation mid-tool-execution.

The `agent.abort()` call cancels the background task's `CancellationToken`,
which propagates to the agent loop and any running tools. After breaking
from the select loop, `agent.finish().await` still runs to restore state.

### Thinking indicator

No longer needed. With real-time streaming, text deltas arrive as they are
generated. The `>` prompt already signals that input was accepted. The brief
pause before the first token arrives is acceptable.

### `fetch_thinking_label()` removal

The Haiku-based thinking label system (`fetch_thinking_label()`,
`thinking_label()`, `THINKING_LABELS`) exists solely to label the blocking
wait during `prompt()`. With streaming, there is no blocking wait, so the
entire thinking label system is removed.

### Files changed

| File | Change |
|------|--------|
| `src/main.rs` | Remove wrappers, `wrap_tools()`, `render_events()`, `run_single_prompt()`, `clear_thinking_line()`, `fetch_thinking_label()`, `thinking_label()`, `THINKING_LABELS`. Add `run_prompt()`. Simplify `build_agent()`. Update event loop and Ctrl+C handling. |

### Tests impacted

Tests that reference `ToolWrapper`, `SubAgentWrapper`, `wrap_tools()`,
`render_events()`, `run_single_prompt()`, or `format_tool_summary()` will need
updating. `format_tool_summary()` tests should be preserved. Wrapper-specific
tests are deleted since those types no longer exist:

| Test to delete | Reason |
|------|--------|
| `wrap_tools_skips_spawn_agent` | `wrap_tools()` removed |
| `wrap_tools_still_wraps_regular_tools` | `wrap_tools()` removed |
| `subagent_wrapper_on_update_receives_events` | `SubAgentWrapper` removed |
| `subagent_wrapper_progress_events_contain_text_deltas` | `SubAgentWrapper` removed |

New tests for `run_prompt()` should use `MockProvider` to verify:
- Text events are collected correctly
- Usage is accumulated from `AgentEnd` messages
- Tool execution events are processed without panic

## Acceptance Criteria

1. `StreamProviderWrapper`, `ToolWrapper`, `SubAgentWrapper`, `wrap_tools()`,
   `render_events()`, `run_single_prompt()`, `clear_thinking_line()`,
   `fetch_thinking_label()`, `thinking_label()`, and `THINKING_LABELS` are
   removed from `src/main.rs`.
2. A new `run_prompt()` function handles all agent output via `AgentEvent`
   matching in a `tokio::select!` loop.
3. `ToolExecutionStart` events print a tool summary line (yellow).
4. `ToolExecutionEnd` events print success/error status with elapsed duration.
5. `MessageUpdate::Text` events print text deltas to stdout as they arrive.
6. `MessageUpdate::Thinking` events print thinking deltas in dim text.
7. `AgentEnd` events accumulate usage across all assistant messages in the turn.
8. Ctrl+C during a prompt calls `agent.abort()` and returns cleanly without
   killing the process.
9. `agent.finish().await` is called after the event loop to restore state.
10. `build_agent()` no longer accepts a `use_color` parameter and passes tools
    unwrapped (no `StreamProviderWrapper`, no `ToolWrapper`, no
    `SubAgentWrapper`).
11. All existing tests pass or are updated; no wrapper-specific tests remain.
12. `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, and
    `cargo test` all pass.

## Dependencies

- yoagent `streaming-prompt` branch (DONE -- already in beezle's Cargo.toml).
