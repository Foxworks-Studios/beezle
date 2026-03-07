//! Permission guard wrapper for `AgentTool` implementations.
//!
//! [`PermissionGuard`] wraps any tool and enforces the permission policy,
//! runs pre/post hooks, and prompts the user when the verdict is `Ask`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex, RwLock, broadcast};
use yoagent::{AgentTool, ToolContext, ToolError, ToolResult};

use super::hooks::{HookInput, HookManager};
use super::{PermissionPolicy, PermissionResponse, PermissionVerdict};

/// Counter for generating unique request IDs.
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generates a unique request ID for permission prompts.
fn generate_request_id() -> String {
    let count = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    format!("perm-{ts}-{count}")
}

/// A request broadcast to the terminal (or other UI) asking the user
/// whether to allow a tool invocation.
#[derive(Debug, Clone)]
pub struct PermissionPromptRequest {
    /// Unique identifier for correlating the response.
    pub id: String,
    /// Name of the tool requesting permission.
    pub tool_name: String,
    /// The tool's input parameters.
    pub tool_input: serde_json::Value,
}

/// Wraps an [`AgentTool`] to enforce permissions, run hooks, and prompt
/// the user when the policy verdict is `Ask`.
pub struct PermissionGuard {
    /// The wrapped tool.
    inner: Box<dyn AgentTool>,
    /// Shared permission policy (read for checks, write for session grants).
    policy: Arc<RwLock<PermissionPolicy>>,
    /// Hook manager for pre/post tool-use lifecycle events.
    hooks: Arc<HookManager>,
    /// Broadcast sender for permission prompt requests.
    prompt_tx: broadcast::Sender<PermissionPromptRequest>,
    /// Shared map where prompt responses are deposited, keyed by request ID.
    pending_responses: Arc<Mutex<HashMap<String, PermissionResponse>>>,
    /// Session ID for hook inputs.
    session_id: String,
    /// Current working directory for hook inputs.
    cwd: String,
}

impl PermissionGuard {
    /// Creates a new `PermissionGuard` wrapping the given tool.
    pub fn new(
        inner: Box<dyn AgentTool>,
        policy: Arc<RwLock<PermissionPolicy>>,
        hooks: Arc<HookManager>,
        prompt_tx: broadcast::Sender<PermissionPromptRequest>,
        pending_responses: Arc<Mutex<HashMap<String, PermissionResponse>>>,
    ) -> Self {
        Self {
            inner,
            policy,
            hooks,
            prompt_tx,
            pending_responses,
            session_id: String::new(),
            cwd: String::new(),
        }
    }

    /// Sets the session ID used in hook inputs.
    pub fn with_session_id(mut self, session_id: String) -> Self {
        self.session_id = session_id;
        self
    }

    /// Sets the current working directory used in hook inputs.
    pub fn with_cwd(mut self, cwd: String) -> Self {
        self.cwd = cwd;
        self
    }

    /// Polls the pending responses map for a response to the given request ID.
    ///
    /// Polls every 50ms for up to 5 minutes before timing out.
    async fn wait_for_response(&self, request_id: &str) -> Result<PermissionResponse, ToolError> {
        let timeout = Duration::from_secs(300);
        let poll_interval = Duration::from_millis(50);
        let start = std::time::Instant::now();

        loop {
            {
                let mut map = self.pending_responses.lock().await;
                if let Some(response) = map.remove(request_id) {
                    return Ok(response);
                }
            }
            if start.elapsed() >= timeout {
                return Err(ToolError::Failed("permission prompt timed out".to_string()));
            }
            tokio::time::sleep(poll_interval).await;
        }
    }
}

#[async_trait]
impl AgentTool for PermissionGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let tool_name = self.inner.name().to_string();

        // 1. Run pre_tool_use hooks.
        let hook_result = self
            .hooks
            .run(&HookInput::PreToolUse {
                session_id: self.session_id.clone(),
                cwd: self.cwd.clone(),
                tool_name: tool_name.clone(),
                tool_input: params.clone(),
            })
            .await;

        if hook_result.blocked {
            return Err(ToolError::Failed(
                hook_result
                    .reason
                    .unwrap_or_else(|| "blocked by pre_tool_use hook".to_string()),
            ));
        }

        let params = hook_result.updated_input.unwrap_or(params);

        // 2. Check permission policy.
        let verdict = {
            let policy = self.policy.read().await;
            policy.check(&tool_name, &params)
        };

        match verdict {
            PermissionVerdict::Allow => {}
            PermissionVerdict::Deny => {
                return Err(ToolError::Failed(format!(
                    "permission denied for tool '{tool_name}'"
                )));
            }
            PermissionVerdict::Ask => {
                let request_id = generate_request_id();
                let request = PermissionPromptRequest {
                    id: request_id.clone(),
                    tool_name: tool_name.clone(),
                    tool_input: params.clone(),
                };

                // Broadcast the prompt request (ignore send errors if no receivers).
                let _ = self.prompt_tx.send(request);

                let response = self.wait_for_response(&request_id).await?;

                match response {
                    PermissionResponse::Yes => {}
                    PermissionResponse::No => {
                        return Err(ToolError::Failed(format!(
                            "permission denied for tool '{tool_name}'"
                        )));
                    }
                    PermissionResponse::Always => {
                        let mut policy = self.policy.write().await;
                        policy.grant_session(&tool_name, &params);
                    }
                }
            }
        }

        // 3. Execute the inner tool.
        let result = self.inner.execute(params.clone(), ctx).await;

        // 4. Run post hooks.
        match &result {
            Ok(tool_result) => {
                let output_value =
                    serde_json::to_value(tool_result).unwrap_or(serde_json::Value::Null);
                self.hooks
                    .run(&HookInput::PostToolUse {
                        session_id: self.session_id.clone(),
                        cwd: self.cwd.clone(),
                        tool_name: tool_name.clone(),
                        tool_input: params,
                        tool_output: output_value,
                    })
                    .await;
            }
            Err(e) => {
                self.hooks
                    .run(&HookInput::PostToolUseFailure {
                        session_id: self.session_id.clone(),
                        cwd: self.cwd.clone(),
                        tool_name: tool_name.clone(),
                        tool_input: params,
                        error: e.to_string(),
                    })
                    .await;
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use yoagent::Content;

    /// A simple mock tool for testing.
    struct MockTool {
        should_fail: bool,
    }

    impl MockTool {
        fn succeeding() -> Self {
            Self { should_fail: false }
        }

        fn failing() -> Self {
            Self { should_fail: true }
        }
    }

    #[async_trait]
    impl AgentTool for MockTool {
        fn name(&self) -> &str {
            "bash"
        }

        fn label(&self) -> &str {
            "Mock Bash"
        }

        fn description(&self) -> &str {
            "A mock tool for testing"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: ToolContext,
        ) -> Result<ToolResult, ToolError> {
            if self.should_fail {
                Err(ToolError::Failed("mock failure".to_string()))
            } else {
                Ok(ToolResult {
                    content: vec![Content::Text {
                        text: "ok".to_string(),
                    }],
                    details: json!({}),
                })
            }
        }
    }

    fn test_ctx() -> ToolContext {
        ToolContext {
            tool_call_id: "test-call".to_string(),
            tool_name: "bash".to_string(),
            cancel: tokio_util::sync::CancellationToken::new(),
            on_update: None,
            on_progress: None,
        }
    }

    fn make_guard(
        tool: Box<dyn AgentTool>,
        policy: PermissionPolicy,
    ) -> (
        PermissionGuard,
        broadcast::Receiver<PermissionPromptRequest>,
        Arc<Mutex<HashMap<String, PermissionResponse>>>,
    ) {
        let policy = Arc::new(RwLock::new(policy));
        let hooks = Arc::new(HookManager::empty());
        let (prompt_tx, prompt_rx) = broadcast::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let guard = PermissionGuard::new(tool, policy, hooks, prompt_tx, Arc::clone(&pending));
        (guard, prompt_rx, pending)
    }

    use super::super::PermissionRule;

    // ── Delegation tests ──────────────────────────────────────────

    #[test]
    fn delegates_name_to_inner() {
        let (guard, _, _) = make_guard(
            Box::new(MockTool::succeeding()),
            PermissionPolicy {
                allow: vec![],
                deny: vec![],
                session_grants: vec![],
            },
        );
        assert_eq!(guard.name(), "bash");
    }

    #[test]
    fn delegates_label_to_inner() {
        let (guard, _, _) = make_guard(
            Box::new(MockTool::succeeding()),
            PermissionPolicy {
                allow: vec![],
                deny: vec![],
                session_grants: vec![],
            },
        );
        assert_eq!(guard.label(), "Mock Bash");
    }

    #[test]
    fn delegates_description_to_inner() {
        let (guard, _, _) = make_guard(
            Box::new(MockTool::succeeding()),
            PermissionPolicy {
                allow: vec![],
                deny: vec![],
                session_grants: vec![],
            },
        );
        assert_eq!(guard.description(), "A mock tool for testing");
    }

    #[test]
    fn delegates_parameters_schema_to_inner() {
        let (guard, _, _) = make_guard(
            Box::new(MockTool::succeeding()),
            PermissionPolicy {
                allow: vec![],
                deny: vec![],
                session_grants: vec![],
            },
        );
        assert_eq!(guard.parameters_schema(), json!({"type": "object"}));
    }

    // ── Allow verdict tests ───────────────────────────────────────

    #[tokio::test]
    async fn allow_verdict_calls_inner_tool() {
        let policy = PermissionPolicy {
            allow: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            }],
            deny: vec![],
            session_grants: vec![],
        };
        let (guard, _, _) = make_guard(Box::new(MockTool::succeeding()), policy);
        let result = guard
            .execute(json!({"command": "cargo test"}), test_ctx())
            .await;
        assert!(result.is_ok());
    }

    // ── Deny verdict tests ────────────────────────────────────────

    #[tokio::test]
    async fn deny_verdict_returns_error_without_calling_inner() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            }],
            session_grants: vec![],
        };
        let (guard, _, _) = make_guard(Box::new(MockTool::succeeding()), policy);
        let result = guard
            .execute(json!({"command": "rm -rf /"}), test_ctx())
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("permission denied"));
    }

    // ── Ask verdict tests ─────────────────────────────────────────

    #[tokio::test]
    async fn ask_verdict_yes_allows_single_invocation() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        let (guard, mut prompt_rx, pending) = make_guard(Box::new(MockTool::succeeding()), policy);

        // Spawn a responder that answers Yes.
        tokio::spawn(async move {
            let request = prompt_rx.recv().await.unwrap();
            let mut map = pending.lock().await;
            map.insert(request.id, PermissionResponse::Yes);
        });

        let result = guard
            .execute(json!({"command": "cargo test"}), test_ctx())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn ask_verdict_no_returns_permission_denied() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        let (guard, mut prompt_rx, pending) = make_guard(Box::new(MockTool::succeeding()), policy);

        tokio::spawn(async move {
            let request = prompt_rx.recv().await.unwrap();
            let mut map = pending.lock().await;
            map.insert(request.id, PermissionResponse::No);
        });

        let result = guard
            .execute(json!({"command": "cargo test"}), test_ctx())
            .await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("permission denied")
        );
    }

    #[tokio::test]
    async fn ask_verdict_always_grants_session_and_proceeds() {
        let policy_inner = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        let policy = Arc::new(RwLock::new(policy_inner));
        let hooks = Arc::new(HookManager::empty());
        let (prompt_tx, mut prompt_rx) = broadcast::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));

        let guard = PermissionGuard::new(
            Box::new(MockTool::succeeding()),
            Arc::clone(&policy),
            hooks,
            prompt_tx,
            Arc::clone(&pending),
        );

        let pending_clone = Arc::clone(&pending);
        tokio::spawn(async move {
            let request = prompt_rx.recv().await.unwrap();
            let mut map = pending_clone.lock().await;
            map.insert(request.id, PermissionResponse::Always);
        });

        let result = guard
            .execute(json!({"command": "cargo test"}), test_ctx())
            .await;
        assert!(result.is_ok());

        // Verify that a session grant was added.
        let p = policy.read().await;
        assert!(!p.session_grants.is_empty());
    }

    // ── Hook tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn pre_hook_block_short_circuits_execution() {
        let policy = PermissionPolicy {
            allow: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            }],
            deny: vec![],
            session_grants: vec![],
        };
        let policy = Arc::new(RwLock::new(policy));

        // Create a hook manager with a blocking hook.
        let hooks = Arc::new(HookManager::from_handlers(vec![
            super::super::hooks::HookHandler {
                event: super::super::hooks::HookEventType::PreToolUse,
                matcher: None,
                command: "echo 'blocked by hook' >&2; exit 2".to_string(),
                timeout_secs: 10,
            },
        ]));

        let (prompt_tx, _) = broadcast::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let guard = PermissionGuard::new(
            Box::new(MockTool::succeeding()),
            policy,
            hooks,
            prompt_tx,
            pending,
        );

        let result = guard
            .execute(json!({"command": "bad stuff"}), test_ctx())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("blocked by hook"));
    }

    #[tokio::test]
    async fn pre_hook_updated_input_replaces_params() {
        let policy = PermissionPolicy {
            allow: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            }],
            deny: vec![],
            session_grants: vec![],
        };
        let policy = Arc::new(RwLock::new(policy));

        // Hook that returns updated_input.
        let hooks = Arc::new(HookManager::from_handlers(vec![
            super::super::hooks::HookHandler {
                event: super::super::hooks::HookEventType::PreToolUse,
                matcher: None,
                command: r#"echo '{"updated_input":{"command":"safe command"}}'"#.to_string(),
                timeout_secs: 10,
            },
        ]));

        let (prompt_tx, _) = broadcast::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));

        // Use a tool that echoes back params so we can verify updated_input was used.
        let guard = PermissionGuard::new(Box::new(EchoTool), policy, hooks, prompt_tx, pending);

        let result = guard
            .execute(json!({"command": "original command"}), test_ctx())
            .await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        // The EchoTool returns the params as content text.
        let text = &tool_result.content[0];
        match text {
            Content::Text { text } => {
                assert!(text.contains("safe command"));
            }
            _ => panic!("expected text content"),
        }
    }

    /// A tool that echoes its input params as text for testing updated_input.
    struct EchoTool;

    #[async_trait]
    impl AgentTool for EchoTool {
        fn name(&self) -> &str {
            "bash"
        }
        fn label(&self) -> &str {
            "Echo"
        }
        fn description(&self) -> &str {
            "Echoes params"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            json!({"type": "object"})
        }
        async fn execute(
            &self,
            params: serde_json::Value,
            _ctx: ToolContext,
        ) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                content: vec![Content::Text {
                    text: params.to_string(),
                }],
                details: json!({}),
            })
        }
    }

    #[tokio::test]
    async fn post_hook_fires_after_success() {
        let policy = PermissionPolicy {
            allow: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            }],
            deny: vec![],
            session_grants: vec![],
        };
        let policy = Arc::new(RwLock::new(policy));

        // Post hook that succeeds (just verify it doesn't break execution).
        let hooks = Arc::new(HookManager::from_handlers(vec![
            super::super::hooks::HookHandler {
                event: super::super::hooks::HookEventType::PostToolUse,
                matcher: None,
                command: "true".to_string(),
                timeout_secs: 10,
            },
        ]));

        let (prompt_tx, _) = broadcast::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let guard = PermissionGuard::new(
            Box::new(MockTool::succeeding()),
            policy,
            hooks,
            prompt_tx,
            pending,
        );

        let result = guard
            .execute(json!({"command": "cargo test"}), test_ctx())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn post_failure_hook_fires_after_failed_execution() {
        let policy = PermissionPolicy {
            allow: vec![PermissionRule {
                tool: "bash".to_string(),
                pattern: "*".to_string(),
            }],
            deny: vec![],
            session_grants: vec![],
        };
        let policy = Arc::new(RwLock::new(policy));

        // Post-failure hook.
        let hooks = Arc::new(HookManager::from_handlers(vec![
            super::super::hooks::HookHandler {
                event: super::super::hooks::HookEventType::PostToolUseFailure,
                matcher: None,
                command: "true".to_string(),
                timeout_secs: 10,
            },
        ]));

        let (prompt_tx, _) = broadcast::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let guard = PermissionGuard::new(
            Box::new(MockTool::failing()),
            policy,
            hooks,
            prompt_tx,
            pending,
        );

        let result = guard.execute(json!({"command": "fail"}), test_ctx()).await;
        assert!(result.is_err());
    }

    // ── PermissionPromptRequest tests ─────────────────────────────

    #[tokio::test]
    async fn ask_broadcasts_prompt_request_with_tool_info() {
        let policy = PermissionPolicy {
            allow: vec![],
            deny: vec![],
            session_grants: vec![],
        };
        let (guard, mut prompt_rx, pending) = make_guard(Box::new(MockTool::succeeding()), policy);

        let pending_clone = Arc::clone(&pending);
        tokio::spawn(async move {
            let request = prompt_rx.recv().await.unwrap();
            assert_eq!(request.tool_name, "bash");
            assert_eq!(request.tool_input, json!({"command": "cargo test"}));
            assert!(!request.id.is_empty());
            // Respond so the guard doesn't hang.
            let mut map = pending_clone.lock().await;
            map.insert(request.id, PermissionResponse::Yes);
        });

        let _ = guard
            .execute(json!({"command": "cargo test"}), test_ctx())
            .await;
    }

    #[test]
    fn generate_request_id_is_unique() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();
        assert_ne!(id1, id2);
    }
}
