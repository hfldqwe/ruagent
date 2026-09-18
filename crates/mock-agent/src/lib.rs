//! ruagent-mock-agent: scriptable mock ACP agent.
//!
//! An ACP agent over stdio with selectable behaviors, used to test the
//! ruagent client without real harnesses or API keys. See design §12.
//!
//! ```text
//! ruagent-mock-agent --behavior echo
//! ruagent-mock-agent --behavior scripted --replies replies.json
//! ```

use agent_client_protocol::schema::v1::{
    ContentBlock, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption,
};
use std::collections::HashMap;

/// All selectable behaviors.
pub const BEHAVIORS: &[&str] = &[
    "echo",
    "toolcall",
    "permission",
    "plan",
    "crash",
    "approve",
    "judge",
    "scripted",
];

// ---------------------------------------------------------------------------
// Advertised session options (model / mode / reasoning effort): mirrors
// what real runtimes advertise, so the daemon's option-catalog caching
// and role-default application are testable end to end.
// ---------------------------------------------------------------------------

/// (option id, choices) — the first choice is the default.
pub const OPTIONS: &[(&str, &[&str])] = &[
    ("model", &["mock-pro", "mock-max"]),
    ("mode", &["ask", "auto"]),
    ("reasoning_effort", &["off", "high", "max"]),
];

/// The startup current values (first choice of each option).
pub fn default_current() -> HashMap<String, String> {
    OPTIONS
        .iter()
        .map(|(id, choices)| ((*id).to_string(), choices[0].to_string()))
        .collect()
}

/// The choices of one option id.
fn choices_of(id: &str) -> Option<&'static [&'static str]> {
    OPTIONS.iter().find(|(i, _)| *i == id).map(|(_, c)| *c)
}

fn option_name(id: &str) -> String {
    match id {
        "model" => "Model".into(),
        "mode" => "Mode".into(),
        "reasoning_effort" => "Reasoning effort".into(),
        other => other.to_string(),
    }
}

fn option_category(id: &str) -> SessionConfigOptionCategory {
    match id {
        "model" => SessionConfigOptionCategory::Model,
        "mode" => SessionConfigOptionCategory::Mode,
        "reasoning_effort" => SessionConfigOptionCategory::ThoughtLevel,
        other => SessionConfigOptionCategory::Other(other.to_string()),
    }
}

/// Build the full advertisement for the current selections.
pub fn advertise(current: &HashMap<String, String>) -> Vec<SessionConfigOption> {
    OPTIONS
        .iter()
        .map(|(id, choices)| {
            let value = current
                .get(*id)
                .cloned()
                .unwrap_or_else(|| choices[0].to_string());
            SessionConfigOption::select(
                *id,
                option_name(id),
                value,
                choices
                    .iter()
                    .map(|c| SessionConfigSelectOption::new((*c).to_string(), (*c).to_string()))
                    .collect::<Vec<_>>(),
            )
            .category(option_category(id))
        })
        .collect()
}

/// Set one option's current value; validates the id and the choice.
pub fn set_option(
    current: &mut HashMap<String, String>,
    id: &str,
    value: &str,
) -> Result<(), String> {
    let choices = choices_of(id).ok_or_else(|| format!("unknown option `{id}`"))?;
    if !choices.contains(&value) {
        return Err(format!("unknown value `{value}` for option `{id}`"));
    }
    current.insert(id.to_string(), value.to_string());
    Ok(())
}

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
    /// Reply with the first `[RUN <id>]` candidate in the prompt — plays
    /// the fan-out judge in tests (the run ids only exist at runtime, so
    /// a marker-keyed script cannot reference them).
    Judge,
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
            "judge" => Some(Behavior::Judge),
            "scripted" => Some(Behavior::Scripted),
            _ => None,
        }
    }
}

/// Extract every `[RUN <id>]` candidate marker from a judge prompt.
/// Empty when the prompt carries no candidates.
pub fn judge_candidates(prompt: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = prompt;
    while let Some(open) = rest.find("[RUN ") {
        rest = &rest[open + "[RUN ".len()..];
        let Some(close) = rest.find(']') else { break };
        let id = rest[..close].trim();
        if !id.is_empty() && !ids.iter().any(|i| i == id) {
            ids.push(id.to_string());
        }
        rest = &rest[close + 1..];
    }
    ids
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
        assert_eq!(Behavior::parse("judge"), Some(Behavior::Judge));
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

    #[test]
    fn judge_candidates_extracted_in_order_deduped() {
        let prompt = "Candidates:\n[RUN run-aaaa-bbbb-1111] agent=claude\nresult 1\n[RUN run-aaaa-bbbb-2222] agent=dsh\nresult 2\nReply with RUN: and WHY:";
        assert_eq!(
            judge_candidates(prompt),
            ["run-aaaa-bbbb-1111", "run-aaaa-bbbb-2222"]
        );
        // No markers, unclosed marker, duplicates, blank id.
        assert!(judge_candidates("no markers here").is_empty());
        assert!(judge_candidates("[RUN never-closed").is_empty());
        assert!(judge_candidates("[RUN ] nothing").is_empty());
        assert_eq!(
            judge_candidates("[RUN x] [RUN x] [RUN y]"),
            ["x", "y"],
            "duplicates collapse"
        );
    }
}
