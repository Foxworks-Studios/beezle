//! Beezle: A fully-featured AI coding agent CLI.
//!
//! Built on the `yoagent` agent loop crate, with planned multi-channel
//! input support (Discord, Slack, Telegram, WhatsApp).

pub mod agent;
pub mod bus;
pub mod channels;
pub mod config;
pub mod context;
pub mod memory;
pub mod session;
pub mod tools;
