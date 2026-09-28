//! Backward compatibility of the public run-event shape (t47, DEP-INT-1 follow-up).
//!
//! `RunEvent::ContextInjected` gained `path` and `budget` in t19 (the integration
//! unit). Transcripts on disk are append-only JSONL and are read months later, so
//! the OLD shape -- an object with `type` and `render` and nothing else -- has to
//! keep deserializing. The proof is this test, not the `#[serde(default)]`
//! attribute: an attribute is a claim, a deserialization is a reading.
//!
//! Both halves are asserted: the old shape yields `None` (unknown, not a made-up
//! `"run"`), and the new shape's values are actually READ when present. A test
//! that only checked the first half would pass even if the fields were dropped on
//! the floor.

use ruagent_core::RunEvent;

/// The bytes an old transcript holds for an injected-context event: no `path`,
/// no `budget`. Taken from the shape the daemon wrote before t19.
const OLD_EVENT: &str = r#"{"type":"context_injected","render":"<relevant_memories>\nold bytes\n</relevant_memories>\n"}"#;

#[test]
fn an_old_context_injected_event_still_deserializes() {
    let ev: RunEvent = serde_json::from_str(OLD_EVENT)
        .expect("an old ContextInjected event (no path, no budget) must still deserialize");
    match ev {
        RunEvent::ContextInjected {
            render,
            path,
            budget,
        } => {
            assert!(
                render.contains("old bytes"),
                "the old render bytes must survive verbatim, got {render:?}"
            );
            assert_eq!(
                path, None,
                "an old event carries no path: unknown, not \"run\""
            );
            assert_eq!(budget, None, "an old event carries no budget: not measured");
        }
        other => panic!("old bytes deserialized into the wrong variant: {other:?}"),
    }
}

#[test]
fn a_new_context_injected_event_still_reads_its_values() {
    let new = r#"{"type":"context_injected","render":"x","path":"chat","budget":{"used_chars":1}}"#;
    match serde_json::from_str::<RunEvent>(new).expect("the new shape must deserialize") {
        RunEvent::ContextInjected {
            render,
            path,
            budget,
        } => {
            assert_eq!(render, "x");
            // The half that catches "the field exists but is never read".
            assert_eq!(path.as_deref(), Some("chat"));
            assert!(
                budget.is_some(),
                "a present budget must be read, not dropped"
            );
        }
        other => panic!("new bytes deserialized into the wrong variant: {other:?}"),
    }
}
