//! Memory tools for reading and writing agent memory via `yoagent` tool traits.
//!
//! Provides [`MemoryReadTool`] and [`MemoryWriteTool`], both backed by a shared
//! [`MemoryStore`](crate::memory::MemoryStore) instance.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use yoagent::{AgentTool, Content, ToolContext, ToolError, ToolResult};

use crate::memory::MemoryStore;

/// Tool that reads the agent's persistent memory (long-term or daily notes).
///
/// Returns the raw markdown content of `MEMORY.md` (long-term) or today's
/// daily notes file.
pub struct MemoryReadTool {
    /// Shared memory store backing this tool.
    store: Arc<MemoryStore>,
}

impl MemoryReadTool {
    /// Creates a new `MemoryReadTool` backed by the given store.
    ///
    /// # Arguments
    ///
    /// * `store` - Shared memory store instance.
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl AgentTool for MemoryReadTool {
    fn name(&self) -> &str {
        "memory_read"
    }

    fn label(&self) -> &str {
        "Memory Read"
    }

    fn description(&self) -> &str {
        "Read the agent's persistent memory. Use target \"long_term\" for stable \
         facts and preferences, or \"daily\" for today's session notes."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["target"],
            "properties": {
                "target": {
                    "type": "string",
                    "enum": ["long_term", "daily"],
                    "description": "Which memory to read: \"long_term\" for MEMORY.md, \"daily\" for today's notes."
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let target = params
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidArgs(
                    "missing or invalid \"target\" field; expected \"long_term\" or \"daily\""
                        .to_string(),
                )
            })?;

        let content = match target {
            "long_term" => self
                .store
                .read_long_term()
                .map_err(|e| ToolError::Failed(e.to_string()))?,
            "daily" => {
                let today = self.store.today();
                self.store
                    .read_daily(today)
                    .map_err(|e| ToolError::Failed(e.to_string()))?
            }
            other => {
                return Err(ToolError::InvalidArgs(format!(
                    "invalid target \"{other}\"; expected \"long_term\" or \"daily\""
                )));
            }
        };

        Ok(ToolResult {
            content: vec![Content::Text { text: content }],
            details: json!({}),
        })
    }
}

/// Tool that writes to the agent's persistent memory (long-term or daily notes).
///
/// With `target = "daily"`, appends a timestamped entry to today's daily notes.
/// With `target = "long_term"`, replaces the entire contents of `MEMORY.md`.
pub struct MemoryWriteTool {
    /// Shared memory store backing this tool.
    store: Arc<MemoryStore>,
}

impl MemoryWriteTool {
    /// Creates a new `MemoryWriteTool` backed by the given store.
    ///
    /// # Arguments
    ///
    /// * `store` - Shared memory store instance.
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl AgentTool for MemoryWriteTool {
    fn name(&self) -> &str {
        "memory_write"
    }

    fn label(&self) -> &str {
        "Memory Write"
    }

    fn description(&self) -> &str {
        "Write to the agent's persistent memory. Use target \"daily\" to append \
         a timestamped note, or \"long_term\" to replace MEMORY.md contents."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["target", "content"],
            "properties": {
                "target": {
                    "type": "string",
                    "enum": ["long_term", "daily"],
                    "description": "Which memory to write: \"long_term\" for MEMORY.md, \"daily\" for today's notes."
                },
                "content": {
                    "type": "string",
                    "description": "The text to write or append."
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let target = params
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidArgs(
                    "missing or invalid \"target\" field; expected \"long_term\" or \"daily\""
                        .to_string(),
                )
            })?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidArgs(
                    "missing or invalid \"content\" field; expected a string".to_string(),
                )
            })?;

        let message = match target {
            "daily" => {
                self.store
                    .append_daily(content)
                    .map_err(|e| ToolError::Failed(e.to_string()))?;
                "Appended to daily notes."
            }
            "long_term" => {
                self.store
                    .write_long_term(content)
                    .map_err(|e| ToolError::Failed(e.to_string()))?;
                "Long-term memory updated."
            }
            other => {
                return Err(ToolError::InvalidArgs(format!(
                    "invalid target \"{other}\"; expected \"long_term\" or \"daily\""
                )));
            }
        };

        Ok(ToolResult {
            content: vec![Content::Text {
                text: message.to_string(),
            }],
            details: json!({}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{DateTime, Local, TimeZone};
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    use crate::memory::Clock;

    /// Test clock that always returns a fixed time.
    #[derive(Debug, Clone)]
    struct FakeClock(DateTime<Local>);

    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Local> {
            self.0
        }
    }

    /// Helper: returns 2026-03-05T14:30:00 in local time.
    fn fixed_time() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 3, 5, 14, 30, 0)
            .single()
            .expect("invalid fixed time")
    }

    /// Helper: creates a shared `MemoryStore` backed by a temp directory.
    fn test_store() -> (Arc<MemoryStore>, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let store = Arc::new(MemoryStore::new(
            dir.path().join("memory"),
            Arc::new(FakeClock(fixed_time())),
        ));
        (store, dir)
    }

    /// Helper: creates a minimal `ToolContext` for testing.
    fn test_ctx() -> ToolContext {
        ToolContext {
            tool_call_id: "test-id".to_string(),
            tool_name: "test".to_string(),
            cancel: CancellationToken::new(),
            on_update: None,
            on_progress: None,
        }
    }

    /// Helper: extracts the first text content from a `ToolResult`.
    fn text_of(result: &ToolResult) -> &str {
        match &result.content[0] {
            Content::Text { text } => text.as_str(),
            other => panic!("expected Content::Text, got {other:?}"),
        }
    }

    // ----- MemoryReadTool tests -----

    #[tokio::test]
    async fn read_long_term_returns_file_content() {
        let (store, _dir) = test_store();
        // Seed MEMORY.md with known content.
        store
            .write_long_term("prior facts")
            .expect("seed write failed");

        let tool = MemoryReadTool::new(Arc::clone(&store));
        let result = tool
            .execute(json!({"target": "long_term"}), test_ctx())
            .await
            .expect("execute failed");

        assert_eq!(text_of(&result), "prior facts");
    }

    #[tokio::test]
    async fn read_daily_returns_todays_notes() {
        let (store, _dir) = test_store();
        store.append_daily("morning note").expect("append failed");

        let tool = MemoryReadTool::new(Arc::clone(&store));
        let result = tool
            .execute(json!({"target": "daily"}), test_ctx())
            .await
            .expect("execute failed");

        assert!(
            text_of(&result).contains("morning note"),
            "daily read should contain appended text"
        );
    }

    #[tokio::test]
    async fn read_missing_target_returns_invalid_args() {
        let (store, _dir) = test_store();
        let tool = MemoryReadTool::new(Arc::clone(&store));

        let err = tool
            .execute(json!({}), test_ctx())
            .await
            .expect_err("should fail on missing target");

        assert!(
            matches!(err, ToolError::InvalidArgs(_)),
            "expected InvalidArgs, got {err:?}"
        );
    }

    #[tokio::test]
    async fn read_invalid_target_returns_invalid_args() {
        let (store, _dir) = test_store();
        let tool = MemoryReadTool::new(Arc::clone(&store));

        let err = tool
            .execute(json!({"target": "bogus"}), test_ctx())
            .await
            .expect_err("should fail on invalid target");

        assert!(
            matches!(err, ToolError::InvalidArgs(_)),
            "expected InvalidArgs, got {err:?}"
        );
    }

    // ----- MemoryWriteTool tests -----

    #[tokio::test]
    async fn write_daily_then_read_contains_text() {
        let (store, _dir) = test_store();
        let write_tool = MemoryWriteTool::new(Arc::clone(&store));
        let read_tool = MemoryReadTool::new(Arc::clone(&store));

        let write_result = write_tool
            .execute(
                json!({"target": "daily", "content": "standup note"}),
                test_ctx(),
            )
            .await
            .expect("write execute failed");

        assert_eq!(text_of(&write_result), "Appended to daily notes.");

        let read_result = read_tool
            .execute(json!({"target": "daily"}), test_ctx())
            .await
            .expect("read execute failed");

        assert!(
            text_of(&read_result).contains("standup note"),
            "daily read should contain written text"
        );
    }

    #[tokio::test]
    async fn write_long_term_replaces_content() {
        let (store, _dir) = test_store();
        let tool = MemoryWriteTool::new(Arc::clone(&store));

        let write_result = tool
            .execute(
                json!({"target": "long_term", "content": "new facts"}),
                test_ctx(),
            )
            .await
            .expect("write execute failed");

        assert_eq!(text_of(&write_result), "Long-term memory updated.");

        let stored = store.read_long_term().expect("read_long_term failed");
        assert_eq!(stored, "new facts");
    }

    #[tokio::test]
    async fn write_missing_content_returns_invalid_args() {
        let (store, _dir) = test_store();
        let tool = MemoryWriteTool::new(Arc::clone(&store));

        let err = tool
            .execute(json!({"target": "daily"}), test_ctx())
            .await
            .expect_err("should fail on missing content");

        assert!(
            matches!(err, ToolError::InvalidArgs(_)),
            "expected InvalidArgs, got {err:?}"
        );
    }

    #[tokio::test]
    async fn write_missing_target_returns_invalid_args() {
        let (store, _dir) = test_store();
        let tool = MemoryWriteTool::new(Arc::clone(&store));

        let err = tool
            .execute(json!({"content": "some text"}), test_ctx())
            .await
            .expect_err("should fail on missing target");

        assert!(
            matches!(err, ToolError::InvalidArgs(_)),
            "expected InvalidArgs, got {err:?}"
        );
    }

    // ----- Schema / metadata tests -----

    #[test]
    fn read_tool_name_and_schema() {
        let (store, _dir) = test_store();
        let tool = MemoryReadTool::new(store);
        assert_eq!(tool.name(), "memory_read");

        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().expect("required is array");
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "target");

        let target_enum = schema["properties"]["target"]["enum"]
            .as_array()
            .expect("enum is array");
        assert_eq!(target_enum, &[json!("long_term"), json!("daily")]);
    }

    #[test]
    fn write_tool_name_and_schema() {
        let (store, _dir) = test_store();
        let tool = MemoryWriteTool::new(store);
        assert_eq!(tool.name(), "memory_write");

        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().expect("required is array");
        assert!(required.contains(&json!("target")));
        assert!(required.contains(&json!("content")));

        let content_type = &schema["properties"]["content"]["type"];
        assert_eq!(content_type, "string");
    }
}
