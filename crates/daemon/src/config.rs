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
    /// The capability plane as the FILE defines it (design
    /// docs/plans/capability-plugins-design.md §4.5). `load` is the only place
    /// that turns an unknown id, an undeclared option key or an out-of-range
    /// value into a boot failure, and the only place that warns when the
    /// configuration narrows unattended distillation. The LIVE value lives on
    /// `ChatManager` (one shared handle); boot installs this value into it.
    pub capabilities: crate::capability::CapabilityPlane,
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

        // The capability plane (L3: a name nobody recognises is a hard error
        // here, never a silent default). An absent `[capabilities]` table is
        // legacy mode, which is what the shipped policy.toml has.
        let capabilities = crate::capability::CapabilityPlane::from_policy(&policy)
            .map_err(anyhow::Error::from)
            .with_context(|| format!("parsing [capabilities] in {}", policy_path.display()))?;
        if policy.distill.auto
            && !capabilities.gate(crate::capability::CapabilityId::DistillSession, true)
        {
            tracing::warn!(
                "capability `distill_session` is off while [distill].auto = true: unattended \
                 distillation will not run. Add [capabilities.distill_session] enabled = true to \
                 restore it (docs/plans/capability-plugins-design.md §5.4)."
            );
        }

        Ok(Self {
            root,
            agents,
            mcp,
            policy,
            routing,
            capabilities,
        })
    }

    /// Look up an agent by name.
    pub fn agent(&self, name: &str) -> Option<&AgentCard> {
        self.agents.iter().find(|a| a.name == name)
    }

    // `default_agent()` was deleted in t43. Two reasons, both recorded here so nobody re-adds it:
    // the routing fallback's "first enabled card" rule now lives in `api.rs::default_route_agent`
    // and reads the REGISTRY (the API stopped calling this config-file view in t38, leaving this
    // accessor with no production caller), and the fallback's new guard against the permission
    // approver must NEVER be moved here -- it keys on the approver `policy.toml` designates, not on
    // any attribute of `agents.toml`.
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
        self.profile_entries(profile, agent_name)
            .into_iter()
            .map(|(n, e)| mcp_entry_to_acp(n, e))
            .collect()
    }

    /// The registry entries a profile would inject, in injection order.
    ///
    /// Split out of [Self::expand_profile] so the pre-flight below can inspect
    /// the raw command/url before they become ACP values.
    fn profile_entries<'a>(
        &'a self,
        profile: Option<&str>,
        agent_name: &str,
    ) -> Vec<(&'a String, &'a McpEntry)> {
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
            .collect()
    }

    /// Cheap pre-flight of everything a profile would inject.
    ///
    /// The harness reports a broken injection as an opaque
    /// `Internal error: { "details": "mcp-client(mcp): initial connection or
    /// tool synchronization failed" }` — no server, no command, nothing to act
    /// on. This runs on every session start (microseconds: it only resolves
    /// paths) so the failure can be named before the harness ever sees it.
    pub fn preflight_profile(
        &self,
        profile: Option<&str>,
        agent_name: &str,
    ) -> Vec<McpPreflightFailure> {
        self.profile_entries(profile, agent_name)
            .into_iter()
            .filter_map(|(name, entry)| {
                let target = ruagent_mcp::health::PingTarget {
                    command: entry.command.clone(),
                    args: entry.args.clone(),
                    url: entry.url.clone(),
                };
                match ruagent_mcp::health::preflight(&target) {
                    Ok(()) => None,
                    Err(reason) => Some(McpPreflightFailure {
                        server: name.clone(),
                        command: ruagent_mcp::health::describe(&target),
                        reason,
                    }),
                }
            })
            .collect()
    }

    /// [Self::expand_profile] plus the pre-flight, as one fallible step.
    ///
    /// A server whose command cannot be resolved at all is a configuration
    /// error, not a transient one: failing here — naming the server, the
    /// command and the reason — is strictly better than letting the harness
    /// refuse `session/new` with an unactionable string. (Servers that merely
    /// fail their health check are still excluded, not fatal: that path is the
    /// existing issue-#24 behaviour.)
    pub fn expand_profile_preflighted(
        &self,
        profile: Option<&str>,
        agent_name: &str,
    ) -> Result<Vec<agent_client_protocol::schema::v1::McpServer>, String> {
        let failures = self.preflight_profile(profile, agent_name);
        if !failures.is_empty() {
            let detail = failures
                .iter()
                .map(|f| {
                    format!(
                        "MCP 服务器 [{}] 无法启动：{}（配置的命令 = {}）",
                        f.server, f.reason, f.command
                    )
                })
                .collect::<Vec<_>>()
                .join("；");
            return Err(format!(
                "{detail}。请修正 ~/.ruagent/config/mcp.toml 里该 server 的 command/url，\
                 或把它从 profile [{}] 中移除后重试。",
                profile.unwrap_or("(none)")
            ));
        }
        Ok(self.expand_profile(profile, agent_name))
    }
}

/// One server that cannot even be started.
#[derive(Debug, Clone, PartialEq)]
pub struct McpPreflightFailure {
    pub server: String,
    /// The command (or url) as configured, for the message.
    pub command: String,
    pub reason: String,
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
# graph = false          # memories only — skip entity/relation extraction
#                        # (default true: memories + graph)
#
# Capability plane (docs/plans/capability-plugins-design.md).
# ABSENT [capabilities] table = today's behaviour, every gate passes through.
# The moment the table EXISTS, the registry defaults apply: llm-tier
# capabilities default OFF (they cost model tokens), free-tier new ones too.
# Unknown capability ids and unknown option keys are startup errors.
#
# [capabilities.session_extract_rules]
# enabled = true                  # zero-token transcript -> memory candidates
# max_per_input = 32
#
# [capabilities.knowledge_ingest_graph]
# enabled = true                  # knowledge base -> knowledge graph, zero tokens
# max_per_input = 96              # candidates per document
# max_docs_per_pass = 20          # documents per sweep (rides the 60s scan)
#
# [capabilities.distill_session]
# enabled = true                  # unattended ACP extraction on session close
#
# [capabilities.recall_leg_wiki]
# enabled = false                 # switch one recall leg off
# [capabilities.recall_leg_memory_semantic]
# weight = 1.0                    # today's value; raise to prefer the leg
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
                graph: Some(false),
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
        assert!(out.contains("graph = false"));
        // The result reparses as the same policy.
        let policy = ruagent_policy::PolicyConfig::parse(&out).unwrap();
        assert!(!policy.distill.auto);
        assert_eq!(policy.distill.language.as_deref(), Some("简体中文"));
        assert!(policy.distill.agent.is_none());
        assert_eq!(policy.distill.graph, Some(false));
        // A later None removes the graph key again (back to the default).
        editor
            .update(&ruagent_policy::DistillConfig {
                auto: false,
                agent: None,
                language: None,
                prompt: None,
                graph: None,
            })
            .unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(!out.contains("graph ="));
        assert_eq!(
            ruagent_policy::PolicyConfig::parse(&out)
                .unwrap()
                .distill
                .graph,
            None
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    use super::*;

    #[test]
    fn capabilities_editor_round_trips_preserving_comments() {
        let dir = std::env::temp_dir().join(format!(
            "ruagent-cap-edit-{}-{:x}",
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

# the wiki leg is off
[capabilities.recall_leg_wiki]
enabled = false
",
        )
        .unwrap();
        // Replace the table with a DIFFERENT set: wiki leaves it, the graph leg
        // joins it and the memory semantic leg gains options.
        let mut next: BTreeMap<String, ruagent_policy::CapabilityFile> = BTreeMap::new();
        next.insert(
            "recall_leg_graph".into(),
            ruagent_policy::CapabilityFile {
                enabled: Some(false),
                ..Default::default()
            },
        );
        next.insert(
            "recall_leg_memory_semantic".into(),
            ruagent_policy::CapabilityFile {
                weight: Some(1.5),
                min_score: Some(0.25),
                ..Default::default()
            },
        );
        CapabilitiesEditor::new(&path).update(&next).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        // Comments, ordering and other sections survive.
        assert!(out.contains("# top comment"), "{out}");
        assert!(out.contains("[permissions]"), "{out}");
        assert!(out.contains("# distillation"), "{out}");
        assert!(out.contains("auto = true"), "{out}");
        assert!(
            !out.contains("recall_leg_wiki"),
            "the replaced table is gone: {out}"
        );
        assert!(out.contains("[capabilities.recall_leg_graph]"), "{out}");
        assert!(out.contains("weight = 1.5"), "{out}");
        assert!(out.contains("min_score = 0.25"), "{out}");
        // ...and the result reparses into the plane it describes.
        let policy = PolicyConfig::parse(&out).unwrap();
        let plane = crate::capability::CapabilityPlane::from_policy(&policy).unwrap();
        assert!(plane.table_present());
        assert!(
            !plane.gate(crate::capability::CapabilityId::RecallLegGraph, true),
            "the graph leg the file names is off"
        );
        assert_eq!(
            plane
                .options(crate::capability::CapabilityId::RecallLegMemorySemantic)
                .weight,
            Some(1.5)
        );
        assert_eq!(
            plane.configured(crate::capability::CapabilityId::RecallLegWiki),
            "default",
            "the id the replacement dropped is back at its registry default"
        );
        // A key set back to None is REMOVED, not written empty.
        let mut fewer: BTreeMap<String, ruagent_policy::CapabilityFile> = BTreeMap::new();
        fewer.insert(
            "recall_leg_memory_semantic".into(),
            ruagent_policy::CapabilityFile {
                weight: Some(1.5),
                ..Default::default()
            },
        );
        CapabilitiesEditor::new(&path).update(&fewer).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(!out.contains("min_score"), "{out}");
        assert!(!out.contains("recall_leg_graph"), "{out}");
        let policy = PolicyConfig::parse(&out).unwrap();
        let plane = crate::capability::CapabilityPlane::from_policy(&policy).unwrap();
        let semantic = plane.options(crate::capability::CapabilityId::RecallLegMemorySemantic);
        assert_eq!(semantic.weight, Some(1.5), "the kept key stays");
        assert_eq!(semantic.min_score, None, "the removed key is really gone");
        assert!(
            plane.gate(crate::capability::CapabilityId::RecallLegGraph, true),
            "an id the file no longer names is back at its registry default"
        );
        // An EMPTY map removes the table entirely: back to legacy mode (L1).
        CapabilitiesEditor::new(&path)
            .update(&BTreeMap::new())
            .unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(!out.contains("[capabilities"), "{out}");
        assert!(
            out.contains("# top comment"),
            "the rest of the file survives"
        );
        let policy = PolicyConfig::parse(&out).unwrap();
        assert!(
            !crate::capability::CapabilityPlane::from_policy(&policy)
                .unwrap()
                .table_present()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn default_policy_toml_ships_capabilities_commented_out() {
        // The one failure mode this increment cannot ship with: an uncommented
        // (even empty) `[capabilities]` header is a PRESENT table, and its
        // registry defaults switch the llm tier off.
        assert!(
            !DEFAULT_POLICY_TOML
                .lines()
                .any(|l| l.trim_start().starts_with("[capabilities")),
            "the shipped policy.toml must not carry an active [capabilities] table"
        );
        let policy = PolicyConfig::parse(DEFAULT_POLICY_TOML).unwrap();
        assert!(policy.capabilities.is_none(), "absent table = legacy mode");
        assert!(
            !crate::capability::CapabilityPlane::from_policy(&policy)
                .unwrap()
                .table_present()
        );
        // The block that documents the plane is still there for a reader.
        assert!(DEFAULT_POLICY_TOML.contains("# [capabilities.session_extract_rules]"));
    }

    #[test]
    fn fresh_root_is_legacy_mode() {
        let dir = std::env::temp_dir().join(format!("ruagent-cfg-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = DaemonConfig::load(&dir).unwrap();
        assert!(
            !cfg.capabilities.table_present(),
            "a fresh root is legacy mode: every gate answers with today's behaviour (L1)"
        );
        assert!(
            cfg.capabilities
                .gate(crate::capability::CapabilityId::MemoryInjectChat, true)
        );
        assert!(
            !cfg.capabilities
                .gate(crate::capability::CapabilityId::MemoryInjectChat, false),
            "and a legacy mode gate can only narrow, never create work (L2)"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_capability_id_fails_the_load() {
        // L3: a typo in a table name is a boot failure that names it, never a
        // silently ignored section.
        let dir = std::env::temp_dir().join(format!("ruagent-cfg-badcap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::write(
            dir.join("config").join("policy.toml"),
            "[capabilities.session_extract_rule]\nenabled = true\n",
        )
        .unwrap();
        let err = DaemonConfig::load(&dir).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("session_extract_rule"), "{msg}");
        assert!(
            msg.contains("session_extract_rules"),
            "lists the known ids: {msg}"
        );
        assert!(msg.contains("policy.toml"), "names the file it read: {msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

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
        // t43 deleted `cfg.default_agent()`; the routing fallback's rule lives in api.rs now, so this
        // asserts the property that accessor existed for: the shipped config has an enabled agent.
        assert!(cfg.agents.iter().any(|a| a.enabled));
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
        set_or_remove_bool(tbl, "graph", cfg.graph);
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, doc.to_string())
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("renaming into {}", self.path.display()))?;
        Ok(())
    }
}

/// Round-trip editor for the `[capabilities]` table of `policy.toml` — the
/// same discipline as [`DistillEditor`] (toml_edit keeps comments, key order
/// and every other section; temp file + rename is atomic; the mutex serialises
/// interleaved writers), applied to the capability plane.
///
/// It deliberately does NOT validate ids, keys or ranges: validation belongs to
/// `CapabilityPlane::from_policy`, which the `PUT` handler calls BEFORE this
/// write, so a refused update leaves the file untouched (design §5.3).
pub struct CapabilitiesEditor {
    path: PathBuf,
    lock: std::sync::Mutex<()>,
}

impl CapabilitiesEditor {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: std::sync::Mutex::new(()),
        }
    }

    /// REPLACE the whole `[capabilities]` table with `entries`.
    ///
    /// * an entry whose table is empty writes `[capabilities.<id>]` with no
    ///   keys (legal: every declared default applies);
    /// * every key set to `None` is REMOVED from that table, not written empty
    ///   (the `set_or_remove` rule);
    /// * an EMPTY map REMOVES the `[capabilities]` table entirely, which is
    ///   what returns the daemon to legacy mode (L1).
    pub fn update(&self, entries: &BTreeMap<String, ruagent_policy::CapabilityFile>) -> Result<()> {
        let _guard = self.lock.lock().expect("capabilities editor lock");
        let text = std::fs::read_to_string(&self.path)
            .with_context(|| format!("reading {}", self.path.display()))?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .with_context(|| format!("parsing {}", self.path.display()))?;
        if entries.is_empty() {
            doc.remove("capabilities");
        } else {
            doc["capabilities"] = toml_edit::Item::Table(toml_edit::Table::new());
            let root = doc["capabilities"]
                .as_table_mut()
                .context("[capabilities] is not a table")?;
            for (id, file) in entries {
                if !root.contains_key(id) {
                    root[id] = toml_edit::Item::Table(toml_edit::Table::new());
                }
                let tbl = root[id]
                    .as_table_mut()
                    .with_context(|| format!("[capabilities.{id}] is not a table"))?;
                set_or_remove_bool(tbl, "enabled", file.enabled);
                set_or_remove_f64(tbl, "weight", file.weight);
                set_or_remove_f64(tbl, "min_score", file.min_score);
                set_or_remove_u32(tbl, "max_per_input", file.max_per_input);
                set_or_remove_f64(tbl, "min_confidence", file.min_confidence);
                set_or_remove_u32(tbl, "max_docs_per_pass", file.max_docs_per_pass);
            }
        }
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

/// The numeric twin of [`set_or_remove_bool`] for an `f64` option key.
fn set_or_remove_f64(tbl: &mut toml_edit::Table, key: &str, v: Option<f64>) {
    match v {
        Some(v) => {
            tbl[key] = toml_edit::value(v);
        }
        None => {
            tbl.remove(key);
        }
    }
}

/// The numeric twin of [`set_or_remove_bool`] for a `u32` option key.
fn set_or_remove_u32(tbl: &mut toml_edit::Table, key: &str, v: Option<u32>) {
    match v {
        Some(v) => {
            tbl[key] = toml_edit::value(i64::from(v));
        }
        None => {
            tbl.remove(key);
        }
    }
}

/// The bool twin of [`set_or_remove`]: None removes the key (back to
/// the file's absent-key default), Some writes it verbatim.
fn set_or_remove_bool(tbl: &mut toml_edit::Table, key: &str, v: Option<bool>) {
    match v {
        Some(v) => {
            tbl[key] = toml_edit::value(v);
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

# A default agent, BY NAME (enabled in t43). `claude` is the first enabled card of the shipped
# agents.toml, so this states today's outcome explicitly instead of leaving it to alphabetical
# accident -- and the permission-approver guard in the fallback can then never be what a fresh
# install hits. An install that already has a routing.toml keeps its own file: the seed is written
# only when the file is missing.
default = "claude"

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

#[cfg(test)]
mod mcp_preflight_tests {
    use super::*;

    /// Regression (t63): an MCP server that cannot be started must be named —
    /// server, command and reason — BEFORE the harness turns it into
    /// "Internal error: { mcp-client(mcp): initial connection or tool
    /// synchronization failed }", which tells the user nothing.
    fn mcp_config_with(entries: &[(&str, Option<&str>, &[&str])], profile: &[&str]) -> McpConfig {
        let mut servers = BTreeMap::new();
        for (name, command, args) in entries {
            servers.insert(
                (*name).to_string(),
                McpEntry {
                    command: command.map(|c| c.to_string()),
                    args: args.iter().map(|s| s.to_string()).collect(),
                    url: None,
                    inject_for: None,
                },
            );
        }
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "default".to_string(),
            profile.iter().map(|s| s.to_string()).collect(),
        );
        McpConfig {
            servers,
            profiles,
            health: std::sync::Arc::new(crate::mcphealth::McpHealth::default()),
        }
    }

    #[test]
    fn preflight_names_a_server_whose_command_does_not_exist() {
        let cfg = mcp_config_with(
            &[("ruagent", Some(r"C:\nope-9f3a\ruagent.exe"), &["mcp-serve"])],
            &["ruagent"],
        );
        let failures = cfg.preflight_profile(Some("default"), "dsh");
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert_eq!(failures[0].server, "ruagent");
        assert!(failures[0].command.contains("nope-9f3a"), "{failures:?}");
        assert!(failures[0].reason.contains("nope-9f3a"), "{failures:?}");
    }

    #[test]
    fn expand_profile_preflighted_reports_server_and_command() {
        let cfg = mcp_config_with(
            &[("ruagent", Some(r"C:\nope-9f3a\ruagent.exe"), &["mcp-serve"])],
            &["ruagent"],
        );
        let err = cfg
            .expand_profile_preflighted(Some("default"), "dsh")
            .expect_err("must fail before the harness ever sees it");
        // The three things the user needs: which server, which command, why.
        assert!(err.contains("ruagent"), "{err}");
        assert!(err.contains("nope-9f3a"), "{err}");
        assert!(err.contains("mcp-serve"), "{err}");
        assert!(err.contains("mcp.toml"), "must say what to fix: {err}");
        // ...and it must NOT be the harness's opaque wording.
        assert!(!err.contains("mcp-client"), "{err}");
    }

    #[test]
    fn expand_profile_preflighted_passes_a_real_command() {
        let exe = std::env::current_exe().expect("current exe");
        let cfg = mcp_config_with(
            &[("ruagent", Some(&exe.display().to_string()), &["mcp-serve"])],
            &["ruagent"],
        );
        let servers = cfg
            .expand_profile_preflighted(Some("default"), "dsh")
            .expect("a real command passes");
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn a_healthy_profile_and_a_missing_profile_both_expand_without_error() {
        let exe = std::env::current_exe().expect("current exe");
        let cfg = mcp_config_with(
            &[("ruagent", Some(&exe.display().to_string()), &[])],
            &["ruagent"],
        );
        assert!(cfg.expand_profile_preflighted(None, "dsh").is_ok());
        assert!(cfg.expand_profile_preflighted(Some("nope"), "dsh").is_ok());
    }
}
