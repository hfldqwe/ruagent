//! Mapping from ACP v1 schema types onto the normalized
//! [`ruagent_core::RunEvent`] model. See design §4.3.
//!
//! Everything not represented in our M1 event model (audio content,
//! commands/modes/config notifications) is dropped with `None` — the
//! transcript stays complete at the ACP layer only if we later choose to
//! persist raw frames as well.

use agent_client_protocol::schema::v1;
use ruagent_core::{ContentBlock, PermissionKind, RunEvent};

/// Map an ACP content block. Returns `None` for blocks outside the M1
/// subset (audio).
pub fn content_block(block: v1::ContentBlock) -> Option<ContentBlock> {
    Some(match block {
        v1::ContentBlock::Text(t) => ContentBlock::Text { text: t.text },
        v1::ContentBlock::Image(i) => ContentBlock::Image {
            data: i.data,
            mime_type: i.mime_type,
        },
        v1::ContentBlock::ResourceLink(r) => ContentBlock::ResourceLink {
            uri: r.uri,
            name: r.name,
        },
        v1::ContentBlock::Resource(r) => {
            let (uri, mime_type, text, blob) = match r.resource {
                v1::EmbeddedResourceResource::TextResourceContents(t) => {
                    (t.uri, t.mime_type, Some(t.text), None)
                }
                v1::EmbeddedResourceResource::BlobResourceContents(b) => {
                    (b.uri, b.mime_type, None, Some(b.blob))
                }
                _ => return None,
            };
            ContentBlock::Resource {
                uri,
                mime_type,
                text,
                blob,
            }
        }
        _ => return None,
    })
}

/// Map a tool-call content block (a different enum from prompt content).
fn tool_call_content(c: &v1::ToolCallContent) -> Option<ContentBlock> {
    match c {
        v1::ToolCallContent::Content(inner) => content_block(inner.content.clone()),
        _ => None,
    }
}

/// Map an ACP `SessionUpdate` onto a normalized run event. Returns `None`
/// for updates we do not model in M1.
pub fn session_update(update: v1::SessionUpdate) -> Option<RunEvent> {
    use v1::SessionUpdate as U;
    Some(match update {
        U::UserMessageChunk(_) => return None, // our own prompt echoed back
        U::AgentMessageChunk(c) => RunEvent::AgentMessageChunk {
            content: vec![content_block(c.content)?],
        },
        U::AgentThoughtChunk(c) => RunEvent::AgentThoughtChunk {
            content: vec![content_block(c.content)?],
        },
        U::ToolCall(t) => RunEvent::ToolCall {
            tool_call_id: t.tool_call_id.to_string(),
            title: t.title,
            raw_input: t.raw_input.unwrap_or(serde_json::Value::Null),
        },
        U::ToolCallUpdate(t) => RunEvent::ToolCallUpdate {
            tool_call_id: t.tool_call_id.to_string(),
            content: t
                .fields
                .content
                .iter()
                .flatten()
                .filter_map(tool_call_content)
                .collect::<Vec<_>>(),
            raw_output: t.fields.raw_output.clone(),
        },
        U::Plan(p) => RunEvent::Plan {
            entries: p
                .entries
                .iter()
                .map(|e| ruagent_core::PlanEntry {
                    content: e.content.clone(),
                    status: plan_status(e.status.clone()),
                })
                .collect(),
        },
        U::UsageUpdate(u) => RunEvent::UsageUpdate {
            usage: ruagent_core::ContextUsage {
                used: u.used,
                size: u.size,
                cost_usd: u.cost.map(|c| c.amount),
            },
        },
        // AvailableCommands / CurrentMode / ConfigOption / SessionInfo
        // updates are not modeled in M1.
        _ => return None,
    })
}

fn plan_status(s: v1::PlanEntryStatus) -> ruagent_core::PlanEntryStatus {
    match s {
        v1::PlanEntryStatus::Pending => ruagent_core::PlanEntryStatus::Pending,
        v1::PlanEntryStatus::InProgress => ruagent_core::PlanEntryStatus::InProgress,
        v1::PlanEntryStatus::Completed => ruagent_core::PlanEntryStatus::Completed,
        _ => ruagent_core::PlanEntryStatus::Pending,
    }
}

/// Map an ACP stop reason onto ours.
pub fn stop_reason(r: v1::StopReason) -> ruagent_core::StopReason {
    match r {
        v1::StopReason::EndTurn => ruagent_core::StopReason::EndTurn,
        v1::StopReason::MaxTokens => ruagent_core::StopReason::MaxTokens,
        v1::StopReason::MaxTurnRequests => ruagent_core::StopReason::MaxTurns,
        v1::StopReason::Refusal => ruagent_core::StopReason::Refusal,
        v1::StopReason::Cancelled => ruagent_core::StopReason::Cancelled,
        _ => ruagent_core::StopReason::Error,
    }
}

/// Map a permission option kind.
pub fn permission_kind(k: v1::PermissionOptionKind) -> PermissionKind {
    match k {
        v1::PermissionOptionKind::AllowOnce => PermissionKind::AllowOnce,
        v1::PermissionOptionKind::AllowAlways => PermissionKind::AllowAlways,
        v1::PermissionOptionKind::RejectOnce => PermissionKind::RejectOnce,
        v1::PermissionOptionKind::RejectAlways => PermissionKind::RejectAlways,
        _ => PermissionKind::RejectOnce,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{ContentChunk, TextContent};

    #[test]
    fn maps_message_chunk() {
        let chunk = v1::SessionUpdate::AgentMessageChunk(ContentChunk::new(
            v1::ContentBlock::Text(TextContent::new("hi")),
        ));
        let ev = session_update(chunk).unwrap();
        match ev {
            RunEvent::AgentMessageChunk { content } => {
                assert_eq!(content[0].as_text(), Some("hi"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn maps_stop_reasons() {
        assert_eq!(
            stop_reason(v1::StopReason::MaxTurnRequests),
            ruagent_core::StopReason::MaxTurns
        );
    }
}
