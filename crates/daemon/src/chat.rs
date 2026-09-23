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

/// Marker between the INJECTED context (role prompt, memory, handoff) and the
/// user's own words inside the single prompt block.
///
/// Why a marker rather than a smarter parser (t101): the injected blocks ride
/// in the SAME text block as the user's message, because the ACP layer joins
/// them (context, separator, text) before sending. The harness then records the
/// whole thing as one user message, and the sessions indexer -- which cannot
/// tell a prompt from a question -- used it as the row preview. So the row was
/// named after the platform's own prompt.
///
/// The tempting fix is to strip a leading bracket block. It is wrong: a user
/// may genuinely start a message with a bracket, and the rule would delete real
/// content -- the object-set-must-match-the-intent failure this team keeps
/// catching. This marker is EMITTED BY US, so the split is structural: it fires
/// only where we actually injected something, and a message with no injection
/// is untouched.
///
/// U+2063 (INVISIBLE SEPARATOR) makes an accidental collision with typed text
/// effectively impossible while staying invisible in any transcript a human
/// reads.
/// The injection block headers THIS daemon puts at the top of a prompt.
///
/// ONE list, deliberately: the indexer discards a title that starts with one of
/// these, and the audit's contract row 50 (panel/tools/design-audit.mjs) judges
/// the same literals. Producer, consumer and judge share one object set, so they
/// cannot drift apart.
///
/// The SHORT prefixes are what matter. A harness truncates the first message to
/// build a title, so the stored title is often a cut-off header, not the full one.
/// One named constant per block header the platform emits.
///
/// Three constraints on this list, learned the hard way:
///
/// 1. Only headers the PLATFORM emits. A user may legitimately start a message
///    with a bracket, and that must never be treated as ours.
/// 2. A new header MUST be registered here. That is no longer a convention:
///    every emitter builds its block through the builders below, and the test
///    every_emitted_block_header_is_registered fails the moment a builder
///    emits a header that is not in the array. The earlier version kept the
///    literals in two places and the list had already drifted 2 short of
///    reality.
/// 3. The members are the SHORT discriminable PREFIXES, not the whole block.
///    A harness truncates the first message to build a title -- the stored
///    title is often a cut-off header -- and block bodies contain variables.
///    Matching is starts-with, never contains: a user quoting a header in
///    their own message is a mention, not the object.
pub const HDR_ROLE: &str = "[role — you are";
pub const HDR_MEMORY: &str = "[memory context";
pub const HDR_RESUME: &str = "[conversation resume";
pub const HDR_RETRY: &str = "[retry context";

/// Every header the platform can emit. Built FROM the named constants, so the
/// set and the literals have exactly one source.
pub const INJECTED_HEADERS: [&str; 4] = [HDR_ROLE, HDR_MEMORY, HDR_RESUME, HDR_RETRY];

/// The role prompt block. Body is the agent's role text.
pub fn role_block(role: &str) -> String {
    format!("{HDR_ROLE}]\n{role}")
}

/// The memory-context block. Body is the already-rendered memory text.
pub fn memory_block(body: &str) -> String {
    format!("{HDR_MEMORY} — what the platform remembers]\n{body}")
}

/// The conversation-resume (handoff) block.
pub fn resume_block(tail: &str) -> String {
    format!(
        "{HDR_RESUME} — you are continuing your earlier conversation with the user; the transcript below is where it left off]\n{tail}"
    )
}

/// The retry block head, for the two places that append their own tail.
pub fn retry_head() -> &'static str {
    HDR_RETRY
}

pub const USER_TEXT_SENTINEL: &str = "\n\n\u{2063}ruagent:user-text\u{2063}\n\n";

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
    /// The project directory this chat works in (None = the per-chat
    /// scratch workspace). Survives handoff, resume and model restarts
    /// — the conversation stays in its project.
    pub cwd: Option<PathBuf>,
    pub created_at: String,
    session: ChatSession,
    last_active: Arc<Mutex<Instant>>,
    /// Whether this chat is generating a reply RIGHT NOW.
    ///
    /// A RUN state, not a lifetime state. Set when a prompt goes out,
    /// cleared by the session own terminal event (Stopped or Error) --
    /// never inferred from active, from message_count, or from an
    /// updated_at window. See ChatHistoryEntry::active for the other half
    /// of the pair and for the implication between them.
    generating: Arc<std::sync::atomic::AtomicBool>,
    /// Whether the memory context already went out with a prompt (once
    /// per chat — the first message is the natural recall query).
    memory_injected: Arc<std::sync::atomic::AtomicBool>,
    /// The agent's portable role prompt (two-layer model): injected
    /// ahead of the first prompt on every runtime.
    agent_prompt: Option<String>,
    /// Prior-conversation tail for an agent handoff (switch_agent):
    /// rides the first prompt's context, ahead of the memory context —
    /// the new agent takes over mid-conversation.
    handoff: Option<String>,
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
            // A handoff tail rides first: the conversation the new
            // agent is taking over.
            if let Some(h) = &self.handoff {
                ctx = Some(match ctx {
                    Some(c) => format!(
                        "{h}

{c}"
                    ),
                    None => h.clone(),
                });
            }
            // The role prompt rides next — what this agent IS.
            if let Some(role) = &self.agent_prompt {
                let role_block = role_block(role);
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
                        ctx = Some(memory_block(&format!(
                            "[relevant memories for this conversation]
{sem}"
                        )))
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
        // Set BEFORE the prompt is queued: the history route may be polled
        // the instant the POST returns, and a flag set afterwards would
        // read false on that first poll.
        self.generating
            .store(true, std::sync::atomic::Ordering::SeqCst);
        // The sentinel goes on the END of the context, so the boundary sits
        // exactly where the injection stops. With no context nothing was
        // injected and nothing is marked -- a plain prompt is unchanged.
        let context = context.map(|c| format!("{c}{USER_TEXT_SENTINEL}"));
        let sent = self.send(ChatCommand::Prompt { text, context });
        if sent.is_err() {
            // Nothing went out, so nothing will ever clear it.
            self.generating
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        sent
    }

    /// Is this chat generating a reply right now?
    pub fn is_generating(&self) -> bool {
        self.generating.load(std::sync::atomic::Ordering::SeqCst)
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
    Option<String>, // cwd
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
    /// The project directory the conversation ran in (None = scratch).
    pub cwd: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// A live chat the daemon is still holding (history can reopen the
    /// transcript; a live one also streams).
    ///
    /// LIFETIME, not run state: a chat stays active for as long as the
    /// daemon holds it, including the whole time it sits idle waiting for
    /// the next message (an idle reaper eventually drops it). It answers
    /// the question: is this conversation still open? NOT: is it working?
    pub active: bool,
    /// Whether this chat is generating a reply RIGHT NOW.
    ///
    /// RUN state, separate from active. True from the moment a prompt is
    /// accepted until the session reports the turn finished (Stopped, or
    /// Error). Read from the live session, never derived from active, from
    /// message_count, or from an updated_at window.
    ///
    /// Implication: generating == true REQUIRES active == true (a chat
    /// that is generating is by definition still held). The converse does
    /// NOT hold -- an active chat is normally idle, which is exactly the
    /// distinction active alone could not express.
    pub generating: bool,
    pub message_count: Option<u32>,
    pub preview: Option<String>,
    /// Sessions-index key of the transcript (the shared viewer route).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

/// Outcome of ChatManager::delete, so the API can report it honestly instead
/// of collapsing removed and never-there into one status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatDelete {
    /// The row was removed. was_live says whether a session had to be
    /// stopped and closed first.
    Deleted { was_live: bool },
    /// No such row -- an idempotent no-op, not an error.
    NotFound,
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
    /// Session → memory distillation policy (auto on close). Behind a
    /// RwLock: the panel's settings card swaps it at runtime (the file
    /// edit and this value update together).
    pub distill_policy: std::sync::RwLock<crate::distill::AutoDistill>,
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

/// Idle reaping: a chat untouched for this long closes (and
/// auto-distills). 60 minutes — sessions belong to projects now
/// (cwd), and several concurrent ones must survive a coffee break,
/// not just a single prompt cycle.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
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
            distill_policy: std::sync::RwLock::new(distill_policy),
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
                    "cwd": c.cwd.as_ref().map(|p| p.display().to_string()),
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
        let mut out = memory_block("") + "\n";
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
    /// `cwd` pins the session to a project directory — the agent works
    /// there instead of a fresh scratch dir under workspaces/chat-<id>.
    /// Records a chats-table history row.
    pub async fn start(
        &self,
        card: &AgentCard,
        model: Option<String>,
        cwd: Option<PathBuf>,
    ) -> Result<Chat> {
        // A project chat must work in an existing directory: a typo'd
        // path fails the start request, not the first agent spawn.
        if let Some(dir) = &cwd
            && !dir.is_dir()
        {
            anyhow::bail!("cwd `{}` is not an existing directory", dir.display());
        }
        self.start_inner(card, model, &card.name, true, None, None, cwd)
            .await
    }

    /// Hand the conversation over to another agent (role or runtime):
    /// the old session closes (and auto-distills — its segment is done),
    /// a new one starts on the target card, and a bounded tail of the
    /// prior conversation rides the new session's first prompt as
    /// handoff context. The conversation continues; the engine changes.
    pub async fn switch_agent(&self, id: RunId, card: &AgentCard) -> Result<Chat> {
        let old = self
            .chat(id)
            .ok_or_else(|| anyhow::anyhow!("chat not found"))?;
        let old_label = old.agent.clone();
        // The new session continues in the same project (cwd), not a
        // fresh scratch dir.
        let old_cwd = old.cwd.clone();
        let msgs = crate::sessions::parse_file_messages("ruagent", &self.transcript_path(id));
        // Bounded tail: the last 8 turns, ≤ 3000 chars, char-boundary safe.
        let mut tail = String::new();
        for m in msgs.iter().rev().take(8).collect::<Vec<_>>().iter().rev() {
            let line = format!("[{}] {}", m.role, m.text);
            if tail.len() + line.len() + 1 > 3000 {
                break;
            }
            tail.push_str(&line);
            tail.push('\n');
        }
        let handoff = if tail.trim().is_empty() {
            None
        } else {
            Some(format!(
                "[conversation handoff — you are taking over a conversation previously held with `{old_label}`; treat the tail below as established context and continue naturally]
{tail}"
            ))
        };
        self.close(id);
        self.start_inner(card, None, &card.name, true, handoff, None, old_cwd)
            .await
    }

    /// Resume a closed conversation: restart the session on the same
    /// agent, under the SAME RunId — the transcript appends, the
    /// history row survives (one thread, not fragments) — with the
    /// prior conversation handed over as context on the next prompt.
    /// Session switching, ChatGPT-style: click a past conversation and
    /// keep talking.
    pub async fn resume_chat(&self, id: RunId, card: &AgentCard) -> Result<Chat> {
        if self.chat(id).is_some() {
            anyhow::bail!("chat is live — attach instead of resuming");
        }
        let (label, cwd): (String, Option<String>) = {
            let key = id.to_string();
            self.db
                .call(move |conn| {
                    conn.query_row("SELECT agent, cwd FROM chats WHERE id = ?1", [&key], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                })
                .await
                .map_err(|_| anyhow::anyhow!("chat not found in history"))??
        };
        let msgs = crate::sessions::parse_file_messages("ruagent", &self.transcript_path(id));
        // A resume needs more context than a mid-chat handoff: the last
        // 16 turns, <= 8000 chars.
        let mut tail = String::new();
        for m in msgs.iter().rev().take(16).collect::<Vec<_>>().iter().rev() {
            let line = format!("[{}] {}", m.role, m.text);
            if tail.len() + line.len() + 1 > 8000 {
                break;
            }
            tail.push_str(&line);
            tail.push('\n');
        }
        let handoff = if tail.trim().is_empty() {
            None
        } else {
            Some(resume_block(&tail))
        };
        self.start_inner(
            card,
            None,
            &label,
            true,
            handoff,
            Some(id),
            cwd.map(PathBuf::from),
        )
        .await
    }

    /// The spawn path shared by chats and probes. `label` is the history
    /// identity (the role the user picked); `record` decides whether a
    /// chats row is written (probes must not pollute history); `cwd` is
    /// the project directory (None = a fresh scratch workspace).
    #[allow(clippy::too_many_arguments)]
    async fn start_inner(
        &self,
        card: &AgentCard,
        model: Option<String>,
        label: &str,
        record: bool,
        handoff: Option<String>,
        reuse: Option<RunId>,
        cwd: Option<PathBuf>,
    ) -> Result<Chat> {
        let spec = ruagent_acp::adapter_for(card.harness)
            .spawn_spec(card)
            .with_context(|| format!("resolving spawn command for `{}`", card.name))?;
        // A resumed conversation keeps its RunId: the transcript
        // appends, the history row survives, the rail shows one thread.
        let id = reuse.unwrap_or_else(RunId::generate);
        // A project chat works in its directory as-is (it exists —
        // validated at start); everything else gets a fresh scratch
        // workspace under workspaces/.
        let workspace = match &cwd {
            Some(dir) => dir.clone(),
            None => {
                let ws = self.root.join("workspaces").join(format!("chat-{id}"));
                std::fs::create_dir_all(&ws)
                    .with_context(|| format!("creating workspace {}", ws.display()))?;
                ws
            }
        };

        // Pre-flight the injected MCP servers (see the runs API for why): a
        // command that cannot be resolved is a configuration error worth naming
        // here, not an opaque harness "mcp-client(mcp)" failure later.
        let mcp = self
            .mcp
            .expand_profile_preflighted(card.mcp_profile.as_deref(), &card.name)
            .map_err(anyhow::Error::msg)?;

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
            cwd,
            created_at: Utc::now().to_rfc3339(),
            session,
            last_active: Arc::new(Mutex::new(Instant::now())),
            generating: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            memory_injected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            agent_prompt: card.prompt.clone(),
            handoff,
        };

        // Run-state watcher: generating is cleared by the session own
        // terminal event, not by a timer and not by watching the clock.
        // Stopped covers a normal turn and a cancelled one; Error is the
        // other way a turn ends. Every other event leaves the flag alone.
        {
            let mut sub = chat.session.subscribe();
            let generating = chat.generating.clone();
            tokio::spawn(async move {
                loop {
                    match sub.recv().await {
                        Ok(RunEvent::Stopped { .. }) | Ok(RunEvent::Error { .. }) => {
                            generating.store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => return, // chat closed
                    }
                }
            });
        }

        // History row: who (agent identity) on which engine, in which
        // project.
        if record {
            let id_s = id.to_string();
            let agent_s = chat.agent.clone();
            let runtime_s = chat.runtime.clone();
            let model_s = chat.model.clone();
            let cwd_s = chat.cwd.as_ref().map(|p| p.display().to_string());
            let now = Utc::now().timestamp_millis();
            let db = self.db.clone();
            let _ = db
                .call(move |conn| {
                    // Resume (same id): refresh the engine identity but
                    // keep the row's title and created_at — it is the
                    // same conversation continuing.
                    conn.execute(
                        "INSERT INTO chats
                             (id, agent, runtime, model, title, created_at, updated_at, cwd)
                         VALUES (?1,?2,?3,?4,NULL,?5,?5,?6)
                         ON CONFLICT(id) DO UPDATE SET
                             agent = excluded.agent,
                             runtime = excluded.runtime,
                             model = excluded.model,
                             updated_at = excluded.updated_at,
                             cwd = excluded.cwd",
                        rusqlite::params![id_s, agent_s, runtime_s, model_s, now, cwd_s],
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

    /// Delete one chat-history row.
    ///
    /// WHAT IT REMOVES: the `chats` row -- i.e. the entry in
    /// GET /api/v1/chats. A LIVE chat is stopped first and then closed, so a
    /// chat that is generating cannot keep writing after its record is gone.
    ///
    /// WHAT IT DOES NOT REMOVE: the transcript JSONL under transcripts/, the
    /// `sessions` index row, and anything memory ingestion already
    /// produced -- closing a chat still runs the auto-distill hook, so
    /// memories distilled from it survive the delete. Deleting the history
    /// entry is not the same as retracting the conversation's content.
    ///
    /// WHY NO TOMBSTONE (checked, not assumed): nothing rebuilds the
    /// `chats` table. A workspace-wide grep for writes to it finds
    /// exactly two -- the INSERT ... ON CONFLICT in start_inner (chat
    /// start/resume) and the first-prompt title UPDATE. There is no periodic
    /// re-index of `chats`, unlike `sessions`, which
    /// SessionIndexer rewrites with INSERT OR REPLACE every 60s and which
    /// therefore needs session_deletions. A resume of a deleted id DOES
    /// re-create the row; that is a deliberate user action, not a background
    /// rebuild, so it needs no marker.
    ///
    /// IDEMPOTENT: deleting an absent id is NotFound, not an error, and
    /// repeating a delete changes nothing.
    pub async fn delete(&self, id: RunId) -> Result<ChatDelete, anyhow::Error> {
        // Terminate the run BEFORE dropping the record: the reverse order
        // leaves a live session writing into a chat the list no longer
        // knows about.
        let live = self.chats.lock().expect("chats lock").get(&id).cloned();
        let was_live = live.is_some();
        if let Some(c) = &live
            && c.is_generating()
        {
            let _ = c.send(ChatCommand::Stop);
        }
        // close() removes it from the registry and sends Shutdown; dropping
        // the broadcast sender is what ends attached SSE streams -- they get
        // the terminal end event from the Err(_) arm in chat_events.
        self.close(id);
        let removed = self.db.delete_chat(&id.to_string()).await?;
        Ok(if removed {
            ChatDelete::Deleted { was_live }
        } else {
            ChatDelete::NotFound
        })
    }

    /// Spawn a background distillation for a closed chat (policy-gated).
    fn maybe_auto_distill(&self, id: RunId) {
        let Some(distiller) = self.auto_distiller() else {
            return;
        };
        let key = crate::sessions::session_key_of(&self.transcript_path(id));
        let agent = self
            .distill_policy
            .read()
            .expect("distill policy")
            .agent
            .clone();
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
        // The restarted session keeps the project cwd — switching model
        // is not switching projects.
        let cwd = chat.cwd.clone();
        let chat = self
            .start_inner(card, model, label, true, None, None, cwd)
            .await?;
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
                let chat = self
                    .start_inner(card, None, &card.name, false, None, None, None)
                    .await?;
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
                        "SELECT id, agent, runtime, model, title, created_at, updated_at, cwd
                           FROM chats WHERE agent = ?1 ORDER BY updated_at DESC LIMIT ?2",
                        vec![a, &limit],
                    ),
                    None => (
                        "SELECT id, agent, runtime, model, title, created_at, updated_at, cwd
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
                            r.get::<_, Option<String>>(7)?,
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
                |(id, agent, runtime, model, title, created_at, updated_at, cwd)| {
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
                        cwd,
                        created_at,
                        updated_at,
                        active: false,
                        generating: false,
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

        // Both flags come from the SAME live snapshot: active is registry
        // membership, generating is the session run state. Taking them
        // together keeps the implication (generating => active) true by
        // construction rather than by luck.
        let live: HashMap<String, bool> = {
            let chats = self.chats.lock().expect("chats lock");
            chats
                .iter()
                .map(|(id, c)| (id.to_string(), c.is_generating()))
                .collect()
        };
        for e in &mut entries {
            if let Some(key) = &e.session_key
                && let Some((count, preview)) = index.get(key)
            {
                e.message_count = Some(*count);
                e.preview.clone_from(preview);
            }
            // The index is the cheap source, but it is written by a 60s
            // background scan. A chat created by sending its first message
            // therefore has NO index row yet, and used to report
            // message_count: None -- which the panel's rule (t90: a chat with
            // no messages is not a chat) reads as "no messages" and HIDES.
            // The user saw the conversation they had just started vanish from
            // the list. Real state and reported state disagreed; this is the
            // daemon's job to fix, not the rule's.
            //
            // So when the index has nothing, count the chat's OWN transcript.
            // Same object the rule asks about -- "did the user ever say
            // anything here" -- and a chat with no message still counts 0 and
            // stays hidden. The rule is NOT relaxed.
            if e.message_count.is_none()
                && let Ok(rid) = e.id.parse::<RunId>()
            {
                let lines =
                    ruagent_store::read_transcript(&self.transcript_path(rid)).unwrap_or_default();
                let n = lines
                    .iter()
                    .filter(|l| matches!(l.event, ruagent_core::RunEvent::UserMessage { .. }))
                    .count();
                e.message_count = Some(u32::try_from(n).unwrap_or(u32::MAX));
            }
            e.active = live.contains_key(&e.id);
            e.generating = live.get(&e.id).copied().unwrap_or(false);
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
        let policy = self.distill_policy.read().expect("distill policy").clone();
        if !policy.auto {
            return None;
        }
        Some(crate::distill::Distiller {
            db: self.db.clone(),
            root: self.root.clone(),
            embedder: self.embedder.clone(),
            registry: self.registry.clone(),
            language: policy.language,
            prompt_override: policy.prompt,
            graph: policy.graph,
        })
    }

    /// Swap the live distillation policy (the settings card; the
    /// policy.toml file edit happens before this call).
    pub fn set_distill_policy(&self, policy: crate::distill::AutoDistill) {
        *self.distill_policy.write().expect("distill policy") = policy;
    }

    /// The current live distillation policy (manual distill shares it).
    pub fn distill_policy_now(&self) -> crate::distill::AutoDistill {
        self.distill_policy.read().expect("distill policy").clone()
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

#[cfg(test)]
mod generating_tests {
    use super::*;

    /// The mock ACP agent, built by the same workspace run.
    fn mock_agent() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.parent()?; // target/<profile>
        let name = if cfg!(windows) {
            "ruagent-mock-agent.exe"
        } else {
            "ruagent-mock-agent"
        };
        let p = dir.join(name);
        p.is_file().then_some(p)
    }

    /// Delete removes the HISTORY ROW, and does it without disturbing
    /// anything else -- driven by the mock agent, no real harness.
    ///
    /// Covers the four things the contract asks to be pinned:
    ///   * a deleted chat leaves the list while its neighbour stays;
    ///   * deleting an absent id is NotFound (idempotent, not an error);
    ///   * close() (the OLD endpoint) still keeps the row -- its semantics
    ///     are unchanged, which is the whole reason a second entry point
    ///     was added rather than changing that one;
    ///   * deleting a GENERATING chat stops the run first, so no live
    ///     session survives its own record.
    #[tokio::test]
    async fn delete_removes_the_row_and_stops_a_live_run_first() {
        let Some(mock) = mock_agent() else {
            panic!("mock agent binary not built next to the test exe");
        };
        let root = std::env::temp_dir().join(format!(
            "ruagent-del-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
        let chats = ChatManager::new(
            db.clone(),
            root.clone(),
            Arc::new(|_, _| {}),
            crate::config::McpConfig::default(),
            crate::distill::AutoDistill::default(),
            None,
            crate::distill::AgentRegistry::default(),
        );
        let card: AgentCard = serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000002",
            "name": "mock",
            "harness": "mock",
            "command": format!("{} --behavior echo", mock.display()),
            "description": "test",
            "model": null,
            "reasoning_effort": null,
            "context_window": null,
            "mcp_profile": null,
            "models": [],
            "tags": [],
            "enabled": true
        }))
        .expect("agent card");

        let victim = chats.start(&card, None, None).await.expect("start victim");
        // Captured before the drop at the end of this test: the assertions
        // above it still need the id after the handle itself is gone.
        let vid = victim.id;
        let neighbour = chats
            .start(&card, None, None)
            .await
            .expect("start neighbour");
        let embedder: Arc<dyn ruagent_knowledge::embed::Embedder> =
            Arc::new(ruagent_knowledge::embed::HashEmbedder::default());
        victim
            .send_prompt(&db, embedder, "hello".into())
            .await
            .expect("send prompt");

        let before = chats.history(None, 50).await;
        assert!(before.iter().any(|e| e.id == vid.to_string()));
        assert!(before.iter().any(|e| e.id == neighbour.id.to_string()));
        assert!(victim.is_generating(), "the victim is mid-turn");

        // A subscriber stands in for an attached SSE client: it is what the
        // event stream is fed from, so "the sender is dropped" IS "the
        // session can no longer produce events".
        let mut witness = victim.subscribe();

        // Delete WHILE GENERATING: the run must be terminated first.
        let out = chats.delete(vid).await.expect("delete");
        assert_eq!(out, ChatDelete::Deleted { was_live: true });
        assert!(
            chats.chat(vid).is_none(),
            "a deleted chat must not stay in the live registry"
        );
        let after = chats.history(None, 50).await;
        assert!(
            !after.iter().any(|e| e.id == vid.to_string()),
            "the deleted row is gone from the list"
        );
        assert!(
            after.iter().any(|e| e.id == neighbour.id.to_string()),
            "the neighbour is untouched"
        );
        assert_eq!(after.len(), before.len() - 1);

        // The session is really gone. A broadcast channel reports Closed once
        // every sender is dropped, so this only holds after OUR handle goes:
        // that is the point. It proves the registry no longer holds the
        // session, and that nothing else in-process does either.
        //
        // In the daemon the SSE handler ALSO holds a Chat clone, so the
        // channel alone cannot end an attached stream -- which is exactly why
        // chat_events now polls the registry as a second liveness signal and
        // sends the terminal end event when the chat disappears.
        // What the contract asks is that the agent produces NO FURTHER
        // MESSAGES. Two details make that measurable:
        //   * the witness subscribed before the prompt, so its buffer holds
        //     events from BEFORE the delete -- those are drained, not counted;
        //   * teardown is asynchronous (Shutdown ends the turn), so a
        //     Stop/cancel event may still land. A settle window absorbs that
        //     tail; the QUIET WINDOW after it is the actual claim.
        drop(victim);
        let settle = tokio::time::Instant::now() + std::time::Duration::from_millis(1200);
        while tokio::time::Instant::now() < settle {
            let _ = witness.try_recv();
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
        let quiet = tokio::time::Instant::now() + std::time::Duration::from_millis(1200);
        let mut after_teardown = 0usize;
        while tokio::time::Instant::now() < quiet {
            if witness.try_recv().is_ok() {
                after_teardown += 1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
        assert_eq!(
            after_teardown, 0,
            "a deleted chat must not produce another message"
        );

        // Idempotent: a second delete is NotFound, not an error.
        assert_eq!(
            chats.delete(vid).await.expect("second delete"),
            ChatDelete::NotFound
        );
        assert_eq!(chats.history(None, 50).await.len(), after.len());

        // The OLD endpoint's semantics are unchanged: close keeps the row.
        assert!(chats.close(neighbour.id));
        let closed = chats.history(None, 50).await;
        assert!(
            closed.iter().any(|e| e.id == neighbour.id.to_string()),
            "close() must still leave the history row (unchanged semantics)"
        );
        assert!(
            !closed
                .iter()
                .find(|e| e.id == neighbour.id.to_string())
                .unwrap()
                .active
        );
    }

    /// generating tracks the RUN; active tracks the LIFETIME. Both halves of
    /// the pair are asserted on one real chat, driven by the mock agent:
    ///   - while the prompt is in flight: generating == true AND active == true
    ///   - after the turn ends:           generating == false AND active == true
    /// The second line is the whole point -- active alone cannot express it.
    #[tokio::test]
    async fn generating_tracks_the_run_while_active_tracks_the_lifetime() {
        let Some(mock) = mock_agent() else {
            panic!("mock agent binary not built next to the test exe");
        };
        let root = std::env::temp_dir().join(format!(
            "ruagent-gen-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
        let chats = ChatManager::new(
            db.clone(),
            root.clone(),
            Arc::new(|_, _| {}),
            crate::config::McpConfig::default(),
            crate::distill::AutoDistill::default(),
            None,
            crate::distill::AgentRegistry::default(),
        );

        let card: AgentCard = serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "mock",
            "harness": "mock",
            "command": format!("{} --behavior echo", mock.display()),
            "description": "test",
            "model": null,
            "reasoning_effort": null,
            "context_window": null,
            "mcp_profile": null,
            "models": [],
            "tags": [],
            "enabled": true
        }))
        .expect("agent card");

        let chat = chats.start(&card, None, None).await.expect("start chat");
        let id = chat.id;
        let embedder: Arc<dyn ruagent_knowledge::embed::Embedder> =
            Arc::new(ruagent_knowledge::embed::HashEmbedder::default());

        // (a) generating, and therefore active too.
        chat.send_prompt(&db, embedder, "hello".into())
            .await
            .expect("send prompt");
        assert!(
            chat.is_generating(),
            "generating must be true the moment the prompt is accepted"
        );
        let during = chats.history(None, 50).await;
        let e = during
            .iter()
            .find(|e| e.id == id.to_string())
            .expect("chat in history");
        assert!(
            e.generating,
            "history must report generating during the turn"
        );
        assert!(e.active, "a generating chat is by definition still held");

        // (b) held but idle: the reply landed, the chat is STILL active.
        let mut idle = None;
        for _ in 0..200 {
            let h = chats.history(None, 50).await;
            if let Some(e) = h.iter().find(|e| e.id == id.to_string())
                && !e.generating
            {
                idle = Some(e.clone());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let idle = idle.expect("the turn never finished");
        assert!(!idle.generating, "generating must clear when the turn ends");
        assert!(
            idle.active,
            "the chat is STILL held after the reply -- this is exactly the"
        );
    }
}
