//! Normalized run event model. See design §4.3 (event normalization) and
//! §8.1 (trace as replay).
//!
//! Every ACP `session/update` notification is mapped onto [`RunEvent`],
//! appended to the run's JSONL transcript, and forwarded over WebSocket.
//! Our own lifecycle events (state changes, routing, context injection,
//! permission resolutions) share the same stream so one timeline tells the
//! whole story.

use crate::routing::RoutingDecision;
use crate::run::{RunStatus, StopReason};
use crate::usage::UsageTotals;
use serde::{Deserialize, Serialize};

/// A content block, mirroring the ACP content model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        /// Base64-encoded bytes.
        data: String,
        mime_type: String,
    },
    /// A reference to an external resource without inline content.
    ResourceLink {
        uri: String,
        name: String,
    },
    Resource {
        uri: String,
        mime_type: Option<String>,
        /// Base64-encoded bytes, when the agent inlined them.
        blob: Option<String>,
    },
}

impl ContentBlock {
    /// Plain-text projection (empty for non-text blocks).
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// Status of one plan entry in an agent's work plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanEntryStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub status: PlanEntryStatus,
}

/// Options offered by an agent for a permission request (ACP semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOption {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

/// Who resolved a permission request. See design §9.2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum PermissionResolution {
    /// A deterministic rule auto-answered.
    Rule { rule_id: String },
    /// The configured approver agent decided on behalf of the human.
    ApproverAgent { agent: crate::id::AgentId },
    /// The human decided via inbox/CLI/panel.
    Human,
}

/// The normalized event stream of a run. Serialized to JSONL as
/// `{"ts": ..., "seq": ..., "event": ...}` by the store layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEvent {
    /// Our own state machine moved.
    StateChanged { status: RunStatus },
    /// Why this run went to its agent(s). Always recorded (design §5.4).
    Routed { decision: RoutingDecision },
    /// Context blocks injected at run start (design §6.4) — rendered form
    /// as the agent received it, for context observability (§8.1).
    ContextInjected { render: String },
    /// ACP `agent_message_chunk`.
    AgentMessageChunk {
        content: Vec<ContentBlock>,
        thinking: Option<String>,
    },
    /// ACP `agent_thought_chunk`.
    AgentThoughtChunk { content: Vec<ContentBlock> },
    /// ACP `tool_call`.
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        input: serde_json::Value,
    },
    /// ACP `tool_call_update` (partial output of a running tool call).
    ToolCallUpdate {
        tool_call_id: String,
        content: Vec<ContentBlock>,
    },
    /// ACP `plan`.
    Plan { entries: Vec<PlanEntry> },
    /// ACP `usage_update`.
    UsageUpdate { usage: UsageTotals },
    /// The agent asked for permission; surfaced to the policy center.
    PermissionRequested {
        tool_call_id: String,
        tool_name: String,
        input: serde_json::Value,
        options: Vec<PermissionOption>,
    },
    /// The policy center resolved a permission request (who + what).
    PermissionResolved {
        tool_call_id: String,
        outcome: PermissionOption,
        resolution: PermissionResolution,
    },
    /// Terminal: the agent reported a stop reason.
    Stopped { stop_reason: StopReason },
    /// Terminal or transient error (crash, timeout, protocol violation).
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_roundtrip_through_json() {
        // The JSONL contract: every event must survive a serde roundtrip.
        let events = vec![
            RunEvent::StateChanged {
                status: RunStatus::Running,
            },
            RunEvent::AgentMessageChunk {
                content: vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
                thinking: None,
            },
            RunEvent::ToolCall {
                tool_call_id: "tc1".into(),
                tool_name: "fs.write".into(),
                input: serde_json::json!({"path": "a.txt"}),
            },
            RunEvent::UsageUpdate {
                usage: UsageTotals {
                    input_tokens: 10,
                    output_tokens: 5,
                    ..Default::default()
                },
            },
        ];
        for e in &events {
            let json = serde_json::to_string(e).unwrap();
            let back: RunEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, e);
        }
    }
}
