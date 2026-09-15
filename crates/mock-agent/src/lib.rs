//! ruagent-mock-agent: scriptable mock ACP agent.
//!
//! An ACP agent over stdio with selectable behaviors, used to test the
//! ruagent client without real harnesses or API keys. See design §12.
//!
//! ```text
//! ruagent-mock-agent --behavior echo
//! ruagent-mock-agent --behavior scripted --replies replies.json
//! ```

use agent_client_protocol::schema::v1::ContentBlock;

/// All selectable behaviors.
pub const BEHAVIORS: &[&str] = &[
    "echo",
    "toolcall",
    "permission",
    "plan",
    "crash",
    "approve",
    "scripted",
];

/// What the mock agent does when prompted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behavior {
    /// Stream a message chunk echoing the prompt, report usage, end turn.
    Echo,
    /// Emit a tool_call + tool_call_update with raw output, end turn.
    ToolCall,
    /// Ask for permission before writing; react to the answer.
    Permission,
    /// Publish a plan, then a message, then end turn.
    Plan,
    /// Stream one chunk, then exit(1) — simulates a crashed harness.
    Crash,
    /// Reply with exactly "ALLOW" — plays the approver agent in tests.
    Approve,
    /// Reply from a marker-keyed script file — see [`MockArgs`].
    Scripted,
}

/// One scripted reply: sent when the prompt contains `marker`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ScriptedReply {
    /// Substring that must appear in the prompt text.
    pub marker: String,
    /// The reply text.
    pub reply: String,
}

/// Parsed process arguments.
#[derive(Debug, Clone)]
pub struct MockArgs {
    pub behavior: Behavior,
    /// Marker-keyed replies for `--behavior scripted` (may be empty).
    pub replies: Vec<ScriptedReply>,
}

impl MockArgs {
    /// Parse `--behavior <name>` and `--replies <file>` from process
    /// args. Exits with a usage message on unknown arguments or a
    /// malformed replies file.
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut behavior = "echo";
        let mut replies_file: Option<String> = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--behavior" | "-b" => {
                    if let Some(v) = args.get(i + 1) {
                        behavior = v;
                    }
                    i += 2;
                }
                "--replies" => {
                    if let Some(v) = args.get(i + 1) {
                        replies_file = Some(v.clone());
                    }
                    i += 2;
                }
                other => {
                    eprintln!("unknown argument: {other}");
                    eprintln!(
                        "usage: ruagent-mock-agent [--behavior <{}>] [--replies <file>]",
                        BEHAVIORS.join("|")
                    );
                    std::process::exit(2);
                }
            }
        }
        let behavior = match Behavior::parse(behavior) {
            Some(b) => b,
            None => {
                eprintln!(
                    "unknown behavior: {behavior} (expected one of: {})",
                    BEHAVIORS.join(", ")
                );
                std::process::exit(2);
            }
        };
        // The replies file is read at startup: each ruagent one-shot
        // call spawns a fresh process, so sequential counters reset —
        // replies are keyed by prompt content, not order.
        let replies = match (&replies_file, behavior) {
            (Some(path), Behavior::Scripted) => match std::fs::read_to_string(path) {
                Ok(text) => match serde_json::from_str(&text) {
                    Ok(replies) => replies,
                    Err(e) => {
                        eprintln!("malformed replies file {path}: {e}");
                        std::process::exit(2);
                    }
                },
                Err(e) => {
                    eprintln!("cannot read replies file {path}: {e}");
                    std::process::exit(2);
                }
            },
            _ => Vec::new(),
        };
        Self { behavior, replies }
    }
}

impl Behavior {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "echo" => Some(Behavior::Echo),
            "toolcall" => Some(Behavior::ToolCall),
            "permission" => Some(Behavior::Permission),
            "plan" => Some(Behavior::Plan),
            "crash" => Some(Behavior::Crash),
            "approve" => Some(Behavior::Approve),
            "scripted" => Some(Behavior::Scripted),
            _ => None,
        }
    }
}

/// Concatenate the text of a prompt's content blocks.
pub fn prompt_text(prompt: &[ContentBlock]) -> String {
    let mut out = String::new();
    for b in prompt {
        if let ContentBlock::Text(t) = b {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&t.text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::TextContent;

    #[test]
    fn parses_behaviors() {
        assert_eq!(Behavior::parse("echo"), Some(Behavior::Echo));
        assert_eq!(Behavior::parse("crash"), Some(Behavior::Crash));
        assert_eq!(Behavior::parse("scripted"), Some(Behavior::Scripted));
        assert_eq!(Behavior::parse("nope"), None);
    }

    #[test]
    fn extracts_prompt_text() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("hello")),
            ContentBlock::Text(TextContent::new("world")),
        ];
        assert_eq!(prompt_text(&blocks), "hello\nworld");
    }

    #[test]
    fn scripted_replies_parse() {
        let json = r##"[{"marker":"WIKI PLANNER","reply":"{\"pages\":[]}"},{"marker":"slug: x","reply":"# X"}]"##;
        let replies: Vec<ScriptedReply> = serde_json::from_str(json).unwrap();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0].marker, "WIKI PLANNER");
        assert_eq!(replies[1].reply, "# X");
    }
}
