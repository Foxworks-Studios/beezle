# Tickets for PRD 007: Permissions System

**Source PRD:** /home/travis/Development/beezle/prd/007-permissions-system.md
**Created:** 2026-03-07
**Total Tickets:** 7
**Estimated Total Complexity:** 15 (S=1, M=2, L=3: 1+3+3+3+2+2+1=15)

---

### Ticket 1: Core Permission Types, `parse_rule()`, and `pattern_matches()`

**Description:**
Create `src/permissions/mod.rs` with all foundational types (`PermissionRule`,
`ToolCategory`, `PermissionVerdict`, `PermissionResponse`, `PermissionError`)
and the two pure functions `parse_rule()` and `pattern_matches()`. These are
the bedrock for every downstream ticket. Register the new module in `src/lib.rs`.

**Scope:**
- Create: `src/permissions/mod.rs`
- Modify: `src/lib.rs`

**Acceptance Criteria:**
- [ ] `src/permissions/mod.rs` compiles with all public types and `PermissionError` using `thiserror`.
- [ ] `parse_rule("Bash(cargo test:*)")` returns `PermissionRule { tool: "Bash", pattern: "cargo test:*" }`.
- [ ] `parse_rule("Read()")` returns `Ok` with an empty pattern (valid).
- [ ] `parse_rule("NoParen")` returns `Err(PermissionError::InvalidRule(...))`.
- [ ] `pattern_matches("cargo test:*", "cargo test --release")` returns `true` (`:*` prefix match).
- [ ] `pattern_matches("cargo test:*", "cargo fmt")` returns `false`.
- [ ] `pattern_matches("/src/**", "/src/main.rs")` returns `true` (`**` recursive glob).
- [ ] `pattern_matches("/src/**", "/tests/foo.rs")` returns `false`.
- [ ] `pattern_matches("/src/*.rs", "/src/main.rs")` returns `true` (`*` single segment).
- [ ] `pattern_matches("/src/*.rs", "/src/nested/main.rs")` returns `false`.
- [ ] `pattern_matches("domain:docs.rs", "https://docs.rs/tokio")` returns `true`.
- [ ] `pattern_matches("domain:docs.rs", "https://crates.io/tokio")` returns `false`.
- [ ] `pattern_matches("*", "anything")` returns `true` (bare wildcard).
- [ ] Unit tests for all cases above pass under `cargo test`.
- [ ] `cargo clippy -- -D warnings` passes with no warnings.

**Dependencies:** None
**Complexity:** M
**Maps to PRD AC:** AC 22, AC 23

---

### Ticket 2: `PermissionPolicy` — Settings Loading and `check()`

**Description:**
Implement `PermissionPolicy` in `src/permissions/mod.rs`: loading and merging
the three-tier settings files (`~/.beezle/settings.json`,
`.beezle/settings.json`, `.beezle/local.settings.json`), the `check()` method
with resolution order (session grants → deny → allow → category defaults), and
`grant_session()`. Add the `PermissionSettings` deserialization struct.

**Scope:**
- Modify: `src/permissions/mod.rs`

**Acceptance Criteria:**
- [ ] `PermissionPolicy::load(cwd)` reads all three tiers; missing files are silently ignored.
- [ ] Malformed JSON in any tier emits a `tracing::warn!` and skips that tier (no panic, no hard error).
- [ ] `allow` and `deny` lists from all three tiers are unioned (later tiers appended, not overwriting).
- [ ] `policy.check("bash", &args)` returns `Allow` when a matching allow rule exists and no deny rule matches.
- [ ] `policy.check("bash", &args)` returns `Deny` when a deny rule matches, even if an allow rule also matches.
- [ ] `policy.check("bash", &args)` returns `Ask` when no rule matches and the category default is `Ask`.
- [ ] `policy.check("read_file", &args)` returns `Allow` when no rules match (hardcoded `Read` category default).
- [ ] `grant_session()` adds a session grant that makes `check()` return `Allow` for subsequent matching calls.
- [ ] Unit test: three tiers merge correctly (rules from all three tiers are active).
- [ ] Unit test: deny-over-allow precedence confirmed.
- [ ] Unit test: session grant survives multiple `check()` calls.
- [ ] `cargo test` passes; `cargo clippy -- -D warnings` clean.

**Dependencies:** Ticket 1
**Complexity:** L
**Maps to PRD AC:** AC 3, AC 4, AC 5, AC 6, AC 7, AC 8, AC 9, AC 24, AC 25

---

### Ticket 3: Hooks Module (`src/permissions/hooks.rs`)

**Description:**
Create `src/permissions/hooks.rs` with `HookEventType`, `HookHandler`,
`HookInput`, `HookOutput`, `HookResult`, `HookManager`, and `execute_hook()`.
Implement the JSON stdin/stdout protocol: exit 0 parses `HookOutput`, exit 2
blocks with stderr reason, other non-zero exits are non-blocking logged errors,
and timeouts are enforced. Add the `regex` dependency to `Cargo.toml`.

**Scope:**
- Create: `src/permissions/hooks.rs`
- Modify: `src/permissions/mod.rs` (add `pub mod hooks;`)
- Modify: `Cargo.toml` (add `regex = "1"`)

**Acceptance Criteria:**
- [ ] `HookInput` variants for all eight lifecycle events serialize correctly to JSON with `hook_event_name` field.
- [ ] `execute_hook()` with a command that exits 0 and prints valid JSON returns `Ok(HookOutput { ... })`.
- [ ] `execute_hook()` with a command that exits 0 and prints empty stdout returns `Ok(HookOutput::default())`.
- [ ] `execute_hook()` with a command that exits 2 returns `Ok(HookOutput)` with `permission_decision: Some("deny")` and `stop_reason` set from stderr.
- [ ] `execute_hook()` with a command that exits 1 returns `Ok(HookOutput::default())` and logs a `WARN` (non-blocking).
- [ ] `execute_hook()` with a command that sleeps past `timeout_secs` returns an error/timeout variant.
- [ ] `HookManager::load(cwd)` reads hooks from the merged settings files; missing `hooks` key is treated as empty list.
- [ ] `HookHandler` with `matcher: Some(regex)` only fires for tool names matching the regex.
- [ ] `HookHandler` with `matcher: None` fires for all events of the configured type.
- [ ] `HookManager::run()` aggregates results: first `blocked=true` short-circuits remaining hooks.
- [ ] Unit tests for `HookInput` serialization for each of the eight event types pass.
- [ ] Unit tests for `execute_hook()` covering exit 0 (JSON), exit 0 (empty), exit 2, exit 1, timeout pass.
- [ ] `cargo test` passes; `cargo clippy -- -D warnings` clean.

**Dependencies:** Ticket 1
**Complexity:** L
**Maps to PRD AC:** AC 13, AC 14, AC 15, AC 16, AC 17, AC 18, AC 19, AC 20, AC 21, AC 26, AC 27

---

### Ticket 4: `PermissionGuard` — yoagent `AgentTool` Wrapper

**Description:**
Implement `PermissionGuard` in `src/permissions/mod.rs` (or a new
`src/permissions/guard.rs` sub-module). It wraps any `dyn AgentTool`, runs
pre/post hooks, checks `PermissionPolicy`, and sends a
`PermissionPromptRequest` over a `tokio::sync::broadcast` channel when the
verdict is `Ask`. Define the `PermissionPromptRequest` and `pending_responses`
map types. The prompt response is polled from a shared
`Arc<Mutex<HashMap<String, PermissionResponse>>>` keyed by request UUID.

**Scope:**
- Modify: `src/permissions/mod.rs` (add `PermissionGuard`, `PermissionPromptRequest`, prompt channel types)
- Create: `src/permissions/guard.rs` (if splitting out; wire with `mod guard` in `mod.rs`)

**Acceptance Criteria:**
- [ ] `PermissionGuard::new(inner, policy, hooks, prompt_tx, pending)` compiles and implements `AgentTool`.
- [ ] `name()`, `description()`, and `parameters_schema()` delegate to the inner tool.
- [ ] When `policy.check()` returns `Allow`, the inner tool's `execute()` is called without prompting.
- [ ] When `policy.check()` returns `Deny`, `execute()` returns a permission-denied error without calling the inner tool.
- [ ] When `policy.check()` returns `Ask`, a `PermissionPromptRequest` is broadcast and execution waits for a response.
- [ ] A `PermissionResponse::Yes` response allows the single invocation.
- [ ] A `PermissionResponse::No` response returns a permission-denied error.
- [ ] A `PermissionResponse::Always` response calls `policy.write().grant_session()` and then proceeds.
- [ ] `pre_tool_use` hooks run before the policy check; a `blocked=true` result short-circuits execution.
- [ ] `updated_input` from a hook replaces the params before the inner tool is called.
- [ ] `post_tool_use` hooks fire after a successful inner execution.
- [ ] `post_tool_use_failure` hooks fire after a failed inner execution.
- [ ] `cargo build` succeeds with no warnings.

**Dependencies:** Ticket 2, Ticket 3
**Complexity:** L
**Maps to PRD AC:** AC 10, AC 11, AC 12, AC 13, AC 14, AC 15, AC 16, AC 17, AC 18

---

### Ticket 5: Terminal Permission Prompt Display and Response

**Description:**
Extend `src/channels/terminal.rs` to subscribe to the `PermissionGuard`'s
broadcast channel and display the interactive `[Y]es / [N]o / [A]lways`
prompt when a `PermissionPromptRequest` arrives. Parse the user's keystroke
and write the `PermissionResponse` back into the shared `pending_responses`
map so `PermissionGuard` can unblock.

**Scope:**
- Modify: `src/channels/terminal.rs`

**Acceptance Criteria:**
- [ ] When a `PermissionPromptRequest` is received, the terminal prints a prompt in the format `? <tool>: <args>\n  [Y]es  [N]o  [A]lways`.
- [ ] Typing `y` or `Y` (then Enter) writes `PermissionResponse::Yes` into `pending_responses` for the request ID.
- [ ] Typing `n` or `N` (then Enter) writes `PermissionResponse::No` into `pending_responses`.
- [ ] Typing `a` or `A` (then Enter) writes `PermissionResponse::Always` into `pending_responses`.
- [ ] Unrecognized input re-displays the prompt without crashing.
- [ ] The prompt subscriber task does not block the main REPL input loop (runs as a concurrent `tokio::spawn`).
- [ ] `cargo build` succeeds with no warnings.

**Dependencies:** Ticket 4
**Complexity:** M
**Maps to PRD AC:** AC 2, AC 10, AC 11, AC 12

---

### Ticket 6: `main.rs` Wiring — Load Policy, Wrap Tools, Fire Session Hooks

**Description:**
Update `src/main.rs` to load `PermissionPolicy` and `HookManager` at startup,
wrap every tool from `default_tools()` and custom tools in a `PermissionGuard`,
pass the prompt broadcast channel to the terminal channel, and fire
`HookInput::SessionStart` / `HookInput::SessionEnd` at the appropriate
lifecycle points. Also fire `UserPromptSubmit` before dispatching each user
message.

**Scope:**
- Modify: `src/main.rs`

**Acceptance Criteria:**
- [ ] `PermissionPolicy::load(cwd)` and `HookManager::load(cwd)` are called before the agent is built.
- [ ] All tools passed to the agent are wrapped in `PermissionGuard`.
- [ ] The `prompt_tx` broadcast sender and `pending_responses` map are created and threaded through to both `PermissionGuard` instances and the terminal channel.
- [ ] `HookInput::SessionStart` fires before the REPL loop begins.
- [ ] `HookInput::SessionEnd` fires on clean exit (including `--prompt` single-shot mode).
- [ ] `HookInput::UserPromptSubmit` fires before each user message is dispatched to the agent.
- [ ] `cargo build` succeeds with no warnings.
- [ ] `cargo clippy -- -D warnings` passes.

**Dependencies:** Ticket 2, Ticket 3, Ticket 4, Ticket 5
**Complexity:** M
**Maps to PRD AC:** AC 1, AC 2, AC 28

---

### Ticket 7: Verification and Integration Test

**Description:**
Run the full PRD 007 acceptance criteria checklist end-to-end. Verify that all
tickets integrate correctly as a cohesive permissions system.

**Scope:**
- Modify: none (read-only verification; fix any integration issues found)

**Acceptance Criteria:**
- [ ] AC 1: `cargo build` produces zero warnings and zero errors.
- [ ] AC 2: `bash` tool prompts the user before executing when no allow rule matches.
- [ ] AC 3: `read_file` executes without prompting by default (Read category default = Allow).
- [ ] AC 4: `Bash(cargo test:*)` rule allows `cargo test` and `cargo test --release` without prompting.
- [ ] AC 5: Deny rule `Bash(rm -rf:*)` blocks even when `Bash(*)` is in the allow list.
- [ ] AC 6: Deny takes precedence over allow when both rules match.
- [ ] AC 7: Global, project, and local settings tiers all merge (rules from all three are active).
- [ ] AC 8: Missing settings files are silently ignored.
- [ ] AC 9: Malformed JSON in a settings file emits a `WARN` log and falls back to defaults.
- [ ] AC 10: Choosing "Yes" at the prompt allows the single invocation.
- [ ] AC 11: Choosing "No" at the prompt returns a permission denied error to the agent.
- [ ] AC 12: Choosing "Always" stops prompting for matching invocations for the rest of the session.
- [ ] AC 13: `pre_tool_use` hook stdin JSON contains `tool_name` and `tool_input` fields.
- [ ] AC 14: `pre_tool_use` hook returning `{"permission_decision": "deny"}` blocks execution.
- [ ] AC 15: `pre_tool_use` hook returning `{"updated_input": {...}}` replaces tool arguments.
- [ ] AC 16: `pre_tool_use` hook exiting with code 2 blocks execution; stderr is the reason.
- [ ] AC 17: `post_tool_use` hook fires after successful execution with `tool_output`.
- [ ] AC 18: `post_tool_use_failure` hook fires after failed execution with `error`.
- [ ] AC 19: Hook `matcher` regex filters by tool name.
- [ ] AC 20: Hook timeout defaults to 10 seconds; configurable per hook entry.
- [ ] AC 21: Non-blocking hook errors (exit 1) are logged and do not block execution.
- [ ] AC 22: `parse_rule()` unit tests pass for valid rules, missing parens, empty pattern.
- [ ] AC 23: `pattern_matches()` unit tests pass for exact match, `*`, `**`, `domain:`, `:*`.
- [ ] AC 24: `PermissionPolicy::check()` unit tests pass for all verdict paths.
- [ ] AC 25: Settings file merging unit tests pass across all three tiers.
- [ ] AC 26: `execute_hook()` unit tests pass for exit 0 (JSON), exit 0 (empty), exit 2, exit 1, timeout.
- [ ] AC 27: `HookInput` serialization unit tests pass for all eight event types.
- [ ] AC 28: `cargo clippy -- -D warnings` passes.
- [ ] No regressions in pre-existing tests (`cargo test`).
- [ ] `cargo fmt --check` passes.

**Dependencies:** All previous tickets
**Complexity:** S
**Maps to PRD AC:** AC 1–28

---

## AC Coverage Matrix

| PRD AC # | Description | Covered By Ticket(s) | Status |
|----------|-------------|----------------------|--------|
| 1 | `cargo build` zero warnings/errors | Ticket 6, Ticket 7 | Covered |
| 2 | `bash` prompts user when no allow rule matches | Ticket 5, Ticket 6, Ticket 7 | Covered |
| 3 | `read_file` executes without prompting by default | Ticket 2, Ticket 7 | Covered |
| 4 | `Bash(cargo test:*)` allows `cargo test` variants without prompting | Ticket 2, Ticket 7 | Covered |
| 5 | Deny rule blocks even when a broader allow rule matches | Ticket 2, Ticket 7 | Covered |
| 6 | Deny takes precedence over allow | Ticket 2, Ticket 7 | Covered |
| 7 | Three settings tiers merge correctly | Ticket 2, Ticket 7 | Covered |
| 8 | Missing settings files silently ignored | Ticket 2, Ticket 7 | Covered |
| 9 | Malformed settings files produce WARN log and fall back | Ticket 2, Ticket 7 | Covered |
| 10 | "Yes" allows single invocation | Ticket 4, Ticket 5, Ticket 7 | Covered |
| 11 | "No" returns permission denied error | Ticket 4, Ticket 5, Ticket 7 | Covered |
| 12 | "Always" stops prompting for rest of session | Ticket 4, Ticket 5, Ticket 7 | Covered |
| 13 | `pre_tool_use` hooks receive JSON with `tool_name` and `tool_input` | Ticket 3, Ticket 4, Ticket 7 | Covered |
| 14 | `pre_tool_use` hook `{"permission_decision": "deny"}` blocks | Ticket 3, Ticket 4, Ticket 7 | Covered |
| 15 | `pre_tool_use` hook `{"updated_input": ...}` replaces args | Ticket 3, Ticket 4, Ticket 7 | Covered |
| 16 | `pre_tool_use` hook exit 2 blocks; stderr is reason | Ticket 3, Ticket 7 | Covered |
| 17 | `post_tool_use` fires after success with `tool_output` | Ticket 3, Ticket 4, Ticket 7 | Covered |
| 18 | `post_tool_use_failure` fires after failure with `error` | Ticket 3, Ticket 4, Ticket 7 | Covered |
| 19 | Hook matchers filter by tool name regex | Ticket 3, Ticket 7 | Covered |
| 20 | Hook timeouts default 10s; configurable | Ticket 3, Ticket 7 | Covered |
| 21 | Non-blocking hook errors logged, do not block | Ticket 3, Ticket 7 | Covered |
| 22 | Unit tests for `parse_rule()` | Ticket 1, Ticket 7 | Covered |
| 23 | Unit tests for `pattern_matches()` | Ticket 1, Ticket 7 | Covered |
| 24 | Unit tests for `PermissionPolicy::check()` | Ticket 2, Ticket 7 | Covered |
| 25 | Unit tests for settings file merging | Ticket 2, Ticket 7 | Covered |
| 26 | Unit tests for `execute_hook()` | Ticket 3, Ticket 7 | Covered |
| 27 | Unit tests for `HookInput` serialization | Ticket 3, Ticket 7 | Covered |
| 28 | `cargo clippy -- -D warnings` passes | Ticket 6, Ticket 7 | Covered |
