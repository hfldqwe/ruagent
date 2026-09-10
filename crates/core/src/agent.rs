//! Agent cards: registration + capability model. See design §4.1.
//!
//! The card has three layers (design table): machine params, semantic
//! description, and the statistical layer (accumulated elsewhere, see
//! `agents_stats` in the store).

use crate::id::AgentId;
use serde::{Deserialize, Serialize};

/// Which harness adapter drives this agent. See design §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    ClaudeCode,
    OpenCode,
    Dsh,
    /// The built-in scriptable mock agent (tests). See design §12.
    Mock,
}

impl HarnessKind {
    /// The default spawn command for each harness (overridable in config).
    pub fn default_command(self) -> &'static str {
        match self {
            HarnessKind::ClaudeCode => "npx @agentclientprotocol/claude-agent-acp",
            HarnessKind::OpenCode => "opencode acp",
            HarnessKind::Dsh => "dsh --profile acp",
            HarnessKind::Mock => "ruagent-mock-agent",
        }
    }
}

/// Reasoning/thinking effort, mapped by adapters onto each harness's native
/// knob (e.g. dsh `set_config_option`, Claude model params).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
}

/// A registered agent: identity + machine params + semantic hints.
/// See design §4.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCard {
    pub id: AgentId,
    /// Stable slug used in config and CLI (`claude`, `dsh`, …).
    pub name: String,
    pub harness: HarnessKind,
    /// Spawn command (argv). `None` uses the harness default.
    pub command: Option<String>,
    /// Free-text semantic description — cold-start hint for the LLM router.
    pub description: String,
    /// Machine param: default model.
    pub model: Option<String>,
    /// Machine param: default reasoning effort.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Machine param: context window in tokens.
    pub context_window: Option<u32>,
    /// Default MCP profile injected for this agent. See design §7.1.
    pub mcp_profile: Option<String>,
    /// Lightweight tags consumed by routing rules.
    pub tags: Vec<String>,
    pub enabled: bool,
}

impl AgentCard {
    /// Effective spawn argv (command string split on whitespace; config
    /// keeps it simple — adapters may need structured args later).
    pub fn spawn_command(&self) -> &str {
        self.command
            .as_deref()
            .unwrap_or_else(|| self.harness.default_command())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_commands_match_design() {
        assert_eq!(
            HarnessKind::ClaudeCode.default_command(),
            "npx @agentclientprotocol/claude-agent-acp"
        );
        assert_eq!(HarnessKind::Dsh.default_command(), "dsh --profile acp");
    }
}
