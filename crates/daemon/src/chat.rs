//! ChatManager: terminal-like persistent conversations over ACP. One
//! spawned agent process per chat, multi-turn prompts on one session,
//! model switching (live via `set_config_option` when the agent supports
//! it, session restart otherwise).
//!
//! Three cross-cutting records:
//! - **Chat history** (`chats` table): every chat ever started, with its
//!   agent identity (survives runtime switches), engine, model and title
//!   (first prompt) — the panel's history drawer reads it and joins the
//!   sessions index for message counts.
//! - **Option catalog cache** (`agent_options` table): the model list /
//!   permission modes / thinking levels each runtime advertises over ACP,
//!   persisted per RUNTIME name. Loaded at boot so the pickers are
//!   instant; refreshed by every live chat, by a background loop
//!   (boot + every few hours) and by the manual sync button.
//! - **Role defaults** (`[agent.X.options]`): canonical option values a
//!   role pins (mode / effort), applied on the runtime's advertised
//!   option ids at chat start.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use ruagent_acp::adapter::HarnessAdapter as _;
use ruagent_acp::chat::{
    ChatCommand, ChatOptions, ChatSession, SessionOptionState, find_canonical, start_chat,
};
use ruagent_core::{AgentCard, RunEvent, RunId};
use ruagent_store::{TranscriptWriter, transcript_path};
use tokio::sync::mpsc;

/// One live chat.
#[derive(Clone)]
pub struct Chat {
    pub id: RunId,
    /// The card the user picked (role or runtime name) — survives
    /// runtime switches, so history stays attributed to the role.
    pub agent: String,
    /// The `[runtime.X]` engine this chat currently runs on.
    pub runtime: String,
    pub model: Option<String>,
    pub created_at: String,
    session: ChatSession,
    last_active: Arc<Mutex<Instant>>,
    /// Whether the memory context already went out with a prompt (once
    /// per chat — the first message is the natural recall query).
    memory_injected: Arc<std::sync::atomic::AtomicBool>,
    /// The agent's portable role prompt (two-layer model): injected
    /// ahead of the first prompt on every runtime.
    agent_prompt: Option<String>,
}

impl Chat {
    pub fn send(&self, cmd: ChatCommand) -> Result<()> {
        *self.last_active.lock().expect("chat last_active lock") = Instant::now();
        self.session.send(cmd).context("sending chat command")
    }

    /// Send a user prompt; the first one carries the memory context and
    /// becomes the chat's history title.
    pub async fn send_prompt(
        &self,
        db: &ruagent_store::Db,
        embedder: std::sync::Arc<dyn ruagent_knowledge::embed::Embedder>,
        text: String,
    ) -> Result<()> {
        let context = if self
            .memory_injected
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            None
        } else {
            let mut ctx = ChatManager::memory_context_for(db, &text).await;
            // The role prompt rides first — what this agent IS.
            if let Some(role) = &self.agent_prompt {
                let role_block = format!(
                    "[role — you are]
{role}"
                );
                ctx = Some(match ctx {
                    Some(c) => format!(
                        "{role_block}

{c}"
                    ),
                    None => role_block,
                });
            }
            // semantic leg: the first message is the query
            let hits = crate::memembed::semantic_search(db, embedder, &text, 4, 0.34).await;
            if !hits.is_empty() {
                let sem = hits
                    .iter()
                    .map(|(_, _, _, c, _)| format!("- {}", truncate_chars(c, 180)))
                    .collect::<Vec<_>>()
                    .join(
                        "
",
                    );
                match &mut ctx {
                    Some(c) => {
                        c.push_str(
                            "
[relevant memories for this conversation]
",
                        );
                        c.push_str(&sem);
                    }
                    None => {
                        ctx = Some(format!(
                            "[memory context — what the platform remembers]
[relevant memories for this conversation]
{sem}"
                        ))
                    }
                }
            }
            ctx
        };
        // History: the first prompt becomes the title, every prompt
        // refreshes updated_at (chat ordering in the history drawer).
        {
            let id = self.id.to_string();
            let title = truncate_chars(&text, 80);
            let now = Utc::now().timestamp_millis();
            let _ = db
                .call(move |conn| {
                    conn.execute(
                        "UPDATE chats SET title = COALESCE(title, ?1), updated_at = ?2
                          WHERE id = ?3",
                        rusqlite::params![title, now, id],
                    )
                })
                .await;
        }
        self.send(ChatCommand::Prompt { text, context })
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RunEvent> {
        self.session.subscribe()
    }

    pub fn options_now(&self) -> Option<Vec<SessionOptionState>> {
        self.session.options_now()
    }

    pub fn options_watch(&self) -> tokio::sync::watch::Receiver<Option<Vec<SessionOptionState>>> {
        self.session.options_watch()
    }
}

/// The chat registry + idle reaper.
type AskParker = Arc<dyn Fn(ruagent_acp::permission::PermissionAsk, RunId) + Send + Sync>;
type AskDropper = Arc<dyn Fn(RunId) + Send + Sync>;

/// One cached option catalog (what a runtime advertises) plus when the
/// persisted copy was written — the panel shows the sync time.
#[derive(Debug, Clone)]
pub struct CachedOptions {
    pub options: Vec<SessionOptionState>,
    pub updated_at: i64,
}

/// Result of one `Db::call` round-trip (the channel result wrapping the
/// closure's own Result).
type DbCall<T> = Result<Result<T, ruagent_store::DbError>, ruagent_store::DbError>;

/// One raw `chats`-table row, column order.
type ChatRow = (
    String,         // id
    String,         // agent
    Option<String>, // runtime
    Option<String>, // model
    Option<String>, // title
    i64,            // created_at
    i64,            // updated_at
);

/// One sessions-index join row: (key, message_count, preview).
type SessionIndexRow = (String, u32, Option<String>);

/// A chats-table row joined with the sessions index (viewer route).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatHistoryEntry {
    pub id: String,
    pub agent: String,
    pub runtime: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// A live chat the daemon is still holding (history can reopen the
    /// transcript; a live one also streams).
    pub active: bool,
    pub message_count: Option<u32>,
    pub preview: Option<String>,
    /// Sessions-index key of the transcript (the shared viewer route).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

pub struct ChatManager {
    db: ruagent_store::Db,
    root: PathBuf,
    chats: Arc<Mutex<HashMap<RunId, Chat>>>,
    park_ask: AskParker,
    /// Drops parked asks when a chat closes — wired to
    /// `RunManager::drop_pending_for` once both exist (chats are built
    /// before the RunManager). Optional so tests can omit it.
    drop_asks: Mutex<Option<AskDropper>>,
    mcp: crate::config::McpConfig,
    /// Session → memory distillation policy (auto on close).
    pub distill_policy: crate::distill::AutoDistill,
    /// Agent registry view for the distiller.
    pub registry: crate::distill::AgentRegistry,
    /// Shared embedder for distillation writes (set at boot).
    pub embedder: Option<std::sync::Arc<dyn ruagent_knowledge::embed::Embedder>>,
    /// Advertised session options (model, reasoning effort, permission
    /// mode, …) per RUNTIME name, refreshed whenever any chat or probe
    /// reports them. Seeded from the `agent_options` table at boot —
    /// what the panel pickers show, with no probe spawn on cold start.
    model_cache: Arc<Mutex<HashMap<String, CachedOptions>>>,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How long to wait for a spawned agent to report its model catalog.
const MODELS_WAIT: Duration = Duration::from_secs(20);
/// Background option-catalog refresh cadence.
pub const OPTIONS_REFRESH: Duration = Duration::from_secs(6 * 3600);

impl ChatManager {
    pub fn new(
        db: ruagent_store::Db,
        root: PathBuf,
        park_ask: AskParker,
        mcp: crate::config::McpConfig,
        distill_policy: crate::distill::AutoDistill,
        embedder: Option<std::sync::Arc<dyn ruagent_knowledge::embed::Embedder>>,
        registry: crate::distill::AgentRegistry,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            db,
            root,
            chats: Arc::new(Mutex::new(HashMap::new())),
            park_ask,
            drop_asks: Mutex::new(None),
            mcp,
            distill_policy,
            registry,
            embedder,
            model_cache: Arc::new(Mutex::new(HashMap::new())),
        });
        this.spawn_idle_reaper();
        this
    }

    /// The runtime a card runs on: roles name theirs, runtime cards are
    /// their own engine.
    pub fn runtime_of(card: &AgentCard) -> String {
        card.runtime
            .clone()
            .or_else(|| card.runtimes.first().cloned())
            .unwrap_or_else(|| card.name.clone())
    }

    pub fn chat(&self, id: RunId) -> Option<Chat> {
        self.chats.lock().expect("chats lock").get(&id).cloned()
    }

    pub fn list(&self) -> Vec<serde_json::Value> {
        self.chats
            .lock()
            .expect("chats lock")
            .values()
            .map(|c| {
                serde_json::json!({
                    "id": c.id.to_string(),
                    "agent": c.agent,
                    "runtime": c.runtime,
                    "model": c.model,
                    "created_at": c.created_at,
                })
            })
            .collect()
    }

    /// The cross-session memory digest (L3 of the unified memory layer):
    /// compact profile + durable observations, capped hard. Injected
    /// ahead of a chat's first prompt so a fresh session "remembers"
    /// the user without any recall call.
    /// The cross-session memory context for a chat's FIRST prompt:
    /// (a) a stable, small profile block (who the user is — always
    /// relevant). The semantic leg (recall on the first message) is
    /// attached by `Chat::send_prompt`.
    pub async fn memory_context_for(db: &ruagent_store::Db, _query: &str) -> Option<String> {
        let rows: Vec<(String, String, String)> = db
            .call(|conn| -> Result<_, ruagent_store::DbError> {
                let mut stmt = conn
                    .prepare(
                        "SELECT store, namespace, content FROM memories
                          WHERE superseded_at IS NULL
                            AND ((store = 'profile' AND namespace = 'user')
                              OR (store = 'observation' AND namespace IN ('user','global'))
                              OR (store IN ('procedure','lesson') AND namespace = 'global'))
                          ORDER BY updated_at DESC LIMIT 12",
                    )
                    .map_err(ruagent_store::DbError::from)?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map_err(ruagent_store::DbError::from)?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .ok()?
            .ok()?;
        if rows.is_empty() {
            return None;
        }
        let mut out = String::from(
            "[memory context — what the platform remembers about you and your work]
",
        );
        let mut budget = 700usize;
        for (store, ns, content) in &rows {
            let content = content.trim();
            let cut = content
                .char_indices()
                .nth(160)
                .map_or(content.len(), |(i, _)| i);
            let line = if cut < content.len() {
                format!("{}…", &content[..cut])
            } else {
                content.to_string()
            };
            let entry = format!("- ({store}/{ns}) {line}");
            if budget < entry.len() {
                break;
            }
            budget -= entry.len();
            out.push_str(&entry);
            out.push('\n');
        }
        Some(out)
    }

    /// Start a new chat on the given agent (optionally with a model).
    /// Workspace = a fresh dir under workspaces/chat-<id>. Records a
    /// chats-table history row.
    pub async fn start(&self, card: &AgentCard, model: Option<String>) -> Result<Chat> {
        self.start_inner(card, model, &card.name, true).await
    }

    /// The spawn path shared by chats and probes. `label` is the history
    /// identity (the role the user picked); `record` decides whether a
    /// chats row is written (probes must not pollute history).
    async fn start_inner(
        &self,
        card: &AgentCard,
        model: Option<String>,
        label: &str,
        record: bool,
    ) -> Result<Chat> {
        let spec = ruagent_acp::adapter_for(card.harness)
            .spawn_spec(card)
            .with_context(|| format!("resolving spawn command for `{}`", card.name))?;
        let id = RunId::generate();
        let workspace = self.root.join("workspaces").join(format!("chat-{id}"));
        std::fs::create_dir_all(&workspace)
            .with_context(|| format!("creating workspace {}", workspace.display()))?;

        let mcp = self
            .mcp
            .expand_profile(card.mcp_profile.as_deref(), &card.name);

        // Per-chat permission channel: each ask is parked in the shared
        // inbox keyed by this chat's id (rules may auto-answer; otherwise
        // it reaches the human — same inbox the panel serves).
        let (ask_tx, mut ask_rx) =
            mpsc::unbounded_channel::<ruagent_acp::permission::PermissionAsk>();
        {
            let park = self.park_ask.clone();
            tokio::spawn(async move {
                while let Some(ask) = ask_rx.recv().await {
                    park(ask, id);
                }
            });
        }

        let session = start_chat(
            ChatOptions {
                program: spec.program,
                args: spec.args,
                cwd: workspace,
                mcp_servers: mcp,
                model: model.clone(),
            },
            ask_tx,
        )
        .context("starting chat session")?;

        // Transcript forwarder: every chat event lands in the chat's JSONL
        // (SSE replays from it) — matches the run transcript pattern.
        // Probes (record=false) write nothing: an empty transcript file
        // would make doctor's "has CLI histories" probe think this
        // machine has session history to sync.
        if record {
            let mut sub = session.subscribe();
            let path = self.transcript_path(id);
            tokio::spawn(async move {
                let mut writer = match TranscriptWriter::create(&path) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::warn!(chat = %id, error = %e, "chat transcript open failed");
                        return;
                    }
                };
                loop {
                    match sub.recv().await {
                        Ok(event) => {
                            let _ = writer.append(&event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(chat = %id, skipped = n, "chat forwarder lagged");
                        }
                        Err(_) => {
                            // Sender dropped: chat closed.
                            let _ = writer.flush();
                            return;
                        }
                    }
                }
            });
        }

        let runtime = Self::runtime_of(card);

        // Option-state tracker: whenever this session's advertised options
        // or current selections change, refresh the per-runtime catalog
        // (memory + `agent_options` table) the panel pickers read.
        {
            let mut watch = session.options_watch();
            let cache = self.model_cache.clone();
            let db = self.db.clone();
            let runtime_key = runtime.clone();
            let mut last: Option<Vec<SessionOptionState>> = None;
            tokio::spawn(async move {
                loop {
                    if let Some(state) = watch.borrow_and_update().clone()
                        && last.as_ref() != Some(&state)
                    {
                        last = Some(state.clone());
                        let updated_at = Utc::now().timestamp_millis();
                        cache.lock().expect("model cache lock").insert(
                            runtime_key.clone(),
                            CachedOptions {
                                options: state.clone(),
                                updated_at,
                            },
                        );
                        persist_options(&db, &runtime_key, &state, updated_at);
                    }
                    if watch.changed().await.is_err() {
                        return; // chat closed
                    }
                }
            });
        }

        let chat = Chat {
            id,
            agent: label.to_string(),
            runtime,
            model,
            created_at: Utc::now().to_rfc3339(),
            session,
            last_active: Arc::new(Mutex::new(Instant::now())),
            memory_injected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            agent_prompt: card.prompt.clone(),
        };

        // History row: who (agent identity) on which engine.
        if record {
            let id_s = id.to_string();
            let agent_s = chat.agent.clone();
            let runtime_s = chat.runtime.clone();
            let model_s = chat.model.clone();
            let now = Utc::now().timestamp_millis();
            let db = self.db.clone();
            let _ = db
                .call(move |conn| {
                    conn.execute(
                        "INSERT OR REPLACE INTO chats
                             (id, agent, runtime, model, title, created_at, updated_at)
                         VALUES (?1,?2,?3,?4,NULL,?5,?5)",
                        rusqlite::params![id_s, agent_s, runtime_s, model_s, now],
                    )
                })
                .await;
        }

        // Role defaults (`[agent.X.options]`): applied when the runtime
        // reports its options — canonical keys are mapped onto whatever
        // option ids this engine advertises.
        if !card.options.is_empty() {
            let chat = chat.clone();
            let defaults = card.options.clone();
            tokio::spawn(async move {
                apply_role_defaults(&chat, &defaults).await;
            });
        }

        self.chats
            .lock()
            .expect("chats lock")
            .insert(id, chat.clone());
        Ok(chat)
    }

    /// Close and remove a chat. When auto-distill is on, the session
    /// becomes memories + graph entries in the background.
    /// Wire the ask-dropper (RunManager::drop_pending_for). Called once
    /// at boot, after both managers exist.
    pub fn set_ask_dropper(&self, f: AskDropper) {
        *self.drop_asks.lock().expect("drop_asks lock") = Some(f);
    }

    /// Close one chat: shut the session down, drop its parked asks
    /// (fail-closed on the agent side, out of the inbox — issue #40),
    /// then maybe auto-distill.
    pub fn close(&self, id: RunId) -> bool {
        let chat = self.chats.lock().expect("chats lock").remove(&id);
        match chat {
            Some(c) => {
                let _ = c.send(ChatCommand::Shutdown);
                if let Some(drop) = self.drop_asks.lock().expect("drop_asks lock").as_ref() {
                    drop(id);
                }
                self.maybe_auto_distill(id);
                true
            }
            None => false,
        }
    }

    /// Spawn a background distillation for a closed chat (policy-gated).
    fn maybe_auto_distill(&self, id: RunId) {
        let Some(distiller) = self.auto_distiller() else {
            return;
        };
        let key = crate::sessions::session_key_of(&self.transcript_path(id));
        let agent = self.distill_policy.agent.clone();
        tokio::spawn(async move {
            match crate::distill::distill_with_agent(&distiller, &key, agent.as_deref()).await {
                Ok(o) => tracing::info!(
                    session = %key,
                    memories = o.memories_written,
                    entities = o.entities_written,
                    "auto-distilled"
                ),
                Err(e) => tracing::warn!(session = %key, error = %e, "auto-distill failed"),
            }
        });
    }

    /// Switch model. Prefers a live switch (`session/set_config_option`
    /// — context preserved); falls back to restarting the session on the
    /// given engine card, keeping `label` as the history identity (the
    /// role the user picked). Returns the chat plus whether a restart
    /// happened.
    pub async fn switch_model(
        &self,
        id: RunId,
        model: Option<String>,
        card: &AgentCard,
        label: &str,
    ) -> Result<(Chat, bool)> {
        let chat = self
            .chat(id)
            .ok_or_else(|| anyhow::anyhow!("chat not found"))?;
        if let Some(model) = model.clone().filter(|m| !m.trim().is_empty()) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            chat.send(ChatCommand::SetConfig {
                id: "model".into(),
                value: model.clone(),
                reply: tx,
            })
            .ok();
            let answer = tokio::time::timeout(MODELS_WAIT, rx).await;
            match answer {
                Ok(Ok(Ok(options))) => {
                    // Live switch: update the registry entry in place.
                    let effective = options
                        .iter()
                        .find(|o| o.category.as_deref() == Some("model") || o.id == "model")
                        .and_then(|o| o.current.clone())
                        .or_else(|| Some(model.clone()));
                    let mut chats = self.chats.lock().expect("chats lock");
                    if let Some(stored) = chats.get_mut(&id) {
                        stored.model = effective.clone();
                    }
                    drop(chats);
                    let mut updated = chat;
                    updated.model = effective;
                    return Ok((updated, false));
                }
                Ok(Ok(Err(reason))) => {
                    tracing::info!(chat = %id, model = %model, %reason,
                        "live model switch rejected, restarting session");
                }
                // Reply dropped or timed out: fall through to restart.
                _ => {}
            }
        }
        self.close(id);
        let chat = self.start_inner(card, model, label, true).await?;
        Ok((chat, true))
    }

    /// Set one advertised non-model session option live (reasoning
    /// effort, permission mode, …). No restart fallback: the agent either
    /// accepts the value or the error is surfaced.
    pub async fn set_option(
        &self,
        id: RunId,
        option_id: String,
        value: String,
    ) -> Result<Vec<SessionOptionState>, String> {
        let chat = self.chat(id).ok_or_else(|| "chat not found".to_string())?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        chat.send(ChatCommand::SetConfig {
            id: option_id,
            value,
            reply: tx,
        })
        .map_err(|e| format!("{e}"))?;
        match tokio::time::timeout(MODELS_WAIT, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err("chat closed".into()),
            Err(_) => Err("agent did not answer in time".into()),
        }
    }

    /// The advertised session options for a card's runtime: memory cache
    /// (seeded from the DB at boot), a live chat, or a throwaway probe
    /// session (spawned, queried, closed — never recorded in history).
    /// Empty options mean the runtime advertises none (free-text model
    /// input). Returns whether the answer came from cache.
    pub async fn agent_options(&self, card: &AgentCard) -> Result<(CachedOptions, bool)> {
        let runtime = Self::runtime_of(card);
        // Cache first — seeded from `agent_options` at daemon boot and
        // refreshed by every chat/probe since.
        if let Some(entry) = self
            .model_cache
            .lock()
            .expect("model cache lock")
            .get(&runtime)
        {
            return Ok((entry.clone(), true));
        }
        // Nothing cached: probe (a live chat on this runtime is already
        // asking the agent — wait for its report instead of spawning).
        let entry = self.probe_options(card, &runtime).await?;
        Ok((entry, false))
    }

    /// Force a fresh read bypassing every cache (the manual sync
    /// button): live chat when one exists, else a throwaway probe.
    /// Persists the result.
    pub async fn refresh_agent_options(&self, card: &AgentCard) -> Result<CachedOptions> {
        let runtime = Self::runtime_of(card);
        let entry = self.probe_options(card, &runtime).await?;
        Ok(entry)
    }

    /// Read a runtime's advertised options from a live chat when one
    /// exists, else spawn a throwaway probe session. Updates the cache
    /// and the `agent_options` table.
    async fn probe_options(&self, card: &AgentCard, runtime: &str) -> Result<CachedOptions> {
        let live = {
            let chats = self.chats.lock().expect("chats lock");
            chats.values().find(|c| c.runtime == runtime).cloned()
        };
        let state = match live {
            Some(c) => wait_options(&c).await,
            None => {
                // Probe: start (unrecorded), read the options, close.
                let chat = self.start_inner(card, None, &card.name, false).await?;
                let state = wait_options(&chat).await;
                self.close(chat.id);
                state
            }
        };
        let updated_at = Utc::now().timestamp_millis();
        let entry = CachedOptions {
            options: state.clone(),
            updated_at,
        };
        self.model_cache
            .lock()
            .expect("model cache lock")
            .insert(runtime.to_string(), entry.clone());
        persist_options(&self.db, runtime, &state, updated_at);
        Ok(entry)
    }

    /// Seed the option cache from the `agent_options` table at boot —
    /// after a daemon restart the pickers are instant (no probe spawn).
    pub async fn load_option_cache(&self) {
        let rows: DbCall<Vec<(String, String, i64)>> = self
            .db
            .call(|conn| {
                let mut stmt = conn
                    .prepare("SELECT runtime, options, updated_at FROM agent_options")
                    .map_err(ruagent_store::DbError::from)?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    })
                    .map_err(ruagent_store::DbError::from)?;
                Ok(rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
            })
            .await;
        let Ok(Ok(rows)) = rows else {
            return;
        };
        let mut n = 0usize;
        let mut cache = self.model_cache.lock().expect("model cache lock");
        for (runtime, json, updated_at) in rows {
            if let Ok(options) = serde_json::from_str::<Vec<SessionOptionState>>(&json) {
                // Don't overwrite entries a live chat already refreshed.
                cache.entry(runtime).or_insert(CachedOptions {
                    options,
                    updated_at,
                });
                n += 1;
            }
        }
        if n > 0 {
            tracing::info!(runtimes = n, "option catalogs loaded from db");
        }
    }

    /// Re-probe every enabled runtime sequentially (boot + periodic
    /// loop). Roles are skipped — they share their runtime's catalog.
    pub async fn refresh_all_options(&self, cards: &[AgentCard]) {
        for card in cards
            .iter()
            .filter(|c| c.enabled && c.prompt.is_none() && c.runtime.is_none())
        {
            match self.refresh_agent_options(card).await {
                Ok(entry) => tracing::info!(
                    runtime = %card.name,
                    options = entry.options.len(),
                    "option catalog refreshed"
                ),
                Err(e) => {
                    tracing::warn!(runtime = %card.name, error = %e, "option catalog refresh failed")
                }
            }
        }
    }

    /// Chat history from the `chats` table, newest first, joined with
    /// the sessions index (message count / preview) and live state.
    pub async fn history(&self, agent: Option<&str>, limit: u32) -> Vec<ChatHistoryEntry> {
        let filter = agent.map(|a| a.to_string());
        let rows: DbCall<Vec<ChatRow>> = self
            .db
            .call(move |conn| {
                let (sql, params): (&str, Vec<&dyn rusqlite::ToSql>) = match &filter {
                    Some(a) => (
                        "SELECT id, agent, runtime, model, title, created_at, updated_at
                           FROM chats WHERE agent = ?1 ORDER BY updated_at DESC LIMIT ?2",
                        vec![a, &limit],
                    ),
                    None => (
                        "SELECT id, agent, runtime, model, title, created_at, updated_at
                           FROM chats ORDER BY updated_at DESC LIMIT ?1",
                        vec![&limit],
                    ),
                };
                let mut stmt = conn.prepare(sql).map_err(ruagent_store::DbError::from)?;
                let rows = stmt
                    .query_map(params.as_slice(), |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, i64>(5)?,
                            r.get::<_, i64>(6)?,
                        ))
                    })
                    .map_err(ruagent_store::DbError::from)?;
                Ok(rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
            })
            .await;
        let Ok(Ok(rows)) = rows else {
            return Vec::new();
        };
        if rows.is_empty() {
            return Vec::new();
        }

        // Session keys (viewer route) + sessions-index enrichment.
        let mut entries: Vec<ChatHistoryEntry> = rows
            .into_iter()
            .map(
                |(id, agent, runtime, model, title, created_at, updated_at)| {
                    let session_key = id
                        .parse::<RunId>()
                        .ok()
                        .map(|rid| crate::sessions::session_key_of(&self.transcript_path(rid)));
                    ChatHistoryEntry {
                        id,
                        agent,
                        runtime,
                        model,
                        title,
                        created_at,
                        updated_at,
                        active: false,
                        message_count: None,
                        preview: None,
                        session_key,
                    }
                },
            )
            .collect();

        let keys: Vec<String> = entries
            .iter()
            .filter_map(|e| e.session_key.clone())
            .collect();
        let index: HashMap<String, (u32, Option<String>)> = if keys.is_empty() {
            HashMap::new()
        } else {
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT key, message_count, preview FROM sessions WHERE key IN ({placeholders})"
            );
            let res: DbCall<Vec<SessionIndexRow>> = self
                .db
                .call(move |conn| {
                    let mut stmt = conn.prepare(&sql).map_err(ruagent_store::DbError::from)?;
                    let rows = stmt
                        .query_map(rusqlite::params_from_iter(keys.iter()), |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, u32>(1)?,
                                r.get::<_, Option<String>>(2)?,
                            ))
                        })
                        .map_err(ruagent_store::DbError::from)?;
                    Ok(rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                })
                .await;
            res.ok()
                .and_then(|r| r.ok())
                .map(|rows| rows.into_iter().map(|(k, n, p)| (k, (n, p))).collect())
                .unwrap_or_default()
        };

        let live: Vec<String> = {
            let chats = self.chats.lock().expect("chats lock");
            chats.keys().map(|id| id.to_string()).collect()
        };
        for e in &mut entries {
            if let Some(key) = &e.session_key
                && let Some((count, preview)) = index.get(key)
            {
                e.message_count = Some(*count);
                e.preview.clone_from(preview);
            }
            e.active = live.contains(&e.id);
        }
        entries
    }

    /// Chat transcript paths keyed by agent identity (sessions-view
    /// enrichment: "which role was this ruagent conversation with?").
    pub async fn session_keys_by_agent(&self, limit: u32) -> HashMap<String, String> {
        self.history(None, limit)
            .await
            .into_iter()
            .filter_map(|e| e.session_key.map(|k| (k, e.agent)))
            .collect()
    }

    fn auto_distiller(&self) -> Option<crate::distill::Distiller> {
        if !self.distill_policy.auto {
            return None;
        }
        Some(crate::distill::Distiller {
            db: self.db.clone(),
            root: self.root.clone(),
            embedder: self.embedder.clone(),
            registry: self.registry.clone(),
            language: self.distill_policy.language.clone(),
            prompt_override: self.distill_policy.prompt.clone(),
        })
    }

    /// Chat transcripts live next to run transcripts.
    pub fn transcript_path(&self, id: RunId) -> PathBuf {
        transcript_path(self.root.join("data").join("transcripts"), &id)
    }

    pub fn db(&self) -> &ruagent_store::Db {
        &self.db
    }

    fn spawn_idle_reaper(self: &Arc<Self>) {
        let chats = self.chats.clone();
        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let mut to_close = Vec::new();
                {
                    let map = chats.lock().expect("chats lock");
                    for (id, c) in map.iter() {
                        let idle = c.last_active.lock().expect("last_active").elapsed();
                        if idle > IDLE_TIMEOUT {
                            tracing::info!(chat = %id, "chat idle, closing");
                            to_close.push(*id);
                        }
                    }
                }
                for id in to_close {
                    // Same path as explicit close: shutdown, drop parked
                    // asks, maybe distill.
                    mgr.close(id);
                }
            }
        });
    }
}

/// Wait for a session to report its advertised options (bounded).
async fn wait_options(chat: &Chat) -> Vec<SessionOptionState> {
    let mut watch = chat.options_watch();
    if watch.borrow().is_none() {
        let _ = tokio::time::timeout(MODELS_WAIT, watch.changed()).await;
    }
    watch.borrow_and_update().clone().unwrap_or_default()
}

/// Apply a role's canonical option defaults onto a live chat: wait for
/// the runtime's advertised options, map canonical keys (`mode`,
/// `effort`) onto the option ids this engine uses, set each.
async fn apply_role_defaults(chat: &Chat, defaults: &std::collections::BTreeMap<String, String>) {
    let mut watch = chat.options_watch();
    if watch.borrow().is_none()
        && tokio::time::timeout(MODELS_WAIT, watch.changed())
            .await
            .is_err()
    {
        tracing::warn!(chat = %chat.id, "role defaults: runtime reported no options");
        return;
    }
    let Some(options) = watch.borrow_and_update().clone() else {
        return;
    };
    for (canonical, value) in defaults {
        let Some(opt) = find_canonical(&options, canonical) else {
            tracing::debug!(
                chat = %chat.id,
                canonical,
                "runtime has no option for this role default; skipped"
            );
            continue;
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        if chat
            .send(ChatCommand::SetConfig {
                id: opt.id.clone(),
                value: value.clone(),
                reply: tx,
            })
            .is_err()
        {
            return; // chat closed
        }
        match tokio::time::timeout(MODELS_WAIT, rx).await {
            Ok(Ok(Ok(_))) => tracing::info!(
                chat = %chat.id,
                option = %opt.id,
                value,
                "role default applied"
            ),
            Ok(Ok(Err(reason))) => {
                tracing::warn!(chat = %chat.id, option = %opt.id, value, reason,
                    "role default rejected by runtime")
            }
            _ => tracing::warn!(chat = %chat.id, option = %opt.id,
                "role default: runtime did not answer"),
        }
    }
}

/// Persist one runtime's option catalog (fire-and-forget upsert).
fn persist_options(
    db: &ruagent_store::Db,
    runtime: &str,
    options: &[SessionOptionState],
    updated_at: i64,
) {
    let Ok(json) = serde_json::to_string(options) else {
        return;
    };
    let runtime = runtime.to_string();
    let db = db.clone();
    tokio::spawn(async move {
        let _ = db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agent_options (runtime, options, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(runtime) DO UPDATE
                       SET options = excluded.options, updated_at = excluded.updated_at",
                    rusqlite::params![runtime, json, updated_at],
                )
            })
            .await;
    });
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
