//! Daemon configuration: `~/.ruagent/config/{agents,mcp,policy}.toml`,
//! created with sensible defaults on first run (design §4.1, §7.1, §9.1).

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use ruagent_core::{AgentCard, HarnessKind, ReasoningEffort};
use ruagent_policy::PolicyConfig;

/// Everything the daemon needs at boot.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Data root (`~/.ruagent`).
    pub root: PathBuf,
    pub agents: Vec<AgentCard>,
    pub mcp: McpConfig,
    pub policy: PolicyConfig,
    pub routing: RoutingFile,
}

impl DaemonConfig {
    /// Load (or initialize) the configuration under `root`.
    pub fn load(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let config_dir = root.join("config");
        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("creating {}", config_dir.display()))?;

        let agents_path = config_dir.join("agents.toml");
        if !agents_path.exists() {
            std::fs::write(&agents_path, DEFAULT_AGENTS_TOML)
                .with_context(|| format!("writing {}", agents_path.display()))?;
            tracing::info!("wrote default {}", agents_path.display());
        }
        let mcp_path = config_dir.join("mcp.toml");
        if !mcp_path.exists() {
            std::fs::write(&mcp_path, DEFAULT_MCP_TOML)
                .with_context(|| format!("writing {}", mcp_path.display()))?;
        }
        let policy_path = config_dir.join("policy.toml");
        if !policy_path.exists() {
            std::fs::write(&policy_path, DEFAULT_POLICY_TOML)
                .with_context(|| format!("writing {}", policy_path.display()))?;
        }

        let agents_text = std::fs::read_to_string(&agents_path)?;
        let agents = parse_agents(&agents_text)
            .with_context(|| format!("parsing {}", agents_path.display()))?;

        let mcp_text = std::fs::read_to_string(&mcp_path)?;
        let mcp = parse_mcp(&mcp_text).with_context(|| format!("parsing {mcp_path:?}"))?;

        let policy_text = std::fs::read_to_string(&policy_path)?;
        let policy = PolicyConfig::parse(&policy_text)
            .with_context(|| format!("parsing {policy_path:?}"))?;

        let routing_path = config_dir.join("routing.toml");
        if !routing_path.exists() {
            std::fs::write(&routing_path, DEFAULT_ROUTING_TOML)
                .with_context(|| format!("writing {routing_path:?}"))?;
        }
        let routing_text = std::fs::read_to_string(&routing_path)?;
        let routing =
            parse_routing(&routing_text).with_context(|| format!("parsing {routing_path:?}"))?;

        Ok(Self {
            root,
            agents,
            mcp,
            policy,
            routing,
        })
    }

    /// Look up an agent by name.
    pub fn agent(&self, name: &str) -> Option<&AgentCard> {
        self.agents.iter().find(|a| a.name == name)
    }

    /// The first enabled agent (routing default).
    pub fn default_agent(&self) -> Option<&AgentCard> {
        self.agents.iter().find(|a| a.enabled)
    }
}

// ---------------------------------------------------------------------------
// agents.toml
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct AgentsFile {
    #[serde(default)]
    agent: BTreeMap<String, AgentEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct AgentEntry {
    harness: String,
    command: Option<String>,
    description: Option<String>,
    model: Option<String>,
    models: Option<Vec<String>>,
    reasoning_effort: Option<String>,
    context_window: Option<u32>,
    mcp_profile: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    enabled: Option<bool>,
}

fn parse_agents(text: &str) -> Result<Vec<AgentCard>> {
    let file: AgentsFile = toml::from_str(text)?;
    let mut out = Vec::new();
    for (name, entry) in file.agent {
        let harness = match entry.harness.as_str() {
            "claude-code" | "claude" => HarnessKind::ClaudeCode,
            "opencode" => HarnessKind::OpenCode,
            "dsh" | "deepseek" => HarnessKind::Dsh,
            "mock" => HarnessKind::Mock,
            other => anyhow::bail!("unknown harness `{other}` for agent `{name}`"),
        };
        let reasoning_effort = entry.reasoning_effort.as_deref().and_then(|s| match s {
            "minimal" => Some(ReasoningEffort::Minimal),
            "low" => Some(ReasoningEffort::Low),
            "medium" => Some(ReasoningEffort::Medium),
            "high" => Some(ReasoningEffort::High),
            _ => None,
        });
        out.push(AgentCard {
            id: ruagent_core::AgentId::generate(), // replaced by the stable DB id
            name: name.clone(),
            harness,
            command: entry.command,
            description: entry.description.unwrap_or_default(),
            model: entry.model,
            models: entry.models.unwrap_or_default(),
            reasoning_effort,
            context_window: entry.context_window,
            mcp_profile: entry.mcp_profile,
            tags: entry.tags,
            enabled: entry.enabled.unwrap_or(true),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// mcp.toml
// ---------------------------------------------------------------------------

/// The MCP registry as loaded from `mcp.toml`.
#[derive(Debug, Clone, Default)]
pub struct McpConfig {
    pub servers: BTreeMap<String, McpEntry>,
    pub profiles: BTreeMap<String, Vec<String>>,
}

/// One registered MCP server.
#[derive(Debug, Clone, PartialEq)]
pub struct McpEntry {
    /// Spawn command (stdio transport) or URL (http/sse).
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    /// Restrict injection to these agent names (None = all).
    pub inject_for: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct McpFile {
    #[serde(default)]
    mcp: BTreeMap<String, McpEntryFile>,
    #[serde(default)]
    profile: BTreeMap<String, ProfileFile>,
}

#[derive(Debug, serde::Deserialize)]
struct McpEntryFile {
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    url: Option<String>,
    inject_for: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct ProfileFile {
    #[serde(default)]
    servers: Vec<String>,
}

fn parse_mcp(text: &str) -> Result<McpConfig> {
    let file: McpFile = toml::from_str(text)?;
    Ok(McpConfig {
        servers: file
            .mcp
            .into_iter()
            .map(|(name, e)| {
                (
                    name,
                    McpEntry {
                        command: e.command,
                        args: e.args,
                        url: e.url,
                        inject_for: e.inject_for,
                    },
                )
            })
            .collect(),
        profiles: file
            .profile
            .into_iter()
            .map(|(name, p)| (name, p.servers))
            .collect(),
    })
}

impl McpConfig {
    /// Expand a profile into ACP MCP servers for one agent (design §7.1:
    /// injection is an overlay, `inject_for` filters membership).
    pub fn expand_profile(
        &self,
        profile: Option<&str>,
        agent_name: &str,
    ) -> Vec<agent_client_protocol::schema::v1::McpServer> {
        let Some(profile_name) = profile else {
            return vec![];
        };
        let Some(server_names) = self.profiles.get(profile_name) else {
            return vec![];
        };
        server_names
            .iter()
            .filter_map(|n| self.servers.get(n))
            .filter(|e| {
                e.inject_for
                    .as_ref()
                    .map(|names| names.iter().any(|n| n == agent_name))
                    .unwrap_or(true)
            })
            .map(mcp_entry_to_acp)
            .collect()
    }
}

fn mcp_entry_to_acp(e: &McpEntry) -> agent_client_protocol::schema::v1::McpServer {
    use agent_client_protocol::schema::v1::{McpServer, McpServerHttp, McpServerStdio};
    if let Some(url) = &e.url {
        // http vs sse is indistinguishable from config alone; http is the
        // modern default and agents that only support sse will say so.
        return McpServer::Http(McpServerHttp::new("mcp", url.clone()));
    }
    let command = e.command.clone().unwrap_or_default();
    let mut stdio = McpServerStdio::new("mcp", command);
    stdio = stdio.args(e.args.clone());
    McpServer::Stdio(stdio)
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const DEFAULT_AGENTS_TOML: &str = r#"# ruagent agent registry. See design §4.1.
#
# capability card layers:
#   machine params : model / reasoning_effort / context_window (mapped by
#                    adapters onto each harness's native knobs)
#   description    : free text, cold-start hint for the (future) LLM router
#   statistics     : accumulated by the daemon from run history

[agent.claude]
harness = "claude-code"
command = "npx @agentclientprotocol/claude-agent-acp"
description = "Claude Code via the official ACP adapter"
mcp_profile = "default"

[agent.opencode]
harness = "opencode"
command = "opencode acp"
description = "OpenCode (native ACP)"
mcp_profile = "default"

[agent.dsh]
harness = "dsh"
command = "dsh --profile acp"
description = "DeepSeek Harness (native ACP)"
mcp_profile = "default"

[agent.mock]
harness = "mock"
command = "ruagent-mock-agent --behavior echo"
description = "Built-in scriptable mock agent (testing); enable and point command at the binary"
enabled = false
"#;

const DEFAULT_MCP_TOML: &str = r#"# ruagent MCP registry. See design §7.1.
# Injection is an overlay: each CLI's own MCP config is never touched.
#
# [mcp.filesystem]
# command = "npx @modelcontextprotocol/server-filesystem"
# args = ["--root", "C:/projects"]
# inject_for = ["claude", "opencode"]     # optional membership filter
#
# [mcp.fetch]
# url = "https://mcp.example.com/sse"

# The platform's own MCP server: memory, knowledge and task tools for
# every agent (design SS6.5). Requires the ruagent binary reachable as
# `ruagent` on PATH and a running daemon (`ruagent serve`).
[mcp.ruagent]
command = "ruagent"
args = ["mcp-serve"]

[profile.default]
servers = ["ruagent"]
"#;

const DEFAULT_POLICY_TOML: &str = r#"# ruagent permission policy (M1: deterministic rules, design §9.2).
# First matching rule wins; `default` applies otherwise. Actions:
#   allow  — auto-select the first allow option
#   reject — auto-select the first reject option
#   ask    — park in the human inbox (fail-closed default)

[permissions]
default = "ask"

# [[permissions.rules]]
# title_contains = "read"
# action = "allow"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_agents_parse() {
        let agents = parse_agents(DEFAULT_AGENTS_TOML).unwrap();
        assert_eq!(agents.len(), 4);
        assert!(agents.iter().any(|a| a.name == "claude" && a.enabled));
        assert!(agents.iter().any(|a| a.name == "mock" && !a.enabled));
    }

    #[test]
    fn mcp_profile_expansion_filters_membership() {
        let text = r#"
[mcp.fs]
command = "npx fs-server"
inject_for = ["claude"]

[mcp.all]
command = "npx all-server"

[profile.default]
servers = ["fs", "all"]
"#;
        let mcp = parse_mcp(text).unwrap();
        let claude = mcp.expand_profile(Some("default"), "claude");
        assert_eq!(claude.len(), 2);
        let opencode = mcp.expand_profile(Some("default"), "opencode");
        assert_eq!(opencode.len(), 1); // fs is claude-only
        let none = mcp.expand_profile(None, "claude");
        assert!(none.is_empty());
        let missing = mcp.expand_profile(Some("nope"), "claude");
        assert!(missing.is_empty());
    }

    #[test]
    fn config_load_creates_defaults() {
        let dir = std::env::temp_dir().join(format!("ruagent-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = DaemonConfig::load(&dir).unwrap();
        assert!(dir.join("config/agents.toml").exists());
        assert_eq!(cfg.agents.len(), 4);
        assert!(cfg.agent("claude").is_some());
        assert!(cfg.default_agent().is_some());
        std::fs::remove_dir_all(&dir).ok();
    }
}

// ---------------------------------------------------------------------------
// routing.toml
// ---------------------------------------------------------------------------

/// `routing.toml` as written by users: agents by NAME.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct RoutingFile {
    #[serde(default)]
    pub routes: Vec<RouteEntry>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RouteEntry {
    pub project: Option<String>,
    pub title_contains: Option<String>,
    pub agent: String,
}

fn parse_routing(text: &str) -> Result<RoutingFile> {
    Ok(toml::from_str(text)?)
}

const DEFAULT_ROUTING_TOML: &str = r#"# ruagent routing rules (design §5.4/§9.1).
# Cascade: explicit --agent > first matching route > default.
# The LLM router tier is a pluggable future addition (off by default).
# NOTE: top-level keys (default) must come BEFORE any [[routes]] section.

# default = "claude"

# [[routes]]
# project = "ruagent"
# agent = "claude"
"#;

#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn routing_file_parses() {
        // Top-level keys must come BEFORE [[routes]] sections: in TOML,
        // a key after an array-of-tables header belongs to that table.
        let f = parse_routing(
            r#"
default = "dsh"

[[routes]]
project = "ruagent"
agent = "claude"

[[routes]]
title_contains = "review"
agent = "opencode"
"#,
        )
        .unwrap();
        assert_eq!(f.routes.len(), 2);
        assert_eq!(f.default.as_deref(), Some("dsh"));
    }

    #[test]
    fn default_routing_toml_parses() {
        assert!(parse_routing(DEFAULT_ROUTING_TOML).is_ok());
    }
}
