//! MCP health registry (issue #24): the last check result per
//! registered server, refreshed by a background loop (boot + every
//! few minutes). Injection consults it — a server whose last check
//! failed is excluded with a warning instead of breaking session
//! startup; the panel shows the live state.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::config::McpConfig;

/// How often the background loop re-checks every server.
const RESCAN: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// One server's last check result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerHealth {
    pub state: &'static str, // "ok" | "down"
    pub tools: Option<u32>,
    pub latency_ms: u64,
    pub error: Option<String>,
    /// Epoch ms of the check.
    pub checked_at: i64,
}

/// The live registry. Clones of the parent [`McpConfig`] share one.
#[derive(Default)]
pub struct McpHealth {
    states: RwLock<HashMap<String, ServerHealth>>,
}

impl std::fmt::Debug for McpHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpHealth")
            .field("checked", &self.states.read().map(|s| s.len()).unwrap_or(0))
            .finish()
    }
}

impl McpHealth {
    pub fn is_down(&self, server: &str) -> bool {
        self.states
            .read()
            .expect("mcp health lock")
            .get(server)
            .is_some_and(|s| s.state == "down")
    }

    /// The names of every server whose last check failed.
    pub fn down_set(&self) -> HashSet<String> {
        self.states
            .read()
            .expect("mcp health lock")
            .iter()
            .filter(|(_, s)| s.state == "down")
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// The full last-known state (API/panel).
    pub fn snapshot(&self) -> HashMap<String, ServerHealth> {
        self.states.read().expect("mcp health lock").clone()
    }

    /// Check every registered server (parallel, each bounded by the
    /// ping timeout) and store the results.
    pub async fn refresh(&self, cfg: &McpConfig) {
        let mut checks = Vec::new();
        for (name, entry) in &cfg.servers {
            let target = ruagent_mcp::health::PingTarget {
                command: entry.command.clone(),
                args: entry.args.clone(),
                url: entry.url.clone(),
            };
            let name = name.clone();
            checks.push(async move {
                let outcome = ruagent_mcp::health::ping(&target).await;
                (name, outcome)
            });
        }
        let results = futures::future::join_all(checks).await;
        let now = chrono::Utc::now().timestamp_millis();
        let mut states = self.states.write().expect("mcp health lock");
        for (name, outcome) in results {
            let state = if outcome.ok { "ok" } else { "down" };
            if outcome.ok {
                tracing::info!(
                    server = %name,
                    tools = outcome.tools.unwrap_or(0),
                    latency_ms = outcome.latency_ms,
                    "mcp health ok"
                );
            } else {
                tracing::warn!(
                    server = %name,
                    error = outcome.error.as_deref().unwrap_or("-"),
                    "mcp health check failed — server excluded from injection"
                );
            }
            states.insert(
                name,
                ServerHealth {
                    state,
                    tools: outcome.tools,
                    latency_ms: outcome.latency_ms,
                    error: outcome.error,
                    checked_at: now,
                },
            );
        }
    }

    /// The background loop: check at boot, then every RESCAN.
    pub fn spawn_loop(self: &Arc<Self>, cfg: McpConfig) {
        let health = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                health.refresh(&cfg).await;
                tokio::time::sleep(RESCAN).await;
            }
        });
    }
}
