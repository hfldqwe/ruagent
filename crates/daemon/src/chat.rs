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
    /// The daemon's knowledge handle (t260), if the manager was given one.
    /// The chat path needs it to put knowledge/wiki hits into the FIRST
    /// prompt's context; without it there is simply no knowledge block.
    knowledge: Option<Arc<ruagent_knowledge::Knowledge>>,
    /// The capability plane, as a HANDLE rather than a snapshot (t8): the same
    /// Arc the manager holds, so a `PUT /api/v1/capabilities` narrows the next
    /// first prompt without a restart. `Chat` is `Clone` and this is an Arc.
    capabilities: std::sync::Arc<std::sync::RwLock<crate::capability::CapabilityPlane>>,
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
        // t137: the budget rides out of this block NEXT TO the bytes it measured,
        // so the pair cannot drift: both come from the same
        // `render_context_report` call in the producer.
        let (context, ctx_budget) = if self
            .memory_injected
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            (None, None)
        } else {
            // `memory_inject_chat` gates the RETRIEVAL block only (design §12):
            // the role and handoff blocks below still ride, because they are the
            // agent's identity rather than evidence pulled from the stores. The
            // LIVE plane is read here, so a runtime PUT takes effect on the next
            // chat whose first prompt goes out, with no restart.
            let inject = self
                .capabilities
                .read()
                .expect("capability plane")
                .gate(crate::capability::CapabilityId::MemoryInjectChat, true);
            let (mut ctx, ctx_budget) = if inject {
                // t137: the producer's own report rides WITH the bytes it
                // measured. `Ok(None)` = no render happened, so the event says
                // "not collected" (`None`) rather than an all-zero object that
                // would read as a measured empty injection.
                match ChatManager::injection_context(db, self.knowledge.as_deref(), embedder, &text)
                    .await?
                {
                    Some((render, report)) => (Some(render), Some(report)),
                    None => (None, None),
                }
            } else {
                (None, None)
            };
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
            (ctx, ctx_budget)
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
        // The budget rides the SAME command as the bytes it measured: the
        // sentinel above changes the string the agent sees, not what was
        // RENDERED, so the report still describes the render (t137).
        let sent = self.send(ChatCommand::Prompt {
            text,
            context,
            context_budget: ctx_budget,
        });
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
    /// The daemon's ONE knowledge handle (t260). Set by lib.rs at boot; None
    /// in tests that do not need retrieval, which yields no knowledge block
    /// rather than a second embedder decision.
    knowledge: Mutex<Option<Arc<ruagent_knowledge::Knowledge>>>,
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
    /// The capability plane (design §4.5). ONE shared handle: boot installs the
    /// plane `policy.toml` defines, `PUT /api/v1/capabilities` writes the new
    /// value through it, and every consumer that reads `capabilities()` sees
    /// the swap at once.
    ///
    /// `new` initialises it to legacy mode — a constructor default must not be
    /// able to change behaviour (L1), so every existing `ChatManager::new` call
    /// site keeps compiling and keeps today's behaviour.
    capabilities: std::sync::Arc<std::sync::RwLock<crate::capability::CapabilityPlane>>,
}

/// How many turns this chat's transcript holds. ZERO IS NORMAL: a chat opened
/// and closed without a conversation leaves one to three JSONL lines and no
/// turns, and the sessions index deliberately holds no row for such a
/// transcript (56 of the 147 live transcripts are exactly this shape).
///
/// Asking the distiller about one of those came back as
/// `Query returned no rows` — rusqlite's error for the missing index row — and
/// was logged as `WARN auto-distill failed`. That is the failure this fixes: 21
/// such WARNs in one window, with 0 real ERRORs. A log that only ever shows
/// false red teaches its reader to ignore red, and then a real failure is
/// invisible (t313).
fn transcript_turns(path: &std::path::Path) -> usize {
    crate::sessions::parse_file_messages("ruagent", path).len()
}

/// What one background distill attempt did, so a test can assert the outcome
/// without parsing logs (t313).
#[derive(Debug, PartialEq)]
enum DistillAttempt {
    /// The chat had no turns: there was nothing to distill.
    NothingToDistill,
    Distilled {
        memories: u32,
        entities: u32,
    },
    /// A real failure — this is the one that must stay visible.
    Failed,
}

/// The background half of `maybe_auto_distill`, split out so a test can drive it
/// without a live ChatManager (t313).
///
/// `plan` is the UNATTENDED plan the capability plane resolved (t8): it decides
/// whether this pass runs the free rule extractor, the ACP one, or both, and it
/// is never empty here — the caller returns early on an empty plan, so no agent
/// is selected and no turn is spent when nothing is enabled.
async fn auto_distill_now(
    distiller: &crate::distill::Distiller,
    key: &str,
    agent: Option<&str>,
    turns: usize,
    plan: crate::extract_plane::ExtractPlan,
) -> DistillAttempt {
    if turns == 0 {
        // debug, not warn: nothing went wrong, and the caller asked for
        // nothing (a chat with no turns has nothing to extract).
        tracing::debug!(session = %key, "nothing to distill (this chat has no turns)");
        return DistillAttempt::NothingToDistill;
    }
    // THE CARD IS LAZY (t14). Only a plan that includes the ACP tier has any use
    // for an agent, so a rules-only pass is handed `None` and runs with an EMPTY
    // ENABLED SET — the environment the free tier exists for (no agents
    // configured, no API key). Selecting an agent first made a rules-only pass
    // fail with `no enabled agent available`, which was the one failure that
    // could never be true: the rules extractor never touches the card.
    let card = if plan.acp {
        let agents = distiller.registry.list_enabled();
        match crate::distill::select_agent(&agents, agent) {
            Ok(card) => Some(card.clone()),
            Err(e) => {
                tracing::warn!(session = %key, error = %e, "auto-distill failed");
                return DistillAttempt::Failed;
            }
        }
    } else {
        None
    };
    match distiller
        .distill_plan(key, card.as_ref(), plan, false)
        .await
    {
        Ok(o) => {
            tracing::info!(
                session = %key,
                memories = o.memories_written,
                entities = o.entities_written,
                source = %o.source,
                "auto-distilled"
            );
            DistillAttempt::Distilled {
                memories: o.memories_written,
                entities: o.entities_written,
            }
        }
        Err(e) => {
            tracing::warn!(session = %key, error = %e, "auto-distill failed");
            DistillAttempt::Failed
        }
    }
}

/// Search wider than the block will keep: the contract then picks the top
/// KNOWLEDGE_SOURCES + WIKI_PAGES by score, so a wiki page cannot crowd a
/// source out before the selection rule ever sees it.
const CHAT_KNOWLEDGE_SEARCH_N: u32 = 12;

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
            knowledge: Mutex::new(None),
            mcp,
            distill_policy: std::sync::RwLock::new(distill_policy),
            registry,
            embedder,
            model_cache: Arc::new(Mutex::new(HashMap::new())),
            capabilities: std::sync::Arc::new(std::sync::RwLock::new(
                crate::capability::CapabilityPlane::legacy(),
            )),
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

    /// The FIRST-prompt context for a chat, through the ONE injection contract
    /// (crates/memory/src/inject.rs).
    ///
    /// WHY THIS REPLACED memory_context_for (t260): that function wrote its own
    /// SQL, its own block header, its own 700-BYTE budget and its own
    /// per-entry cut with no visible marker. So the chat path had no tag
    /// blocks, no visible truncation, no dropped-item count -- and the two
    /// paths had already drifted once (the header wording changed in a097c5e
    /// and the old transcripts kept the old wording).
    ///
    /// BLOCK PRIORITY (drop order, and WHY -- the full rationale lives with the
    /// tags in crates/memory/src/inject.rs): user_profile > relevant_memories >
    /// knowledge > wiki > project_context. Tag order is first-seen and the
    /// total budget drops the LAST blocks first, so the caller pushes in that
    /// order: facts about the user, then evidence, then leads, then the
    /// narrowest scope. Every drop is counted in the render.
    ///
    /// Returns None when nothing was found: no context is better than a
    /// placeholder that costs tokens and says nothing.
    ///
    /// t137: the render now hands back its OWN budget report next to the bytes,
    /// and this is a `Result` because serialising that report can fail — a
    /// failure is an ERROR, never a silent `None` (that absorber shape is what
    /// this generation audits). `Ok(None)` = nothing was found, i.e. no render
    /// happened and the event must say "not collected"; `Ok(Some((text,
    /// report)))` = the render ran, so the report is a MEASUREMENT even when it
    /// is all zero. Same shape as the run path (`runs.rs::render_run_injection`).
    pub async fn injection_context(
        db: &ruagent_store::Db,
        knowledge: Option<&ruagent_knowledge::Knowledge>,
        embedder: std::sync::Arc<dyn ruagent_knowledge::embed::Embedder>,
        query: &str,
    ) -> Result<Option<(String, serde_json::Value)>> {
        use ruagent_memory::inject::{
            ContextItem, InjectionBudget, KNOWLEDGE_SOURCES, WIKI_PAGES, render_context_report,
        };

        let mut items: Vec<ContextItem> = Vec::new();

        // 1 + 2. WHICH memories, and by what rule: the chat preset carries both
        //        the groups and the query leg, and the rule itself lives in ONE
        //        place (memembed::select_injection_memories). This file no
        //        longer holds a single selection number -- before t278 it held
        //        five group limits, a top-n and a threshold of its own.
        //        A chat has no PROJECT NAME (it is pinned to a cwd), so the
        //        project argument is None: see CHAT_SELECTION's groups.
        for m in crate::memembed::select_injection_memories(
            db,
            Some(embedder),
            query,
            None,
            &ruagent_memory::inject::CHAT_SELECTION,
        )
        .await
        {
            items.push(ContextItem::dated(m.tag, m.content, &m.updated_at));
        }

        // 3. What the knowledge base says about it. The selection rule lives in
        //    the contract (knowledge_items): top N sources and top M generated
        //    pages, by the retrieval's own score, sources first. No hits means
        //    no block -- never an invented placeholder.
        //
        //    t19 (DEP-INT-8 / R-A H-1..H-4 / R-D D.7): the hits now reach the
        //    contract ENRICHED -- each one carries its scale-carrying relevance and,
        //    for a wiki page, the lead. The renderer, the budget and the truncation
        //    vocabulary are untouched: only the input shape changed, which is why
        //    the no-enrichment render stays byte-identical (golden test).
        if let Some(k) = knowledge
            && let Ok(page) = k.search_page(query, CHAT_KNOWLEDGE_SEARCH_N).await
        {
            let mut enriched: Vec<ruagent_memory::inject::EnrichedHit> = Vec::new();
            for r in &page.hits {
                let relevance = r.relevance.as_ref().map(|v| {
                    crate::memembed::relevance_meta(
                        v.value,
                        v.kind.as_str(),
                        v.version,
                        Some(v.query_background),
                    )
                });
                // G8 / RV-D2-1: the lead comes from the ASYNC `lead_for`, which is
                // the only entry point that can report a RECORDED coverage.
                // `lead_from` reports `None` by construction, so wiring it would
                // make the block's `coverage=` permanently `unknown` -- a
                // constructively-empty attainment of a criterion that only asks
                // whether the field is present.
                let lead = if r.hit.document.starts_with("wiki/") {
                    let slug = r
                        .hit
                        .document
                        .trim_start_matches("wiki/")
                        .trim_end_matches(".md");
                    let record = crate::wiki::page_record(db, slug).await;
                    crate::wiki::lead_for(k, &record, slug)
                        .await
                        .as_ref()
                        .map(crate::memembed::lead_meta)
                } else {
                    None
                };
                enriched.push(crate::memembed::enriched_hit(&r.hit, relevance, lead));
            }
            items.extend(ruagent_memory::inject::knowledge_items_enriched(
                &enriched,
                KNOWLEDGE_SOURCES,
                WIKI_PAGES,
            ));
        }

        // G10 / F5 closure (t63): the graph evidence producer mem-core shipped in
        // t58, called from THIS path. It goes in before the sort below so the
        // block order and the drop order stay owned by `tag_rank` alone; no new
        // parameter and no new dependency (the daemon already depends on
        // `ruagent_graph`). `GRAPH_PATHS` is the producer's per-turn path budget.
        if let Ok(evidence) =
            ruagent_graph::retrieve(db, &ruagent_graph::GraphQuery::for_text(query)).await
        {
            items.extend(crate::memembed::graph_evidence_items(
                &evidence,
                ruagent_memory::inject::GRAPH_PATHS,
            ));
        }

        // The BLOCK order is the contract drop order, not the order these
        // items happened to be discovered in (t278): one list, sorted by the
        // rank the contract owns, so a caller cannot reorder blocks by
        // accident and the drop order is the same on both paths.
        items.sort_by_key(|i| ruagent_memory::inject::tag_rank(i.tag));

        if items.is_empty() {
            return Ok(None);
        }
        let (out, report) = render_context_report(&items, &InjectionBudget::default());
        if out.trim().is_empty() {
            return Ok(None);
        }
        // The report is serialised WHERE IT IS PRODUCED, and a failure is a real
        // error (`with_context`), never a `None` that would read as "not
        // collected" while a render did happen (t137).
        let budget = serde_json::to_value(&report)
            .with_context(|| "serialising the chat injection budget report")?;
        Ok(Some((out, budget)))
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
            capabilities: std::sync::Arc::clone(&self.capabilities),
            agent_prompt: card.prompt.clone(),
            handoff,
            knowledge: self.knowledge_handle(),
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

    /// Hand the manager the ONE knowledge handle the daemon built at boot.
    ///
    /// WHY A SETTER AND NOT A new() PARAMETER: ChatManager::new is called from
    /// the api test harness, seven mock-agent tests and lib.rs; widening its
    /// signature would edit five files to deliver one handle. Optional by
    /// construction, so a caller that does not set it gets NO knowledge block
    /// (honest) instead of a second embedder decision -- which would be a
    /// second source of truth for what the vectors mean.
    pub fn set_knowledge(&self, k: Arc<ruagent_knowledge::Knowledge>) {
        *self.knowledge.lock().expect("knowledge lock") = Some(k);
    }

    /// The handle, if one was set.
    fn knowledge_handle(&self) -> Option<Arc<ruagent_knowledge::Knowledge>> {
        self.knowledge.lock().expect("knowledge lock").clone()
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
    ///
    /// THE GATE (design §6, §10.3): the effective condition is
    /// `[distill].auto AND gate(distill_session, true)`, plus the free
    /// `session_extract_rules` tier, which is opt-in and needs no legacy flag.
    /// Both are resolved ONCE into an `ExtractPlan` from the live plane; an
    /// empty plan returns before the spawn, so no agent is selected, no chat
    /// turn is spent and no `distill_log` row is written.
    fn maybe_auto_distill(&self, id: RunId) {
        let Some(distiller) = self.auto_distiller() else {
            return;
        };
        let plan = self.unattended_plan();
        if plan.is_empty() {
            return;
        }
        let key = crate::sessions::session_key_of(&self.transcript_path(id));
        let agent = self
            .distill_policy
            .read()
            .expect("distill policy")
            .agent
            .clone();
        // Read the turn count HERE, before the spawn: a chat with no turns has
        // nothing to distill, and asking the distiller about it anyway is what
        // produced the false WARNs (t313).
        let turns = transcript_turns(&self.transcript_path(id));
        tokio::spawn(async move {
            auto_distill_now(&distiller, &key, agent.as_deref(), turns, plan).await;
        });
    }

    /// The UNATTENDED extraction plan, resolved from the LIVE plane (t4's seam,
    /// t8's wiring): `session_extract_rules` is an `enabled` opt-in with no
    /// legacy flag to narrow, `distill_session` may only narrow `[distill].auto`
    /// (law L2). Reading the plane per call is what lets a toggle take effect
    /// without a restart.
    fn unattended_plan(&self) -> crate::extract_plane::ExtractPlan {
        let policy_auto = self.distill_policy.read().expect("distill policy").auto;
        let plane = self.capabilities.read().expect("capability plane").clone();
        crate::extract_plane::ExtractPlan::unattended(&plane, policy_auto)
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
        Ok(self.record_probe(runtime, &state, updated_at))
    }

    /// Record what a probe reported.
    ///
    /// t192: an empty probe is NOT evidence that the runtime advertises no
    /// options. `wait_options` turns its own timeout into an empty Vec, so a slow
    /// (cold start) or mis-registered first probe used to be cached AND persisted
    /// as "this runtime has no options" -- after which every picker stayed empty,
    /// including across a restart via `load_option_cache`, and nothing retried
    /// it. Keep the last good catalog instead: only a probe that actually
    /// reported options may replace what we know, and an empty one is not even
    /// remembered (so the next open probes again).
    ///
    /// Split out of `probe_options` so the rule can be tested without an agent
    /// that fails to advertise: the decision is the contract here, the spawn is
    /// not.
    fn record_probe(
        &self,
        runtime: &str,
        state: &[SessionOptionState],
        updated_at: i64,
    ) -> CachedOptions {
        if state.is_empty() {
            let previous = self
                .model_cache
                .lock()
                .expect("model cache lock")
                .get(runtime)
                .cloned();
            return previous.unwrap_or(CachedOptions {
                options: Vec::new(),
                updated_at,
            });
        }
        let entry = CachedOptions {
            options: state.to_vec(),
            updated_at,
        };
        self.model_cache
            .lock()
            .expect("model cache lock")
            .insert(runtime.to_string(), entry.clone());
        persist_options(&self.db, runtime, state, updated_at);
        entry
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
                    ruagent_store::read_transcript(self.transcript_path(rid)).unwrap_or_default();
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

    /// The distiller for the UNATTENDED path, or `None` when no agent registry
    /// handle exists.
    ///
    /// It deliberately does NOT return `None` for `[distill].auto == false`
    /// any more (t8): the auto flag is one INPUT to the plan, not the decision.
    /// `session_extract_rules` is a free, opt-in capability with no legacy flag
    /// to narrow, so with `auto = false` an operator who enables it still gets
    /// the deterministic tier — and with the shipped configuration the plan is
    /// empty, so `maybe_auto_distill` returns before the spawn exactly as
    /// before.
    fn auto_distiller(&self) -> Option<crate::distill::Distiller> {
        let policy = self.distill_policy.read().expect("distill policy").clone();
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

    /// The live capability plane. Read it, clone it, drop the guard — never
    /// hold it across an `.await` (a `std::sync::RwLock`, the same discipline
    /// `distill_policy` follows).
    pub fn capabilities(&self) -> crate::capability::CapabilityPlane {
        self.capabilities.read().expect("capability plane").clone()
    }

    /// Install/swap the live plane (boot installs what `policy.toml` defines;
    /// `PUT /api/v1/capabilities` swaps it after writing the file).
    pub fn set_capabilities(&self, plane: crate::capability::CapabilityPlane) {
        *self.capabilities.write().expect("capability plane") = plane;
    }

    /// The shared handle itself, so boot can install the ONE Arc on the
    /// RunManager (design §4.5) instead of keeping a second copy in sync.
    pub fn capabilities_handle(
        &self,
    ) -> std::sync::Arc<std::sync::RwLock<crate::capability::CapabilityPlane>> {
        std::sync::Arc::clone(&self.capabilities)
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

    /// t192: a probe that reports NOTHING must not become what we know.
    ///
    /// `wait_options` turns its own timeout into an empty Vec, so before this
    /// rule a slow (cold start) or mis-registered first probe was cached AND
    /// persisted as "this runtime advertises no options" -- after which every
    /// picker stayed empty, including across a restart via `load_option_cache`,
    /// and nothing retried it. Three halves are pinned here:
    ///   * an empty report leaves the last good catalog alone (cache AND db);
    ///   * an empty report on a COLD runtime is not remembered either, so the
    ///     next open probes again instead of trusting the emptiness;
    ///   * a report that does carry options still replaces what we know.
    #[tokio::test]
    async fn an_empty_probe_neither_overwrites_nor_persists_the_catalog() {
        let root = std::env::temp_dir().join(format!(
            "ruagent-opts-test-{}-{}",
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
        let option = |value: &str| -> SessionOptionState {
            serde_json::from_value(serde_json::json!({
                "id": "model",
                "name": "Model",
                "category": "model",
                "choices": [{ "value": value, "name": value }],
                "current": value,
            }))
            .expect("session option")
        };
        let cached = |runtime: &str| -> Option<CachedOptions> {
            chats
                .model_cache
                .lock()
                .expect("model cache lock")
                .get(runtime)
                .cloned()
        };
        async fn persisted(db: &ruagent_store::Db, runtime: &str) -> Option<i64> {
            let runtime = runtime.to_string();
            let res = db
                .call(move |conn| -> Result<Option<i64>, ruagent_store::DbError> {
                    Ok(conn
                        .query_row(
                            "SELECT updated_at FROM agent_options WHERE runtime = ?1",
                            [runtime],
                            |r| r.get::<_, i64>(0),
                        )
                        .ok())
                })
                .await;
            res.ok().and_then(|inner| inner.ok()).flatten()
        }

        // 1. A good probe is recorded, in the cache and in the table. The cache
        //    write is synchronous; the table write is spawned, so wait for the
        //    row instead of racing it.
        let good = chats.record_probe("mock", &[option("mock-pro")], 1);
        assert_eq!(good.options.len(), 1);
        assert_eq!(cached("mock").map(|c| c.options.len()), Some(1));
        let mut stored = None;
        for _ in 0..100 {
            stored = persisted(&db, "mock").await;
            if stored.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(stored, Some(1), "a real probe must be persisted");

        // 2. An empty probe must not touch either one. Its table write would be
        //    spawned too, so give that window a chance to (not) happen before
        //    asserting the row is still the one from step 1.
        let after_empty = chats.record_probe("mock", &[], 2);
        assert_eq!(
            after_empty.options.len(),
            1,
            "an empty probe must hand back the last good catalog"
        );
        assert_eq!(
            cached("mock").map(|c| c.updated_at),
            Some(1),
            "an empty probe must not overwrite the cached catalog"
        );
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(
            persisted(&db, "mock").await,
            Some(1),
            "an empty probe must not persist over the stored catalog"
        );

        // 3. Cold + empty is not remembered as knowledge: nothing cached, so the
        //    next open probes again.
        let cold = chats.record_probe("cold-runtime", &[], 3);
        assert!(cold.options.is_empty());
        assert!(
            cached("cold-runtime").is_none(),
            "an empty probe on a cold runtime must not be cached as a catalog"
        );

        // 4. The rule is not "never update": a real report still replaces it.
        let after_real = chats.record_probe("mock", &[option("mock-max")], 4);
        assert_eq!(
            after_real.options.first().map(|o| o.current.clone()),
            Some(Some("mock-max".to_string()))
        );
        assert_eq!(cached("mock").map(|c| c.updated_at), Some(4));
        let _ = std::fs::remove_dir_all(&root);
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
            // `slowreply` is the SUBJECT this test needs: a turn that provably
            // spans the delete (it runs 10s and ends only when nothing cancels
            // it) and that emits NO message unless it completes. `echo` replied
            // within milliseconds, so "delete WHILE GENERATING" was itself a
            // race -- and the message it produced before the delete was merely
            // DELIVERED after it, which is why the old shape read as a
            // violation.
            "command": format!("{} --behavior slowreply", mock.display()),
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

        // What the contract asks is that the deleted chat produces NO FURTHER
        // MESSAGE -- and the END of teardown is an EVENT, not a duration.
        //
        // The old shape was a fixed 1200 ms `settle` window followed by a
        // 1200 ms window that counted EVERY event as a message. Under
        // compile-scale load that is a mis-measurement, measured (V-B7, the
        // same bytes passing alone and failing while a cold build ran):
        //   * the in-flight prompt's own dispatch events -- `StateChanged
        //     { Running }` and `UserMessage { "hello" }` -- were DELIVERED
        //     2216 ms and 2373 ms after the delete, i.e. after the guess had
        //     expired, and the quiet window counted them;
        //   * the claim is about `RunEvent::AgentMessageChunk`; counting every
        //     variant reports a control event as "producing another message".
        // Neither was the product producing anything: no message arrives after
        // the delete.
        //
        // The synchronization point is the run's OWN end-of-turn signal --
        // `RunEvent::Stopped`/`RunEvent::Error`, the same pair the run-state
        // watcher in `start` uses to clear `generating` -- or the session's
        // sender going away (a broadcast channel reports Closed once every
        // sender is dropped: `delete` -> `close` -> `ChatCommand::Shutdown`).
        // Waiting for either is what the fixed duration was trying to guess.
        //
        // The BOUND is 5 s, and the number is not a taste: the subject is
        // `--behavior slowreply`, whose own turn runs 10 s and ends only when
        // nothing cancels it, so a run the delete did NOT stop cannot reach a
        // terminal event inside it. The wait absorbs the load; the assertion
        // keeps the claim.
        drop(victim);
        const TEARDOWN_BOUND: std::time::Duration = std::time::Duration::from_secs(5);
        const QUIET_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);
        let t_delete = tokio::time::Instant::now();
        let deadline = t_delete + TEARDOWN_BOUND;
        let mut messages = 0usize;
        let mut torn_down = false;
        let mut seen: Vec<String> = Vec::new();
        while tokio::time::Instant::now() < deadline {
            match witness.try_recv() {
                Ok(ev) => {
                    let kind = match ev {
                        ruagent_core::RunEvent::AgentMessageChunk { .. } => {
                            messages += 1;
                            "AgentMessageChunk"
                        }
                        ruagent_core::RunEvent::Stopped { .. } => {
                            torn_down = true;
                            "Stopped"
                        }
                        ruagent_core::RunEvent::Error { .. } => {
                            torn_down = true;
                            "Error"
                        }
                        ruagent_core::RunEvent::StateChanged { .. } => "StateChanged",
                        ruagent_core::RunEvent::UserMessage { .. } => "UserMessage",
                        ruagent_core::RunEvent::UsageUpdate { .. } => "UsageUpdate",
                        _ => "other",
                    };
                    seen.push(format!("{}ms:{kind}", t_delete.elapsed().as_millis()));
                    if torn_down {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    torn_down = true;
                    seen.push("Closed".to_string());
                    break;
                }
                Err(_) => {}
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            torn_down,
            "delete must stop the live run BEFORE dropping the record: {TEARDOWN_BOUND:?} \
             after the delete the run had produced neither its terminal event nor released \
             its session (a `slowreply` turn ends at 10s, so this can only be a run that \
             was never stopped); events seen after the delete: {seen:?}, messages={messages}"
        );

        // The tail after that signal: the same claim, bounded, and a message
        // here can only come from a producer that outlived the run.
        let quiet = tokio::time::Instant::now() + QUIET_WINDOW;
        while tokio::time::Instant::now() < quiet {
            if let Ok(ruagent_core::RunEvent::AgentMessageChunk { .. }) = witness.try_recv() {
                messages += 1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            messages, 0,
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
    /// t127: message_count must reflect the REAL state, not whatever the 60s
    /// sessions-index has reached so far.
    ///
    /// The panel rule (t90: a chat with no messages is not a chat) reads this
    /// field. A fresh database has no index row, and the daemon used to report
    /// None -- which the rule reads as no messages and HIDES. A user who had
    /// just sent their first message watched that conversation vanish from the
    /// list. Real state and reported state disagreed.
    ///
    /// Both directions are asserted. (a) proves the rule was NOT relaxed: a
    /// chat with no message still reports 0 and is still hidden. (b) proves
    /// the fix: one message reports 1 at once, with no index row anywhere.
    ///
    /// (b) is bounded by the WRITER, not by a duration (t149): it waits for the
    /// run's own transcript to carry the user's line -- the exact fact the count
    /// is derived from -- and only then asserts, so a starved machine costs the
    /// wait, never the assertion. It also asserts the premise the paragraph above
    /// states and nothing checked: that the index has NOT caught up (if it had,
    /// the count below could be the index's, and the assertion would be proving
    /// less than it claims).
    #[tokio::test]
    async fn message_count_is_real_before_the_index_catches_up() {
        let Some(mock) = mock_agent() else {
            panic!("mock agent binary not built next to the test exe");
        };
        let root = std::env::temp_dir().join(format!(
            "ruagent-count-test-{}-{}",
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
            "id": "00000000-0000-0000-0000-000000000003",
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
        .unwrap();
        let embedder: Arc<dyn ruagent_knowledge::embed::Embedder> =
            Arc::new(ruagent_knowledge::embed::HashEmbedder::default());

        // (a) REVERSE EVIDENCE: no message gives 0, NOT None, so the t90 rule
        // still hides a genuinely empty chat. The rule was not relaxed; only
        // the reported value was made true.
        let empty = chats.start(&card, None, None).await.expect("start empty");
        let h = chats.history(None, 50).await;
        let e = h
            .iter()
            .find(|e| e.id == empty.id.to_string())
            .expect("a fresh chat is still listed by the API");
        assert_eq!(e.message_count, Some(0), "no message must be 0, never None");

        // (b) THE FIX: one message gives 1 at once, with NO index row at all.
        empty
            .send_prompt(&db, embedder, "hello".into())
            .await
            .expect("send prompt");

        // SYNC ON THE WRITER, THEN ASSERT (t149; the flake was `left: Some(0) /
        // right: Some(1)` under a compile/link burst). What is waited for is a
        // SIGNAL, not a duration: when the index has no row, `history` derives this
        // count from the run's own transcript (`chat.rs:1566-1576` ->
        // `ruagent_store::read_transcript`), and every append is flushed before it
        // returns (`store/src/transcript.rs:36-47`), so the moment the line is
        // readable the value the assertion reads is settled.
        //
        // The run's EVENT stream is not usable as this sync point, and that is read
        // rather than preferred: the bus a test can subscribe to
        // (`Chat::subscribe`, `chat.rs:278`) is fed by the ACP layer
        // (`crates/acp/src/chat.rs:534` sends `RunEvent::UserMessage`), while the
        // transcript is appended by the run loop (`runs.rs:1506` -> `emit` ->
        // `append_event` -> `TranscriptWriter::append`) -- two different tasks, so
        // "the event arrived" does not order the append. The file does.
        //
        // 5 s is not a taste: the writer is the same process and the append lands
        // before the agent's own reply in the ordinary case (the subject is
        // `--behavior echo`), so this bound sits orders of magnitude above the
        // path's own latency; what it absorbs is the machine being starved, and it
        // fails LOUDLY when even that is not enough instead of looping quietly.
        const USER_MESSAGE_BOUND: std::time::Duration = std::time::Duration::from_secs(5);
        let transcript = chats.transcript_path(empty.id);
        let deadline = tokio::time::Instant::now() + USER_MESSAGE_BOUND;
        let landed = loop {
            let n = ruagent_store::read_transcript(&transcript)
                .unwrap_or_default()
                .iter()
                .filter(|l| matches!(l.event, ruagent_core::RunEvent::UserMessage { .. }))
                .count();
            if n > 0 || tokio::time::Instant::now() >= deadline {
                break n;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        assert!(
            landed > 0,
            "the run's writer must land the user's line within {USER_MESSAGE_BOUND:?} \
             (`runs.rs:1506` -> `store/src/transcript.rs:45` appends AND flushes), so a \
             timeout here is a signal that never arrived, not a duration that was too \
             short: {} bytes at {}",
            std::fs::metadata(&transcript).map(|m| m.len()).unwrap_or(0),
            transcript.display()
        );

        // The premise the paragraph above the test STATES ("with NO index row at
        // all") and nothing asserted: if the 60 s indexer (`sessions.rs:213`) had
        // already written this key, the count below could be the index's rather than
        // real state's. An unreadable index is not evidence of "no row" either, which
        // is why this compares an Option instead of absorbing the error into a 0.
        let h = chats.history(None, 50).await;
        let entry = h.iter().find(|e| e.id == empty.id.to_string());
        let key = entry.and_then(|e| e.session_key.clone());
        let indexed: Option<i64> = match key {
            Some(key) => db
                .call(move |conn| {
                    conn.query_row(
                        "SELECT COUNT(*) FROM sessions WHERE key = ?1",
                        rusqlite::params![key],
                        |r| r.get::<_, i64>(0),
                    )
                    .map_err(ruagent_store::DbError::from)
                })
                .await
                .ok()
                .and_then(|r| r.ok()),
            None => None,
        };
        assert_eq!(
            indexed,
            Some(0),
            "the index must not have caught up yet: this direction proves the count is \
             REAL STATE and not the 60 s index's, so a row here (or an index that could \
             not be read, which is not the same as 'no row') would leave the assertion \
             below vacuous"
        );

        let seen = entry.and_then(|e| e.message_count);
        assert_eq!(seen, Some(1), "the count must be real at once");
    }
}

#[cfg(test)]
mod t313_tests {
    use super::*;

    /// Unique per call: a clock-only name can repeat between parallel tests,
    /// and then two tests share one SQLite file (t313's sibling flake).
    fn t313_root(tag: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::env::temp_dir().join(format!("ruagent-t313-{tag}-{}-{n}", std::process::id()))
    }

    async fn distiller_without_agents() -> crate::distill::Distiller {
        let root = t313_root("distiller");
        std::fs::create_dir_all(&root).unwrap();
        crate::distill::Distiller {
            db: ruagent_store::Db::open(root.join("ruagent.db")).unwrap(),
            root,
            embedder: None,
            // Empty on purpose: the "real failure" side of this test is a
            // distiller with no agent to extract with.
            registry: crate::distill::AgentRegistry::default(),
            language: None,
            prompt_override: None,
            graph: false,
        }
    }

    /// The transcript shapes that reach auto-distill, measured rather than
    /// assumed: the live store holds 147 transcripts, 56 of them 1-3 lines and
    /// absent from the sessions index.
    #[test]
    fn a_chat_without_turns_has_nothing_to_distill() {
        let dir = t313_root("turns");
        std::fs::create_dir_all(&dir).unwrap();
        let closed = dir.join("run-closed.jsonl");
        std::fs::write(
            &closed,
            "{\"ts\":\"2026-09-26T14:00:00Z\",\"seq\":0,\"event\":{\"type\":\"session_start\"}}\n",
        )
        .unwrap();
        let missing = dir.join("run-missing.jsonl");
        let real = dir.join("run-real.jsonl");
        std::fs::write(
            &real,
            "{\"ts\":\"2026-09-26T14:00:01Z\",\"seq\":1,\"event\":{\"type\":\"user_message\",\"text\":\"hello\"}}\n{\"ts\":\"2026-09-26T14:00:02Z\",\"seq\":2,\"event\":{\"type\":\"agent_message_chunk\",\"text\":\"hi\"}}\n",
        )
        .unwrap();
        println!(
            "READING turns: session-start-only={} missing-file={} real-chat={}",
            transcript_turns(&closed),
            transcript_turns(&missing),
            transcript_turns(&real)
        );
        assert_eq!(transcript_turns(&closed), 0);
        assert_eq!(transcript_turns(&missing), 0);
        assert!(transcript_turns(&real) > 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Both directions of the decision: the no-op is not a failure, and a real
    /// failure still takes the visible path.
    #[tokio::test]
    async fn a_no_op_is_not_a_failure_and_a_real_one_still_fails() {
        let distiller = distiller_without_agents().await;
        // The plan is what the caller resolved from the plane; this test drives
        // the turn-count and agent-selection paths, so it passes today's
        // ACP-only plan.
        let plan = crate::extract_plane::ExtractPlan::acp_only();
        let quiet = auto_distill_now(&distiller, "ruagent:t313-empty", None, 0, plan).await;
        println!("READING turns=0 => {quiet:?}");
        assert_eq!(quiet, DistillAttempt::NothingToDistill);
        let real = auto_distill_now(&distiller, "ruagent:t313-real", None, 3, plan).await;
        println!("READING turns=3 with no enabled agent => {real:?}");
        assert_eq!(real, DistillAttempt::Failed);
    }

    // The captured-log variant of this test lived here. It was removed because
    // the capture could not be made reliable: tracing@'s subscriber is
    // process-wide, a sibling test installs it first, and a scoped subscriber did
    // not restore it -- so the buffer came back empty and the test went red about
    // half the time (3 green / 2 red over five runs). What it proved is kept
    // where it cannot flake: the two attempts above assert the DECISION, and the
    // level itself is a standalone reading recorded in the task output
    // (DEBUG nothing to distill for the no-op; WARN auto-distill failed for the
    // real failure).
}

/// t8: the UNATTENDED gate, driven through the real `maybe_auto_distill` on a
/// real ChatManager, in both directions, with `distill_log` as the observable.
///
/// WHY THE AGENT CARD POINTS AT A BINARY THAT DOES NOT EXIST: it is the
/// zero-token proof. If the ACP tier ran, the spawn would fail and the row would
/// be `failed` (that is exactly what case 4 asserts). Case 2 getting an `ok` row
/// therefore means the rules tier ran with no agent at all — zero tokens, no
/// child process, no network.
#[cfg(test)]
mod t8_tests {
    use super::*;

    /// One transcript turn pair, at the path `close` reads for a chat id. The
    /// sentence carries a PREF marker (`记住`/`以后`), so the deterministic tier
    /// has something to extract and the row is a real write, not an "empty".
    const TRANSCRIPT: &str = "{\"ts\":\"2026-09-26T14:00:01Z\",\"seq\":1,\"event\":{\"type\":\"user_message\",\"text\":\"记住：以后回答都要简洁，不要长篇大论。\"}}\n{\"ts\":\"2026-09-26T14:00:02Z\",\"seq\":2,\"event\":{\"type\":\"agent_message_chunk\",\"text\":\"understood\"}}\n";

    /// `(home, root)` for one test: `root` is the daemon's home
    /// (`<home>/.ruagent`), which is also the layout `SessionIndexer` scans
    /// (`<home>/.ruagent/data/transcripts`), so the transcript below is
    /// indexable exactly as a real one is.
    fn t8_root(tag: &str) -> (PathBuf, PathBuf) {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let home =
            std::env::temp_dir().join(format!("ruagent-t8-{tag}-{}-{n}", std::process::id()));
        let root = home.join(".ruagent");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("data").join("transcripts")).unwrap();
        std::fs::write(
            root.join("config").join("agents.toml"),
            // A real card, an imaginary command: the rules tier never spawns it,
            // the ACP tier cannot succeed with it.
            "[agent.t8mock]\nharness = \"mock\"\ncommand = \"definitely-not-a-real-t8-binary\"\ndescription = \"t8\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("config").join("mcp.toml"),
            "[profile.default]\nservers = []\n",
        )
        .unwrap();
        std::fs::write(
            root.join("config").join("policy.toml"),
            "[permissions]\ndefault = \"ask\"\n",
        )
        .unwrap();
        (home, root)
    }

    /// `(session_key, status, failure_reason)` per `distill_log` row, in write
    /// order. The reason rides along because "failed" without it is exactly the
    /// opaque reading this repo refuses to ship.
    async fn rows(db: &ruagent_store::Db) -> Vec<(String, String, String)> {
        db.call_flat(|conn| {
            let mut stmt =
                conn.prepare("SELECT session_key, status, COALESCE(failure_reason, '') FROM distill_log ORDER BY rowid")?;
            let out = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(out)
        })
        .await
        .expect("distill_log is readable")
    }

    async fn wait_for_rows(db: &ruagent_store::Db, want: usize) -> Vec<(String, String, String)> {
        for _ in 0..60 {
            let got = rows(db).await;
            if got.len() >= want {
                return got;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        rows(db).await
    }

    #[tokio::test]
    async fn the_unattended_plan_decides_and_the_default_spends_nothing() {
        let (home, root) = t8_root("unattended");
        let cfg = crate::config::DaemonConfig::load(&root).unwrap();
        let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
        let card = cfg.agent("t8mock").expect("the card").clone();
        let manager = ChatManager::new(
            db.clone(),
            root.clone(),
            Arc::new(|_, _| {}),
            cfg.mcp.clone(),
            // TODAY: `[distill].auto` defaults to false.
            crate::distill::AutoDistill {
                auto: false,
                graph: false,
                ..Default::default()
            },
            None,
            crate::distill::AgentRegistry {
                enabled: vec![card],
            },
        );
        let id = RunId::generate();
        let transcript = transcript_path(root.join("data").join("transcripts"), &id);
        std::fs::write(&transcript, TRANSCRIPT).unwrap();
        // The distiller reads the SESSION INDEX, not the file: index it the way
        // boot's scanner does, or every pass fails with "Query returned no rows".
        crate::sessions::SessionIndexer::new(db.clone(), home.clone())
            .scan()
            .await
            .expect("the transcript indexes");
        assert!(
            transcript_turns(&transcript) > 0,
            "the fixture must have turns, or case 1 would pass for the wrong reason"
        );

        // (1) THE DEFAULT — legacy plane, auto = false: nothing runs at all,
        //     not even a failed attempt (a failed attempt WOULD log a row).
        manager.maybe_auto_distill(id);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            rows(&db).await.is_empty(),
            "the default configuration must distill nothing: {:?}",
            rows(&db).await
        );

        // (2) THE FREE TIER, opted in: `session_extract_rules` alone runs the
        //     deterministic extractor. The row is `ok` although the card's
        //     command cannot be spawned, so this pass spent zero tokens.
        manager.set_capabilities(
            crate::capability::CapabilityPlane::from_policy(
                &ruagent_policy::PolicyConfig::parse(
                    "[capabilities.session_extract_rules]\nenabled = true\n",
                )
                .unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(manager.unattended_plan().label(), "rules");
        manager.maybe_auto_distill(id);
        let got = wait_for_rows(&db, 1).await;
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].1, "ok", "a rules pass needs no agent: {got:?}");
        // (3) OFF AGAIN — the same close writes nothing new: the capability has
        //     an observable effect in BOTH directions.
        manager.set_capabilities(crate::capability::CapabilityPlane::legacy());
        assert!(manager.unattended_plan().is_empty());
        manager.maybe_auto_distill(id);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(rows(&db).await.len(), 1, "off means no new row");

        // (4) THE LLM TIER IS THE ONLY ACP TRIGGER: with `[distill].auto = true`
        //     and no table, the ACP turn IS attempted (and fails against a
        //     command that does not exist) — the paid path is reachable and
        //     gated by the legacy flag, never by the free tier.
        manager.set_distill_policy(crate::distill::AutoDistill {
            auto: true,
            graph: false,
            ..Default::default()
        });
        assert_eq!(manager.unattended_plan().label(), "acp");
        manager.maybe_auto_distill(id);
        let got = wait_for_rows(&db, 2).await;
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[1].1, "failed", "the ACP tier was attempted: {got:?}");
        std::fs::remove_dir_all(&home).ok();
    }

    /// The other free-tier capability this file owns the gate for: the chat
    /// injection. The DECISION is `plane.gate(MemoryInjectChat, true)` and it is
    /// read inside `send_prompt`; the end-to-end proof (a real chat whose
    /// `context_injected` event carries the memory marker with the capability on
    /// and not with it off) lives in the t8 live probe and in
    /// `tests/injection_e2e.rs`, which needs the mock-agent binary.
    #[test]
    fn the_chat_injection_gate_reads_the_plane_in_both_directions() {
        use crate::capability::{CapabilityId, CapabilityPlane};
        let legacy = CapabilityPlane::legacy();
        assert!(legacy.gate(CapabilityId::MemoryInjectChat, true));
        let off = CapabilityPlane::from_policy(
            &ruagent_policy::PolicyConfig::parse(
                "[capabilities.memory_inject_chat]\nenabled = false\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!off.gate(CapabilityId::MemoryInjectChat, true));
        assert_eq!(off.configured(CapabilityId::MemoryInjectChat), "file");
        // The gate is per-capability: switching injection off does not touch the
        // run path's own gate.
        assert!(off.gate(CapabilityId::MemoryInjectRuns, true));
    }

    /// `memories` row count, so "the free tier WROTE something" is a reading and
    /// not an inference from a status string.
    async fn memories_count(db: &ruagent_store::Db) -> i64 {
        db.call_flat(|conn| conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)))
            .await
            .expect("the memories table is readable")
    }

    /// The one key the indexer produced for the fixture above.
    async fn only_session_key(db: &ruagent_store::Db) -> String {
        db.call_flat(|conn| conn.query_row("SELECT key FROM sessions LIMIT 1", [], |r| r.get(0)))
            .await
            .expect("the fixture is indexed")
    }

    /// t14: THE FREE TIER NEEDS NO AGENT AT ALL — not merely "does not spawn the
    /// one it was handed".
    ///
    /// The t8 test above proved a rules pass does not SPAWN its card (the card's
    /// command does not exist and the row is still `ok`). That is a different
    /// claim from the one the feature makes: the zero-token tier exists for the
    /// machine with NO agents configured and no API key, so the shape that matters
    /// is an EMPTY ENABLED SET. With the agent selected before the plan, exactly
    /// that machine got `no enabled agent available` — and the auto path did not
    /// even reach the extractor.
    #[tokio::test]
    async fn the_rules_pass_writes_with_every_agent_disabled() {
        let (home, root) = t8_root("noagent");
        let cfg = crate::config::DaemonConfig::load(&root).unwrap();
        let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
        let manager = ChatManager::new(
            db.clone(),
            root.clone(),
            Arc::new(|_, _| {}),
            cfg.mcp.clone(),
            crate::distill::AutoDistill {
                auto: false,
                graph: false,
                ..Default::default()
            },
            None,
            // EVERY AGENT `enabled = false`: this is the registry the daemon
            // builds, and it is empty.
            crate::distill::AgentRegistry::default(),
        );
        assert!(
            manager.registry.list_enabled().is_empty(),
            "the premise of this test: not one enabled agent"
        );

        let id = RunId::generate();
        let transcript = transcript_path(root.join("data").join("transcripts"), &id);
        std::fs::write(&transcript, TRANSCRIPT).unwrap();
        crate::sessions::SessionIndexer::new(db.clone(), home.clone())
            .scan()
            .await
            .expect("the transcript indexes");
        let key = only_session_key(&db).await;

        manager.set_capabilities(
            crate::capability::CapabilityPlane::from_policy(
                &ruagent_policy::PolicyConfig::parse(
                    "[capabilities.session_extract_rules]\nenabled = true\n",
                )
                .unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(manager.unattended_plan().label(), "rules");

        manager.maybe_auto_distill(id);
        let got = wait_for_rows(&db, 1).await;
        assert_eq!(got.len(), 1, "the pass must record one attempt: {got:?}");
        assert_eq!(
            got[0].1, "ok",
            "a rules pass must SUCCEED with no enabled agent: {got:?}"
        );
        assert!(
            !got.iter().any(|r| r.1 == "failed"),
            "no failed row may appear: {got:?}"
        );
        assert!(
            memories_count(&db).await > 0,
            "the free tier must WRITE a memory row, not just log an attempt"
        );

        // THE OTHER HALF, AND IT MUST NOT REGRESS: a plan that DOES need a model
        // still fails, visibly, on the same empty registry. Lazy resolution is
        // not "no agent is fine everywhere".
        let distiller = crate::distill::Distiller {
            db: db.clone(),
            root: root.clone(),
            embedder: None,
            registry: crate::distill::AgentRegistry::default(),
            language: None,
            prompt_override: None,
            graph: false,
        };
        let before = memories_count(&db).await;
        let attempt = auto_distill_now(
            &distiller,
            &key,
            None,
            1,
            crate::extract_plane::ExtractPlan::acp_only(),
        )
        .await;
        assert_eq!(
            attempt,
            DistillAttempt::Failed,
            "an ACP-only plan with no enabled agent must still fail visibly"
        );
        assert_eq!(
            memories_count(&db).await,
            before,
            "a plan that needs an agent must not write anything when it has none"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}
