//! ruagent-daemon: the resident service holding all state (design §3).
//!
//! Boot sequence: load/init config → open SQLite → assign stable agent
//! ids → recover interrupted runs → serve the API.

pub mod api;
pub mod capability;
pub mod chat;
pub mod config;
pub mod distill;
pub mod extract_plane;
pub mod knowledge_graph;
pub mod mcphealth;
pub mod memembed;
pub mod orphans;
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
/// t76 / audit A-3: a non-loopback bind is a CAPABILITY, never a default.
///
/// This daemon is local-first and single-user, and it has **no authentication of
/// any kind** (audit t71: `Authorization`/`bearer` have 0 hits in the tree). So
/// binding anything other than a loopback address publishes *every* read and
/// write endpoint to whoever can reach the port -- including the one-shot
/// `POST /api/v1/memory/migrate-distilled-prefix`, which rewrites memory bodies.
/// Before t76, `ruagent serve --addr 0.0.0.0:8787` did that silently.
///
/// `RUAGENT_ALLOW_REMOTE=1` (or the CLI's `--allow-remote`) is the explicit
/// consent. Loopback callers are unaffected: this is checked only when the
/// address is not loopback.
fn remote_bind_allowed() -> bool {
    matches!(
        std::env::var("RUAGENT_ALLOW_REMOTE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

pub async fn serve(root: PathBuf, addr: SocketAddr) -> Result<()> {
    serve_with_remote(root, addr, false).await
}

/// `serve`, with the explicit-consent bit for a non-loopback bind (t76).
pub async fn serve_with_remote(root: PathBuf, addr: SocketAddr, allow_remote: bool) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ruagent=info".into()),
        )
        .init();

    // Orphan sweep (issue #44): a daemon that died leaves its agent
    // children running — kill them before anything new spawns.
    orphans::boot(&root);

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

    // ONE handle, shared (t260): the knowledge base is opened once and every
    // consumer -- the API, the background scanner, and BOTH injection producers
    // (chat and runs) -- holds a clone of this Arc. Before t260 the injection
    // paths had no handle at all, so injecting knowledge would have meant
    // deciding an embedder a second time; the vectors would then have had two
    // sources of truth.
    let knowledge = std::sync::Arc::new(knowledge);

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
            language: config.policy.distill.language.clone(),
            prompt: config.policy.distill.prompt.clone(),
            graph: config.policy.distill.graph.unwrap_or(true),
        },
        Some(knowledge.embedder()),
        distill::AgentRegistry {
            enabled: agents_cards.iter().filter(|c| c.enabled).cloned().collect(),
        },
    );

    // t8: THE LIVE-PLANE INSTALL. The registry is assembled ONCE, from the
    // config `DaemonConfig::load` already validated (an unknown id, an
    // undeclared option key or an out-of-range value fails the boot there,
    // naming what was refused), and handed to BOTH managers as one shared
    // handle. Three consequences, and each is a requirement of this increment:
    //   * a `[capabilities]` table in policy.toml takes effect AT BOOT — before
    //     this line the live plane was `legacy()`, so the file had no effect
    //     until someone happened to PUT;
    //   * `PUT /api/v1/capabilities` writes through this Arc, so a toggle is
    //     seen by the chat prompt path, the run prompt path, the 60 s knowledge
    //     sweep and the recall/ingest/distill routes at once, with no restart;
    //   * no consumer re-reads policy.toml or invents a default of its own.
    chats.set_capabilities(config.capabilities.clone());
    mgr.set_capabilities(chats.capabilities_handle());

    {
        let kb_scanner = knowledge.clone();
        // t4: the KB → graph sweep rides THIS loop (design §13.1) — no new
        // background job. The plane is read from the shared handle on every pass,
        // so turning `knowledge_ingest_graph` on or off takes effect at the next
        // scan without a restart; with it off (the default) the sweep is a no-op
        // that touches neither the graph nor the ledger.
        let caps = chats.capabilities_handle();
        let db_for_ingest = db.clone();
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
                knowledge_graph::sweep_if_enabled(&db_for_ingest, &kb_scanner, &caps).await;
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }

    // Chat asks die with the chat (issue #40): closing a chat drops its
    // parked asks — the agent side fails closed and the inbox stays clean.
    {
        let mgr = Arc::clone(&mgr);
        chats.set_ask_dropper(Arc::new(move |id| mgr.drop_pending_for(id)));
    }

    // t260: both injection producers get the same handle, so the knowledge
    // block is produced from the same vectors the API answers from.
    chats.set_knowledge(std::sync::Arc::clone(&knowledge));
    mgr.set_knowledge(std::sync::Arc::clone(&knowledge));

    let state = api::AppState {
        mgr,
        config: Arc::new(config),
        knowledge: std::sync::Arc::clone(&knowledge),
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

    // MCP health (issue #24): a live initialize + tools/list roundtrip
    // per registered server, boot + every few minutes. Down servers are
    // excluded from injection (they would break session startup) and
    // the panel shows the state.
    state.config.mcp.health.spawn_loop(state.config.mcp.clone());

    let app = api::router(state);

    // t76 / audit A-3: refuse a non-loopback bind that nobody asked for.
    let allow_remote = allow_remote || remote_bind_allowed();
    if !addr.ip().is_loopback() && !allow_remote {
        let msg = format!(
            "refusing to bind {addr}: this daemon has NO authentication, so a non-loopback bind \
             publishes every endpoint -- including POST /api/v1/memory/migrate-distilled-prefix, \
             which rewrites stored memory bodies -- to anyone who can reach the port. \
             Pass --allow-remote or set RUAGENT_ALLOW_REMOTE=1 to accept that exposure \
             (see audit A-3, docs/design/reviews/gen3-audit-security.md)."
        );
        tracing::warn!("{msg}");
        eprintln!("ruagent: WARN: {msg}");
        anyhow::bail!(msg);
    }

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    // Name the ACTUAL bound address (a port of 0 is resolved by the kernel) and
    // say out loud which side of the loopback line we are on.
    let local = listener.local_addr().unwrap_or(addr);
    if local.ip().is_loopback() {
        tracing::info!("ruagent daemon listening on http://{local} (loopback only, no auth)");
    } else {
        tracing::warn!(
            "ruagent daemon listening on http://{local} -- EXPOSED to non-loopback (explicitly \
             allowed by --allow-remote/RUAGENT_ALLOW_REMOTE): every read and write endpoint is \
             reachable from the network and there is no authentication"
        );
    }
    axum::serve(listener, app).await?;
    Ok(())
}
