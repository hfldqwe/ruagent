//! ChatManager: terminal-like persistent conversations over ACP. One
//! spawned agent process per chat, multi-turn prompts on one session,
//! model switching (live via `set_config_option` when the agent supports
//! it, session restart otherwise).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use ruagent_acp::adapter::HarnessAdapter as _;
use ruagent_acp::chat::{ChatCommand, ChatOptions, ChatSession, SessionOptionState, start_chat};
use ruagent_core::{AgentCard, RunEvent, RunId};
use ruagent_store::{TranscriptWriter, transcript_path};
use tokio::sync::mpsc;

/// One live chat.
#[derive(Clone)]
pub struct Chat {
    pub id: RunId,
    pub agent: String,
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

    /// Send a user prompt; the first one carries the memory context.
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

pub struct ChatManager {
    db: ruagent_store::Db,
    root: PathBuf,
    chats: Arc<Mutex<HashMap<RunId, Chat>>>,
    park_ask: AskParker,
    mcp: crate::config::McpConfig,
    /// Session → memory distillation policy (auto on close).
    pub distill_policy: crate::distill::AutoDistill,
    /// Agent registry view for the distiller.
    pub registry: crate::distill::AgentRegistry,
    /// Shared embedder for distillation writes (set at boot).
    pub embedder: Option<std::sync::Arc<dyn ruagent_knowledge::embed::Embedder>>,
    /// Advertised session options (model, reasoning effort, permission
    /// mode, …) per agent name, refreshed whenever any chat or probe
    /// reports them. What the panel pickers show.
    model_cache: Arc<Mutex<HashMap<String, Vec<SessionOptionState>>>>,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How long to wait for a spawned agent to report its model catalog.
const MODELS_WAIT: Duration = Duration::from_secs(20);

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
            mcp,
            distill_policy,
            registry,
            embedder,
            model_cache: Arc::new(Mutex::new(HashMap::new())),
        });
        this.spawn_idle_reaper();
        this
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
    /// Workspace = a fresh dir under workspaces/chat-<id>.
    pub async fn start(&self, card: &AgentCard, model: Option<String>) -> Result<Chat> {
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
        {
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

        // Option-state tracker: whenever this session's advertised options
        // or current selections change, refresh the per-agent cache the
        // panel pickers read.
        {
            let mut watch = session.options_watch();
            let cache = self.model_cache.clone();
            let agent_name = card.name.clone();
            tokio::spawn(async move {
                loop {
                    if let Some(state) = watch.borrow_and_update().clone() {
                        cache
                            .lock()
                            .expect("model cache lock")
                            .insert(agent_name.clone(), state);
                    }
                    if watch.changed().await.is_err() {
                        return; // chat closed
                    }
                }
            });
        }

        let chat = Chat {
            id,
            agent: card.name.clone(),
            model,
            created_at: Utc::now().to_rfc3339(),
            session,
            last_active: Arc::new(Mutex::new(Instant::now())),
            memory_injected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            agent_prompt: card.prompt.clone(),
        };
        self.chats
            .lock()
            .expect("chats lock")
            .insert(id, chat.clone());
        Ok(chat)
    }

    /// Close and remove a chat. When auto-distill is on, the session
    /// becomes memories + graph entries in the background.
    pub fn close(&self, id: RunId) -> bool {
        let chat = self.chats.lock().expect("chats lock").remove(&id);
        match chat {
            Some(c) => {
                let _ = c.send(ChatCommand::Shutdown);
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
    /// same agent when the agent rejects the value or the option.
    /// Returns the chat plus whether a restart happened.
    pub async fn switch_model(
        &self,
        id: RunId,
        model: Option<String>,
        card: &AgentCard,
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
        let chat = self.start(card, model).await?;
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

    /// The advertised session options for an agent: cache, a live chat, or
    /// a throwaway probe session (spawned, queried, closed). Empty means
    /// the agent advertises no options (free-text model input).
    pub async fn agent_options(&self, card: &AgentCard) -> Result<Vec<SessionOptionState>> {
        // Cache first — refreshed by every chat/probe since daemon start.
        if let Some(state) = self
            .model_cache
            .lock()
            .expect("model cache lock")
            .get(&card.name)
        {
            return Ok(state.clone());
        }
        // A live chat is already asking the agent: wait for its report.
        let live = {
            let chats = self.chats.lock().expect("chats lock");
            chats.values().find(|c| c.agent == card.name).cloned()
        };
        let (watch, probe_id) = match live {
            Some(c) => (Some(c.options_watch()), None),
            None => {
                // Probe: start a chat, read the options, close it.
                let chat = self.start(card, None).await?;
                (Some(chat.options_watch()), Some(chat.id))
            }
        };
        let state = match watch {
            Some(mut w) => {
                if w.borrow().is_none() {
                    let _ = tokio::time::timeout(MODELS_WAIT, w.changed()).await;
                }
                w.borrow_and_update().clone()
            }
            None => None,
        };
        if let Some(id) = probe_id {
            self.close(id);
        }
        let state = state.unwrap_or_default();
        self.model_cache
            .lock()
            .expect("model cache lock")
            .insert(card.name.clone(), state.clone());
        Ok(state)
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
                    let chat = {
                        let mut map = chats.lock().expect("chats lock");
                        map.remove(&id)
                    };
                    if let Some(c) = chat {
                        let _ = c.send(ChatCommand::Shutdown);
                        // Auto-distill (policy): same path as explicit close.
                        mgr.maybe_auto_distill(id);
                    }
                }
            }
        });
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
