//! Hook system for lifecycle event processing.
//!
//! Hooks are user-defined shell commands triggered by lifecycle events.
//! They receive JSON on stdin and optionally return JSON on stdout.
//! Exit codes determine behavior:
//! - **0**: Success. Parse stdout as [`HookOutput`] (empty = no-op).
//! - **2**: Block. stderr is the reason; tool execution is prevented.
//! - **Other non-zero**: Non-blocking error. Logged, execution continues.

use std::path::Path;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Default timeout for hook commands in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// Hook event types corresponding to lifecycle events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventType {
    /// Before tool execution.
    PreToolUse,
    /// After successful tool execution.
    PostToolUse,
    /// After failed tool execution.
    PostToolUseFailure,
    /// When user sends a message.
    UserPromptSubmit,
    /// When a session begins.
    SessionStart,
    /// When a session ends.
    SessionEnd,
    /// When a sub-agent spawns.
    SubagentStart,
    /// When a sub-agent completes.
    SubagentStop,
}

/// JSON payload piped to hook commands on stdin.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "hook_event_name", rename_all = "snake_case")]
pub enum HookInput {
    /// Pre-tool-use event.
    PreToolUse {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
        /// Name of the tool being invoked.
        tool_name: String,
        /// Tool input arguments.
        tool_input: serde_json::Value,
    },
    /// Post-tool-use event (success).
    PostToolUse {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
        /// Name of the tool that executed.
        tool_name: String,
        /// Tool input arguments.
        tool_input: serde_json::Value,
        /// Tool output result.
        tool_output: serde_json::Value,
    },
    /// Post-tool-use event (failure).
    PostToolUseFailure {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
        /// Name of the tool that failed.
        tool_name: String,
        /// Tool input arguments.
        tool_input: serde_json::Value,
        /// Error description.
        error: String,
    },
    /// User prompt submission event.
    UserPromptSubmit {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
        /// The user's prompt text.
        prompt: String,
    },
    /// Session start event.
    SessionStart {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
        /// Input channel source (e.g. "terminal").
        source_channel: String,
    },
    /// Session end event.
    SessionEnd {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
    },
    /// Sub-agent start event.
    SubagentStart {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
        /// Name of the sub-agent.
        agent_name: String,
    },
    /// Sub-agent stop event.
    SubagentStop {
        /// Session identifier.
        session_id: String,
        /// Current working directory.
        cwd: String,
        /// Name of the sub-agent.
        agent_name: String,
    },
}

impl HookInput {
    /// Returns the event type for this input.
    pub fn event_type(&self) -> HookEventType {
        match self {
            Self::PreToolUse { .. } => HookEventType::PreToolUse,
            Self::PostToolUse { .. } => HookEventType::PostToolUse,
            Self::PostToolUseFailure { .. } => HookEventType::PostToolUseFailure,
            Self::UserPromptSubmit { .. } => HookEventType::UserPromptSubmit,
            Self::SessionStart { .. } => HookEventType::SessionStart,
            Self::SessionEnd { .. } => HookEventType::SessionEnd,
            Self::SubagentStart { .. } => HookEventType::SubagentStart,
            Self::SubagentStop { .. } => HookEventType::SubagentStop,
        }
    }

    /// Returns the matcher target for this event (tool name, agent name, etc.).
    /// Returns `None` for events that have no matcher target.
    pub fn matcher_target(&self) -> Option<&str> {
        match self {
            Self::PreToolUse { tool_name, .. }
            | Self::PostToolUse { tool_name, .. }
            | Self::PostToolUseFailure { tool_name, .. } => Some(tool_name),
            Self::SessionStart { source_channel, .. } => Some(source_channel),
            Self::SubagentStart { agent_name, .. } | Self::SubagentStop { agent_name, .. } => {
                Some(agent_name)
            }
            Self::UserPromptSubmit { .. } | Self::SessionEnd { .. } => None,
        }
    }
}

/// JSON payload returned from hook commands on stdout.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct HookOutput {
    /// Permission decision: `"allow"` or `"deny"` (pre_tool_use only).
    pub permission_decision: Option<String>,
    /// Replacement tool args JSON (pre_tool_use only).
    pub updated_input: Option<serde_json::Value>,
    /// Text injected into the conversation.
    pub additional_context: Option<String>,
    /// Set to `false` to stop the agent.
    pub continue_execution: Option<bool>,
    /// Reason for stopping or blocking.
    pub stop_reason: Option<String>,
}

/// Aggregated result from running all matching hooks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HookResult {
    /// Whether execution was blocked by a hook.
    pub blocked: bool,
    /// Reason for blocking (if `blocked` is true).
    pub reason: Option<String>,
    /// Replacement tool args from a hook.
    pub updated_input: Option<serde_json::Value>,
    /// Additional context text from hooks.
    pub additional_context: Option<String>,
    /// Whether the agent should stop.
    pub should_stop: bool,
}

/// Errors that can occur during hook execution.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// The hook command timed out.
    #[error("hook timed out after {0} seconds")]
    Timeout(u64),
    /// An I/O error occurred spawning or communicating with the hook process.
    #[error("hook i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// Failed to serialize hook input to JSON.
    #[error("hook input serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// A configured hook handler from settings.
#[derive(Debug, Clone)]
pub struct HookHandler {
    /// Which lifecycle event this hook responds to.
    pub event: HookEventType,
    /// Optional regex to match against the event's target.
    pub matcher: Option<Regex>,
    /// Shell command to execute via `sh -c`.
    pub command: String,
    /// Timeout in seconds.
    pub timeout_secs: u64,
}

impl HookHandler {
    /// Returns `true` if this handler should fire for the given input.
    pub fn matches(&self, input: &HookInput) -> bool {
        if self.event != input.event_type() {
            return false;
        }
        match (&self.matcher, input.matcher_target()) {
            (Some(re), Some(target)) => re.is_match(target),
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

/// Raw hook configuration as deserialized from settings JSON.
#[derive(Debug, Deserialize)]
struct RawHookConfig {
    event: String,
    #[serde(default)]
    matcher: Option<String>,
    command: String,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

/// Top-level settings structure for hook extraction.
#[derive(Debug, Deserialize, Default)]
struct SettingsFile {
    #[serde(default)]
    hooks: Vec<RawHookConfig>,
}

/// Manages all configured hooks and dispatches events.
#[derive(Debug)]
pub struct HookManager {
    /// All configured hook handlers.
    handlers: Vec<HookHandler>,
}

impl HookManager {
    /// Creates a `HookManager` with no handlers.
    pub fn empty() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Creates a `HookManager` with the given handlers (for testing).
    pub fn from_handlers(handlers: Vec<HookHandler>) -> Self {
        Self { handlers }
    }

    /// Loads hooks from the merged settings files.
    ///
    /// Reads from three tiers:
    /// 1. `~/.beezle/settings.json` (global)
    /// 2. `<cwd>/.beezle/settings.json` (project)
    /// 3. `<cwd>/.beezle/local.settings.json` (local)
    ///
    /// Missing files or missing `hooks` keys are treated as empty lists.
    pub fn load(cwd: &Path) -> Self {
        let mut handlers = Vec::new();

        let global_path = dirs::home_dir().map(|h| h.join(".beezle").join("settings.json"));
        let project_path = cwd.join(".beezle").join("settings.json");
        let local_path = cwd.join(".beezle").join("local.settings.json");

        let paths: Vec<std::path::PathBuf> = [global_path, Some(project_path), Some(local_path)]
            .into_iter()
            .flatten()
            .collect();

        for path in paths {
            if let Ok(content) = std::fs::read_to_string(&path) {
                match serde_json::from_str::<SettingsFile>(&content) {
                    Ok(settings) => {
                        for raw in settings.hooks {
                            if let Some(handler) = Self::parse_handler(raw) {
                                handlers.push(handler);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "failed to parse hooks from settings file"
                        );
                    }
                }
            }
        }

        Self { handlers }
    }

    /// Parses a raw hook config into a `HookHandler`, returning `None` on failure.
    fn parse_handler(raw: RawHookConfig) -> Option<HookHandler> {
        let event = match raw.event.as_str() {
            "pre_tool_use" => HookEventType::PreToolUse,
            "post_tool_use" => HookEventType::PostToolUse,
            "post_tool_use_failure" => HookEventType::PostToolUseFailure,
            "user_prompt_submit" => HookEventType::UserPromptSubmit,
            "session_start" => HookEventType::SessionStart,
            "session_end" => HookEventType::SessionEnd,
            "subagent_start" => HookEventType::SubagentStart,
            "subagent_stop" => HookEventType::SubagentStop,
            unknown => {
                tracing::warn!(event = unknown, "unknown hook event type");
                return None;
            }
        };

        let matcher = raw.matcher.and_then(|m| {
            Regex::new(&m)
                .map_err(|e| {
                    tracing::warn!(matcher = m, error = %e, "invalid hook matcher regex");
                    e
                })
                .ok()
        });

        Some(HookHandler {
            event,
            matcher,
            command: raw.command,
            timeout_secs: raw.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
        })
    }

    /// Returns a reference to all handlers (for testing).
    pub fn handlers(&self) -> &[HookHandler] {
        &self.handlers
    }

    /// Runs all matching hooks for the given input, aggregating results.
    ///
    /// Short-circuits on the first hook that blocks execution.
    pub async fn run(&self, input: &HookInput) -> HookResult {
        let mut result = HookResult::default();

        for handler in &self.handlers {
            if !handler.matches(input) {
                continue;
            }

            match execute_hook(&handler.command, input, handler.timeout_secs).await {
                Ok(output) => {
                    if output.permission_decision.as_deref() == Some("deny") {
                        result.blocked = true;
                        result.reason = output.stop_reason.or(output.additional_context.clone());
                        return result;
                    }
                    if output.updated_input.is_some() {
                        result.updated_input = output.updated_input;
                    }
                    if output.additional_context.is_some() {
                        result.additional_context = output.additional_context;
                    }
                    if output.continue_execution == Some(false) {
                        result.should_stop = true;
                        result.reason = output.stop_reason;
                    }
                }
                Err(HookError::Timeout(secs)) => {
                    tracing::warn!(
                        command = handler.command,
                        timeout_secs = secs,
                        "hook timed out"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        command = handler.command,
                        error = %e,
                        "hook execution error"
                    );
                }
            }
        }

        result
    }
}

/// Execute a single hook command.
///
/// Spawns the command via `sh -c`, pipes the serialized [`HookInput`] to
/// stdin, and interprets the exit code and stdout/stderr according to the
/// hook protocol.
pub async fn execute_hook(
    command: &str,
    input: &HookInput,
    timeout_secs: u64,
) -> Result<HookOutput, HookError> {
    let input_json = serde_json::to_string(input)?;

    let mut child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // Write input to stdin.
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(input_json.as_bytes()).await?;
        // Drop stdin to close the pipe so the child can read EOF.
    }

    // Wait with timeout.
    let output = tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
        .await
        .map_err(|_| {
            // Kill the child process on timeout.
            // The child handle is consumed by wait_with_output, so we can't kill
            // it here directly. The drop of the child process handle will clean up.
            HookError::Timeout(timeout_secs)
        })?
        .map_err(HookError::Io)?;

    let exit_code = output.status.code().unwrap_or(1);

    match exit_code {
        0 => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                Ok(HookOutput::default())
            } else {
                serde_json::from_str(trimmed)
                    .map_err(|e| {
                        tracing::warn!(
                            stdout = %trimmed,
                            error = %e,
                            "failed to parse hook JSON output, treating as empty"
                        );
                        // Return default on parse error for non-blocking behavior.
                        e
                    })
                    .or_else(|_| Ok(HookOutput::default()))
            }
        }
        2 => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Ok(HookOutput {
                permission_decision: Some("deny".to_string()),
                stop_reason: if stderr.is_empty() {
                    None
                } else {
                    Some(stderr)
                },
                ..HookOutput::default()
            })
        }
        other => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(
                exit_code = other,
                stderr = %stderr.trim(),
                "hook exited with non-zero status (non-blocking)"
            );
            Ok(HookOutput::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── HookInput serialization tests ────────────────────────────────

    #[test]
    fn hook_input_pre_tool_use_serialization() {
        let input = HookInput::PreToolUse {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            tool_name: "bash".into(),
            tool_input: json!({"command": "ls"}),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "pre_tool_use");
        assert_eq!(json["tool_name"], "bash");
        assert_eq!(json["tool_input"]["command"], "ls");
    }

    #[test]
    fn hook_input_post_tool_use_serialization() {
        let input = HookInput::PostToolUse {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            tool_name: "read".into(),
            tool_input: json!({"path": "/file"}),
            tool_output: json!({"content": "data"}),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "post_tool_use");
        assert_eq!(json["tool_output"]["content"], "data");
    }

    #[test]
    fn hook_input_post_tool_use_failure_serialization() {
        let input = HookInput::PostToolUseFailure {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            tool_name: "bash".into(),
            tool_input: json!({}),
            error: "command failed".into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "post_tool_use_failure");
        assert_eq!(json["error"], "command failed");
    }

    #[test]
    fn hook_input_user_prompt_submit_serialization() {
        let input = HookInput::UserPromptSubmit {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            prompt: "hello".into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "user_prompt_submit");
        assert_eq!(json["prompt"], "hello");
    }

    #[test]
    fn hook_input_session_start_serialization() {
        let input = HookInput::SessionStart {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            source_channel: "terminal".into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "session_start");
        assert_eq!(json["source_channel"], "terminal");
    }

    #[test]
    fn hook_input_session_end_serialization() {
        let input = HookInput::SessionEnd {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "session_end");
        assert_eq!(json["session_id"], "s1");
    }

    #[test]
    fn hook_input_subagent_start_serialization() {
        let input = HookInput::SubagentStart {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            agent_name: "code-review".into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "subagent_start");
        assert_eq!(json["agent_name"], "code-review");
    }

    #[test]
    fn hook_input_subagent_stop_serialization() {
        let input = HookInput::SubagentStop {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            agent_name: "code-review".into(),
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["hook_event_name"], "subagent_stop");
        assert_eq!(json["agent_name"], "code-review");
    }

    // ── execute_hook tests ───────────────────────────────────────────

    fn test_input() -> HookInput {
        HookInput::PreToolUse {
            session_id: "test".into(),
            cwd: "/tmp".into(),
            tool_name: "bash".into(),
            tool_input: json!({"command": "ls"}),
        }
    }

    #[tokio::test]
    async fn execute_hook_exit_0_with_json() {
        let output = execute_hook(
            r#"echo '{"permission_decision":"allow","additional_context":"ok"}'"#,
            &test_input(),
            10,
        )
        .await
        .unwrap();

        assert_eq!(output.permission_decision, Some("allow".to_string()));
        assert_eq!(output.additional_context, Some("ok".to_string()));
    }

    #[tokio::test]
    async fn execute_hook_exit_0_empty_stdout() {
        let output = execute_hook("true", &test_input(), 10).await.unwrap();
        assert_eq!(output, HookOutput::default());
    }

    #[tokio::test]
    async fn execute_hook_exit_2_blocks() {
        let output = execute_hook("echo 'blocked by policy' >&2; exit 2", &test_input(), 10)
            .await
            .unwrap();

        assert_eq!(output.permission_decision, Some("deny".to_string()));
        assert_eq!(output.stop_reason, Some("blocked by policy".to_string()));
    }

    #[tokio::test]
    async fn execute_hook_exit_1_non_blocking() {
        let output = execute_hook("echo 'error' >&2; exit 1", &test_input(), 10)
            .await
            .unwrap();

        assert_eq!(output, HookOutput::default());
    }

    #[tokio::test]
    async fn execute_hook_timeout() {
        let result = execute_hook("sleep 30", &test_input(), 1).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, HookError::Timeout(1)),
            "expected Timeout(1), got: {err:?}"
        );
    }

    // ── HookHandler matching tests ───────────────────────────────────

    #[test]
    fn handler_with_matcher_filters_by_regex() {
        let handler = HookHandler {
            event: HookEventType::PreToolUse,
            matcher: Some(Regex::new("^bash$").unwrap()),
            command: "true".into(),
            timeout_secs: 10,
        };

        let matching = HookInput::PreToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "bash".into(),
            tool_input: json!({}),
        };
        assert!(handler.matches(&matching));

        let non_matching = HookInput::PreToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "read_file".into(),
            tool_input: json!({}),
        };
        assert!(!handler.matches(&non_matching));
    }

    #[test]
    fn handler_without_matcher_fires_for_all() {
        let handler = HookHandler {
            event: HookEventType::PreToolUse,
            matcher: None,
            command: "true".into(),
            timeout_secs: 10,
        };

        let input = HookInput::PreToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "anything".into(),
            tool_input: json!({}),
        };
        assert!(handler.matches(&input));
    }

    #[test]
    fn handler_wrong_event_does_not_match() {
        let handler = HookHandler {
            event: HookEventType::PostToolUse,
            matcher: None,
            command: "true".into(),
            timeout_secs: 10,
        };

        let input = HookInput::PreToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "bash".into(),
            tool_input: json!({}),
        };
        assert!(!handler.matches(&input));
    }

    // ── HookManager tests ────────────────────────────────────────────

    #[test]
    fn hook_manager_load_missing_dir() {
        let manager = HookManager::load(Path::new("/nonexistent/path"));
        assert!(manager.handlers().is_empty());
    }

    #[tokio::test]
    async fn hook_manager_load_from_settings() {
        let dir = tempfile::TempDir::new().unwrap();
        let beezle_dir = dir.path().join(".beezle");
        std::fs::create_dir_all(&beezle_dir).unwrap();
        std::fs::write(
            beezle_dir.join("settings.json"),
            r#"{
                "hooks": [
                    {
                        "event": "pre_tool_use",
                        "matcher": "bash",
                        "command": "true",
                        "timeout_secs": 5
                    },
                    {
                        "event": "session_start",
                        "command": "echo hello"
                    }
                ]
            }"#,
        )
        .unwrap();

        let manager = HookManager::load(dir.path());
        assert_eq!(manager.handlers().len(), 2);
        assert_eq!(manager.handlers()[0].event, HookEventType::PreToolUse);
        assert!(manager.handlers()[0].matcher.is_some());
        assert_eq!(manager.handlers()[0].timeout_secs, 5);
        assert_eq!(manager.handlers()[1].event, HookEventType::SessionStart);
        assert!(manager.handlers()[1].matcher.is_none());
        assert_eq!(manager.handlers()[1].timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[tokio::test]
    async fn hook_manager_run_aggregates_and_short_circuits() {
        let manager = HookManager {
            handlers: vec![
                HookHandler {
                    event: HookEventType::PreToolUse,
                    matcher: None,
                    command: r#"echo '{"additional_context":"context1"}'"#.into(),
                    timeout_secs: 10,
                },
                HookHandler {
                    event: HookEventType::PreToolUse,
                    matcher: None,
                    command: "echo 'blocked' >&2; exit 2".into(),
                    timeout_secs: 10,
                },
                HookHandler {
                    event: HookEventType::PreToolUse,
                    matcher: None,
                    command: r#"echo '{"additional_context":"should not run"}'"#.into(),
                    timeout_secs: 10,
                },
            ],
        };

        let input = HookInput::PreToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "bash".into(),
            tool_input: json!({}),
        };

        let result = manager.run(&input).await;
        assert!(result.blocked);
        assert_eq!(result.reason, Some("blocked".to_string()));
        // The additional_context from the first hook should be set before the block.
        // But since the second hook blocks, the result's additional_context is not
        // from the third hook.
    }

    #[tokio::test]
    async fn hook_manager_run_no_matching_hooks() {
        let manager = HookManager::empty();
        let input = HookInput::SessionEnd {
            session_id: "s".into(),
            cwd: "/".into(),
        };
        let result = manager.run(&input).await;
        assert!(!result.blocked);
        assert_eq!(result, HookResult::default());
    }

    #[test]
    fn hook_manager_load_missing_hooks_key() {
        let dir = tempfile::TempDir::new().unwrap();
        let beezle_dir = dir.path().join(".beezle");
        std::fs::create_dir_all(&beezle_dir).unwrap();
        std::fs::write(
            beezle_dir.join("settings.json"),
            r#"{"permissions": {"allow": []}}"#,
        )
        .unwrap();

        let manager = HookManager::load(dir.path());
        assert!(manager.handlers().is_empty());
    }

    #[test]
    fn handler_with_pipe_regex_matcher() {
        let handler = HookHandler {
            event: HookEventType::PostToolUse,
            matcher: Some(Regex::new("write_file|edit_file").unwrap()),
            command: "cargo fmt".into(),
            timeout_secs: 30,
        };

        let write_input = HookInput::PostToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "write_file".into(),
            tool_input: json!({}),
            tool_output: json!({}),
        };
        assert!(handler.matches(&write_input));

        let edit_input = HookInput::PostToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "edit_file".into(),
            tool_input: json!({}),
            tool_output: json!({}),
        };
        assert!(handler.matches(&edit_input));

        let bash_input = HookInput::PostToolUse {
            session_id: "s".into(),
            cwd: "/".into(),
            tool_name: "bash".into(),
            tool_input: json!({}),
            tool_output: json!({}),
        };
        assert!(!handler.matches(&bash_input));
    }
}
