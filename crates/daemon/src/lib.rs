//! ruagent-daemon: the resident service holding all state (design §3).
//!
//! Boot sequence: load/init config → open SQLite → assign stable agent
//! ids → recover interrupted runs → serve the API.

pub mod api;
pub mod config;
pub mod runs;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use ruagent_core::RunStatus;
use ruagent_store::Db;

/// Data root resolution: `$RUAGENT_HOME`, else `~/.ruagent`.
pub fn default_root() -> PathBuf {
    if let Ok(dir) = std::env::var("RUAGENT_HOME") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ruagent")
}

/// Start the daemon on `addr` with data rooted at `root`.
pub async fn serve(root: PathBuf, addr: SocketAddr) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ruagent=info".into()),
        )
        .init();

    let config = config::DaemonConfig::load(&root)?;
    let db = Db::open(root.join("data").join("ruagent.db")).with_context(|| "opening database")?;

    // Stable agent ids: reuse the stored id per name, else register.
    let mut agents = config.agents.clone();
    for card in &mut agents {
        if let Some(id) = db.agent_id_by_name(&card.name).await? {
            card.id = id;
        }
        db.upsert_agent(card).await?;
    }
    tracing::info!(count = agents.len(), "agent registry loaded");

    // Crash recovery (design §8.3): orphans from a previous daemon are
    // marked interrupted; their transcripts survive for inspection.
    let mut recovered = 0usize;
    for status in [
        RunStatus::Queued,
        RunStatus::Spawning,
        RunStatus::Running,
        RunStatus::WaitingPermission,
    ] {
        for mut run in db.list_runs_by_status(status).await? {
            run.status = RunStatus::Interrupted;
            run.error = Some("daemon restarted mid-run".into());
            run.updated_at = chrono::Utc::now();
            db.update_run(&run).await?;
            recovered += 1;
        }
    }
    if recovered > 0 {
        tracing::warn!(count = recovered, "runs marked interrupted by restart");
    }

    let mgr = Arc::new(runs::RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        config.policy.to_policy(),
        config.mcp.clone(),
    ));

    let state = api::AppState {
        mgr,
        config: Arc::new(config),
    };
    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!("ruagent daemon listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
