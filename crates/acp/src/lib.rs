//! ruagent-acp: ACP client (JSON-RPC over stdio) and harness adapters.
//!
//! The M1 execution model is connection-per-run: spawn a fresh agent
//! subprocess per run, initialize, create a session (with MCP injection),
//! stream one prompt to completion. Session resume and multiplexed
//! connections arrive in M2 (design §5, §4.2).

pub mod adapter;
pub mod chat;
pub mod fs_tools;
pub mod map;
pub mod permission;
pub mod run;

pub use adapter::{SpawnSpec, StandardAdapter, adapter_for, resolve_program};
pub use chat::{ChatCommand, ChatOptions, ChatSession, start_chat};
pub use run::{RunOptions, RunOutcome, run_once, split_command_line};

/// Errors from the ACP layer.
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    #[error("ACP protocol error: {0}")]
    Protocol(#[from] agent_client_protocol::Error),
    #[error("invalid agent command `{0}`")]
    Command(String),
}
