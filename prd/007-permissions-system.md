# PRD 007: Permissions System

**Status:** DRAFT (revised 2026-03-06)
**Created:** 2026-03-04
**Revised:** 2026-03-06
**Author:** PRD Writer Agent

---

## Problem Statement

Currently all tools execute without user consent. The agent can run arbitrary
shell commands, write files, and access the network with no guardrails.
Claude Code has a pattern-based permission system with allow/deny rules,
tiered settings files, and tool hooks. Beezle needs the same.

## Goals

- Pattern-based permission rules matching Claude Code's format:
  `Tool(pattern)` syntax (e.g. `Bash(cargo test:*)`, `Read(/src/**)`,
  `WebFetch(domain:docs.rs)`).
- Three-tier settings files (JSON):
  - Global: `~/.beezle/settings.json`
  - Project (shared, committed): `.beezle/settings.json`
  - Local (personal, gitignored): `.beezle/local.settings.json`
- Each settings file has `allow` and `deny` lists. Deny takes precedence.
- Interactive permission prompt when no rule matches.
- Tool hooks: user-defined shell commands triggered by lifecycle events,
  following beezle-rs's JSON stdin/stdout protocol.

## Non-Goals

- Does not implement a TUI permission management interface.
- Does not support nested/recursive permission delegation.
- Does not hot-reload settings files while the process is running.
- Does not support MCP server permissions (deferred until MCP is added).

## Dependencies

- PRD 004 (command bus) -- for routing permission prompts to the active
  input channel.

## User Stories

- As a developer, I want `bash` commands to require approval by default so
  the agent can't run destructive commands without my knowledge.
- As a developer, I want to pre-approve safe patterns like
  `Bash(cargo test:*)` and `Read(/src/**)` so I'm not interrupted for
  routine operations.
- As a team lead, I want project-level permissions in `.beezle/settings.json`
  committed to git so all team members share the same safety baseline.
- As a developer, I want local overrides in `.beezle/local.settings.json`
  (gitignored) for personal preferences that don't affect the team.
- As a developer, I want to explicitly deny dangerous patterns like
  `Bash(rm -rf:*)` so they're blocked even if a broader allow rule exists.
- As a developer, I want hooks that run shell commands at lifecycle events
  (pre-tool, post-tool, session start/end) so I can integrate linters,
  formatters, and notifications.

## Technical Approach

### Permission rule format

Rules use Claude Code's `ToolName(pattern)` syntax:

```json
{
  "permissions": {
    "allow": [
      "Read(/var/home/travis/development/beezle/**)",
      "Bash(cargo test:*)",
      "Bash(cargo build:*)",
      "Bash(cargo clippy:*)",
      "Bash(cargo fmt:*)",
      "WebFetch(domain:docs.rs)",
      "WebFetch(domain:crates.io)",
      "WebFetch(domain:github.com)",
      "Skill(orchestrate)"
    ],
    "deny": [
      "Bash(rm -rf:*)",
      "Bash(git push --force:*)",
      "Bash(git reset --hard:*)"
    ]
  }
}
```

#### Pattern syntax

| Pattern | Matches |
|---------|---------|
| `Read(/src/**)` | `read_file` with any path under `/src/` |
| `Read(/src/*.rs)` | `read_file` with any `.rs` file directly in `/src/` |
| `Bash(cargo test:*)` | `bash` where command starts with `cargo test` |
| `Bash(*)` | Any bash command |
| `Write(/src/**)` | `write_file` / `edit_file` with path under `/src/` |
| `Edit(/src/**)` | `edit_file` with path under `/src/` |
| `WebFetch(domain:docs.rs)` | `web_fetch` calls to `docs.rs` |
| `Skill(orchestrate)` | The `orchestrate` skill |

The pattern inside parentheses is matched against the tool's primary argument:
- `Read`, `Write`, `Edit`: matched against the file path argument
- `Bash`: matched against the command string (`:*` = prefix match)
- `WebFetch`: `domain:X` matches the URL's domain
- `Skill`: matched against the skill name
- Other tools: matched against `serde_json::to_string(&args)`

`**` is a recursive glob. `*` matches any single path segment or suffix.

### Settings file hierarchy

Three tiers, merged in order (later tiers' rules are appended):

| Tier | File | Scope | In git? |
|------|------|-------|---------|
| 1 (lowest) | `~/.beezle/settings.json` | Global (all projects) | No |
| 2 | `.beezle/settings.json` | Project (shared) | Yes |
| 3 (highest) | `.beezle/local.settings.json` | Project (personal) | No (gitignored) |

Merge rules:
1. Start with empty allow/deny sets.
2. For each tier (global -> project -> local), union the `allow` and `deny`
   lists.
3. At evaluation time, **deny takes precedence over allow**. If a tool
   invocation matches both an allow and a deny rule, it is denied.

### Permission resolution hierarchy

Resolution order (first match wins):

1. **Session grants** -- runtime "Always" approvals (highest precedence).
2. **Deny rules** -- if any deny rule matches, block immediately.
3. **Allow rules** -- if any allow rule matches, proceed.
4. **Category defaults** -- hardcoded fallback by tool category.

#### Default policies (when no settings files exist)

| Category | Tools | Default |
|----------|-------|---------|
| Read | `read_file`, `list_files`, `search` | Allow |
| Write | `write_file`, `edit_file` | Ask |
| Execute | `bash` | Ask |
| Network | `web_fetch`, `web_search` | Ask |

### New module: `src/permissions/mod.rs`

```rust
/// A parsed permission rule like `Bash(cargo test:*)`.
#[derive(Debug, Clone)]
pub struct PermissionRule {
    pub tool: String,      // "Bash", "Read", "Write", "Edit", etc.
    pub pattern: String,   // "cargo test:*", "/src/**", "domain:docs.rs"
}

/// Tool category for default policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    Read,
    Write,
    Execute,
    Network,
}

/// The merged permission policy from all settings tiers.
pub struct PermissionPolicy {
    allow: Vec<PermissionRule>,
    deny: Vec<PermissionRule>,
    /// Session-scoped grants from interactive "Always" responses.
    session_grants: HashSet<String>,
}

/// Result of checking a tool invocation against the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionVerdict {
    Allow,
    Deny,
    Ask,
}

/// The user's response to a permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    Yes,
    No,
    Always,
}

impl PermissionPolicy {
    /// Load and merge all three settings tiers.
    pub fn load(cwd: &Path) -> Self;

    /// Check whether a tool invocation is allowed.
    pub fn check(&self, tool_name: &str, args: &serde_json::Value) -> PermissionVerdict;

    /// Add a session-scoped grant (from interactive "Always" response).
    pub fn grant_session(&mut self, tool_name: &str, args: &serde_json::Value);

    /// Categorize a tool name into a ToolCategory.
    pub fn categorize(tool_name: &str) -> ToolCategory;
}

/// Parse a rule string like "Bash(cargo test:*)" into a PermissionRule.
pub fn parse_rule(rule: &str) -> Result<PermissionRule, PermissionError>;

/// Check whether a pattern matches a value.
/// Supports `*` (single segment), `**` (recursive), `:*` (prefix), `domain:X`.
pub fn pattern_matches(pattern: &str, value: &str) -> bool;
```

### New module: `src/permissions/hooks.rs`

Hooks are shell commands triggered by lifecycle events. They receive JSON on
stdin and optionally return JSON on stdout. Exit codes determine behavior:
- **0**: Success. Parse stdout as `HookOutput` (empty = no-op).
- **2**: Block. stderr is the reason; tool execution is prevented.
- **Other non-zero**: Non-blocking error. Logged, execution continues.

This matches beezle-rs's hook protocol exactly.

#### Lifecycle events

| Event | When | Matcher target |
|-------|------|----------------|
| `pre_tool_use` | Before tool execution | tool name |
| `post_tool_use` | After successful tool execution | tool name |
| `post_tool_use_failure` | After failed tool execution | tool name |
| `user_prompt_submit` | When user sends a message | -- |
| `session_start` | When a session begins | source channel |
| `session_end` | When a session ends | -- |
| `subagent_start` | When a sub-agent spawns | agent name |
| `subagent_stop` | When a sub-agent completes | agent name |

#### Hook configuration in settings.json

```json
{
  "hooks": [
    {
      "event": "pre_tool_use",
      "matcher": "bash",
      "command": "~/.beezle/hooks/check-command.sh",
      "timeout_secs": 10
    },
    {
      "event": "post_tool_use",
      "matcher": "write_file|edit_file",
      "command": "cargo fmt",
      "timeout_secs": 30
    },
    {
      "event": "user_prompt_submit",
      "command": "~/.beezle/hooks/log-prompt.sh"
    }
  ]
}
```

- `event`: which lifecycle event to subscribe to.
- `matcher`: optional regex matched against the event's target (tool name,
  agent name, etc.). If absent, fires for all events of that type.
- `command`: shell command executed via `sh -c`.
- `timeout_secs`: optional timeout (default 10s).

#### Hook input/output JSON

Input (piped to stdin):
```json
{
  "hook_event_name": "pre_tool_use",
  "session_id": "abc123",
  "cwd": "/home/user/project",
  "tool_name": "bash",
  "tool_input": {"command": "rm -rf /tmp/build"}
}
```

Output (from stdout, optional):
```json
{
  "permission_decision": "deny",
  "additional_context": "Destructive command blocked by policy",
  "updated_input": null
}
```

Key output fields:
- `permission_decision`: `"allow"` or `"deny"` (pre_tool_use only).
- `updated_input`: replacement tool args JSON (pre_tool_use only).
- `additional_context`: text injected into the conversation.
- `continue_execution`: `false` to stop the agent.

```rust
/// Hook event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEventType {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    UserPromptSubmit,
    SessionStart,
    SessionEnd,
    SubagentStart,
    SubagentStop,
}

/// A configured hook handler.
pub struct HookHandler {
    pub event: HookEventType,
    pub matcher: Option<regex::Regex>,
    pub command: String,
    pub timeout_secs: u64,
}

/// JSON payload piped to hook commands on stdin.
#[derive(Serialize)]
#[serde(tag = "hook_event_name")]
pub enum HookInput { ... }

/// JSON payload returned from hook commands on stdout.
#[derive(Default, Deserialize)]
pub struct HookOutput {
    pub permission_decision: Option<String>,
    pub updated_input: Option<serde_json::Value>,
    pub additional_context: Option<String>,
    pub continue_execution: Option<bool>,
    pub stop_reason: Option<String>,
}

/// Aggregated result from running all matching hooks.
pub struct HookResult {
    pub blocked: bool,
    pub reason: Option<String>,
    pub updated_input: Option<serde_json::Value>,
    pub additional_context: Option<String>,
    pub should_stop: bool,
}

pub struct HookManager {
    handlers: Vec<HookHandler>,
}

impl HookManager {
    pub fn load(cwd: &Path) -> Self;
    pub async fn run(&self, input: &HookInput) -> HookResult;
}

/// Execute a single hook command.
pub async fn execute_hook(
    command: &str,
    input: &HookInput,
    timeout_secs: u64,
) -> Result<HookOutput, HookError>;
```

### Integration with yoagent: `PermissionGuard` wrapper

Since yoagent's agent loop calls `tool.execute()` directly with no middleware,
permissions are enforced by wrapping each tool in a `PermissionGuard` that
implements `AgentTool`:

```rust
pub struct PermissionGuard {
    inner: Box<dyn AgentTool>,
    policy: Arc<RwLock<PermissionPolicy>>,
    hooks: Arc<HookManager>,
    /// Channel to send permission prompts to the terminal.
    prompt_tx: broadcast::Sender<PermissionPromptRequest>,
    /// Shared map for polling responses.
    pending_responses: Arc<Mutex<HashMap<String, PermissionResponse>>>,
}

impl AgentTool for PermissionGuard {
    // name/label/description/parameters_schema delegate to inner.

    async fn execute(&self, params: Value, ctx: ToolContext) -> Result<ToolResult, ToolError> {
        // 1. Run pre_tool_use hooks (may block or modify input).
        let hook_result = self.hooks.run(&HookInput::PreToolUse { ... }).await;
        if hook_result.blocked { return Err(ToolError::blocked(hook_result.reason)); }
        let params = hook_result.updated_input.unwrap_or(params);

        // 2. Check permission policy.
        match self.policy.read().check(self.inner.name(), &params) {
            PermissionVerdict::Allow => {},
            PermissionVerdict::Deny => return Err(ToolError::permission_denied(...)),
            PermissionVerdict::Ask => {
                let response = self.prompt_user(...).await?;
                match response {
                    PermissionResponse::Yes => {},
                    PermissionResponse::Always => {
                        self.policy.write().grant_session(self.inner.name(), &params);
                    },
                    PermissionResponse::No => return Err(ToolError::permission_denied(...)),
                }
            }
        }

        // 3. Execute the actual tool.
        let result = self.inner.execute(params, ctx).await;

        // 4. Run post_tool_use / post_tool_use_failure hooks.
        match &result {
            Ok(r) => self.hooks.run(&HookInput::PostToolUse { ... }).await,
            Err(e) => self.hooks.run(&HookInput::PostToolUseFailure { ... }).await,
        };

        result
    }
}
```

### Interactive permission prompt

When a tool requires approval, the `PermissionGuard` broadcasts a
`PermissionPromptRequest` via the prompt channel. The terminal channel
displays:

```
  ? bash: cargo test --release
    [Y]es  [N]o  [A]lways
```

- **Yes**: allow this single invocation.
- **No**: deny; return permission error to the agent.
- **Always**: add a session-scoped grant for this tool+pattern; proceed.

The prompt uses a broadcast channel (from beezle-rs's pattern) so any
subscribed input channel can handle it. The response is polled from a shared
`HashMap<String, PermissionResponse>` keyed by request ID.

### Non-tool lifecycle hooks

Hooks for events outside of tool execution (session_start, session_end,
user_prompt_submit, subagent_start/stop) are triggered from `main()` and
`run_prompt()` at the appropriate points. These do not go through
`PermissionGuard` -- they are fired directly by the `HookManager`.

### Registration flow

In `build_agent()` / `main()`:

```
startup
  -> PermissionPolicy::load(cwd)
  -> HookManager::load(cwd)
  -> create prompt broadcast channel
  -> for each tool in tools:
       wrapped = PermissionGuard::new(tool, policy, hooks, prompt_tx, pending)
  -> agent.with_tools(wrapped_tools)
  -> hooks.run(SessionStart)
  ...
  -> on exit: hooks.run(SessionEnd)
```

### Files changed

| File | Change |
|------|--------|
| `src/permissions/mod.rs` | New -- `PermissionRule`, `PermissionPolicy`, `PermissionVerdict`, `PermissionGuard`, `parse_rule()`, `pattern_matches()` |
| `src/permissions/hooks.rs` | New -- `HookEventType`, `HookHandler`, `HookInput`, `HookOutput`, `HookResult`, `HookManager`, `execute_hook()` |
| `src/lib.rs` | Add `pub mod permissions;` |
| `src/main.rs` | Load policy + hooks at startup, wrap tools in `PermissionGuard`, fire session hooks |
| `src/channels/terminal.rs` | Handle permission prompt display and response |
| `Cargo.toml` | Add `regex` dependency (for hook matchers) |

## Acceptance Criteria

### Permissions
1. `cargo build` produces zero warnings and zero errors.
2. `bash` tool prompts the user before executing when no allow rule matches.
3. `read_file` executes without prompting by default (hardcoded category default).
4. A rule `Bash(cargo test:*)` allows `cargo test` and `cargo test --release`
   without prompting.
5. A deny rule `Bash(rm -rf:*)` blocks `rm -rf /` even if `Bash(*)` is in
   the allow list.
6. Deny rules take precedence over allow rules when both match.
7. Three settings tiers merge correctly: global -> project -> local.
8. Missing settings files are silently ignored (empty policy).
9. Malformed settings files produce a `WARN`-level log and fall back to
   defaults.
10. Choosing "Yes" allows the single invocation.
11. Choosing "No" returns a permission denied error to the agent.
12. Choosing "Always" stops prompting for matching invocations for the rest
    of the session.

### Hooks
13. `pre_tool_use` hooks receive JSON on stdin with `tool_name` and
    `tool_input` fields.
14. A `pre_tool_use` hook returning `{"permission_decision": "deny"}` blocks
    tool execution.
15. A `pre_tool_use` hook returning `{"updated_input": {...}}` replaces the
    tool arguments.
16. A `pre_tool_use` hook exiting with code 2 blocks tool execution; stderr
    is the reason.
17. `post_tool_use` hooks fire after successful execution with `tool_output`.
18. `post_tool_use_failure` hooks fire after failed execution with `error`.
19. Hook matchers filter by tool name regex.
20. Hook timeouts default to 10 seconds; configurable per hook.
21. Non-blocking hook errors (exit code != 0, != 2) are logged and do not
    block execution.

### Testing
22. Unit tests for `parse_rule()`: valid rules, missing parens, empty pattern.
23. Unit tests for `pattern_matches()`: exact match, `*` wildcard, `**` glob,
    `domain:` prefix, `:*` suffix.
24. Unit tests for `PermissionPolicy::check()`: allow match, deny match,
    deny-over-allow precedence, no match (Ask), session grants.
25. Unit tests for settings file merging across all three tiers.
26. Unit tests for `execute_hook()`: exit 0 with JSON, exit 0 empty stdout,
    exit 2 blocking, exit 1 non-blocking, timeout.
27. Unit tests for `HookInput` serialization for each event type.
28. `cargo clippy -- -D warnings` passes.

## Open Questions

- **Permission prompt routing for non-terminal channels:** When Discord/Slack
  channels are added, how should permission prompts be routed? For now, the
  prompt blocks the agent loop via the `PermissionGuard` and is displayed in
  the terminal. Future channels will subscribe to the broadcast channel and
  filter by `source_channel`, matching beezle-rs's pattern.
