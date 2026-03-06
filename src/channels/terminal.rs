//! Terminal channel adapter for interactive REPL input.
//!
//! Reads lines from stdin, wraps each as a [`Command`](crate::bus::Command),
//! sends through the [`CommandBus`](crate::bus::CommandBus), and prints
//! the response received via the oneshot channel.

use std::io::{self, Write};

use crate::bus::{ChannelKind, Command, CommandBus};
use crate::channels::Channel;
use tokio::sync::oneshot;

/// Terminal-based input channel for the interactive REPL.
///
/// Reads lines from stdin, sends each as a [`Command`] through the bus,
/// and prints the [`Response`] received back via the oneshot channel.
///
/// # Fields
///
/// * `use_color` - Whether to use ANSI color codes in prompt display.
#[derive(Debug)]
pub struct TerminalChannel {
    /// Whether to use ANSI color codes in prompt display.
    pub use_color: bool,
}

impl TerminalChannel {
    /// Creates a new `TerminalChannel` with the given color preference.
    ///
    /// # Arguments
    ///
    /// * `use_color` - Whether to use ANSI color codes in prompt display.
    pub fn new(use_color: bool) -> Self {
        Self { use_color }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_channel_can_be_constructed() {
        let channel = TerminalChannel::new(true);
        assert!(channel.use_color);

        let channel = TerminalChannel::new(false);
        assert!(!channel.use_color);
    }

    #[test]
    fn terminal_channel_is_object_safe() {
        // Verify the Channel trait can be used as a trait object.
        let channel = TerminalChannel::new(false);
        let _boxed: Box<dyn Channel> = Box::new(channel);
    }
}
