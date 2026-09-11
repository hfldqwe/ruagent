# M3 #21 part 3: the mcp crate — the platform MCP server bridge
# run: python .patch_m3_mcp.py

cargo = '''[package]
name = "ruagent-mcp"
description = "MCP registry and the platform's own MCP server"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
rmcp = { version = "3", features = ["server", "transport-io"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "io-std"] }
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
'''
open('crates/mcp/Cargo.toml', 'w', encoding='utf-8').write(cargo)
print("mcp Cargo.toml written")

lib = '''//! ruagent-mcp: the platform's own MCP server (design SS6.5).
//!
//! A thin stdio-to-HTTP bridge: agents spawn `ruagent mcp-serve` (it is
//! registered in mcp.toml and injected into every session); the tools
//! call the daemon's local REST API. This is the ONLY seam external
//! harnesses need to reach the central memory, knowledge, and task
//! state — no SDK adapters (the Agno lesson).

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, schemars, tool, tool_router};

/// Configuration for the bridge.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Daemon base URL (e.g. http://127.0.0.1:8787).
    pub daemon_url: String,
}

impl BridgeConfig {
    pub fn from_env() -> Self {
        Self {
            daemon_url: std::env::var("RUAGENT_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8787".into()),
        }
    }
}

/// The platform tool set.
#[derive(Debug, Clone)]
pub struct PlatformTools {
    config: BridgeConfig,
    http: reqwest::Client,
    tool_router: ToolRouter<Self>,
}

impl PlatformTools {
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl PlatformTools {
    // -- memory ---------------------------------------------------------

    #[tool(description = "Search the user's long-term memories (facts, preferences, lessons learned across sessions). Use this before asking the user something they may have already told you.")]
    async fn memory_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/search", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("q", &params.query)])
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(hits_to_text(&resp["hits"]))
    }

    #[tool(description = "Store a long-term memory about the user, a project, or a lesson learned. Stores: profile (user facts), observation (general), procedure (how-to, project/global), lesson (project/global). Namespaces: user, global, project:<name>, agent:<name>. Optionally pass supersedes=<memory id> to replace it.")]
    async fn memory_write(
        &self,
        Parameters(params): Parameters<WriteParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/write", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "store": params.store,
                "namespace": params.namespace,
                "content": params.content,
                "supersedes": params.supersedes,
            }))
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(format!("memory write: {}", resp["outcome"].as_str().unwrap_or("?")))
    }

    // -- knowledge ------------------------------------------------------

    #[tool(description = "Search the knowledge base of ingested documents (hybrid semantic + keyword search).")]
    async fn knowledge_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/search", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("q", &params.query)])
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(hits_to_text(&resp["hits"]))
    }

    #[tool(description = "Ingest a document (markdown, notes, code) into the knowledge base so it becomes searchable.")]
    async fn knowledge_ingest(
        &self,
        Parameters(params): Parameters<IngestParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/ingest", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "name": params.name,
                "content": params.content,
            }))
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(format!(
            "ingested {} chunks",
            resp["chunks"].as_i64().unwrap_or(0)
        ))
    }

    // -- platform ops (the symmetric design, SS7.2) ----------------------

    #[tool(description = "List the platform's tasks (durable work items) with their status. Optionally filter by status: pending|in_progress|blocked|done|cancelled.")]
    async fn list_tasks(
        &self,
        Parameters(params): Parameters<TaskFilterParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/tasks", self.config.daemon_url);
        let mut req = self.http.get(&url);
        if let Some(status) = &params.status {
            req = req.query(&[("status", status)]);
        }
        let resp: serde_json::Value = req
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        let tasks = resp["tasks"].as_array().cloned().unwrap_or_default();
        let mut out = String::new();
        for t in tasks.iter().take(20) {
            out.push_str(&format!(
                "[{}] {} ({})\\n",
                t["status"].as_str().unwrap_or("?"),
                t["title"].as_str().unwrap_or("?"),
                t["id"].as_str().unwrap_or("?")
            ));
        }
        if out.is_empty() {
            out.push_str("no tasks");
        }
        Ok(out)
    }
}

impl ServerHandler for PlatformTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "ruagent platform tools: search/write the user's long-term memory, \\
                 search/ingest the knowledge base, and list platform tasks. \\
                 Prefer memory_search before asking the user for known facts.",
            )
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "What to look for")]
    pub query: String,
    #[schemars(description = "Max results (default 8)")]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WriteParams {
    #[schemars(description = "profile | observation | procedure | lesson")]
    pub store: String,
    #[schemars(description = "user | global | project:<name> | agent:<name>")]
    pub namespace: String,
    #[schemars(description = "The memory content, one concise fact")]
    pub content: String,
    #[schemars(description = "Memory id this one replaces (optional)")]
    pub supersedes: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IngestParams {
    #[schemars(description = "Document name")]
    pub name: String,
    #[schemars(description = "Document content")]
    pub content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TaskFilterParams {
    #[schemars(description = "Optional status filter")]
    pub status: Option<String>,
}

fn hits_to_text(hits: &serde_json::Value) -> String {
    let hits = hits.as_array().cloned().unwrap_or_default();
    if hits.is_empty() {
        return "no results".into();
    }
    hits.iter()
        .map(|h| {
            let content = h["content"].as_str().or_else(|| h["result"].as_str()).unwrap_or("?");
            let source = h["document"].as_str().unwrap_or("memory");
            format!("[{source}] {content}")
        })
        .collect::<Vec<_>>()
        .join("\\n---\\n")
}

fn rpc_error(e: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(format!("daemon call failed: {e}"), None)
}

/// Run the MCP server over stdio (the `ruagent mcp-serve` subcommand).
pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::service::{serve_server, InitializeResult};
    let handler = PlatformTools::new(BridgeConfig::from_env());
    let transport = rmcp::transport::stdio();
    let server = serve_server(handler, transport).await?;
    let _join: InitializeResult = server.waiting().await?;
    Ok(())
}
'''
open('crates/mcp/src/lib.rs', 'w', encoding='utf-8').write(lib)
print("mcp lib.rs written")

# ---- CLI: mcp-serve subcommand ----
p = 'cli/Cargo.toml'
s = open(p, encoding='utf-8').read()
if 'ruagent-mcp' not in s:
    s = s.replace('ruagent-daemon.workspace = true',
                  'ruagent-daemon.workspace = true\nruagent-mcp.workspace = true')
    open(p, 'w', encoding='utf-8').write(s)
    print("cli dep added")

p = 'cli/src/main.rs'
s = open(p, encoding='utf-8').read()
s = s.replace('''    /// Run a prompt as a one-off task and stream the result.
    Run {''',
'''    /// Run the platform MCP server over stdio (spawned by agents; talks
    /// to the daemon at RUAGENT_URL).
    McpServe,
    /// Run a prompt as a one-off task and stream the result.
    Run {''')
s = s.replace('''        Cmd::Run { prompt, agent } => run(&cli.url, &prompt, agent.as_deref()),''',
'''        Cmd::McpServe => ruagent_mcp::serve_stdio().await.map_err(|e| anyhow::anyhow!("{e}")),
        Cmd::Run { prompt, agent } => run(&cli.url, &prompt, agent.as_deref()),''')
open(p, 'w', encoding='utf-8').write(s)
print("cli subcommand added")

# ---- default mcp.toml registers the platform server ----
p = 'crates/daemon/src/config.rs'
s = open(p, encoding='utf-8').read()
s = s.replace('''const DEFAULT_MCP_TOML: &str = r#"# ruagent MCP registry. See design SS7.1.
# Injection is an overlay: each CLI's own MCP config is never touched.
#
# [mcp.filesystem]
# command = "npx @modelcontextprotocol/server-filesystem"
# args = ["--root", "C:/projects"]
# inject_for = ["claude", "opencode"]     # optional membership filter
#
# [mcp.fetch]
# url = "https://mcp.example.com/sse"

[profile.default]
servers = []
"#;''',
'''const DEFAULT_MCP_TOML: &str = r#"# ruagent MCP registry. See design SS7.1.
# Injection is an overlay: each CLI's own MCP config is never touched.
#
# [mcp.filesystem]
# command = "npx @modelcontextprotocol/server-filesystem"
# args = ["--root", "C:/projects"]
# inject_for = ["claude", "opencode"]     # optional membership filter
#
# [mcp.fetch]
# url = "https://mcp.example.com/sse"

# The platform's own MCP server: memory, knowledge, task tools for
# every agent (design SS6.5). Requires the ruagent binary reachable as
# `ruagent` on PATH and a running daemon (`ruagent serve`).
[mcp.ruagent]
command = "ruagent"
args = ["mcp-serve"]

[profile.default]
servers = ["ruagent"]
"#;''')
open(p, 'w', encoding='utf-8').write(s)
print("default mcp.toml registers the platform server")
'''
