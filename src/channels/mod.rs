//! Input channel adapters for the command bus.
//!
//! Each channel implements the [`Channel`] trait and runs as an async loop
//! feeding [`Command`](crate::bus::Command)s into the bus. The terminal
//! channel is always available; others (Discord, Telegram) are behind
//! feature flags.

pub mod terminal;

use crate::bus::CommandBus;

/// A channel adapter that feeds user input into the command bus.
///
/// Implementors run an async loop reading from their input source,
/// wrapping input as `Command`s, sending them through the bus, and
/// printing/forwarding responses received via the oneshot channel.
#[async_trait::async_trait]
pub trait Channel: Send + Sync {
    /// Runs the channel's input loop until the source is exhausted or
    /// a shutdown signal is received.
    ///
    /// # Arguments
    ///
    /// * `bus` - The command bus sender to push commands into.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel encounters an unrecoverable failure.
    async fn run(&self, bus: CommandBus) -> Result<(), anyhow::Error>;
}
