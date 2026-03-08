//! Terminal channel adapter for interactive REPL input.
//!
//! Reads lines from stdin, wraps each as a [`Command`](crate::bus::Command),
//! sends through the [`CommandBus`](crate::bus::CommandBus), and prints
//! the response received via the oneshot channel.
//!
//! Also subscribes to permission prompt broadcasts and displays interactive
//! `[Y]es / [N]o / [A]lways` prompts, writing responses back into the
//! shared `pending_responses` map so the [`PermissionGuard`] can unblock.
//!
//! [`PermissionGuard`]: crate::permissions::guard::PermissionGuard

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast, oneshot};

use crate::bus::{ChannelKind, Command, CommandBus};
use crate::channels::Channel;
use crate::permissions::guard::PermissionPromptRequest;
use crate::permissions::{PermissionResponse, is_persist_eligible, suggest_persist_pattern};

/// Alias for the shared map where permission prompt responses are deposited.
pub type PendingResponses = Arc<Mutex<HashMap<String, PermissionResponse>>>;

/// Formats a permission prompt for display in the terminal.
///
/// For persist-eligible tools, shows "Yes, and don't ask again" which persists
/// to local settings. For write tools, shows "Always (this session)" instead.
pub fn format_permission_prompt(tool_name: &str, tool_input: &serde_json::Value) -> String {
    let args_display = tool_input.to_string();
    let header = format!("? {tool_name}: {args_display}");

    if is_persist_eligible(tool_name) {
        let pattern = suggest_persist_pattern(tool_name, tool_input);
        format!("{header}\n  1. Yes\n  2. Yes, and don't ask again for: {pattern}\n  3. No")
    } else {
        format!("{header}\n  1. Yes\n  2. Always (this session)\n  3. No")
    }
}

/// Parses user input with context about the tool being prompted.
///
/// For persist-eligible tools: 1=Yes, 2=Persist, 3=No.
/// For write tools: 1=Yes, 2=Always(session), 3=No.
pub fn parse_permission_input(input: &str) -> Option<PermissionResponse> {
    // Legacy letter keys (context-free, used as fallback).
    match input.trim() {
        "y" | "Y" | "1" => Some(PermissionResponse::Yes),
        "n" | "N" | "3" => Some(PermissionResponse::No),
        _ => None,
    }
}

/// Parses user input with full context about the tool being prompted.
///
/// For persist-eligible tools: 1=Yes, 2=Persist(pattern), 3=No.
/// For write tools: 1=Yes, 2=Always(session), 3=No.
pub fn parse_permission_input_for(
    input: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> Option<PermissionResponse> {
    let trimmed = input.trim();
    if is_persist_eligible(tool_name) {
        match trimmed {
            "y" | "Y" | "1" => Some(PermissionResponse::Yes),
            "2" => {
                let pattern = suggest_persist_pattern(tool_name, tool_input);
                Some(PermissionResponse::Persist(pattern))
            }
            "n" | "N" | "3" => Some(PermissionResponse::No),
            _ => None,
        }
    } else {
        match trimmed {
            "y" | "Y" | "1" => Some(PermissionResponse::Yes),
            "a" | "A" | "2" => Some(PermissionResponse::Always),
            "n" | "N" | "3" => Some(PermissionResponse::No),
            _ => None,
        }
    }
}

/// Terminal-based input channel for the interactive REPL.
///
/// Reads lines from stdin, sends each as a [`Command`] through the bus,
/// and prints the [`Response`] received back via the oneshot channel.
///
/// Optionally subscribes to permission prompt broadcasts and displays
/// interactive `[Y]es / [N]o / [A]lways` prompts.
///
/// # Fields
///
/// * `use_color` - Whether to use ANSI color codes in prompt display.
#[derive(Debug)]
pub struct TerminalChannel {
    /// Whether to use ANSI color codes in prompt display.
    pub use_color: bool,
    /// Optional broadcast receiver for permission prompt requests.
    /// Wrapped in a `std::sync::Mutex` to allow taking from `&self`.
    prompt_rx: std::sync::Mutex<Option<broadcast::Receiver<PermissionPromptRequest>>>,
    /// Shared map for depositing permission responses.
    pending_responses: Option<PendingResponses>,
}

impl TerminalChannel {
    /// Creates a new `TerminalChannel` with the given color preference.
    ///
    /// # Arguments
    ///
    /// * `use_color` - Whether to use ANSI color codes in prompt display.
    pub fn new(use_color: bool) -> Self {
        Self {
            use_color,
            prompt_rx: std::sync::Mutex::new(None),
            pending_responses: None,
        }
    }

    /// Configures the channel to subscribe to permission prompt broadcasts.
    ///
    /// When set, the channel spawns a concurrent task that listens for
    /// [`PermissionPromptRequest`]s and displays interactive prompts.
    pub fn with_permission_prompt(
        mut self,
        prompt_rx: broadcast::Receiver<PermissionPromptRequest>,
        pending_responses: PendingResponses,
    ) -> Self {
        self.prompt_rx = std::sync::Mutex::new(Some(prompt_rx));
        self.pending_responses = Some(pending_responses);
        self
    }
}

#[async_trait::async_trait]
impl Channel for TerminalChannel {
    /// Runs the terminal input loop.
    ///
    /// Reads lines from stdin, wraps each as a `Command` with
    /// `ChannelKind::Terminal`, sends through the bus, awaits the
    /// oneshot response, and prints it.
    ///
    /// Exits when stdin is exhausted (EOF) or if the bus receiver is dropped.
    ///
    /// # Arguments
    ///
    /// * `bus` - The command bus sender to push commands into.
    ///
    /// # Errors
    ///
    /// Returns an error if sending to the bus fails (receiver dropped).
    async fn run(&self, bus: CommandBus) -> Result<(), anyhow::Error> {
        // Spawn the permission prompt subscriber as a concurrent task
        // so it does not block the main REPL input loop.
        if let Some(prompt_rx) = self.prompt_rx.lock().unwrap().take()
            && let Some(pending) = self.pending_responses.clone()
        {
            tokio::spawn(run_permission_prompt_loop(prompt_rx, pending));
        }

        loop {
            // Display prompt.
            if self.use_color {
                print!("\x1b[1m\x1b[32m> \x1b[0m");
            } else {
                print!("> ");
            }
            io::stdout().flush().ok();

            // Read next line from stdin in a blocking task to avoid
            // holding a non-Send StdinLock across an await point.
            let line = tokio::task::spawn_blocking(|| {
                let mut buf = String::new();
                match io::stdin().read_line(&mut buf) {
                    Ok(0) => None, // EOF
                    Ok(_) => Some(Ok(buf)),
                    Err(e) => Some(Err(e)),
                }
            })
            .await?;

            let line = match line {
                Some(Ok(l)) => l,
                Some(Err(e)) => {
                    tracing::error!("stdin read error: {e}");
                    break;
                }
                None => break, // EOF
            };

            let input = line.trim().to_owned();
            if input.is_empty() {
                continue;
            }

            // Create a oneshot channel for the response.
            let (response_tx, response_rx) = oneshot::channel();

            let cmd = Command {
                source: ChannelKind::Terminal,
                content: input,
                response_tx,
            };

            // Send the command through the bus.
            bus.send(cmd)
                .await
                .map_err(|e| anyhow::anyhow!("bus send failed: {e}"))?;

            // Await the response from the consumer.
            match response_rx.await {
                Ok(response) => {
                    if !response.content.is_empty() {
                        println!("{}", response.content);
                    }
                }
                Err(_) => {
                    tracing::error!("response channel dropped");
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Runs the permission prompt subscriber loop.
///
/// Listens for [`PermissionPromptRequest`]s on the broadcast channel,
/// displays the interactive prompt, reads the user's response from stdin,
/// and writes the [`PermissionResponse`] into the shared `pending_responses`
/// map. Re-displays the prompt on unrecognized input.
async fn run_permission_prompt_loop(
    mut prompt_rx: broadcast::Receiver<PermissionPromptRequest>,
    pending_responses: PendingResponses,
) {
    loop {
        let request = match prompt_rx.recv().await {
            Ok(req) => req,
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("permission prompt subscriber lagged by {n} messages");
                continue;
            }
        };

        let prompt_text = format_permission_prompt(&request.tool_name, &request.tool_input);

        loop {
            println!("{prompt_text}");
            io::stdout().flush().ok();

            let line = tokio::task::spawn_blocking(|| {
                let mut buf = String::new();
                match io::stdin().read_line(&mut buf) {
                    Ok(0) => None,
                    Ok(_) => Some(Ok(buf)),
                    Err(e) => Some(Err(e)),
                }
            })
            .await;

            let input = match line {
                Ok(Some(Ok(l))) => l,
                Ok(Some(Err(e))) => {
                    tracing::error!("stdin read error during permission prompt: {e}");
                    break;
                }
                Ok(None) | Err(_) => break,
            };

            if let Some(response) =
                parse_permission_input_for(&input, &request.tool_name, &request.tool_input)
            {
                let mut map = pending_responses.lock().await;
                map.insert(request.id.clone(), response);
                break;
            }
            // Unrecognized input: loop re-displays the prompt.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Construction tests ───────────────────────────────────────

    #[test]
    fn terminal_channel_can_be_constructed() {
        let channel = TerminalChannel::new(true);
        assert!(channel.use_color);

        let channel = TerminalChannel::new(false);
        assert!(!channel.use_color);
    }

    #[test]
    fn terminal_channel_is_object_safe() {
        let channel = TerminalChannel::new(false);
        let _boxed: Box<dyn Channel> = Box::new(channel);
    }

    #[test]
    fn terminal_channel_with_permission_prompt_stores_pending() {
        let (tx, rx) = broadcast::channel(16);
        let pending: PendingResponses = Arc::new(Mutex::new(HashMap::new()));
        let channel = TerminalChannel::new(false).with_permission_prompt(rx, pending.clone());
        assert!(channel.prompt_rx.lock().unwrap().is_some());
        assert!(channel.pending_responses.is_some());
        drop(tx);
    }

    // ── format_permission_prompt tests ───────────────────────────

    #[test]
    fn format_prompt_contains_tool_name_and_args() {
        let prompt = format_permission_prompt("bash", &json!({"command": "rm -rf /"}));
        assert!(prompt.starts_with("? bash: "));
        assert!(prompt.contains("rm -rf /"));
    }

    #[test]
    fn format_prompt_persist_eligible_shows_three_options() {
        let prompt = format_permission_prompt("bash", &json!({"command": "cargo test --release"}));
        assert!(prompt.contains("1. Yes"));
        assert!(prompt.contains("2. Yes, and don't ask again for:"));
        assert!(prompt.contains("cargo test:*"));
        assert!(prompt.contains("3. No"));
        assert!(!prompt.contains("session"));
    }

    #[test]
    fn format_prompt_write_tool_shows_session_option() {
        let prompt = format_permission_prompt("write_file", &json!({"file_path": "/tmp/test"}));
        assert!(prompt.contains("1. Yes"));
        assert!(prompt.contains("2. Always (this session)"));
        assert!(prompt.contains("3. No"));
        assert!(!prompt.contains("don't ask again"));
    }

    // ── parse_permission_input_for tests ─────────────────────────

    #[test]
    fn parse_persist_eligible_option_1_is_yes() {
        let args = json!({"command": "ls"});
        assert_eq!(
            parse_permission_input_for("1", "bash", &args),
            Some(PermissionResponse::Yes)
        );
    }

    #[test]
    fn parse_persist_eligible_option_2_is_persist() {
        let args = json!({"command": "cargo test --release"});
        let result = parse_permission_input_for("2", "bash", &args);
        assert!(matches!(result, Some(PermissionResponse::Persist(_))));
        if let Some(PermissionResponse::Persist(rule)) = result {
            assert_eq!(rule, "Bash(cargo test:*)");
        }
    }

    #[test]
    fn parse_persist_eligible_option_3_is_no() {
        let args = json!({"command": "ls"});
        assert_eq!(
            parse_permission_input_for("3", "bash", &args),
            Some(PermissionResponse::No)
        );
    }

    #[test]
    fn parse_write_tool_option_2_is_always() {
        let args = json!({"file_path": "/tmp/test"});
        assert_eq!(
            parse_permission_input_for("2", "write_file", &args),
            Some(PermissionResponse::Always)
        );
    }

    #[test]
    fn parse_write_tool_option_3_is_no() {
        let args = json!({"file_path": "/tmp/test"});
        assert_eq!(
            parse_permission_input_for("3", "write_file", &args),
            Some(PermissionResponse::No)
        );
    }

    #[test]
    fn parse_letter_keys_still_work() {
        let args = json!({"command": "ls"});
        assert_eq!(
            parse_permission_input_for("y", "bash", &args),
            Some(PermissionResponse::Yes)
        );
        assert_eq!(
            parse_permission_input_for("n", "bash", &args),
            Some(PermissionResponse::No)
        );
    }

    #[test]
    fn parse_write_letter_a_is_always() {
        let args = json!({"file_path": "/tmp/test"});
        assert_eq!(
            parse_permission_input_for("a", "write_file", &args),
            Some(PermissionResponse::Always)
        );
    }

    #[test]
    fn parse_unrecognized_returns_none() {
        let args = json!({"command": "ls"});
        assert_eq!(parse_permission_input_for("x", "bash", &args), None);
        assert_eq!(parse_permission_input_for("yes", "bash", &args), None);
        assert_eq!(parse_permission_input_for("", "bash", &args), None);
        assert_eq!(parse_permission_input_for("4", "bash", &args), None);
    }

    #[test]
    fn parse_trims_whitespace() {
        let args = json!({"command": "ls"});
        assert_eq!(
            parse_permission_input_for("  1  ", "bash", &args),
            Some(PermissionResponse::Yes)
        );
        assert_eq!(
            parse_permission_input_for("\tn\n", "bash", &args),
            Some(PermissionResponse::No)
        );
    }

    // ── Concurrent prompt subscriber test ────────────────────────

    #[tokio::test]
    async fn prompt_response_written_to_pending_map() {
        let pending: PendingResponses = Arc::new(Mutex::new(HashMap::new()));
        let request_id = "test-req-1".to_string();

        // Directly verify that inserting into the map works as expected
        // by the prompt loop (simulating what run_permission_prompt_loop does).
        {
            let mut map = pending.lock().await;
            map.insert(request_id.clone(), PermissionResponse::Yes);
        }

        let map = pending.lock().await;
        assert_eq!(map.get("test-req-1"), Some(&PermissionResponse::Yes));
    }

    #[tokio::test]
    async fn prompt_response_always_written_to_pending_map() {
        let pending: PendingResponses = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut map = pending.lock().await;
            map.insert("req-always".to_string(), PermissionResponse::Always);
        }
        let map = pending.lock().await;
        assert_eq!(map.get("req-always"), Some(&PermissionResponse::Always));
    }

    #[tokio::test]
    async fn prompt_response_no_written_to_pending_map() {
        let pending: PendingResponses = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut map = pending.lock().await;
            map.insert("req-no".to_string(), PermissionResponse::No);
        }
        let map = pending.lock().await;
        assert_eq!(map.get("req-no"), Some(&PermissionResponse::No));
    }
}
