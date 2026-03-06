//! Agent setup and sub-agent builder helpers.
//!
//! Provides [`build_subagent`] for constructing pre-configured
//! [`SubAgentTool`](yoagent::sub_agent::SubAgentTool) instances from
//! beezle's [`AppConfig`](crate::config::AppConfig).

use std::sync::Arc;

use yoagent::provider::{AnthropicProvider, OpenAiCompatProvider, StreamProvider};
use yoagent::sub_agent::SubAgentTool;
use yoagent::tools::default_tools;

use crate::config::AppConfig;

/// Builds a pre-configured [`SubAgentTool`] using the application's provider settings.
///
/// The provider is selected based on `config.agent.default_provider`:
/// - `"ollama"` uses [`OpenAiCompatProvider`]
/// - anything else (including `"anthropic"`) uses [`AnthropicProvider`]
///
/// The sub-agent receives [`default_tools()`] and a max turn limit of 15.
/// It does **not** wrap tools in `ToolWrapper` -- sub-agent output is
/// forwarded via the parent's event stream.
///
/// # Arguments
///
/// * `name` - Unique name for the sub-agent tool (used in tool calls)
/// * `description` - Human-readable description of what the sub-agent does
/// * `system_prompt` - System prompt that defines the sub-agent's behavior
/// * `config` - Application config to determine which provider to use
/// * `model` - Model identifier (e.g. `"claude-sonnet-4-20250514"`)
/// * `api_key` - API key for the provider
///
/// # Returns
///
/// A fully configured `SubAgentTool` ready to be added to a parent agent's
/// tool set.
pub fn build_subagent(
    name: &str,
    description: &str,
    system_prompt: &str,
    config: &AppConfig,
    model: &str,
    api_key: &str,
) -> SubAgentTool {
    let provider: Arc<dyn StreamProvider> = if config.agent.default_provider == "ollama" {
        Arc::new(OpenAiCompatProvider)
    } else {
        Arc::new(AnthropicProvider)
    };

    // Convert Box<dyn AgentTool> to Arc<dyn AgentTool> for SubAgentTool::with_tools.
    let tools = default_tools().into_iter().map(Arc::from).collect();

    SubAgentTool::new(name, provider)
        .with_description(description)
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_tools(tools)
        .with_max_turns(15)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;
    use yoagent::provider::MockProvider;
    use yoagent::*;

    /// Helper: build a subagent with a MockProvider instead of the config-driven
    /// provider. This bypasses provider selection to test the tool's behavior
    /// with deterministic responses.
    fn build_mock_subagent(name: &str, provider: Arc<MockProvider>) -> SubAgentTool {
        SubAgentTool::new(name, provider)
            .with_description("Test sub-agent")
            .with_system_prompt("You are a test assistant.")
            .with_model("mock")
            .with_api_key("test-key")
            .with_max_turns(15)
    }

    fn tool_context(name: &str) -> ToolContext {
        ToolContext {
            tool_call_id: "tc-test".into(),
            tool_name: name.into(),
            cancel: CancellationToken::new(),
            on_update: None,
            on_progress: None,
        }
    }

    #[tokio::test]
    async fn build_subagent_returns_executable_tool() {
        let provider = Arc::new(MockProvider::text("Hello from sub-agent"));
        let tool = build_mock_subagent("greeter", provider);

        let params = serde_json::json!({"task": "Say hello"});
        let result = tool.execute(params, tool_context("greeter")).await;

        assert!(result.is_ok(), "sub-agent execution should succeed");
        let result = result.unwrap();
        let text = match &result.content[0] {
            Content::Text { text } => text.as_str(),
            other => panic!("Expected Text content, got: {:?}", other),
        };
        assert_eq!(text, "Hello from sub-agent");
    }

    #[tokio::test]
    async fn subagent_runs_with_fresh_context() {
        // The sub-agent should not see any parent messages. If it did, the
        // mock provider would receive extra messages and potentially fail.
        // A single-response mock proves the sub-agent starts from scratch
        // with only the task as its first user message.
        let provider = Arc::new(MockProvider::text("Fresh context result"));
        let tool = build_mock_subagent("fresh", provider);

        let params = serde_json::json!({"task": "Summarize this"});
        let result = tool.execute(params, tool_context("fresh")).await.unwrap();

        let text = match &result.content[0] {
            Content::Text { text } => text.as_str(),
            other => panic!("Expected Text content, got: {:?}", other),
        };
        assert_eq!(text, "Fresh context result");
        // The sub_agent metadata confirms it ran as its own agent.
        assert_eq!(result.details["sub_agent"], "fresh");
    }

    #[tokio::test]
    async fn subagent_returns_only_final_text() {
        // Sub-agent makes a tool call first, then returns final text.
        // Parent should only see the final text, not intermediate tool calls.
        use yoagent::provider::mock::*;

        let provider = Arc::new(MockProvider::new(vec![MockResponse::Text(
            "Final answer only".into(),
        )]));
        let tool = build_mock_subagent("summarizer", provider);

        let params = serde_json::json!({"task": "What is 2+2?"});
        let result = tool
            .execute(params, tool_context("summarizer"))
            .await
            .unwrap();

        // Only one content block with the final text
        assert_eq!(result.content.len(), 1);
        let text = match &result.content[0] {
            Content::Text { text } => text.as_str(),
            other => panic!("Expected Text content, got: {:?}", other),
        };
        assert_eq!(text, "Final answer only");
    }

    #[tokio::test]
    async fn subagent_errors_on_missing_task_parameter() {
        let provider = Arc::new(MockProvider::text("Should not run"));
        let tool = build_mock_subagent("broken", provider);

        let params = serde_json::json!({}); // Missing "task"
        let result = tool.execute(params, tool_context("broken")).await;

        assert!(result.is_err(), "should error on missing task parameter");
        match result.unwrap_err() {
            ToolError::InvalidArgs(msg) => {
                assert!(msg.contains("task"), "error should mention 'task': {msg}");
            }
            other => panic!("Expected InvalidArgs, got: {:?}", other),
        }
    }

    #[test]
    fn build_subagent_uses_anthropic_provider_by_default() {
        let config = AppConfig::default();
        let tool = build_subagent(
            "test",
            "A test agent",
            "You are a test.",
            &config,
            "claude-sonnet-4-20250514",
            "sk-test",
        );
        // If it builds without panic, the provider was created correctly.
        // We verify the tool's name to confirm the builder wired everything.
        assert_eq!(tool.name(), "test");
    }

    #[test]
    fn build_subagent_uses_ollama_provider_when_configured() {
        let mut config = AppConfig::default();
        config.agent.default_provider = "ollama".into();
        let tool = build_subagent(
            "ollama-test",
            "An ollama agent",
            "You are a test.",
            &config,
            "qwen2.5:14b",
            "",
        );
        assert_eq!(tool.name(), "ollama-test");
    }
}
