//! ruagent-mock-agent: scriptable mock ACP agent.
//!
//! An ACP agent over stdio with selectable behaviors, used to test the
//! ruagent client without real harnesses or API keys. See design §12.
//!
//! ```text
//! ruagent-mock-agent --behavior echo
//! ```

use agent_client_protocol::schema::v1::ContentBlock;

/// All selectable behaviors.
pub const BEHAVIORS: &[&str] = &["echo", "toolcall", "permission", "plan", "crash", "approve"];

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
            _ => None,
        }
    }

    /// Parse `--behavior <name>` from process args (default: echo).
    /// Exits with a usage message on unknown behavior names.
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut behavior = "echo";
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--behavior" | "-b" => {
                    if let Some(v) = args.get(i + 1) {
                        behavior = v;
                    }
                    i += 2;
                }
                other => {
                    eprintln!("unknown argument: {other}");
                    eprintln!(
                        "usage: ruagent-mock-agent [--behavior <{}>]",
                        BEHAVIORS.join("|")
                    );
                    std::process::exit(2);
                }
            }
        }
        match Self::parse(behavior) {
            Some(b) => b,
            None => {
                eprintln!(
                    "unknown behavior: {behavior} (expected one of: {})",
                    BEHAVIORS.join(", ")
                );
                std::process::exit(2);
            }
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
}
