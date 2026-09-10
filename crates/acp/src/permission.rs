//! Permission bridge between the ACP layer and the policy center.
//!
//! The client-side `session/request_permission` handler forwards the ask
//! to the daemon over a channel and parks on a oneshot; the policy center
//! (rules → approver agent → human inbox, design §9.2) answers whenever it
//! decides. While parked, the run is effectively in `WaitingPermission`.

use ruagent_core::PermissionChoice;
use serde_json::Value;
use tokio::sync::oneshot;

/// A permission request surfaced to the daemon.
#[derive(Debug)]
pub struct PermissionAsk {
    pub tool_call_id: String,
    pub title: String,
    pub raw_input: Value,
    pub choices: Vec<PermissionChoice>,
    /// How the daemon answers. Dropping without sending = cancel.
    pub answer: oneshot::Sender<PermissionAnswer>,
}

/// The daemon's answer to a [`PermissionAsk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionAnswer {
    /// Select the option with this agent-assigned option id.
    Select(String),
    /// Cancel the permission request entirely.
    Cancel,
}
