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

// ---------------------------------------------------------------------------
// The exit trace (issue #64)
// ---------------------------------------------------------------------------
//
// WHY THIS EXISTS. On 2026-10-17 the resident daemon vanished: the log ended on
// an ordinary INFO line (chat idle, closing + mcp health ok), there was no
// panic, no error, no shutdown record, the process was gone and the port
// refused -- and every member's private daemon died in the same moment. A
// symptom with no record cannot be diagnosed twice, because from the outside
// "killed by a Windows job object" and "clean exit" look IDENTICAL when neither
// of them writes anything.
//
// So every daemon writes ONE line into <root>/data/daemon.exit. At boot the line
// says running; every path that KNOWS why it is leaving rewrites it with the
// reason (a graceful stop, a console signal, a panic, a startup error). The
// interesting case is the one that never gets to rewrite it -- TerminateProcess,
// a job object's KILL_ON_JOB_CLOSE, a power loss. That case stays VISIBLE: the
// file still says running for a pid that is no longer alive, so "no record" is
// not "no evidence" -- it IS the evidence, and it is distinguishable from a
// clean exit, both on the next boot (a WARN in the log) and in the launcher's
// status command.
pub mod exit_trace {
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result};

    /// The file name, under <root>/data.
    pub const FILE: &str = "daemon.exit";
    /// The graceful-stop REQUEST a launcher writes (issue #64): a resident daemon
    /// started by WMI has no console to send Ctrl-C to, so the launcher asks for a
    /// clean exit through the root it already owns. Removed by its writer, and
    /// removed at boot if a crash left one behind -- a stale request must never
    /// kill the daemon that comes up after it.
    pub const STOP_REQUEST: &str = "daemon.stop";

    /// What the trace file said, when it was read.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Trace {
        /// The daemon that wrote this line had not left yet.
        Running { pid: u32, since: String },
        /// It said why it left.
        Exit {
            reason: String,
            pid: u32,
            at: String,
            detail: Option<String>,
        },
    }

    impl Trace {
        pub fn is_running(&self) -> bool {
            matches!(self, Trace::Running { .. })
        }

        pub fn pid(&self) -> u32 {
            match self {
                Trace::Running { pid, .. } | Trace::Exit { pid, .. } => *pid,
            }
        }

        /// One line for a human: what the file says, in words.
        pub fn describe(&self) -> String {
            match self {
                Trace::Running { pid, since } => {
                    format!("RUNNING pid={pid} since={since} (no exit record was written)")
                }
                Trace::Exit {
                    reason,
                    pid,
                    at,
                    detail,
                } => match detail {
                    Some(d) => format!("exit reason={reason} pid={pid} at={at} detail={d}"),
                    None => format!("exit reason={reason} pid={pid} at={at}"),
                },
            }
        }
    }

    pub fn path(root: &Path) -> PathBuf {
        root.join("data").join(FILE)
    }

    pub fn stop_request_path(root: &Path) -> PathBuf {
        root.join("data").join(STOP_REQUEST)
    }

    /// The timestamp format every trace line uses: RFC3339, UTC.
    fn now() -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// A detail is written as the REST of the line, so it must not be able to
    /// break the record into two (a panic message contains newlines) or run away
    /// with the file (a panic message can be long). Truncation is REPORTED in the
    /// line rather than silently shortening the evidence.
    fn sanitize(detail: &str) -> String {
        let flat: String = detail
            .chars()
            .map(|c| {
                if c == '\n' || c == '\r' || c == '\t' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let flat = flat.trim();
        const MAX: usize = 500;
        if flat.chars().count() <= MAX {
            return flat.to_string();
        }
        let cut: String = flat.chars().take(MAX).collect();
        format!("{cut} [truncated]")
    }

    fn write(root: &Path, line: &str) -> Result<()> {
        let path = path(root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        // temp + rename, the discipline daemon.pid uses: a torn write must never
        // be readable as a record.
        let tmp = path.with_extension("exit.tmp");
        std::fs::write(&tmp, line).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }

    /// This daemon is up: the line a silent death leaves behind.
    pub fn record_running(root: &Path) -> Result<()> {
        write(
            root,
            &format!("running pid={} since={}\n", std::process::id(), now()),
        )
    }

    /// This daemon is leaving, and here is why.
    ///
    /// The reason is a SHORT token (graceful, stop-request, signal:console,
    /// panic, error, server-returned); the detail is the free text that has to
    /// survive (a panic message, an error, a launcher's note).
    pub fn record_exit(root: &Path, reason: &str, detail: Option<&str>) -> Result<()> {
        let mut line = format!(
            "exit reason={} pid={} at={}",
            reason.replace(' ', "-"),
            std::process::id(),
            now()
        );
        if let Some(d) = detail {
            let d = sanitize(d);
            if !d.is_empty() {
                line.push_str(" detail=");
                line.push_str(&d);
            }
        }
        line.push('\n');
        write(root, &line)
    }

    /// Parse one trace line. Tolerant by design: this file is read to EXPLAIN an
    /// unexplained death, so an unreadable line comes back as None (and the caller
    /// says so) rather than panicking in the reader.
    pub fn parse(text: &str) -> Option<Trace> {
        let text = text.trim();
        let mut it = text.split_whitespace();
        let kind = it.next()?;
        let mut pid: Option<u32> = None;
        let mut since: Option<String> = None;
        let mut at: Option<String> = None;
        let mut reason: Option<String> = None;
        // The detail is the REST of the line: it may contain spaces.
        let (head, detail) = match text.find("detail=") {
            Some(i) => (&text[..i], Some(text[i + 7..].trim().to_string())),
            None => (text, None),
        };
        for tok in head.split_whitespace().skip(1) {
            match tok.split_once('=') {
                Some(("pid", v)) => pid = v.parse::<u32>().ok(),
                Some(("since", v)) => since = Some(v.to_string()),
                Some(("at", v)) => at = Some(v.to_string()),
                Some(("reason", v)) => reason = Some(v.to_string()),
                _ => {}
            }
        }
        match kind {
            "running" => Some(Trace::Running {
                pid: pid?,
                since: since?,
            }),
            "exit" => Some(Trace::Exit {
                reason: reason?,
                pid: pid?,
                at: at?,
                detail,
            }),
            _ => None,
        }
    }

    /// The last record, if the file exists and parses.
    pub fn read(root: &Path) -> Option<Trace> {
        let text = std::fs::read_to_string(path(root)).ok()?;
        parse(&text)
    }

    /// The previous run's record, WHEN IT SAYS RUNNING AND THAT PID IS DEAD.
    ///
    /// This is the whole point of the file (issue #64): a daemon that was killed
    /// could not write anything, so the trace it left is the one it wrote at boot
    /// -- and the fact that its pid is gone makes that line a diagnosis instead of
    /// a stale line. Note what is NOT claimed: an unreachable process table makes
    /// the liveness check answer "not alive" for a daemon that may still be
    /// running, so the caller LOGS this and never acts on it.
    pub fn silent_death(root: &Path) -> Option<Trace> {
        let trace = read(root)?;
        if !trace.is_running() {
            return None;
        }
        if crate::orphans::process_started_at_ms(trace.pid()).is_some() {
            return None;
        }
        Some(trace)
    }

    /// Where the panic hook writes: a panic hook is PROCESS-wide, so the root it
    /// can write to is process-wide too. Set once, at serve time.
    static TRACE_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

    /// Install the panic hook, chained to whatever hook was installed before.
    ///
    /// Idempotent by Once: the first call installs, later calls only try to set a
    /// root they cannot (the OnceLock is already set).
    pub fn install_panic_hook(root: PathBuf) {
        let _ = TRACE_ROOT.set(root);
        static INSTALLED: std::sync::Once = std::sync::Once::new();
        INSTALLED.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                if let Some(root) = TRACE_ROOT.get() {
                    // LOCATION AND MESSAGE, not a backtrace: this line has to be
                    // readable in status, and the log already keeps the backtrace.
                    let where_ = info
                        .location()
                        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                        .unwrap_or_else(|| "unknown location".into());
                    let what = info
                        .payload()
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| info.payload().downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "panic".into());
                    let _ = record_exit(root, "panic", Some(&format!("{what} at {where_}")));
                }
                previous(info);
            }));
        });
    }

    /// Record a panic WITHOUT the hook, for a caller that has one to hand (the
    /// tests, and any future path that catches an unwind itself).
    pub fn record_panic(root: &Path, message: &str, location: &str) -> Result<()> {
        record_exit(root, "panic", Some(&format!("{message} at {location}")))
    }
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

    // Issue #64, the measurement the startup-method question needs: WAS THIS
    // PROCESS ALREADY INSIDE A WINDOWS JOB OBJECT when it started? Asked BEFORE
    // the orphan sweep, because that sweep creates a job of its own -- after it,
    // the answer is always yes and the question is gone. A daemon handed to WMI
    // and one started by a job-managing parent look identical from inside until
    // you ask this, and "killed with no trace" is exactly what an outer job's
    // KILL_ON_JOB_CLOSE looks like.
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        // BOOL is i32; spelled out rather than imported, the same way
        // orphans.rs spells the exit-code out parameter.
        let mut inside: i32 = 0;
        let checked = unsafe {
            IsProcessInJob(
                GetCurrentProcess(),
                std::ptr::null_mut(),
                &mut inside as *mut i32,
            )
        };
        if checked != 0 {
            tracing::info!(
                in_an_outer_job_object = inside != 0,
                "startup job-object measurement (issue #64): true = this process was \
                 already inside somebody else's job when it started"
            );
        } else {
            tracing::warn!("startup job-object measurement (issue #64) could not be taken");
        }
    }

    // Orphan sweep (issue #44): a daemon that died leaves its agent
    // children running — kill them before anything new spawns.
    orphans::boot(&root);

    // Issue #64: leave a trace a kill cannot erase. The PREVIOUS record is read
    // first, because a "running" line whose pid is gone is exactly the shape the
    // 2026-10-17 disappearance had -- and reading it here is what turns the next
    // boot into the place the diagnosis becomes visible.
    if let Some(dead) = exit_trace::silent_death(&root) {
        tracing::warn!(
            trace = %dead.describe(),
            "the previous daemon left NO exit record: it was killed (job object / \
             TerminateProcess / power loss), not shut down"
        );
    }
    exit_trace::install_panic_hook(root.clone());
    if let Err(e) = exit_trace::record_running(&root) {
        tracing::warn!(error = %e, "writing the exit trace failed");
    }
    // A stop request left behind by a crashed run belongs to that run, never to
    // this one: honouring it would make a restart impossible.
    let stale_stop = exit_trace::stop_request_path(&root);
    if stale_stop.is_file() {
        tracing::warn!(
            path = %stale_stop.display(),
            "removing a stop request left by a previous run"
        );
        let _ = std::fs::remove_file(&stale_stop);
    }

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
    // Issue #64: the server now has a SHUTDOWN path, and every way out of it
    // writes the exit trace first -- so "the daemon is gone" and "why" stop being
    // two different questions.
    let why: std::sync::Arc<std::sync::Mutex<Option<String>>> = Default::default();
    let (signal, gate) = shutdown_signal(root.clone(), std::sync::Arc::clone(&why));
    let server = axum::serve(listener, app).with_graceful_shutdown(signal);
    let served = tokio::select! {
        r = server => r,
        _ = async move {
            gate.notified().await;
            tokio::time::sleep(EXIT_DRAIN).await;
        } => Ok(()),
    };
    match served {
        Ok(()) => {
            let reason = why
                .lock()
                .expect("exit reason lock")
                .clone()
                .unwrap_or_else(|| "server-returned".to_string());
            if let Err(e) = exit_trace::record_exit(&root, &reason, None) {
                tracing::warn!(error = %e, "writing the exit trace failed");
            }
            tracing::info!(reason = %reason, "daemon leaving: the exit trace was written");
            Ok(())
        }
        Err(e) => {
            if let Err(werr) = exit_trace::record_exit(&root, "error", Some(&e.to_string())) {
                tracing::warn!(error = %werr, "writing the exit trace failed");
            }
            tracing::error!(error = %e, "the server stopped with an error");
            Err(anyhow::Error::new(e).context("serving the API"))
        }
    }
}

/// How long a graceful stop may take before the process leaves anyway.
///
/// A graceful shutdown waits for in-flight requests, and the panel's SSE streams
/// do not end by themselves -- so an unbounded wait would turn a stop into a
/// hang. EXIT_DRAIN is the whole budget, and the trace on disk already says why.
const EXIT_DRAIN: std::time::Duration = std::time::Duration::from_secs(3);

/// The two ways a stop reaches a console-less daemon, as ONE future.
///
/// Returns the future the server shuts down on, plus a gate that fires at the
/// same moment so the drain above can be bounded. The reason lands in the shared
/// slot BEFORE the future resolves, so the exit trace cannot race it.
fn shutdown_signal(
    root: PathBuf,
    why: std::sync::Arc<std::sync::Mutex<Option<String>>>,
) -> (
    impl std::future::Future<Output = ()>,
    std::sync::Arc<tokio::sync::Notify>,
) {
    let fired = std::sync::Arc::new(tokio::sync::Notify::new());
    let gate = std::sync::Arc::clone(&fired);
    let stop_request = exit_trace::stop_request_path(&root);
    let signal = async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            tokio::select! {
                // Console events: Ctrl-C, Ctrl-Break, and (through the same
                // handler) console close / logoff / shutdown. This is the path a
                // FOREGROUND daemon takes, and the one a machine shutdown leaves.
                _ = tokio::signal::ctrl_c() => {
                    *why.lock().expect("exit reason lock") = Some("signal:console".into());
                    break;
                }
                _ = tick.tick() => {
                    if stop_request.is_file() {
                        *why.lock().expect("exit reason lock") = Some("stop-request".into());
                        break;
                    }
                }
            }
        }
        fired.notify_one();
    };
    (signal, gate)
}

#[cfg(test)]
mod exit_trace_tests {
    use super::exit_trace::{self, Trace};

    fn unique_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ruagent-t20-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// The boot line, and the reason it is enough: a LIVE pid behind it means
    /// nothing is wrong yet.
    #[test]
    fn a_boot_record_says_running_and_names_this_process() {
        let root = unique_root("running");
        exit_trace::record_running(&root).unwrap();
        let raw = std::fs::read_to_string(exit_trace::path(&root)).unwrap();
        println!("READING t20 boot record: {}", raw.trim());
        let trace = exit_trace::read(&root).expect("a record");
        assert!(trace.is_running(), "{trace:?}");
        assert_eq!(trace.pid(), std::process::id());
        assert!(
            exit_trace::silent_death(&root).is_none(),
            "this pid IS alive, so this is not a death"
        );
        crate::test_cleanup::report_test_root_left_behind(&root);
    }

    /// THE WHOLE POINT (issue #64): a daemon that was KILLED could not write
    /// anything, so the trace it leaves is the boot line -- and a boot line whose
    /// pid is gone IS the diagnosis. Measured against a pid that really is gone
    /// (a child, reaped).
    #[test]
    fn a_dead_pid_behind_a_running_line_is_the_trace_of_a_kill() {
        #[cfg(windows)]
        let mut cmd = {
            let mut c = std::process::Command::new("cmd");
            c.args(["/c", "exit", "0"]);
            c
        };
        #[cfg(unix)]
        let mut cmd = std::process::Command::new("true");
        #[cfg(not(any(windows, unix)))]
        let mut cmd = std::process::Command::new("cmd");
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = cmd.spawn().expect("spawn a short-lived child");
        let dead = child.id();
        let _ = child.wait();
        let root = unique_root("killed");
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(
            exit_trace::path(&root),
            format!("running pid={dead} since=2026-10-17T00:00:00.000Z\n"),
        )
        .unwrap();
        let trace = exit_trace::silent_death(&root)
            .expect("a dead pid behind a running line IS the trace of a kill");
        println!("READING t20 silent death: {}", trace.describe());
        assert_eq!(trace.pid(), dead);
        crate::test_cleanup::report_test_root_left_behind(&root);
    }

    /// A clean exit replaces the boot line, so the two are distinguishable BY
    /// READING THE FILE -- which is the acceptance.
    #[test]
    fn a_clean_exit_reads_as_an_exit_not_as_a_silent_death() {
        let root = unique_root("clean");
        exit_trace::record_running(&root).unwrap();
        exit_trace::record_exit(&root, "stop-request", Some("the launcher asked")).unwrap();
        let raw = std::fs::read_to_string(exit_trace::path(&root)).unwrap();
        println!("READING t20 clean record: {}", raw.trim());
        match exit_trace::read(&root).expect("a record") {
            Trace::Exit {
                reason,
                detail,
                pid,
                ..
            } => {
                assert_eq!(reason, "stop-request");
                assert_eq!(detail.as_deref(), Some("the launcher asked"));
                assert_eq!(pid, std::process::id());
            }
            other => panic!("expected an Exit record, got {other:?}"),
        }
        assert!(
            exit_trace::silent_death(&root).is_none(),
            "an exit record is not a silent death"
        );
        crate::test_cleanup::report_test_root_left_behind(&root);
    }

    /// A REAL panic, through the REAL hook: the message and its location have to
    /// survive, because "it panicked" without "where" is the same dead end the
    /// 2026-10-17 disappearance was.
    #[test]
    fn the_panic_hook_writes_reason_panic_with_the_message_and_location() {
        let root = unique_root("panic");
        exit_trace::install_panic_hook(root.clone());
        let handle = std::thread::spawn(|| panic!("t20 deliberate panic"));
        assert!(handle.join().is_err(), "the thread did panic");
        let raw = std::fs::read_to_string(exit_trace::path(&root)).unwrap();
        println!("READING t20 panic record: {}", raw.trim());
        match exit_trace::read(&root).expect("a record") {
            Trace::Exit { reason, detail, .. } => {
                assert_eq!(reason, "panic");
                let d = detail.expect("the message survives");
                assert!(d.contains("t20 deliberate panic"), "{d}");
                assert!(d.contains("lib.rs"), "and its location: {d}");
            }
            other => panic!("expected a panic record, got {other:?}"),
        }
        crate::test_cleanup::report_test_root_left_behind(&root);
    }

    /// The graceful stop of a daemon that HAS no console: a request file in the
    /// root it owns. This is what the launcher's stop uses, and it is measured
    /// here without starting a process.
    #[tokio::test]
    async fn a_stop_request_is_how_a_console_less_daemon_is_asked_to_leave() {
        let root = unique_root("stop");
        let why: std::sync::Arc<std::sync::Mutex<Option<String>>> = Default::default();
        let (signal, _gate) = super::shutdown_signal(root.clone(), std::sync::Arc::clone(&why));
        let waiter = tokio::spawn(signal);
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert!(
            why.lock().unwrap().is_none(),
            "nothing has asked for a stop yet"
        );
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(exit_trace::stop_request_path(&root), "stop\n").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(4), waiter)
            .await
            .expect("the stop request is noticed")
            .unwrap();
        let reason = why.lock().unwrap().clone();
        println!("READING t20 stop request reason: {reason:?}");
        assert_eq!(reason.as_deref(), Some("stop-request"));
        crate::test_cleanup::report_test_root_left_behind(&root);
    }

    /// The file is read to EXPLAIN an unexplained death, so a torn or unknown line
    /// must come back as nothing instead of panicking in the reader.
    #[test]
    fn a_torn_or_unknown_line_parses_as_nothing() {
        assert!(exit_trace::parse("").is_none());
        assert!(exit_trace::parse("garbage").is_none());
        assert!(exit_trace::parse("running pid=notanumber since=x").is_none());
        assert!(exit_trace::parse("exit pid=1 at=x").is_none(), "no reason");
        assert!(exit_trace::parse("exit reason=panic pid=1 at=x").is_some());
        // A detail with spaces (a panic message) survives whole.
        match exit_trace::parse("exit reason=panic pid=1 at=x detail=boom happened here") {
            Some(trace) => assert_eq!(
                trace.describe(),
                "exit reason=panic pid=1 at=x detail=boom happened here"
            ),
            None => panic!("a record with a spaced detail must parse"),
        }
    }

    /// The detail is the rest of the line, so a newline in it must not be able to
    /// turn one record into two unparseable ones.
    #[test]
    fn a_multiline_detail_stays_one_record() {
        let root = unique_root("multiline");
        exit_trace::record_exit(&root, "panic", Some("first\nsecond\tthird")).unwrap();
        let raw = std::fs::read_to_string(exit_trace::path(&root)).unwrap();
        println!("READING t20 flattened record: {}", raw.trim());
        assert_eq!(raw.lines().count(), 1, "one line, whatever the detail held");
        match exit_trace::read(&root).expect("a record") {
            Trace::Exit { detail, .. } => {
                assert_eq!(detail.as_deref(), Some("first second third"))
            }
            other => panic!("expected an Exit record, got {other:?}"),
        }
        crate::test_cleanup::report_test_root_left_behind(&root);
    }
}

/// Test-root cleanup for this crate's unit tests (t31).
///
/// THE SHAPE is the one t27 fixed for the t16 tests and t30 spread to four more suites. It has
/// three parts and a suite needs all three:
///   1. a BOUNDED retry that RETURNS the reason, so a failure can be reported or asserted;
///   2. a REPORTING call site for a root whose holder is the test process itself (an in-process
///      daemon, a live store connection: the removal CANNOT succeed from inside the test);
///   3. a PRUNER that removes, from the NEXT process, what the previous one could not.
///
/// WHY IT LIVES HERE, AND WHERE THE COPIES ARE: an integration-test binary compiles against the
/// public API only, so a cfg(test) module is invisible to it. The daemon's own integration tests
/// (crates/daemon/tests/*.rs) carry the reporting half, and the knowledge crate's test binaries
/// share one in crates/knowledge/tests/common/mod.rs; this module is the ONE place the daemon's
/// unit tests use, so the crate has a single implementation of the rule.
#[cfg(test)]
pub(crate) mod test_cleanup {
    use std::path::Path;
    use std::time::Duration;

    /// Every temp-root prefix this crate's unit tests create, in ONE place so the pruner cannot
    /// be given a list that has drifted from the roots the suites write.
    pub(crate) const TEST_ROOT_PREFIXES: &[&str] = &[
        "ruagent-count-test-",
        "ruagent-del-test-",
        "ruagent-gen-test-",
        "ruagent-opts-test-",
        "ruagent-opts-seed-",
        "ruagent-t313-",
        "ruagent-t8-",
        "ruagent-t329-",
        "ruagent-t45-",
        "ruagent-t20-",
        "ruagent-t10-",
        "ruagent-t326-",
        "ruagent-cfg-",
        "ruagent-cfg-cap-",
        "ruagent-cfg-badcap-",
        "ruagent-cap-edit-",
        "ruagent-distill-edit-",
        "ruagent-reg-",
        "ruagent-skills-",
        "ruagent-internal",
        "ruagent-t16-reclaim-",
        "ruagent-t16-midturn-",
        "ruagent-t27-stuck-",
    ];

    /// Remove a root with a bounded retry, RETURNING the reason (t31).
    ///
    /// It returns instead of printing so a caller can assert on it, and it polls instead of
    /// sleeping once because what it waits for is another task dropping a handle.
    pub(crate) fn try_remove_test_root(root: &Path, budget: Duration) -> Result<(), String> {
        let started = std::time::Instant::now();
        loop {
            match std::fs::remove_dir_all(root) {
                Ok(()) => {
                    if started.elapsed() > Duration::from_millis(250) {
                        println!(
                            "READING t31 cleanup of {} waited {:.1} s for the handles to be released",
                            root.display(),
                            started.elapsed().as_secs_f64()
                        );
                    }
                    return Ok(());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => {
                    if started.elapsed() >= budget {
                        return Err(format!(
                            "{} could NOT be removed within {:.2} s: {e}",
                            root.display(),
                            budget.as_secs_f64()
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }
    }

    /// Remove a root test code OWNS, or PANIC naming it (t31): a cleanup that cannot happen is a
    /// red test, not a directory nobody counts.
    pub(crate) fn remove_test_root(root: &Path, what: &str, budget: Duration) {
        if let Err(why) = try_remove_test_root(root, budget) {
            panic!("{what}: {why}");
        }
    }

    /// Try once, then REPORT -- never silently drop -- a root that is still held (t31).
    ///
    /// WHY NOT AN ASSERTION HERE. The holder is the test process itself (the in-process daemon
    /// keeps a handle inside the root for the whole round), so the removal cannot succeed however
    /// long it waits; a panic would red a test for a fact that is not its own. The failure is
    /// made VISIBLE instead, and the pile is bounded by the pruner below -- which this call also
    /// guarantees has run, so a suite that only ever cleans up still prunes.
    pub(crate) fn report_test_root_left_behind(root: &Path) {
        prune_stale_test_roots(TEST_ROOT_PREFIXES, "daemon unit-test");
        if let Err(why) = try_remove_test_root(root, Duration::from_millis(500)) {
            eprintln!(
                "READING t31 cleanup: {why} -- the root is held by THIS test process, so the \
                 failure is REPORTED rather than swallowed; the next run's pruner removes it"
            );
        }
    }

    /// Is a process with this pid alive? The daemon's own primitive, start time included, so a
    /// RECYCLED pid is not mistaken for the process that made the root.
    fn pid_is_alive(pid: u32) -> bool {
        crate::orphans::process_started_at_ms(pid).is_some()
    }

    /// Remove the roots of DEAD test processes for the given prefixes, once per label.
    ///
    /// THE SAFETY RULE, and why it is deliberately conservative: a root belongs to a run whose
    /// process may still be read while it lives, so a root is pruned only when it has at least one
    /// numeric field AND none of its numeric fields belongs to a live process. Root names put the
    /// pid in different positions ("ruagent-cfg-<pid>", "ruagent-t329-<tag>-<pid>-<n>"), so the
    /// question is asked of every number in the name; a number that happens to be a live process
    /// keeps the root, which costs disk and never a run. Files are left alone: the mock-agent
    /// copies a suite writes are binaries, not directories.
    pub(crate) fn prune_stale_test_roots(prefixes: &[&str], label: &str) {
        static DONE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        {
            let mut done = DONE.lock().expect("prune labels");
            if done.iter().any(|l| l == label) {
                return;
            }
            done.push(label.to_string());
        }
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        let me = std::process::id();
        let (mut pruned, mut failed) = (0usize, 0usize);
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !prefixes.iter().any(|p| name.starts_with(p)) {
                continue;
            }
            let path = e.path();
            if !path.is_dir() || !all_numeric_fields_are_dead(&name, me) {
                continue;
            }
            match std::fs::remove_dir_all(&path) {
                Ok(()) => pruned += 1,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    failed += 1;
                    eprintln!(
                        "READING t31 stale root {} could NOT be removed: {err}",
                        path.display()
                    );
                }
            }
        }
        if pruned > 0 || failed > 0 {
            eprintln!(
                "READING t31 pruned {pruned} stale {label} root(s) of dead test processes; {failed} could not be removed"
            );
        }
    }

    /// True when every number in a root name is safe to prune on: no live process owns it, and it
    /// is not OUR pid. A name with no number at all is NOT prunable (nothing identifies its run).
    fn all_numeric_fields_are_dead(name: &str, me: u32) -> bool {
        all_numeric_fields_are_dead_with(name, me, pid_is_alive)
    }

    /// The rule with the liveness question passed IN, so the test can answer it
    /// deterministically instead of depending on which pids this machine happens to have
    /// live: Windows REUSES pids, and a reused one made the dead-pid case vacuous-red.
    fn all_numeric_fields_are_dead_with(name: &str, me: u32, alive: impl Fn(u32) -> bool) -> bool {
        let mut seen = false;
        for field in name.split(|c: char| !c.is_ascii_digit()) {
            if field.is_empty() {
                continue;
            }
            let Ok(n) = field.parse::<u32>() else {
                continue;
            };
            // A number outside this range is NOT evidence about a run: below 1000 it is
            // almost always part of a NAME (the 2 in ruagent-e2e-mock-bins, the 329 in a
            // tag), and above 100_000_000 a nanosecond stamp cannot be a pid. A name with no
            // pid in it is never pruned, however many of these numbers it carries.
            if !(1000..=100_000_000).contains(&n) {
                continue;
            }
            seen = true;
            if n == me || alive(n) {
                return false;
            }
        }
        seen
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// t31: the shape's OWN proof, so a cleanup that cannot happen cannot go quiet again.
        ///
        /// Two halves, both asserted: the bounded retry RETURNS a reason that NAMES the path, and
        /// the asserting wrapper PANICS on it. The root is released and removed at the end -- a
        /// test written about the leftover pile must not join it.
        #[test]
        fn a_failed_cleanup_is_reported_and_the_asserting_half_panics() {
            let root = std::env::temp_dir().join(format!(
                "ruagent-cfg-{}-t31proof-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            // Hold the directory's database open, exactly as a live test does.
            //
            // TEMPORARY t31 NEGATIVE CONTROL (env-gated, so the tree is never red by default;
            // removed as soon as the red reading is taken). With the flag set the root is NOT
            // held, the removal therefore SUCCEEDS, and this test MUST fail -- which is what
            // proves the assertion below is about the failure and not vacuous.
            let hold = std::env::var_os("RUAGENT_T31_CONTROL_NO_HOLD")
                .is_none()
                .then(|| ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap());
            // An open file does NOT block rmdir on unix, so the failure is made deterministic
            // there the only other way it can be: a read-only directory. This machine is Windows,
            // so the unix branch is reasoned rather than measured here.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&root).unwrap().permissions();
                perms.set_mode(0o555);
                std::fs::set_permissions(&root, perms).unwrap();
            }

            // 1. The failure is RETURNED, and it NAMES the root it could not remove.
            let why = try_remove_test_root(&root, Duration::from_millis(100))
                .expect_err("a root holding an open database must NOT come back as removed");
            println!("READING t31 deliberate cleanup failure: {why}");
            assert!(
                why.contains(&root.display().to_string()),
                "the report must name the root it could not remove: {why}"
            );

            // 2. The call a test makes for a root it OWNS PANICS on it.
            let panicked = std::panic::catch_unwind(|| {
                remove_test_root(&root, "the t31 proof root", Duration::from_millis(100))
            })
            .is_err();
            assert!(
                panicked,
                "remove_test_root must PANIC on a root it could not remove, or the leftover pile \
                 is back in the dark"
            );

            // Release the handle and the unix permission trick, then remove it for real.
            drop(hold);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&root).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&root, perms).unwrap();
            }
            remove_test_root(
                &root,
                "the t31 proof root, after releasing its handle",
                Duration::from_millis(500),
            );
            println!("READING t31 after releasing the handle: the proof root was removed");
        }

        /// The pruner's rule, unit-tested without touching %TEMP%: a name is prunable only when
        /// every number in it is dead, and a name with no number is never prunable.
        #[test]
        fn only_names_whose_numbers_are_all_dead_are_prunable() {
            let me = std::process::id();
            let never = |_pid: u32| false;
            let pid_4242 = |pid: u32| pid == 4242;
            // NO pid in the name: nothing identifies its run, so it is never pruned -- even
            // though it carries digits. MEASURED REGRESSION: the first version of this rule
            // treated ANY number as a pid candidate, so the 2 of "e2e" made the mock-agent
            // copies directory prunable, and the pruner deleted a live suite's agent binaries.
            assert!(!all_numeric_fields_are_dead_with(
                "ruagent-e2e-mock-bins",
                me,
                never
            ));
            assert!(!all_numeric_fields_are_dead_with(
                "ruagent-knowledge-probe",
                me,
                never
            ));
            // A number too large to be a pid is not evidence either way.
            assert!(!all_numeric_fields_are_dead_with(
                "ruagent-cfg-4294967280",
                me,
                never
            ));
            // A plausible pid that is GONE: prunable. This is the reading the pruner acts on.
            assert!(all_numeric_fields_are_dead_with(
                "ruagent-cfg-4242",
                me,
                never
            ));
            // ... and the same name with that pid ALIVE is kept, in whatever position the pid
            // sits, which is what makes the rule safe.
            assert!(!all_numeric_fields_are_dead_with(
                "ruagent-cfg-4242",
                me,
                pid_4242
            ));
            assert!(!all_numeric_fields_are_dead_with(
                "ruagent-t329-phrasings-4242-3",
                me,
                pid_4242
            ));
            // Our OWN pid is live by definition, so a root of this run is never pruned.
            assert!(!all_numeric_fields_are_dead_with(
                &format!("ruagent-cfg-{me}"),
                me,
                never
            ));
        }
    }
}
