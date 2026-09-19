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

/// The parsed `agents.toml` file. Two layers (design §4.1 refactor):
/// `[runtime.X]` = how an engine is spawned; `[agent.Y]` = a portable
/// role (prompt + preferred runtimes). Legacy single-layer files (an
/// agent entry with `harness = …`) keep working — each synthesizes its
/// own runtime.
#[derive(Debug, serde::Deserialize)]
struct AgentsFile {
    #[serde(default)]
    runtime: BTreeMap<String, RuntimeEntry>,
    #[serde(default)]
    agent: BTreeMap<String, AgentEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct RuntimeEntry {
    /// claude-code | opencode | dsh | mock. Defaults to the entry name.
    harness: Option<String>,
    command: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
    /// Machine params for DIRECT runtime runs (the role layer overrides
    /// these when a role runs on the runtime).
    model: Option<String>,
    models: Option<Vec<String>>,
    mcp_profile: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AgentEntry {
    /// Legacy single-layer form: the agent IS a runtime instance.
    harness: Option<String>,
    command: Option<String>,
    description: Option<String>,
    model: Option<String>,
    models: Option<Vec<String>>,
    reasoning_effort: Option<String>,
    context_window: Option<u32>,
    mcp_profile: Option<String>,
    /// The portable role prompt (two-layer form).
    prompt: Option<String>,
    /// Default runtime (a `[runtime.X]` name).
    runtime: Option<String>,
    /// Every runtime this role can run on.
    #[serde(default)]
    runtimes: Vec<String>,
    /// Canonical session-option defaults (`options = { mode = "plan" }`).
    #[serde(default)]
    options: BTreeMap<String, String>,
    #[serde(default)]
    tags: Vec<String>,
    enabled: Option<bool>,
}

pub(crate) fn harness_of(name: &str, spec: Option<&str>) -> Result<HarnessKind> {
    match spec.unwrap_or(name).trim() {
        "claude-code" | "claude" => Ok(HarnessKind::ClaudeCode),
        "opencode" => Ok(HarnessKind::OpenCode),
        "dsh" | "deepseek" => Ok(HarnessKind::Dsh),
        "mock" => Ok(HarnessKind::Mock),
        other => anyhow::bail!("unknown harness `{other}` (runtime `{name}`)"),
    }
}

fn parse_effort(s: Option<&str>) -> Option<ReasoningEffort> {
    s.and_then(|s| match s {
        "minimal" => Some(ReasoningEffort::Minimal),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        _ => None,
    })
}

pub(crate) fn parse_agents(text: &str) -> Result<Vec<AgentCard>> {
    let file: AgentsFile = toml::from_str(text)?;
    let mut out = Vec::new();

    // Two-layer: roles first (what users pick), then raw runtimes.
    if !file.runtime.is_empty() {
        let mut runtime_cards: BTreeMap<String, AgentCard> = BTreeMap::new();
        for (name, entry) in &file.runtime {
            let harness = harness_of(name, entry.harness.as_deref())?;
            runtime_cards.insert(
                name.clone(),
                AgentCard {
                    id: ruagent_core::AgentId::generate(),
                    name: name.clone(),
                    harness,
                    command: entry.command.clone(),
                    description: entry.description.clone().unwrap_or_else(|| name.clone()),
                    model: entry.model.clone(),
                    models: entry.models.clone().unwrap_or_default(),
                    reasoning_effort: None,
                    context_window: None,
                    mcp_profile: entry.mcp_profile.clone(),
                    prompt: None,
                    runtime: None,
                    runtimes: Vec::new(),
                    options: BTreeMap::new(),
                    tags: Vec::new(),
                    enabled: entry.enabled.unwrap_or(true),
                },
            );
        }
        for (name, entry) in &file.agent {
            // Mixed file: a legacy entry (harness set, no runtime) is a
            // self-contained card even when [runtime.*] sections exist.
            if entry.runtime.is_none() && entry.runtimes.is_empty() && entry.harness.is_some() {
                let harness = harness_of(name, entry.harness.as_deref())?;
                out.push(AgentCard {
                    id: ruagent_core::AgentId::generate(),
                    name: name.clone(),
                    harness,
                    command: entry.command.clone(),
                    description: entry.description.clone().unwrap_or_default(),
                    model: entry.model.clone(),
                    models: entry.models.clone().unwrap_or_default(),
                    reasoning_effort: parse_effort(entry.reasoning_effort.as_deref()),
                    context_window: entry.context_window,
                    mcp_profile: entry.mcp_profile.clone(),
                    prompt: entry.prompt.clone(),
                    runtime: None,
                    runtimes: Vec::new(),
                    options: entry.options.clone(),
                    tags: entry.tags.clone(),
                    enabled: entry.enabled.unwrap_or(true),
                });
                continue;
            }
            let Some(default_runtime) = entry
                .runtime
                .clone()
                .or_else(|| entry.runtimes.first().cloned())
            else {
                anyhow::bail!(
                    "agent `{name}` has no runtime: set `runtime = \"…\"` (or `runtimes = […])`)"
                );
            };
            let Some(rc) = runtime_cards.get(&default_runtime) else {
                anyhow::bail!(
                    "agent `{name}`: runtime `{default_runtime}` is not defined in [runtime.*]"
                );
            };
            let allowed: Vec<String> = entry
                .runtimes
                .iter()
                .filter(|r| runtime_cards.contains_key(*r))
                .cloned()
                .collect();
            out.push(AgentCard {
                id: ruagent_core::AgentId::generate(),
                name: name.clone(),
                harness: rc.harness,
                command: rc.command.clone(),
                // No prompt-snippet fallback: a mid-sentence cut reads
                // as corruption, not a preview (the panel renders a
                // labeled prompt preview of its own).
                description: entry.description.clone().unwrap_or_default(),
                model: entry.model.clone(),
                models: entry.models.clone().unwrap_or_default(),
                reasoning_effort: parse_effort(entry.reasoning_effort.as_deref()),
                context_window: entry.context_window,
                mcp_profile: entry.mcp_profile.clone(),
                prompt: entry.prompt.clone(),
                runtime: Some(default_runtime),
                runtimes: allowed,
                options: entry.options.clone(),
                tags: entry.tags.clone(),
                enabled: entry.enabled.unwrap_or(true),
            });
        }
        // Runtime cards that no role claimed: still listed (direct-run
        // instances). The kind classification lives in the API layer
        // (`kind: "runtime"`), not in a description prefix.
        for (_name, card) in runtime_cards {
            if out.iter().any(|a| a.name == card.name) {
                continue;
            }
            out.push(card);
        }
        return Ok(out);
    }

    // Legacy single-layer: every entry is a runtime-instance agent.
    for (name, entry) in file.agent {
        let Some(harness_spec) = entry.harness.clone() else {
            anyhow::bail!(
                "agent `{name}`: two-layer files need [runtime.*] sections; single-layer entries need `harness = …`"
            );
        };
        let harness = harness_of(&name, Some(&harness_spec))?;
        out.push(AgentCard {
            id: ruagent_core::AgentId::generate(), // replaced by the stable DB id
            name: name.clone(),
            harness,
            command: entry.command,
            description: entry.description.unwrap_or_default(),
            model: entry.model,
            models: entry.models.unwrap_or_default(),
            reasoning_effort: parse_effort(entry.reasoning_effort.as_deref()),
            context_window: entry.context_window,
            mcp_profile: entry.mcp_profile,
            prompt: entry.prompt,
            runtime: None,
            runtimes: Vec::new(),
            options: entry.options,
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
    /// Live health registry (issue #24): the last check result per
    /// server, shared by every clone of this config. Populated by the
    /// daemon's background loop; `expand_profile` excludes servers
    /// whose last check failed.
    pub health: std::sync::Arc<crate::mcphealth::McpHealth>,
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
        health: Default::default(),
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
            .filter_map(|n| self.servers.get(n).map(|e| (n, e)))
            .filter(|(_, e)| {
                e.inject_for
                    .as_ref()
                    .map(|names| names.iter().any(|n| n == agent_name))
                    .unwrap_or(true)
            })
            .filter(|(n, _)| {
                // Health gate (issue #24): a server whose last check
                // failed stays out of injection — spawning it would
                // break session startup. Re-checked every few minutes.
                if self.health.is_down(n) {
                    tracing::warn!(server = %n, "mcp server is down — excluded from injection");
                    false
                } else {
                    true
                }
            })
            .map(|(n, e)| mcp_entry_to_acp(n, e))
            .collect()
    }
}

fn mcp_entry_to_acp(name: &str, e: &McpEntry) -> agent_client_protocol::schema::v1::McpServer {
    use agent_client_protocol::schema::v1::{McpServer, McpServerHttp, McpServerStdio};
    if let Some(url) = &e.url {
        // http vs sse is indistinguishable from config alone; http is the
        // modern default and agents that only support sse will say so.
        return McpServer::Http(McpServerHttp::new(name, url.clone()));
    }
    let command = e.command.clone().unwrap_or_default();
    // The registry name travels: agents label servers by it (a
    // hardcoded "mcp" made every injection indistinguishable).
    let mut stdio = McpServerStdio::new(name, command);
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

# Two-layer model (2026-09-14): [runtime.X] = spawnable engines,
# [agent.Y] = portable roles. A role's prompt runs on ANY of its
# runtimes. Example:
#
# [runtime.dsh]
# command = "dsh --profile acp"
#
# [agent.architect]
# prompt = "You are the architecture reviewer. Challenge assumptions, propose alternatives, keep reviews constructive and concrete."
# runtimes = ["dsh", "claude-code"]
# runtime = "dsh"

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
# Session distillation policy:
# [distill]
# auto = true            # distill sessions automatically when they close
# agent = "dsh"          # extraction agent (default: dsh, else first enabled)
# language = "简体中文"   # output language for distilled memories/entities
#                        # (JSON keys and store/namespace stay canonical)
# prompt = """..."""     # full override of the extraction prompt (advanced;
#                        # the transcript is still appended by the daemon)
# First matching rule wins; `default` applies otherwise. Actions:
#   allow  — auto-select the first allow option
#   reject — auto-select the first reject option
#   ask    — park in the human inbox (fail-closed default)

[permissions]
default = "ask"

# Per-harness concurrency (design §8.3): fanning out N agents on one
# runtime queues them instead of spawning N children at once; beyond
# the queue cap new launches are rejected with a clear error.
# [concurrency]
# per_harness = 2        # simultaneous runs per harness kind
# queue_per_harness = 8  # waiting runs before rejection

# [[permissions.rules]]
# title_contains = "read"
# action = "allow"
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn distill_editor_round_trips_preserving_comments() {
        let dir = std::env::temp_dir().join(format!(
            "ruagent-distill-edit-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("policy.toml");
        std::fs::write(
            &path,
            "# top comment
[permissions]
default = \"ask\"

# distillation
[distill]
auto = true
agent = \"dsh\"
",
        )
        .unwrap();
        let editor = DistillEditor::new(&path);
        editor
            .update(&ruagent_policy::DistillConfig {
                auto: false,
                agent: None,
                language: Some("简体中文".into()),
                prompt: None,
            })
            .unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        // Comments, ordering and other sections survive.
        assert!(out.contains("# top comment"));
        assert!(out.contains("[permissions]"));
        assert!(out.contains("# distillation"));
        assert!(out.contains("auto = false"));
        // A None key is removed, not emptied.
        assert!(!out.contains("agent ="));
        assert!(out.contains("简体中文"));
        // The result reparses as the same policy.
        let policy = ruagent_policy::PolicyConfig::parse(&out).unwrap();
        assert!(!policy.distill.auto);
        assert_eq!(policy.distill.language.as_deref(), Some("简体中文"));
        assert!(policy.distill.agent.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

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

// ---------------------------------------------------------------------------
// policy.toml [distill] editing (the panel's settings card)
// ---------------------------------------------------------------------------

/// Round-trip editor for the `[distill]` table of `policy.toml` — the
/// registry::Editor discipline (toml_edit keeps comments/ordering/other
/// sections, temp+rename is atomic, a mutex stops interleaved writes)
/// applied to the distillation policy.
pub struct DistillEditor {
    path: PathBuf,
    lock: std::sync::Mutex<()>,
}

impl DistillEditor {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: std::sync::Mutex::new(()),
        }
    }

    /// Write the whole `[distill]` table: `auto` always, the optional
    /// keys only when present (None removes the key from the file).
    pub fn update(&self, cfg: &ruagent_policy::DistillConfig) -> Result<()> {
        let _guard = self.lock.lock().expect("distill editor lock");
        let text = std::fs::read_to_string(&self.path)
            .with_context(|| format!("reading {}", self.path.display()))?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| format!("parsing {}", self.path.display()))?;
        if !doc.contains_key("distill") {
            doc["distill"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        let tbl = doc["distill"]
            .as_table_mut()
            .context("[distill] is not a table")?;
        tbl["auto"] = toml_edit::value(cfg.auto);
        set_or_remove(tbl, "agent", &cfg.agent);
        set_or_remove(tbl, "language", &cfg.language);
        set_or_remove(tbl, "prompt", &cfg.prompt);
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, doc.to_string())
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("renaming into {}", self.path.display()))?;
        Ok(())
    }
}

fn set_or_remove(tbl: &mut toml_edit::Table, key: &str, v: &Option<String>) {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            tbl[key] = toml_edit::value(s);
        }
        None => {
            tbl.remove(key);
        }
    }
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

    #[test]
    fn two_layer_agents_parse() {
        let toml = r#"
[runtime.dsh]
command = "dsh --profile acp"

[runtime.claude-code]
command = "npx @agentclientprotocol/claude-agent-acp"

[agent.architect]
prompt = "You are the architecture reviewer."
runtimes = ["dsh", "claude-code"]
runtime = "dsh"
description = "架构评审"

[agent.plugin-dev]
prompt = "You write dsh plugins."
runtime = "dsh"
"#;
        let cards = parse_agents(toml).unwrap();
        let arch = cards.iter().find(|c| c.name == "architect").unwrap();
        assert_eq!(arch.runtime.as_deref(), Some("dsh"));
        assert_eq!(arch.runtimes, vec!["dsh", "claude-code"]);
        assert!(arch.prompt.as_deref().unwrap().contains("architecture"));
        // runtimes also present as spawnable cards
        assert!(cards.iter().any(|c| c.name == "claude-code"));
        // plugin-dev defaults to its only runtime
        let plug = cards.iter().find(|c| c.name == "plugin-dev").unwrap();
        assert_eq!(plug.runtime.as_deref(), Some("dsh"));
    }

    #[test]
    fn legacy_agents_still_parse() {
        let toml = r#"
[agent.dsh]
harness = "dsh"
description = "legacy"
"#;
        let cards = parse_agents(toml).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name, "dsh");
        assert!(cards[0].runtime.is_none());
    }
}
