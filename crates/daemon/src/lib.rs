//! ruagent-daemon: the resident service holding all state (design §3).
//!
//! Boot sequence: load/init config → open SQLite → assign stable agent
//! ids → recover interrupted runs → serve the API.

pub mod api;
pub mod chat;
pub mod config;
pub mod distill;
pub mod memembed;
pub mod registry;
pub mod runs;
pub mod sessions;
pub mod skills;
pub mod wiki;

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

    // Session history auto-sync: claude-code / dsh / ruagent transcripts
    // indexed at boot, rescanned every 60s (mtime-incremental).
    // The user's home holds every CLI's store (~/.claude, ~/.dsh); root
    // itself is ~/.ruagent.
    let indexer = {
        let home = root
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| root.clone());
        let idx = std::sync::Arc::new(sessions::SessionIndexer::new(db.clone(), home));
        let runner = idx.clone();
        tokio::spawn(async move {
            runner.run().await;
        });
        idx
    };

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

    let agents_cards = agents.clone();
    let mgr = Arc::new(runs::RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        config.policy.to_policy(),
        config.mcp.clone(),
    ));
    mgr.start_approver_loop();

    // Bootstrap the bundled operator skill into the platform library
    // (design SS7.2 + SS23: teach agents to operate the platform).
    {
        let platform_skills = root.join("skills");
        std::fs::create_dir_all(&platform_skills)?;
        let bundled = std::path::PathBuf::from("skills/ruagent-operator");
        if bundled.join("SKILL.md").is_file() {
            skills::install_skill(&bundled, &platform_skills)?;
        }
    }

    // Knowledge base: real semantics by default — fastembed (bge-small)
    // downloads its model on first use; every failure mode (offline,
    // model download error, table built with another embedder) falls
    // back to the offline hash embedder so the platform always boots.
    // RUAGENT_EMBEDDER=hash forces the fallback deterministically.
    // Model cache under the ruagent home by default (stable across cwd;
    // pre-seeded via `ruagent models pull` or HF_ENDPOINT mirror downloads).
    // SAFETY: single-threaded startup section before any worker threads
    // (or async runtimes) have spawned; the only other env access is the
    // read below.
    if std::env::var_os("HF_HOME").is_none() {
        // SAFETY: see above.
        unsafe {
            std::env::set_var("HF_HOME", root.join("models").join("hub"));
        }
    }
    let knowledge = match std::env::var("RUAGENT_EMBEDDER").as_deref() {
        Ok("hash") => ruagent_knowledge::Knowledge::open(&root, db.clone()).await,
        _ => {
            let fast = ruagent_knowledge::FastEmbedder::try_new().await;
            match fast {
                Ok(fe) => {
                    match ruagent_knowledge::Knowledge::with_embedder(
                        &root,
                        db.clone(),
                        std::sync::Arc::new(fe),
                    )
                    .await
                    {
                        Ok(k) => Ok(k),
                        Err(e) => {
                            tracing::warn!(error = %e,
                                "knowledge table was built with another embedder;                                  falling back to hash embedder (re-ingest to upgrade)");
                            ruagent_knowledge::Knowledge::open(&root, db.clone()).await
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e,
                        "fastembed unavailable (offline?); falling back to hash embedder");
                    ruagent_knowledge::Knowledge::open(&root, db.clone()).await
                }
            }
        }
    }
    .with_context(|| "opening knowledge base")?;
    tracing::info!(
        embedder = knowledge.embedder_name(),
        "knowledge base embedder"
    );

    // An embedder switch migrates the knowledge table at open (see
    // store.rs migrate_embedder); memory rows get the same treatment
    // here — in the background, so a large library never blocks boot.
    {
        let db_m = db.clone();
        let embedder_m = knowledge.embedder();
        tokio::spawn(async move {
            let n = memembed::reembed_stale(&db_m, embedder_m).await;
            if n > 0 {
                tracing::info!(
                    reembedded = n,
                    "memory embeddings migrated to the active model"
                );
            }
        });
    }

    // Knowledge markdown sync (design-study memsearch/EverOS): the
    // `.md` files under <root>/knowledge are the source of truth, the
    // SQLite+LanceDB index a derived shadow. This scan — boot + every
    // 60s, SHA-256-incremental — picks up out-of-band edits (editor,
    // git checkout) and deletions, and reindexes what changed.
    {
        let kb_scanner = knowledge.clone();
        tokio::spawn(async move {
            loop {
                match kb_scanner.scan().await {
                    Ok(report) if report.changed() > 0 => tracing::info!(
                        indexed = report.indexed,
                        unchanged = report.unchanged,
                        removed = report.removed,
                        errors = report.errors,
                        "knowledge scan"
                    ),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "knowledge scan failed"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }

    // Chats (terminal-like sessions) share the permission inbox with runs:
    // every chat ask parks in the same inbox (rules → human), keyed by the
    // chat id.
    let mgr_for_chats = Arc::clone(&mgr);
    let chats = chat::ChatManager::new(
        db.clone(),
        root.clone(),
        Arc::new(move |ask, context_id| mgr_for_chats.park_external_ask(ask, context_id)),
        config.mcp.clone(),
        // Session-distillation policy + shared embedder + registry view.
        distill::AutoDistill {
            auto: config.policy.distill.auto,
            agent: config.policy.distill.agent.clone(),
        },
        Some(knowledge.embedder()),
        distill::AgentRegistry {
            enabled: agents_cards.iter().filter(|c| c.enabled).cloned().collect(),
        },
    );

    let state = api::AppState {
        mgr,
        config: Arc::new(config),
        knowledge: Arc::new(knowledge),
        chats,
        sessions: indexer,
    };

    // Option catalogs (model lists, permission modes, thinking levels)
    // per runtime: load the persisted copies first (the panel pickers
    // are instant after a daemon restart), then refresh them in the
    // background at boot and every few hours.
    {
        let chats = std::sync::Arc::clone(&state.chats);
        let mgr = std::sync::Arc::clone(&state.mgr);
        tokio::spawn(async move {
            chats.load_option_cache().await;
            loop {
                chats.refresh_all_options(&mgr.agents()).await;
                tokio::time::sleep(chat::OPTIONS_REFRESH).await;
            }
        });
    }

    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!("ruagent daemon listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
