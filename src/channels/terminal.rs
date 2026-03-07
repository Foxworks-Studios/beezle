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
use crate::permissions::PermissionResponse;
use crate::permissions::guard::PermissionPromptRequest;

/// Alias for the shared map where permission prompt responses are deposited.
pub type PendingResponses = Arc<Mutex<HashMap<String, PermissionResponse>>>;

/// Formats a permission prompt for display in the terminal.
///
/// Returns a string in the format `? <tool>: <args>\n  [Y]es  [N]o  [A]lways`.
pub fn format_permission_prompt(tool_name: &str, tool_input: &serde_json::Value) -> String {
    let args_display = tool_input.to_string();
    format!("? {tool_name}: {args_display}\n  [Y]es  [N]o  [A]lways")
}

/// Parses a single line of user input into a [`PermissionResponse`].
///
/// Returns `None` if the input is not recognized, signaling that the
/// prompt should be re-displayed.
pub fn parse_permission_input(input: &str) -> Option<PermissionResponse> {
    match input.trim() {
        "y" | "Y" => Some(PermissionResponse::Yes),
        "n" | "N" => Some(PermissionResponse::No),
        "a" | "A" => Some(PermissionResponse::Always),
        _ => None,
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

            if let Some(response) = parse_permission_input(&input) {
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
    fn format_prompt_contains_response_options() {
        let prompt = format_permission_prompt("bash", &json!({"command": "ls"}));
        assert!(prompt.contains("[Y]es"));
        assert!(prompt.contains("[N]o"));
        assert!(prompt.contains("[A]lways"));
    }

    #[test]
    fn format_prompt_has_correct_layout() {
        let prompt = format_permission_prompt("write_file", &json!({"file_path": "/tmp/test"}));
        let lines: Vec<&str> = prompt.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("? write_file: "));
        assert_eq!(lines[1].trim(), "[Y]es  [N]o  [A]lways");
    }

    // ── parse_permission_input tests ─────────────────────────────

    #[test]
    fn parse_yes_lowercase() {
        assert_eq!(parse_permission_input("y"), Some(PermissionResponse::Yes));
    }

    #[test]
    fn parse_yes_uppercase() {
        assert_eq!(parse_permission_input("Y"), Some(PermissionResponse::Yes));
    }

    #[test]
    fn parse_no_lowercase() {
        assert_eq!(parse_permission_input("n"), Some(PermissionResponse::No));
    }

    #[test]
    fn parse_no_uppercase() {
        assert_eq!(parse_permission_input("N"), Some(PermissionResponse::No));
    }

    #[test]
    fn parse_always_lowercase() {
        assert_eq!(
            parse_permission_input("a"),
            Some(PermissionResponse::Always)
        );
    }

    #[test]
    fn parse_always_uppercase() {
        assert_eq!(
            parse_permission_input("A"),
            Some(PermissionResponse::Always)
        );
    }

    #[test]
    fn parse_unrecognized_returns_none() {
        assert_eq!(parse_permission_input("x"), None);
        assert_eq!(parse_permission_input("yes"), None);
        assert_eq!(parse_permission_input(""), None);
        assert_eq!(parse_permission_input("123"), None);
    }

    #[test]
    fn parse_trims_whitespace() {
        assert_eq!(
            parse_permission_input("  y  "),
            Some(PermissionResponse::Yes)
        );
        assert_eq!(
            parse_permission_input("\tn\n"),
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
