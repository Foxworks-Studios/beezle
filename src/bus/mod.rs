//! Unified async command bus for multi-channel input.
//!
//! All input sources (terminal, Discord, Telegram, etc.) send `Command`s
//! through the bus. The agent consumer receives commands from a
//! `CommandBusReceiver` and sends responses back via oneshot channels
//! embedded in each command.

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

/// Identifies which input channel a command originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelKind {
    /// Interactive terminal REPL.
    Terminal,
    /// Discord bot (future).
    Discord,
    /// Telegram bot (future).
    Telegram,
}

/// Response sent back to the originating channel after processing a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// The response text content.
    pub content: String,
}

/// A command sent from an input channel to the agent consumer.
///
/// Each command carries a oneshot sender so the consumer can route
/// the response back to the originating channel.
#[derive(Debug)]
pub struct Command {
    /// Which channel sent this command.
    pub source: ChannelKind,
    /// The raw input text (user message or slash command).
    pub content: String,
    /// Oneshot channel for sending the response back to the source.
    pub response_tx: oneshot::Sender<Response>,
}

/// Sender half of the command bus. Cloneable — each channel holds a clone.
#[derive(Debug, Clone)]
pub struct CommandBus {
    tx: mpsc::Sender<Command>,
}

/// Receiver half of the command bus. Only the agent consumer holds this.
#[derive(Debug)]
pub struct CommandBusReceiver {
    rx: mpsc::Receiver<Command>,
}

/// Creates a new command bus with the given channel capacity.
///
/// Returns a cloneable `CommandBus` (sender) and a single `CommandBusReceiver`.
///
/// # Arguments
///
/// * `capacity` - Maximum number of commands that can be buffered.
///
/// # Returns
///
/// A `(CommandBus, CommandBusReceiver)` pair.
pub fn command_bus(capacity: usize) -> (CommandBus, CommandBusReceiver) {
    let (tx, rx) = mpsc::channel(capacity);
    (CommandBus { tx }, CommandBusReceiver { rx })
}

impl CommandBus {
    /// Sends a command into the bus.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command to send.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the receiver has been dropped.
    pub async fn send(&self, cmd: Command) -> Result<(), mpsc::error::SendError<Command>> {
        self.tx.send(cmd).await
    }
}

impl CommandBusReceiver {
    /// Receives the next command from the bus.
    ///
    /// Returns `None` when all senders have been dropped.
    pub async fn recv(&mut self) -> Option<Command> {
        self.rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_and_receive_command() {
        let (bus, mut rx) = command_bus(16);
        let (resp_tx, resp_rx) = oneshot::channel();

        bus.send(Command {
            source: ChannelKind::Terminal,
            content: "hello".into(),
            response_tx: resp_tx,
        })
        .await
        .unwrap();

        let cmd = rx.recv().await.unwrap();
        assert_eq!(cmd.source, ChannelKind::Terminal);
        assert_eq!(cmd.content, "hello");

        // Send response back through the oneshot.
        cmd.response_tx
            .send(Response {
                content: "world".into(),
            })
            .unwrap();

        let resp = resp_rx.await.unwrap();
        assert_eq!(resp.content, "world");
    }

    #[tokio::test]
    async fn recv_returns_none_when_senders_dropped() {
        let (bus, mut rx) = command_bus(16);
        drop(bus);
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn channel_kind_serialization_roundtrips() {
        let kinds = [
            ChannelKind::Terminal,
            ChannelKind::Discord,
            ChannelKind::Telegram,
        ];
        for kind in &kinds {
            let json = serde_json::to_string(kind).unwrap();
            let parsed: ChannelKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*kind, parsed);
        }
    }

    #[tokio::test]
    async fn response_serialization_roundtrips() {
        let resp = Response {
            content: "test response".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.content, parsed.content);
    }

    #[tokio::test]
    async fn multiple_commands_in_order() {
        let (bus, mut rx) = command_bus(16);

        for i in 0..3 {
            let (resp_tx, _) = oneshot::channel();
            bus.send(Command {
                source: ChannelKind::Terminal,
                content: format!("msg-{i}"),
                response_tx: resp_tx,
            })
            .await
            .unwrap();
        }

        for i in 0..3 {
            let cmd = rx.recv().await.unwrap();
            assert_eq!(cmd.content, format!("msg-{i}"));
        }
    }

    #[tokio::test]
    async fn bus_is_cloneable() {
        let (bus, mut rx) = command_bus(16);
        let bus2 = bus.clone();

        let (resp_tx1, _) = oneshot::channel();
        bus.send(Command {
            source: ChannelKind::Terminal,
            content: "from-original".into(),
            response_tx: resp_tx1,
        })
        .await
        .unwrap();

        let (resp_tx2, _) = oneshot::channel();
        bus2.send(Command {
            source: ChannelKind::Discord,
            content: "from-clone".into(),
            response_tx: resp_tx2,
        })
        .await
        .unwrap();

        let cmd1 = rx.recv().await.unwrap();
        assert_eq!(cmd1.content, "from-original");
        let cmd2 = rx.recv().await.unwrap();
        assert_eq!(cmd2.content, "from-clone");
    }
}
