//! Session distillation: turn a conversation into durable memory,
//! graph entities and relations — the OpenViking pattern (extract →
//! dedup → write), executed by one of the configured agent runtimes
//! through our own ACP machinery. See
//! docs/plans/2026-09-14-distillation-design.md.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use ruagent_acp::adapter::HarnessAdapter as _;
use ruagent_acp::chat::{ChatCommand, ChatOptions, ChatSession};
use ruagent_core::RunEvent;
use ruagent_store::Db;
use serde::Deserialize;

const EXTRACTION_PROMPT: &str = r#"You are a memory distillation engine. Analyze the agent session transcript below and extract DURABLE knowledge — things worth remembering weeks from now. Ignore transient details (file paths being edited, one-off commands, small talk).

Answer-quality signals — judge BEFORE extracting an agent statement as memory:
- If the user CORRECTED the agent ("不对", "no, actually", "应该是…"), extract the corrected fact from the user's message, NOT the agent's wrong answer.
- If the user explicitly confirmed ("对", "就是这样", "perfect"), raise confidence to 1.0.
- If the agent hedged ("可能", "I think", "not sure"), lower confidence to 0.5 or skip entirely.
- An answer the user silently accepted (moved on to a new topic) is normal confidence (0.8).

Attribution rule — every transcript line is labelled with its role (`User:` / `Agent:`):
- An `Agent:` line is the AGENT's own words. It is NOT evidence of what the user said, asked for, preferred or confirmed. Never turn an `Agent:` line into a `profile`/`user` memory ("durable facts about the user"), and never write a memory whose content asserts a user preference, instruction or confirmation that only an `Agent:` line states — even when the agent phrased it as a promise or a standing instruction ("from now on …", "以后都…").
- An `Agent:`-only statement may be extracted as a user fact ONLY when a `User:` line in the SAME transcript states that fact; then the memory comes from the `User:` line, and the agent's wording is context at most.
- Without such a `User:` line, a durable agent-only statement may be recorded as `observation` (never `profile`/`user`), and an agent's own suggestion, plan or offer must be skipped entirely.
- A transcript with no `User:` line at all can therefore never yield a `profile`/`user` memory.

Extract:
1. memories: durable facts about the user (profile), observations, procedures (how-to knowledge), and lessons learned. Only things that generalize beyond this single session. Each memory carries a confidence (0.5–1.0) from the signals above.
2. entities: real-world objects mentioned (people, projects, tools, organizations, products). Entity identity = the real-world object, NOT its category. Merge only explicit aliases of the same object. Also list every OTHER spelling you saw for that same object in `aliases` (an abbreviation, its expansion, a Chinese/English variant) — that list is what lets two mentions of one object resolve to one node later.
3. relations: durable facts between entities (src/dst by entity name). Set `valid_at` to the RFC3339 instant the fact became true ONLY when the transcript states or implies that time. If the transcript gives no time, `valid_at` MUST be null. NEVER put the current time there: null means "no event time is known", while the current clock means "this became true now", and those are different facts.

Respond with ONLY a JSON object, no markdown fences, no commentary:
{
  "memories": [{"store": "profile|observation|procedure|lesson", "namespace": "user|global|project:<name>", "content": "...", "confidence": 0.8}],
  "entities": [{"name": "...", "kind": "person|project|tool|org|product|concept", "summary": "one line", "aliases": ["..."]}],
  "relations": [{"src": "...", "dst": "...", "relation": "snake_case", "fact": "one sentence", "valid_at": "RFC3339 timestamp when the transcript says when it became true, else null"}]
}
Empty arrays are valid. Quality over quantity.
"#;

/// The extraction agent's wire shape. UNCHANGED by the extractor seam (t4):
/// the prompt and the JSON contract are the same, and `extract_plane` maps this
/// onto the seam's candidates. `pub(crate)` so that mapping lives next to the
/// seam instead of here.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Extraction {
    #[serde(default)]
    pub(crate) memories: Vec<ExtractedMemory>,
    #[serde(default)]
    pub(crate) entities: Vec<ExtractedEntity>,
    #[serde(default)]
    pub(crate) relations: Vec<ExtractedRelation>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExtractedMemory {
    pub(crate) store: String,
    pub(crate) namespace: String,
    pub(crate) content: String,
    /// 0.5–1.0, from answer-quality signals in the transcript.
    #[serde(default)]
    pub(crate) confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExtractedEntity {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) summary: Option<String>,
    /// Other spellings of the SAME real-world object. They are written into
    /// `entity_aliases` so a later query for any of them resolves to one node
    /// (gen2: the graph had no alias table, and 4 pairs of live rows were the
    /// same object under two names).
    #[serde(default)]
    pub(crate) aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExtractedRelation {
    pub(crate) src: String,
    pub(crate) dst: String,
    pub(crate) relation: String,
    pub(crate) fact: String,
    /// T: the EVENT time, when the transcript stated one. `None` means "no event
    /// time is known" and is written as a NULL valid_at source, never as now()
    /// (measured before gen2: 66 of 67 live edges had valid_at == created_at, so
    /// the event-time axis carried no information at all).
    #[serde(default)]
    pub(crate) valid_at: Option<String>,
}

/// What a memory write pass did, including the episode it attached the writes to.
///
/// The episode id is returned (not just used internally) because the graph edges
/// written in the same distillation point at the SAME episode: one session's raw
/// material, two derived stores. Named fields, not a tuple, so a reader of a call
/// site cannot mistake one count for another.
#[derive(Debug, Clone, PartialEq)]
struct MemoryWriteOutcome {
    written: u32,
    skipped: u32,
    episode: Option<i64>,
}

/// The graph half's own reading of one write (ruagent-issues-45-55 t5 / #55).
///
/// It carries what the WRITER reported, not a recomputation of it: the write-side
/// dedupe (G7) is the thing that knows a relation was already stated, and
/// `refused` counts the names that state no relation at all. Before this, those
/// two numbers were logged at INFO and thrown away, so "we dropped 8 of your 20
/// relations" was unanswerable from the database.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct GraphOutcome {
    entities: u32,
    relations: u32,
    /// Relations the write-side dedupe recognised as already stated.
    duplicates: u32,
    /// Relations refused because the name does not state a relation.
    refused: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DistillOutcome {
    pub session_key: String,
    pub memories_written: u32,
    pub memories_skipped: u32,
    pub entities_written: u32,
    pub relations_written: u32,
    /// The agent this pass used. EMPTY when none did — a rules-only extraction
    /// (t14) never touches the registry, so naming an agent here would claim a
    /// spawn that did not happen; `source` says which tier produced the row.
    pub agent: String,
    /// WHICH implementation produced this outcome (t4's seam): `"rules"` (zero
    /// tokens), `"acp"` (one chat turn) or `"rules+acp"`. Additive on the wire —
    /// the panel reads the six keys above and ignores this one — and it is the
    /// only way a reader can tell a free extraction from a paid one.
    pub source: String,
    /// True when an input was cut to its bound: "we stopped reading" is a
    /// different fact from "there was nothing there".
    pub truncated: bool,
    /// How many RETRIES this attempt needed (0 = the first try settled it).
    ///
    /// A READING, not a maximum: `0` says the first attempt is the one this row
    /// records, which is a different story from a row that took three
    /// (ruagent-issues-45-55 t4 / issue #45). This is the value the ledger keeps
    /// in `distill_log.retries` (0028) -- 0 measured, not 0 by omission.
    pub retries: u32,
    /// How many EXTRACTION ROUNDS this pass ran (issue #48): 1 = the whole
    /// transcript fitted one input, more = it was cut into windows at turn
    /// boundaries and every window was extracted. `None` = the pass never read
    /// anything (a refusal, a failure, a dry run that never got that far), which
    /// the ledger writes as NULL -- "not measured" is not "measured as one".
    pub windows: Option<u32>,
    /// WHICH MESSAGES THE PASS READ, and under which byte budget (issue #48). The
    /// same measurement `windows` counts, in the only form that can show a LOSS:
    /// a session that was WINDOWED is still read end to end, while a message too
    /// large for one window is CUT, and then `truncated` says text was dropped.
    ///
    /// WHY THE RANGE IS NOW IN `distill_log` (0030; t3 / issue #58): this field
    /// used to be the ONLY place the range existed, on the grounds that 0028's
    /// column set was fixed by the schema task and the range could ride the
    /// response. That left it unreadable the moment the response was gone --
    /// `windows = 3, truncated = 0` in the ledger cannot be checked against a
    /// session, and "which part of this session did that attempt read?" had no
    /// answer anywhere. 0030 adds `from_seq`/`to_seq`/`total_messages` for exactly
    /// this measurement: the three are written TOGETHER, under this one `Option`,
    /// when a pass measured a range -- and stay NULL when it did not.
    pub coverage: Option<TranscriptCoverage>,
    /// Tokens the agent reported in context, and the window it reported them
    /// against (`ContextUsage.used`/`.size`). `None` = no agent reported any
    /// usage -- a rules-only pass, or a provider that says nothing -- which the
    /// ledger writes as NULL, never as 0 (issue #55).
    pub tokens_used: Option<i64>,
    pub context_size: Option<i64>,
    /// The cumulative cost the agent last reported, in USD. `None` = not
    /// reported, not "free".
    pub cost_usd: Option<f64>,
    /// Relations the graph recognised as already stated, and relations it refused
    /// (issue #55). `None` = the graph half did not run (`[distill] graph = false`),
    /// which is not the same reading as a measured 0.
    pub relations_duplicate: Option<u32>,
    pub relations_refused: Option<u32>,
    /// True for a `dry_run`: the extraction ran and the counts are what WOULD be
    /// written. A dry run writes nothing at all — no memory, no graph row, no
    /// episode and no `distill_log` row — which is what makes it safe to call at
    /// any time (§14.3). It is a QUERY about the extraction, not an attempt.
    pub dry_run: bool,
}

impl Default for DistillOutcome {
    fn default() -> Self {
        Self {
            session_key: String::new(),
            memories_written: 0,
            memories_skipped: 0,
            entities_written: 0,
            relations_written: 0,
            agent: String::new(),
            // Today's only implementation, and the one every pre-seam caller
            // asked for; a plan that runs something else says so explicitly.
            source: crate::extract_plane::ExtractSource::Acp
                .as_str()
                .to_string(),
            truncated: false,
            // The first attempt is the one a caller who does not say otherwise
            // is describing; `pass_with_retries` overwrites it with the count it
            // actually took.
            retries: 0,
            // Everything below is "not measured" until a pass measures it: a
            // refusal or a failure must not report a window count, a coverage
            // range or a token spend it never observed.
            windows: None,
            coverage: None,
            tokens_used: None,
            context_size: None,
            cost_usd: None,
            relations_duplicate: None,
            relations_refused: None,
            dry_run: false,
        }
    }
}

/// Which part of a session one pass read (issue #48).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct TranscriptCoverage {
    /// The covered message indices: `from_message` inclusive, `to_message`
    /// exclusive, in the list the pass actually read (the user-text filter runs
    /// first, so this counts what the extractor saw).
    pub from_message: usize,
    pub to_message: usize,
    /// How many messages that list had — the denominator of the range above.
    pub total_messages: usize,
    /// Bytes of rendered transcript handed to the extractor, summed over the
    /// windows.
    pub bytes: usize,
    /// The per-window byte budget the windows were packed under.
    pub budget_bytes: usize,
}

/// ONE window: a range of messages, its rendered text, and whether that text had
/// to be CUT to fit the budget (issue #48).
#[derive(Debug, Clone, PartialEq)]
struct TranscriptWindow {
    range: std::ops::Range<usize>,
    text: String,
    /// True when `text` is a PREFIX of the range's rendering: the text did not fit
    /// the budget, so the tail was dropped. "We stopped reading" is a different
    /// fact from "there was nothing there", and `distill_log.truncated` records it.
    truncated: bool,
}

/// The [distill] policy from policy.toml. The graph flag arrives
/// resolved (None in the file = true): every consumer wants a bool.
#[derive(Debug, Clone)]
pub struct AutoDistill {
    pub auto: bool,
    pub agent: Option<String>,
    pub language: Option<String>,
    pub prompt: Option<String>,
    /// Also extract entities/relations into the graph.
    pub graph: bool,
}

impl Default for AutoDistill {
    fn default() -> Self {
        Self {
            auto: false,
            agent: None,
            language: None,
            prompt: None,
            graph: true,
        }
    }
}

/// Distill `session_key` choosing the extraction agent from the
/// registry: the policy's agent, else dsh, else the first enabled.
pub async fn distill_with_agent(
    distiller: &Distiller,
    session_key: &str,
    preferred: Option<&str>,
) -> Result<DistillOutcome> {
    let agents = distiller.registry.list_enabled();
    let card = select_agent(&agents, preferred)?;
    distiller.distill(session_key, card).await
}

/// Pick the agent for an unattended platform job: the preferred name
/// if enabled, else dsh, else the first enabled.
pub fn select_agent<'a>(
    agents: &'a [ruagent_core::AgentCard],
    preferred: Option<&str>,
) -> Result<&'a ruagent_core::AgentCard> {
    agents
        .iter()
        .find(|a| Some(a.name.as_str()) == preferred)
        .or_else(|| agents.iter().find(|a| a.name == "dsh"))
        .or_else(|| agents.first())
        .ok_or_else(|| anyhow::anyhow!("no enabled agent available"))
}

/// What this entry is ALLOWED to say when a pass produced nothing
/// (ruagent-issues-45-55 t4 / issue #45).
///
/// These two sentences ARE the user-visible text. Everything else -- the table
/// name, the statement, rusqlite's own message -- stays in the daemon log, which
/// is where a reader who can act on it looks. Measured before this: a chat with
/// three turns whose index row had not been written yet failed with
/// `Query returned no rows` -- rusqlite's message for a missing `query_row`,
/// naming a table the user has never heard of and an action nobody can take. It
/// was logged as a distillation failure 21 times in one window, with 0 real
/// errors behind it (t313).
pub const NOT_INDEXED_MESSAGE: &str = "该会话尚未进入索引，正在同步";
pub const NO_CONTENT_MESSAGE: &str = "没有可蒸馏的内容";

/// WHY a distillation produced nothing, in terms the caller can act on.
///
/// An ANSWER, not a defect -- which is why it is a TYPE and not a string built at
/// the failure site: the ledger's status (0024's three states) and the message a
/// route returns both derive from the KIND, so "there was nothing to distil"
/// cannot be recorded as a failure and a real failure cannot be recorded as an
/// empty session.
#[derive(Debug)]
pub enum DistillRefusal {
    /// The session was READ and there is no conversation in it: no messages at
    /// all, or nothing but turns the daemon marked as its own (an injected context
    /// block, or one of our own job prompts).
    ///
    /// WHAT IT DOES NOT COVER (t23, finding t14-F1): an ASSISTANT-ONLY session is
    /// not this refusal -- `user_turns` keeps the agent's turns, so that session is
    /// distilled and answers `status = empty`. See the note at the refusal site in
    /// `distill_once` for the open product question.
    NoContent,
    /// The session is not in the index even after a fresh pass: it has not been
    /// written yet (the common case right after a chat closes), or no indexed
    /// family holds it.
    NotIndexed,
}

impl DistillRefusal {
    /// The ledger STATUS for this answer (0024's three states).
    ///
    /// `empty` for [`DistillRefusal::NoContent`]: the pass read the session and
    /// there was nothing in it -- the state 0020/0024 added the column for.
    ///
    /// `failed` for [`DistillRefusal::NotIndexed`]: NOTHING WAS READ, so this is
    /// not a statement about the session's content. Recording it as `empty` would
    /// say a session holds nothing when the truth is that we could not look, and a
    /// batch pass asking "which sessions still need distilling?" would then skip
    /// it for ever.
    pub fn status(&self) -> &'static str {
        match self {
            DistillRefusal::NoContent => "empty",
            DistillRefusal::NotIndexed => "failed",
        }
    }

    /// `NoContent`, with the DIAGNOSIS sent to the daemon log.
    ///
    /// The cause is deliberately NOT part of the message: which turns were dropped
    /// is our input handling, and mixing that into the sentence a user reads is
    /// the habit that put the SQL text there in the first place.
    fn no_content(session_key: &str, cause: &str) -> anyhow::Error {
        tracing::debug!(session = %session_key, cause = %cause, "nothing to distil");
        anyhow::Error::new(DistillRefusal::NoContent)
    }

    /// `NotIndexed`, with the session key in the daemon log.
    fn not_indexed(session_key: &str) -> anyhow::Error {
        tracing::debug!(session = %session_key, "the session is not in the index");
        anyhow::Error::new(DistillRefusal::NotIndexed)
    }
}

impl std::fmt::Display for DistillRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistillRefusal::NoContent => f.write_str(NO_CONTENT_MESSAGE),
            DistillRefusal::NotIndexed => f.write_str(NOT_INDEXED_MESSAGE),
        }
    }
}

impl std::error::Error for DistillRefusal {}

/// What one ACP turn came back with: the reply text, plus whatever the agent said
/// about its own context window and cost (ruagent-issues-45-55 t5 / issue #55).
pub(crate) struct AgentReply {
    pub text: String,
    pub usage: crate::extract_plane::UsageReport,
}

/// The event fold of ONE ACP turn: text from `AgentMessageChunk`, usage from
/// `UsageUpdate`, and the two terminal events.
///
/// IT IS A STRUCT WITH A TEST rather than a loop inside the spawned task, because
/// this is the half of `ask_agent` that can be checked without spawning an agent:
/// `entry_tests::the_usage_fold_is_the_one_runs_rs_uses` feeds it the same event
/// sequence `runs.rs` folds and asserts the same numbers the mock agent emits.
/// The task itself is then only a `recv` loop, which is not where the defects live.
#[derive(Default)]
struct ReplyCollector {
    text: String,
    usage: crate::extract_plane::UsageReport,
    /// The last `Error` the stream reported, if any.
    error: Option<String>,
}

impl ReplyCollector {
    /// Fold ONE event in. `false` means the turn is over: stop reading.
    fn observe(&mut self, event: &RunEvent) -> bool {
        match event {
            RunEvent::AgentMessageChunk { content } => {
                for block in content {
                    if let ruagent_core::ContentBlock::Text { text } = block {
                        self.text.push_str(text);
                    }
                }
            }
            // THE USAGE HALF (issue #55), folded by `UsageReport::observe` — the ONE
            // place the runs.rs rules (`runs.rs:1494-1501`) are expressed on this
            // path.
            RunEvent::UsageUpdate { usage } => self.usage.observe(*usage),
            RunEvent::Stopped { .. } => return false,
            RunEvent::Error { message } => {
                self.error = Some(message.clone());
                return false;
            }
            _ => {}
        }
        true
    }
}

/// How long ONE fresh index pass may take before a distillation gives up on it.
///
/// That pass is what closes the 60 s index gap (issue #45), so it sits on the
/// critical path of a chat that was just closed -- but a filesystem walk must not
/// be able to hold an unattended spawn open indefinitely. Ten seconds is orders of
/// magnitude above the cost (one stat per candidate file, a parse only for the
/// ones that moved); reaching it means the machine, not the work.
const INDEX_WAIT: Duration = Duration::from_secs(10);

/// How many RETRIES one pass gets (so: up to three attempts), and the wait before
/// the first of them. The waits are 400 ms then 800 ms -- increasing (so a
/// systematic fault is not hammered) and short enough that a doomed pass is over
/// in about a second.
const RETRY_LIMIT: u32 = 2;
const RETRY_FIRST_WAIT: Duration = Duration::from_millis(400);

/// The wait before retry number `retry` (1-based): 400 ms, then 800 ms.
fn retry_wait(retry: u32) -> Duration {
    RETRY_FIRST_WAIT * 2u32.pow(retry.saturating_sub(1))
}

/// Whether a failed attempt gets another one.
///
/// `NoContent` does NOT: it is a fact about the session, read once, and re-reading
/// the same empty conversation cannot change it. Everything else does, including
/// `NotIndexed` -- that one exists because the index is a 60 s cache and the file
/// may land between two attempts.
fn is_retryable(e: &anyhow::Error) -> bool {
    !matches!(
        e.downcast_ref::<DistillRefusal>(),
        Some(DistillRefusal::NoContent)
    )
}

/// Run one fallible attempt, retrying it up to [`RETRY_LIMIT`] times, and report
/// the result together with how many retries it took.
///
/// THE LOOP IS A FUNCTION OF ITS OWN (ruagent-issues-45-55 t4) so that the policy
/// can be tested without a database, a clock or a filesystem: the test that
/// proves "a failure that clears is carried over, a refusal is not retried, and a
/// persistent failure stops at the bound" drives THIS code rather than a copy of
/// it. `subject` is the session key the retry log names.
async fn with_retries<F, Fut, T>(subject: &str, mut attempt: F) -> (Result<T>, u32)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut retries = 0u32;
    loop {
        match attempt().await {
            Ok(ok) => return (Ok(ok), retries),
            Err(e) if retries < RETRY_LIMIT && is_retryable(&e) => {
                retries += 1;
                let wait = retry_wait(retries);
                tracing::warn!(
                    session = %subject,
                    attempt = retries + 1,
                    wait_ms = wait.as_millis() as u64,
                    error = %e,
                    "distillation attempt failed; retrying"
                );
                tokio::time::sleep(wait).await;
            }
            Err(e) => return (Err(e), retries),
        }
    }
}

#[derive(Clone)]
pub struct Distiller {
    pub db: Db,
    pub root: PathBuf, // ruagent home (~/.ruagent)
    /// The shared embedder (knowledge base's) for memory vectors; None
    /// when only the hash fallback is active.
    pub embedder: Option<std::sync::Arc<dyn ruagent_knowledge::embed::Embedder>>,
    /// Agent registry for extraction-agent choice (auto-distill path).
    pub registry: AgentRegistry,
    /// Output language for distilled content, from `[distill] language`.
    pub language: Option<String>,
    /// Full prompt override, from `[distill] prompt`.
    pub prompt_override: Option<String>,
    /// Also write entities/relations into the graph; false = memories
    /// only, from `[distill] graph` (default true).
    pub graph: bool,
}

/// Minimal registry view the distiller needs (no RunManager cycle).
#[derive(Clone, Default)]
pub struct AgentRegistry {
    pub enabled: Vec<ruagent_core::AgentCard>,
}

impl AgentRegistry {
    pub fn list_enabled(&self) -> Vec<ruagent_core::AgentCard> {
        self.enabled.clone()
    }
}

impl Distiller {
    /// The full extraction prompt: base (built-in or `[distill] prompt`
    /// override) + the optional language clause + the transcript tail.
    ///
    /// `pub(crate)` because the ACP half of the extractor seam lives in
    /// `crate::extract_plane` (t4); the prompt itself is unchanged.
    pub(crate) fn compose_prompt(&self) -> String {
        extraction_prompt(self.language.as_deref(), self.prompt_override.as_deref())
    }

    /// Distill one session with TODAY'S single implementation: one ACP chat turn.
    ///
    /// Every pre-seam caller (auto-distill on session close, the manual route
    /// with no body) goes through here, and through `distill_plan` with
    /// `ExtractPlan::acp_only()`, so its behaviour is exactly what it was.
    pub async fn distill(
        &self,
        session_key: &str,
        card: &ruagent_core::AgentCard,
    ) -> Result<DistillOutcome> {
        self.distill_plan(
            session_key,
            Some(card),
            crate::extract_plane::ExtractPlan::acp_only(),
            false,
        )
        .await
    }

    /// Distill one session through the extractor seam with the plan the CALLER
    /// resolved from the capability plane (t4).
    ///
    /// The plan is data, so this function does not branch on which
    /// implementation runs — that is the whole point of the seam. `dry_run`
    /// extracts and reports without writing anything anywhere (no memory, no
    /// graph row, no episode, no `distill_log` row): it prices the extraction.
    ///
    /// THE CARD IS OPTIONAL, AND `None` IS NOT A DEGRADED MODE (t14): only a
    /// plan that includes the ACP tier has anything to do with an agent, so a
    /// rules-only plan is handed `None` and never looks at the registry. Before
    /// t14 every caller selected an agent FIRST, which made the free tier
    /// unusable in the exact environment it exists for — no agents configured,
    /// no API key — and answered `no enabled agent available` for a pass that
    /// would not have spawned anything. A plan that DOES enable the ACP tier
    /// with `None` is a programming error and is refused by name, never
    /// silently degraded to the free tier (see `extract_plane::extract`).
    pub async fn distill_plan(
        &self,
        session_key: &str,
        card: Option<&ruagent_core::AgentCard>,
        plan: crate::extract_plane::ExtractPlan,
        dry_run: bool,
    ) -> Result<DistillOutcome> {
        let prompt_hash = ruagent_memory::write::content_hash(&self.compose_prompt());
        let (result, retries) = self
            .pass_with_retries(session_key, card, plan, dry_run)
            .await;
        match result {
            Ok((mut outcome, status)) => {
                // A READING, NOT A MAXIMUM: how many retries this pass needed (0
                // when the first attempt settled it). `0028`'s `retries` column is
                // NULL only for rows written before the retry path existed.
                outcome.retries = retries;
                if !dry_run {
                    self.log_outcome(&outcome, status, &prompt_hash, None)
                        .await?;
                }
                Ok(outcome)
            }
            Err(e) => {
                // A DRY RUN WRITES NOTHING, FAILURES INCLUDED: it prices an
                // extraction, and a priced pass that produced no row is what makes
                // the route safe to call at any time (§14.3). Preserved across the
                // retry loop deliberately -- the loop changes how many times we
                // try, not what a dry run leaves behind.
                if !dry_run {
                    // TWO KINDS OF "DID NOT WORK", AND THE LEDGER TELLS THEM
                    // APART (issue #45). A REFUSAL is one of our own answers, with
                    // a sentence written for whoever asked; anything else is a
                    // defect and keeps its own cause, which is what an operator
                    // acts on. Neither may be the storage layer's words.
                    let reason = e.to_string();
                    let status = match e.downcast_ref::<DistillRefusal>() {
                        Some(refusal) => refusal.status(),
                        None => "failed",
                    };
                    // A failure MUST NOT create an episode: the panel's
                    // "distilled" boolean is derived from episodes.kind =
                    // 'run_turn' (t350), so a failure row that materialised one
                    // would claim a distillation that did not happen. This row is
                    // the ledger's, and `..default()` keeps it empty of counts.
                    if let Err(log_err) = self
                        .log_outcome(
                            &DistillOutcome {
                                session_key: session_key.to_string(),
                                // Empty when no card was resolved: a rules-only pass
                                // has no agent to name, and inventing one would be a
                                // worse lie than the absence. `source` says which
                                // tier produced the row.
                                agent: card.map(|c| c.name.clone()).unwrap_or_default(),
                                source: plan.label().to_string(),
                                retries,
                                ..DistillOutcome::default()
                            },
                            status,
                            &prompt_hash,
                            Some(&reason),
                        )
                        .await
                    {
                        tracing::warn!(
                            session = %session_key, error = %log_err,
                            "could not record the distillation failure"
                        );
                    }
                }
                Err(e)
            }
        }
    }

    /// ONE pass, with BOUNDED BACKOFF RETRIES (ruagent-issues-45-55 t4 / #45).
    ///
    /// Returns the pass's result and the number of RETRIES it took: `0` means the
    /// first attempt settled it, whether it succeeded or was refused.
    ///
    /// WHY RETRY AT ALL, AND WHY ONLY TWICE. The failures this path actually meets
    /// are RACES, not judgements: the session file lands a moment after the chat
    /// closes and the index pass inside `load_messages` has not seen it yet, or
    /// the single writer's lock is briefly held. A single attempt turns every one
    /// of those into a recorded failure. Bounded, because the caller is an
    /// unattended spawn or an HTTP request and an unbounded loop would hold it
    /// open for as long as the fault lasts.
    ///
    /// WHICH FAILURES GET ANOTHER ATTEMPT is per-kind, and this is the reason the
    /// refusal is a type:
    ///   * `NoContent` is TERMINAL. Three attempts read the same empty
    ///     conversation, and a `retries` of 2 on that row would claim a transient
    ///     fault that never existed -- the "do not fabricate a reading" rule
    ///     0028's columns are built on.
    ///   * `NotIndexed` IS retried, because its cure is the index pass each
    ///     attempt re-runs: this is the race the loop exists for.
    async fn pass_with_retries(
        &self,
        session_key: &str,
        card: Option<&ruagent_core::AgentCard>,
        plan: crate::extract_plane::ExtractPlan,
        dry_run: bool,
    ) -> (Result<(DistillOutcome, &'static str)>, u32) {
        // The loop itself lives in [`with_retries`], which is where the policy is
        // tested; all this adds is which pass it retries.
        with_retries(session_key, || {
            self.distill_once(session_key, card, plan, dry_run)
        })
        .await
    }

    /// The body of one distillation, split out so `distill_plan` can record the
    /// outcome of every path through it (including the error path).
    ///
    /// THE SEAM IS HERE: this function builds the context, hands it to
    /// `crate::extract_plane::extract` together with the plan, and writes the
    /// resulting bundle. It never asks which implementation ran — a
    /// rule-derived candidate and an LLM-derived one are the same three
    /// vectors by the time they arrive.
    async fn distill_once(
        &self,
        session_key: &str,
        card: Option<&ruagent_core::AgentCard>,
        plan: crate::extract_plane::ExtractPlan,
        dry_run: bool,
    ) -> Result<(DistillOutcome, &'static str)> {
        let messages = self.load_messages(session_key).await?;
        // WHAT IS DISTILLED IS THE USER'S OWN WORDS (t21). One filter, before BOTH
        // tiers: the rules tier used to drop platform turns inside the extraction
        // plane while the ACP tier rendered them into the prompt it asked the model to
        // summarise, so the two tiers disagreed about whose words these are. A session
        // that has nothing of the user's in it is refused with the reason instead of
        // being distilled into memories attributed to a session the user never spoke in.
        //
        // WHAT THAT FILTER ACTUALLY DROPS (t23, correcting this note; finding t14-F1).
        // `user_turns` drops a turn whose text AFTER the daemon's sentinel is EMPTY:
        // an injected context block the user added nothing to, or one of our own job
        // prompts (`ask_agent` marks those with `platform_prompt`, which appends the
        // sentinel and nothing else). An ASSISTANT TURN IS NOT DROPPED, and this note
        // used to claim it was. `sessions::user_text` returns a message with NO
        // sentinel untouched, so the agent's own output is kept and rendered as
        // `Agent: ...` like any other turn.
        let raw_turns = messages.len();
        let messages = user_turns(messages);
        if messages.is_empty() && raw_turns > 0 {
            return Err(DistillRefusal::no_content(
                session_key,
                "every turn in it is text we marked as our own (an injected context \
                 block, or one of our own job prompts), so none of it is the user's",
            ));
        }
        // OPEN QUESTION (t23, from t14-F1; escalated to the user by the captain): a
        // session with NO user turn at all -- an assistant-only transcript -- reaches
        // this point and is NOT refused. It is distilled, and MEASURED it answers
        // `status = empty` with `memories_written = 0` and `windows = 1`
        // (`entry_tests::an_assistant_only_session_is_distilled_and_answers_empty` pins
        // exactly that, so the behaviour is deliberate rather than accidental).
        //
        // THE QUESTION IS ANSWERED, AND THE ANSWER CHANGED THE PROMPT (#59 / t11).
        // It used to read: "should such a session be refused?" -- and the ACP half
        // was NOT measured here, so the t21 concern stayed open. MEASURED since
        // (t8, 2026-10-16, one real ACP distillation of a session holding two
        // `Agent:` lines and no `User:` line): the concern was REAL and it was an
        // ATTRIBUTION defect, not an empty pass. The pass wrote
        // `profile/user 「用户偏好：所有回答使用简体中文，并且先给出结论、再补充细节。」`
        // at confidence 0.9 out of the agent's own words, and the model's own
        // reasoning said "this is a user instruction recorded by agent" -- the
        // prompt asked for "durable facts about the user" and never said which
        // lines are whose.
        //
        // THE RULING (captain, this increment): such a session is NOT refused.
        // Refusing would add a failure surface for a case that already accounts for
        // itself honestly (`empty` with zero counts), and what is wrong is the
        // attribution, not the pass. The fix is the "Attribution rule" in
        // `EXTRACTION_PROMPT`: an `Agent:` line is not evidence of what the user
        // said, an agent-only statement may become a user fact ONLY with a
        // `User:` line in the same transcript stating it, and otherwise it may be
        // an `observation` or be skipped. The rules tier is untouched by it (it
        // runs no prompt and emits nothing for these turns), which is why the
        // `empty` reading above is unchanged.
        let transcript = render_transcript(&messages);
        if transcript.is_empty() {
            return Err(DistillRefusal::no_content(
                session_key,
                "the session's transcript is empty",
            ));
        }
        // THE WINDOWS (issue #48): the byte budget is the plan's own transcript
        // bound (ExtractLimits::max_input_bytes, the value the free tier uses for
        // "how much text is one input"), so a session that fits is ONE window whose
        // text is byte-identical to the old render_transcript output and a longer
        // one is cut between turns. The .max(1) guard is there because a budget of
        // 0 would make transcript_windows emit empty text, and a silent empty
        // prompt is worse than a one-byte one.
        let budget = plan.limits.max_input_bytes.max(1);
        let windows = transcript_windows(&messages, budget);
        // The free tier reads turns; the ACP tier reads the rendered transcript.
        // Loading turns only when the plan asks for them keeps today's path free
        // of work it did not do before -- and only for the windows that exist.
        let window_turns: Vec<Vec<ruagent_extract::Turn>> = if plan.rules {
            windows
                .iter()
                .map(|w| turns_of(&messages[w.range.clone()]))
                .collect()
        } else {
            Vec::new()
        };
        let inputs: Vec<crate::extract_plane::ExtractInput<'_>> = windows
            .iter()
            .enumerate()
            .map(|(i, w)| crate::extract_plane::ExtractInput {
                transcript: &w.text,
                turns: window_turns.get(i).map(Vec::as_slice).unwrap_or(&[]),
            })
            .collect();
        // THE ACP IMPLEMENTATION EXISTS ONLY WHEN A CARD DOES (t14): a
        // rules-only plan passes `None` here, so no agent card is needed to run
        // it. A plan that enables the llm tier without a card is refused by the
        // seam itself (`ExtractError::Acp`, "the plan enables the ACP tier but
        // no extractor was supplied"), which is why this is a mapping and not a
        // second guard to keep in step.
        let acp = card.map(|card| crate::extract_plane::AcpExtractor {
            distiller: self,
            card,
        });
        let extraction = crate::extract_plane::extract_windows(session_key, &inputs, plan, acp)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut bundle = extraction.bundle;
        // THE USAGE (issue #55): read through ONE helper, so the fold's accessors
        // have a single reader and a test can drive the same expression.
        let usage = usage_columns(&bundle.usage);
        // A CUT IS A CUT, WHICHEVER HALF MADE IT: the window builder knows about a
        // turn too large for its window, and the producers know about their own
        // bounds (the candidate cap, the turn window, the text window). One flag
        // travels to the ledger, so a reader cannot be told "nothing was dropped"
        // by a pass whose INPUT was sliced.
        bundle.truncated |= windows.iter().any(|w| w.truncated);
        let coverage = TranscriptCoverage {
            from_message: windows.first().map(|w| w.range.start).unwrap_or(0),
            to_message: windows.last().map(|w| w.range.end).unwrap_or(0),
            total_messages: messages.len(),
            bytes: windows.iter().map(|w| w.text.len()).sum(),
            budget_bytes: budget,
        };

        // A dry run reports what WOULD be written and writes nothing: no
        // episode, no memory, no graph row.
        let mem = if dry_run {
            MemoryWriteOutcome {
                written: bundle.memories.len() as u32,
                skipped: 0,
                episode: None,
            }
        } else {
            self.write_memories(&bundle.memories, Some((session_key, &transcript)))
                .await?
        };
        // graph = false: memories only. The graph's own report is then ABSENT
        // (None), not zero: nothing counted duplicates or refusals, and writing 0
        // would claim a measurement nobody took.
        let graph = if dry_run || !self.graph {
            None
        } else {
            Some(self.write_extraction(&bundle, mem.episode).await?)
        };

        // Three states, not two (D5): an extraction that returned NOTHING is a
        // different fact from one whose memories were all duplicates of rows
        // that already existed.
        let status = if bundle.is_empty() { "empty" } else { "ok" };
        Ok((
            DistillOutcome {
                session_key: session_key.to_string(),
                memories_written: mem.written,
                memories_skipped: mem.skipped,
                // A dry run reports what the write WOULD have produced; the graph
                // half's own reading is None in both the dry-run and the graph-off
                // case (see above), so those two columns stay NULL there.
                entities_written: graph.as_ref().map_or_else(
                    || {
                        if dry_run {
                            bundle.entities.len() as u32
                        } else {
                            0
                        }
                    },
                    |g| g.entities,
                ),
                relations_written: graph.as_ref().map_or_else(
                    || {
                        if dry_run {
                            bundle.relations.len() as u32
                        } else {
                            0
                        }
                    },
                    |g| g.relations,
                ),
                relations_duplicate: graph.as_ref().map(|g| g.duplicates),
                relations_refused: graph.as_ref().map(|g| g.refused),
                // THE WINDOWS AND THE USAGE (issues #48, #55): measured by this
                // pass, so they are Some here -- including a measured 1 window and
                // a measured "nothing was cut".
                windows: Some(extraction.windows),
                coverage: Some(coverage),
                tokens_used: usage.0,
                context_size: usage.1,
                cost_usd: usage.2,
                // Empty when no agent card was resolved, i.e. on a rules-only
                // pass (t14): the free tier has no agent, and `source` is the
                // field that says which implementation produced this row.
                agent: card.map(|c| c.name.clone()).unwrap_or_default(),
                source: plan.label().to_string(),
                truncated: bundle.truncated,
                // Filled in by `pass_with_retries`, which is the only thing that
                // knows how many attempts this took: a body cannot count the
                // retries that produced it.
                retries: 0,
                dry_run,
            },
            status,
        ))
    }

    /// Record ONE ATTEMPT in `distill_log`.
    ///
    /// WHAT CHANGED, AND WHY (R-2; the schema half landed as I-SCHEMA-2 / t25,
    /// `0024_distill_attempts.sql`): `distill_log` used to key on
    /// `session_key`, so every attempt for a session hit the same key and the
    /// previous attempt's row was replaced. Measured consequence: a window with
    /// 22 attempts and 21 failures left ONE row in the database, and "how often
    /// does distillation fail?" was unanswerable from the database. 0024 rebuilt
    /// the table with `id INTEGER PRIMARY KEY` (session_key is no longer unique)
    /// and named this statement as the writer that must change: a writer keeping
    /// `ON CONFLICT(session_key)` now fails at PREPARE time with "ON CONFLICT
    /// clause does not match any PRIMARY KEY or UNIQUE constraint", which is
    /// exactly how this was found (the t329 test went red).
    ///
    /// So: a plain INSERT, one row per attempt, and the old "a failure must not
    /// overwrite a recorded success" rule stops being a WHERE clause over one
    /// row and becomes a ROW-LEVEL FACT -- the successful attempt's row is still
    /// there, next to the failed one. Readers who want one outcome per session
    /// read the newest row (`ORDER BY id DESC LIMIT 1`), and readers who want a
    /// success/failure RATE use the `distill_recorded_outcomes` view (rows whose
    /// outcome was actually recorded; the 33 pre-0024 rows have NULL status and
    /// are not part of a denominator they never observed).
    ///
    /// Write the graph half of one distillation, and COMPENSATE if it fails.
    ///
    /// RVC-3: by the time this runs, `write_memories` has already created the
    /// episode for this attempt (on purpose -- mem-core's D2(b): the memories of
    /// the same pass must point at it). If the graph write fails, an unmarked
    /// `run_turn` episode would make the panel's distilled badge claim a
    /// distillation that did not complete, so the episode is MARKED failed here.
    /// This function exists as its own step so a test can drive the REAL failure
    /// path (a real SQL error out of `write_graph`) and assert the compensation,
    /// rather than asserting the log writer and calling the claim proved.
    async fn write_extraction(
        &self,
        bundle: &crate::extract_plane::ExtractBundle,
        episode: Option<i64>,
    ) -> Result<GraphOutcome> {
        match self.write_graph(bundle, episode).await {
            Ok(counts) => Ok(counts),
            Err(e) => {
                if let Some(ep) = episode {
                    let voided = self.void_episode(ep, &e.to_string()).await;
                    tracing::warn!(
                        episode = ep, voided, error = %e,
                        "graph write failed: this attempt's episode is marked failed, so the \
                         panel's distilled badge cannot claim a distillation that did not \
                         complete"
                    );
                }
                Err(e)
            }
        }
    }

    /// Which existing row this content should SUPERSEDE, in the WRITE path (R-3).
    ///
    /// TWO CRITERIA, AND WHY BOTH EXIST:
    ///
    /// * With a real embedder: `memembed::judge_merge` decides. It builds the
    ///   BOUNDED candidate set (top_k = 3, cosine, `tau_scope` derived from that
    ///   scope's own p99 distribution -- no absolute threshold), applies the NAMED
    ///   criterion from `ruagent_memory::dedupe`, and writes the decision into
    ///   `memory_diffs` as `op = 'merge_judged'` with a reason such as
    ///   `candidates=3 rule=lexical_ignorable verdict=merge`. Before this wiring
    ///   that function had **no production caller at all** (V-B/t12 measured
    ///   `op='merge_judged'` = 0 rows on the live database): the write path stopped
    ///   at the lexical judge, so the "bounded candidates + named criterion +
    ///   audited decision" half of R-B C2 did not exist end to end.
    ///
    /// * Without an embedder, or with the offline hash fallback, the lexical judge
    ///   in `mergeable_target` stays. WHY: the candidate rule is a COSINE rank, and
    ///   the hash embedder's cosine is not evidence (measured earlier in this file:
    ///   the polarity pair "用户偏好简体中文" / "用户不使用简体中文" sits at cosine
    ///   0.5000 under `hash-embedder`). Ranking on a meaningless scale would merge
    ///   unrelated rows, which is the one outcome worse than keeping two.
    ///
    /// WHAT IS *NOT* LANDED, SAID OUT LOUD: the LLM ESCALATION half. When the
    /// criterion answers `NeedsJudgement` ("lexical refused, but these two are as
    /// close as rewritten duplicates get -- a named judge decides"), this writer
    /// does not spend a second per-memory LLM call: such a row is written as NEW
    /// content, and the `merge_judged` audit row records that a judgement was
    /// needed. A non-merge stays visible in the ledger instead of being silently
    /// converted into a merge, so the escalation can be added later without
    /// destroying the evidence that it is missing.
    async fn merge_target(
        &self,
        store: ruagent_memory::MemoryStore,
        namespace: &str,
        content: &str,
        existing: &[(i64, String)],
    ) -> Option<i64> {
        use ruagent_memory::MergeVerdict;
        let Some(embedder) = self.embedder.clone() else {
            return mergeable_target(content, existing);
        };
        if embedder.is_fallback() {
            return mergeable_target(content, existing);
        }
        let (verdict, _reason) =
            crate::memembed::judge_merge(&self.db, embedder, store, namespace, content).await;
        match verdict {
            MergeVerdict::Merge { candidate, .. } => Some(candidate),
            // `Same` = byte-identical once normalised: the store's own content-hash
            // check records that as `skip_dedupe`, so it is not a supersede.
            MergeVerdict::Same => None,
            // The escalation half (see above): recorded, not merged.
            MergeVerdict::NeedsJudgement { .. } => None,
            MergeVerdict::New | MergeVerdict::Refused(_) => None,
        }
    }

    /// Err-path compensation for a distillation that died AFTER its episode
    /// existed (RVC-3).
    /// WHY NOT DELETE: `memories.source_episode` points at this episode (that is
    /// the provenance t347 moved out of the memory body), so deleting the row
    /// either fails on the foreign key or forces us to throw away the provenance
    /// of memories that are perfectly good. Marking keeps both facts: the
    /// memories and their origin stay, and the episode stops claiming to be a
    /// completed session distillation.
    ///
    /// The panel's `distilled` boolean is derived from `episodes.kind =
    /// 'run_turn'` (t350, `api.rs` DISTILLED_EPISODE_KIND), so relabelling is
    /// exactly what makes the badge honest. `kind` has no CHECK constraint
    /// (verified in 0020's notes: plain TEXT, seven ops already in use), so the
    /// new value is legal; the reason travels in `distill_log.failure_reason`,
    /// which is the ledger for the same attempt.
    ///
    /// Returns true when a row was actually relabelled (idempotent: running it
    /// twice, or on an episode that is not a run_turn, changes nothing).
    async fn void_episode(&self, episode: i64, reason: &str) -> bool {
        let note = format!(
            "{{\"voided_by\":\"write_graph\",\"reason\":{}}}",
            serde_json::Value::String(reason.chars().take(300).collect())
        );
        self.db
            .call(move |conn| -> Result<usize, ruagent_store::DbError> {
                // `COALESCE(meta, ?2)`: an episode that already carries meta keeps
                // it -- this compensation may never destroy a field it did not write.
                let n = conn
                    .execute(
                        "UPDATE episodes
                            SET kind = 'run_turn_failed',
                                meta = COALESCE(meta, ?2)
                          WHERE id = ?1 AND kind = 'run_turn'",
                        rusqlite::params![episode, note],
                    )
                    .map_err(ruagent_store::DbError::from)?;
                Ok(n)
            })
            .await
            .map(|n| n.unwrap_or(0) > 0)
            .unwrap_or(false)
    }

    /// `status` is one of `ok` | `empty` | `failed` and is written on every
    /// attempt, and `prompt_hash` attributes it to the prompt that produced it.
    /// WHAT EACH NEW COLUMN MEANS, in one place (ruagent-issues-45-55 t5):
    ///
    /// * `retries` is ALWAYS written: 0 is the measured "the first attempt
    ///   settled this", which is a reading, not the absence of one.
    /// * `windows`/`truncated` are written together and only when the caller
    ///   MEASURED them (`DistillOutcome::windows` is `Some` exactly when an
    ///   extraction loop ran). A refusal or a failure read nothing, so it leaves
    ///   NULL -- "we did not look" -- rather than a 1/0 it never observed.
    /// * `from_seq`/`to_seq`/`total_messages` are the SAME RULE, one column group
    ///   further (0030; t3 / issue #58): three readings of ONE measurement
    ///   (`DistillOutcome::coverage`, `Some` exactly when `windows` is), so they
    ///   are written together or not at all. A refusal or a failure has
    ///   `coverage = None` and leaves all three NULL -- never a 0-0 range nobody
    ///   observed, and never a `total_messages` without the range it measures.
    ///   The unit is an index into the message list the pass READ (the user-text
    ///   filter runs first), NOT the transcript's own `seq` field.
    ///   ⚠ SAID PLAINLY, because "a failure read nothing" is not literally true of
    ///   every failure: `distill_plan`'s error branch builds its row from
    ///   `DistillOutcome::default()`, so a pass that MEASURED a range and then
    ///   failed while WRITING it (a memory or a graph write error) records no range
    ///   either. That is the reading `windows`/`truncated` already give on that
    ///   row -- a failed attempt carries NO measurement, taken or not -- and it is
    ///   deliberate: the row describes an attempt that did not complete, and a
    ///   half-pass's range is not the range a reader would take it for.
    /// * `tokens_used`/`context_size`/`cost_usd` come from the agent's own
    ///   `UsageUpdate`s; NULL means THE PROVIDER SAID NOTHING, which is not a
    ///   measured 0 (a rules-only pass spends no tokens and reports none).
    /// * `relations_duplicate`/`relations_refused` come from the graph writer's
    ///   report; NULL means the graph half did not run (`[distill] graph = false`),
    ///   which is not the same reading as a measured 0.
    async fn log_outcome(
        &self,
        outcome: &DistillOutcome,
        status: &str,
        prompt_hash: &str,
        failure_reason: Option<&str>,
    ) -> Result<()> {
        let log = outcome.clone();
        let status = status.to_string();
        let prompt_hash = prompt_hash.to_string();
        let failure_reason = failure_reason.map(|r| r.chars().take(300).collect::<String>());
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO distill_log
                         (session_key, distilled_at, memories_written, entities_written,
                          relations_written, agent, status, failure_reason, prompt_hash,
                          retries, windows, truncated, tokens_used, context_size, cost_usd,
                          relations_duplicate, relations_refused,
                          from_seq, to_seq, total_messages)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                             ?18,?19,?20)",
                    rusqlite::params![
                        log.session_key,
                        chrono::Utc::now().to_rfc3339(),
                        log.memories_written,
                        log.entities_written,
                        log.relations_written,
                        log.agent,
                        status,
                        failure_reason,
                        prompt_hash,
                        log.retries,
                        // The two readings travel together: a row that names a
                        // window count also says whether text was dropped, and a
                        // row that read nothing says neither.
                        log.windows.map(|w| w as i64),
                        log.windows.map(|_| if log.truncated { 1i64 } else { 0i64 }),
                        log.tokens_used,
                        log.context_size,
                        log.cost_usd,
                        log.relations_duplicate.map(|d| d as i64),
                        log.relations_refused.map(|r| r as i64),
                        // The range (0030; t3 / issue #58): three readings of ONE
                        // measurement, so they share one `Option` and appear
                        // together or not at all. `Some(0)` for `from_seq` is a
                        // measured "the range starts at the first message", which
                        // is why the map cannot be replaced by a `unwrap_or(0)`.
                        log.coverage.map(|c| c.from_message as i64),
                        log.coverage.map(|c| c.to_message as i64),
                        log.coverage.map(|c| c.total_messages as i64),
                    ],
                )
            })
            .await??;
        Ok(())
    }

    /// Record a pass that never READ anything: `empty`, with `retries` 0.
    ///
    /// WHY THIS EXISTS SEPARATELY (issue #45): the auto-distill gate knows a
    /// chat's transcript holds no turns BEFORE any distillation is attempted
    /// (t313), and the silent `return` that used to happen there left the ledger
    /// with nothing to say about the session at all -- a chat opened and closed in
    /// ten seconds appeared in no table, so "was this conversation ever considered
    /// for distillation?" had no answer anywhere. `empty` (not `failed`) is the
    /// truthful state: nothing went wrong, and nothing was read.
    ///
    /// `reason` is written down for the same reason a failure's is: a row that
    /// says `empty` with no cause makes the reader guess which of "the transcript
    /// was empty" and "there was nothing of the user's in it" happened.
    pub async fn record_empty_attempt(&self, session_key: &str, reason: &str) -> Result<()> {
        let prompt_hash = ruagent_memory::write::content_hash(&self.compose_prompt());
        let outcome = DistillOutcome {
            session_key: session_key.to_string(),
            ..DistillOutcome::default()
        };
        self.log_outcome(&outcome, "empty", &prompt_hash, Some(reason))
            .await
    }

    /// Raw session messages: the sessions index holds the source and
    /// file path, the file is parsed on demand.
    ///
    /// INDEXES FIRST, THEN READS (issue #45). The row this needs is written by a
    /// 60 s scanner, so "no row" usually means "not yet": `ensure_indexed` runs
    /// one pass and asks again, and only when THAT finds nothing does the caller
    /// get [`DistillRefusal::NotIndexed`] -- an actionable sentence instead of
    /// rusqlite's `Query returned no rows` (which is what the old single
    /// `query_row` propagated, because `??` unwrapped the inner error the
    /// `map_err` above it had not touched).
    ///
    /// A DATABASE ERROR IS NOT A REFUSAL and is not rewritten as one: a writer
    /// that is down, or a store that cannot be read, is a defect -- the cause is
    /// kept so the operator sees it, while the two refusals above are sentences
    /// written for the person who asked.
    async fn load_messages(
        &self,
        session_key: &str,
    ) -> Result<Vec<crate::sessions::SessionMessage>> {
        let indexer = crate::sessions::SessionIndexer::for_daemon_root(self.db.clone(), &self.root);
        let located =
            match tokio::time::timeout(INDEX_WAIT, indexer.ensure_indexed(session_key)).await {
                Ok(Ok(found)) => found,
                Ok(Err(e)) => {
                    return Err(anyhow::Error::new(e)
                        .context(format!("indexing `{session_key}` before distilling it")));
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        session = %session_key,
                        wait_ms = INDEX_WAIT.as_millis() as u64,
                        "the session index pass did not finish in time; refusing this pass as \
                         not-yet-indexed rather than holding the caller open"
                    );
                    crate::sessions::SessionLookup::Missing
                }
            };
        // THE THREE ANSWERS, EACH WITH ITS OWN SENTENCE (issue #45): a row to read,
        // a file the pass read and found EMPTY (the index refuses such a file a row
        // on purpose), and a key no family holds. Collapsing the last two would
        // tell a chat opened and closed in ten seconds that it is "not indexed
        // yet" -- which is both false and a reason a batch pass would keep coming
        // back to it for.
        let (source, ref_path) = match located {
            crate::sessions::SessionLookup::Found(source, ref_path) => (source, ref_path),
            crate::sessions::SessionLookup::Empty => {
                return Err(DistillRefusal::no_content(
                    session_key,
                    "the session's file was read and holds no message",
                ));
            }
            crate::sessions::SessionLookup::Missing => {
                return Err(DistillRefusal::not_indexed(session_key));
            }
        };
        Ok(crate::sessions::parse_file_messages(
            &source,
            &PathBuf::from(&ref_path),
        ))
    }

    /// One-shot ACP run: spawn the agent, ask, collect the reply's TEXT.
    ///
    /// The wiki builder's entry point (design §6.1: same single-call discipline,
    /// no tool loops for unattended jobs). It is a thin wrapper over
    /// [`Self::ask_agent_with_usage`] so there is ONE send, one collector and one
    /// timeout, and a job that does not care about usage cannot drift from the job
    /// that does.
    pub(crate) async fn ask_agent(
        &self,
        card: &ruagent_core::AgentCard,
        prompt: &str,
    ) -> Result<String> {
        Ok(self.ask_agent_with_usage(card, prompt).await?.text)
    }

    /// One-shot ACP run, with what the agent reported about its own context window
    /// and cost (ruagent-issues-45-55 t5 / issue #55).
    ///
    /// THE USAGE RIDES THE SAME RETURN VALUE AS THE TEXT, on purpose: a caller that
    /// records the extraction and forgets what it cost is the defect this fixes,
    /// and a second out-parameter is exactly how that happens.
    pub(crate) async fn ask_agent_with_usage(
        &self,
        card: &ruagent_core::AgentCard,
        prompt: &str,
    ) -> Result<AgentReply> {
        let spec = ruagent_acp::adapter_for(card.harness)
            .spawn_spec(card)
            .with_context(|| format!("resolving spawn command for `{}`", card.name))?;
        let workspace = self.root.join("workspaces").join("distill");
        std::fs::create_dir_all(&workspace)?;

        let (ask_tx, mut ask_rx) =
            tokio::sync::mpsc::unbounded_channel::<ruagent_acp::permission::PermissionAsk>();
        // Distillation runs unattended: auto-cancel every permission ask.
        tokio::spawn(async move {
            while let Some(ask) = ask_rx.recv().await {
                let _ = ask
                    .answer
                    .send(ruagent_acp::permission::PermissionAnswer::Cancel);
            }
        });

        let session: ChatSession = ruagent_acp::chat::start_chat(
            ChatOptions {
                program: spec.program,
                args: spec.args,
                cwd: workspace,
                mcp_servers: vec![],
                model: None,
            },
            ask_tx,
        )
        .context("starting distillation agent")?;

        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<ReplyCollector>();
        let mut sub = session.subscribe();
        tokio::spawn(async move {
            // ONE fold for the whole stream, in `ReplyCollector::observe` — which is
            // also what its unit test drives, so the text and the usage cannot be
            // collected by two implementations that disagree.
            let mut collector = ReplyCollector::default();
            loop {
                match sub.recv().await {
                    Ok(event) => {
                        if !collector.observe(&event) {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            let _ = done_tx.send(collector);
        });

        // No context payload: distillation must not feed the memory
        // layer back into itself. The prompt carries the platform marker (t21), so the
        // session the agent CLI logs for this job does not present our own text — a
        // machine copy of another session's transcript — as the user's words.
        session.send(ChatCommand::Prompt {
            text: platform_prompt(prompt),
            context: None,
            // `None` because this sender RENDERS nothing: with no context there
            // is no injection to measure, so the event says "not collected"
            // (`null`) -- it is NOT an all-zero report, which would read as a
            // measured empty injection (t137, contract §3.2).
            context_budget: None,
        })?;
        let collected = match tokio::time::timeout(Duration::from_secs(300), done_rx).await {
            Ok(Ok(collected)) => collected,
            Ok(Err(_)) => anyhow::bail!("agent stream closed before reply"),
            Err(_) => anyhow::bail!("distillation agent timed out (5 min)"),
        };
        // The error the stream reported is only fatal when the turn produced no
        // reply: an agent that errors after answering has still answered (this is
        // the same reading the old two-channel version had).
        if let Some(err) = collected.error.as_deref()
            && collected.text.trim().is_empty()
        {
            anyhow::bail!("distillation agent error: {err}");
        }
        let _ = session.send(ChatCommand::Shutdown);
        Ok(AgentReply {
            text: collected.text,
            usage: collected.usage,
        })
    }

    /// Insert memories with near-duplicate skip (cosine >= 0.90 against
    /// same store+namespace rows, via the shared embedder).
    /// Write the extracted memories, SUPERSEDING a near-duplicate instead of
    /// skipping it (t329).
    ///
    /// `provenance` is the (session key, transcript) this run distilled: the raw
    /// transcript is recorded as an episode and every memory written here points
    /// at it. `record_episode` is idempotent by content hash, so re-distilling a
    /// session reuses the same episode.
    async fn write_memories(
        &self,
        memories: &[ruagent_extract::MemoryCandidate],
        provenance: Option<(&str, &str)>,
    ) -> Result<MemoryWriteOutcome> {
        let episode = match provenance {
            Some((key, transcript)) => match ruagent_memory::episode::record_episode(
                &self.db,
                ruagent_memory::episode::EpisodeKind::RunTurn,
                transcript,
                Some(key),
            )
            .await
            {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!(
                        session = %key,
                        error = %e,
                        "could not record the distillation episode"
                    );
                    None
                }
            },
            None => None,
        };
        let mut written = 0u32;
        let mut skipped = 0u32;
        for m in memories {
            let store = normalize_store(m.store.as_str());
            let namespace = if m.namespace.is_empty() {
                default_namespace(&store).to_string()
            } else {
                m.namespace.clone()
            };
            // PROVENANCE IS A FIELD, NOT TEXT (t347). This used to be
            // format!("`[distilled] {}`", ...) -- a marker written into the body, which
            // every later reader had to strip again: 156 of 163 rows (96%) carried
            // it, no production reader depended on it, and crates/memory/src/dedupe.rs
            // had to treat the word as noise. The write below passes source_episode,
            // which is where a reader can find the same fact now.
            let content = m.content.trim().to_string();
            // The store/namespace must be resolved BEFORE the merge decision is
            // asked for: `judge_merge` (R-3) takes the typed store, and a
            // governance rejection must happen before anything is written.
            let (store_t, ns_t) = match (
                parse_store(&store),
                ruagent_memory::namespace::Namespace::parse(&namespace),
            ) {
                (Some(s), Some(n)) => (s, n),
                _ => {
                    skipped += 1; // governance: bad store/namespace from the agent
                    continue;
                }
            };
            // Near-duplicate check against the live rows in scope. A hit is no
            // longer a SKIP: t323 measured 8 of 44 `profile` rows saying the same
            // thing in different words, because the check it had was word-based
            // and therefore blind to Chinese. A hit now SUPERSEDES that row, so
            // the store keeps one row per meaning instead of one per phrasing
            // (t329). WHICH row, and by what criterion, is `merge_target`'s job
            // (R-3: it is the caller of `memembed::judge_merge`).
            let supersedes: Option<i64> = {
                let db = self.db.clone();
                let (store_c, ns_c) = (store.clone(), namespace.clone());
                let existing: Vec<(i64, String)> = db
                    .call(
                        move |conn| -> Result<Vec<(i64, String)>, ruagent_store::DbError> {
                            let mut stmt = conn
                                .prepare(
                                    "SELECT id, content FROM memories
                                  WHERE store = ?1 AND namespace = ?2 AND superseded_at IS NULL",
                                )
                                .map_err(ruagent_store::DbError::from)?;
                            let rows = stmt
                                .query_map([&store_c, &ns_c], |r| Ok((r.get(0)?, r.get(1)?)))
                                .map_err(ruagent_store::DbError::from)?;
                            Ok(rows.filter_map(|r| r.ok()).collect())
                        },
                    )
                    .await??;
                self.merge_target(store_t, &namespace, &content, &existing)
                    .await
            };
            // The memory crate's write path handles the content hash,
            // exact-duplicate rejection and the audit trail.
            let (store_t, ns_t) = (store_t, ns_t);
            let outcome = ruagent_memory::write::write_memory(
                &self.db,
                &ruagent_memory::write::MemoryWrite {
                    store: store_t,
                    namespace: ns_t,
                    content: content.clone(),
                    // ONE RULE, IN ONE PLACE (t8 contract): the SIGNAL the
                    // extraction reported is turned into a number HERE and
                    // nowhere else, and this writer no longer silently raises it
                    // to a floor of 0.5. Before this change `clamp(0.5, 1.0)`
                    // made `< 0.5` unreachable on every production path (live
                    // reading: 0 of 157 rows), so the hedged-confidence signal
                    // the prompt asks for (0.4) could never appear in the data.
                    // `Unconfirmed` is the "the extraction said nothing" signal
                    // and is exactly the old `None` branch (0.8).
                    confidence: ruagent_memory::confidence::confidence(&match m.confidence {
                        ruagent_extract::CandidateConfidence::Confirmed => {
                            ruagent_memory::confidence::ConfidenceSignals::confirmed()
                        }
                        ruagent_extract::CandidateConfidence::Corrected => {
                            ruagent_memory::confidence::ConfidenceSignals::corrected()
                        }
                        ruagent_extract::CandidateConfidence::Hedged => {
                            ruagent_memory::confidence::ConfidenceSignals::hedged()
                        }
                        ruagent_extract::CandidateConfidence::Unconfirmed => {
                            ruagent_memory::confidence::ConfidenceSignals::unconfirmed()
                        }
                        ruagent_extract::CandidateConfidence::Explicit(v) => {
                            ruagent_memory::confidence::ConfidenceSignals::explicit(v)
                        }
                    }),
                    source_episode: episode,
                    supersedes,
                },
            )
            .await
            .with_context(|| format!("writing distilled memory to {store}/{namespace}"))?;
            use ruagent_memory::write::WriteOutcome as W;
            match outcome {
                W::Inserted(id) => {
                    if let Some(embedder) = self.embedder.clone() {
                        crate::memembed::embed_row(&self.db, embedder, id, &content).await;
                    }
                    written += 1;
                }
                W::Superseded { new, .. } => {
                    if let Some(embedder) = self.embedder.clone() {
                        crate::memembed::embed_row(&self.db, embedder, new, &content).await;
                    }
                    written += 1;
                }
                W::SkippedDuplicate(_) => skipped += 1,
                W::RejectedNamespace => skipped += 1,
            }
        }
        Ok(MemoryWriteOutcome {
            written,
            skipped,
            episode,
        })
    }

    /// Find-or-create entities, then add relations — through the graph
    /// crate (alias-aware resolution, deterministic supersession, FTS sync).
    ///
    /// `episode` is the id `write_memories` attached to this run's memories: the
    /// SAME raw session material backs both stores, so an edge can answer "which
    /// session said this" (before gen2: 0 of 67 edges had any source at all).
    async fn write_graph(
        &self,
        bundle: &crate::extract_plane::ExtractBundle,
        episode: Option<i64>,
    ) -> Result<GraphOutcome> {
        // ONE transaction for the whole graph half (t81, audit #1). Building the
        // graph item by item -- an `upsert_entity_with_aliases` per entity and an
        // `upsert_fact` per relation, each its own closure through the
        // single-writer actor -- meant a failure at relation 7 left the first 6
        // entities and their aliases COMMITTED. Measured with an injected failure
        // (trigger on `entity_edges`), the failed attempt had already created
        // `entities 2` and `aliases 1` while its episode was marked
        // `run_turn_failed`: the badge was honest and the graph was not.
        // `apply_extraction` drives the SAME per-item rules inside one
        // transaction, so a failure now leaves the graph exactly as it was.
        let entities: Vec<ruagent_graph::ExtractEntity> = bundle
            .entities
            .iter()
            .map(|e| ruagent_graph::ExtractEntity {
                name: e.name.clone(),
                kind: e.kind.map(str::to_string),
                summary: e.summary.clone(),
                aliases: e.aliases.clone(),
            })
            .collect();
        let facts: Vec<ruagent_graph::ExtractFact> = bundle
            .relations
            .iter()
            .map(|r| ruagent_graph::ExtractFact {
                src: r.src.clone(),
                dst: r.dst.clone(),
                relation: r.relation.relation_literal().to_string(),
                fact_text: r.fact.clone(),
                valid_at: r.valid_at.clone(),
                event_time_source: if r.valid_at.is_some() {
                    ruagent_graph::EventTimeSource::Extracted
                } else {
                    ruagent_graph::EventTimeSource::Recorded
                },
            })
            .collect();

        let report = ruagent_graph::apply_extraction(
            &self.db,
            &entities,
            &facts,
            ruagent_graph::AliasSource::Extraction,
            episode,
        )
        .await
        .context("writing this extraction into the graph (all-or-nothing)")?;

        // NOT A SILENT SKIP (G7): a duplicate or a refused name is counted and
        // reported, because "we dropped 8 of your 20 relations" is a reading, not
        // a detail. Relations to entities the extraction did not list are skipped
        // inside the write (the count difference is visible as
        // `relations.len() - report.relations - report.duplicates - report.refused`).
        if report.duplicates > 0 || report.refused > 0 {
            tracing::info!(
                written = report.relations,
                duplicate = report.duplicates,
                refused = report.refused,
                "graph write: deduped at the write side (G7)"
            );
        }
        // RETURNED, not only logged (issue #55): the ledger keeps the two counts,
        // so the question the log line answered is now answerable from the
        // database -- and per attempt rather than per daemon log window.
        Ok(GraphOutcome {
            entities: report.entities,
            relations: report.relations,
            duplicates: report.duplicates,
            refused: report.refused,
        })
    }
}

fn parse_store(s: &str) -> Option<ruagent_memory::MemoryStore> {
    match s.trim().to_lowercase().as_str() {
        "profile" => Some(ruagent_memory::MemoryStore::Profile),
        "procedure" => Some(ruagent_memory::MemoryStore::Procedure),
        "lesson" => Some(ruagent_memory::MemoryStore::Lesson),
        "observation" => Some(ruagent_memory::MemoryStore::Observation),
        _ => None,
    }
}

fn normalize_store(s: &str) -> String {
    match s.trim().to_lowercase().as_str() {
        "profile" => "profile".into(),
        "procedure" => "procedure".into(),
        "lesson" => "lesson".into(),
        _ => "observation".into(),
    }
}

/// One of OUR prompts, carrying the marker that says so (ruagent-close-the-gaps t21).
///
/// Every unattended job prompt this daemon sends goes out through `ask_agent`, and the
/// agent CLI it spawns writes that prompt into its OWN session log — where a later
/// distillation reads it as the first "user" turn of that session. That text is a
/// MACHINE COPY of something else (the extraction prompt embeds another session's
/// rendered transcript; the wiki page writer embeds the source documents), so a
/// candidate extracted from it claims the user said it in the session where the daemon
/// ran the job.
///
/// MEASURED, on a private root with the real routes (t21): distilling a wiki WRITER job
/// session wrote a memory for a sentence the user never said there, and the row's
/// provenance pointed at the JOB — `memories.source_episode` -> `episodes.source_run =
/// claude-code:<writer job>` — while the user's own session held the same sentence as a
/// DIFFERENT episode that nothing pointed at.
///
/// The marker is the daemon's own, not a new convention: `chat::USER_TEXT_SENTINEL` plus
/// its one reader `sessions::user_text` define "the user's words" as what FOLLOWS the
/// sentinel. Appending it to a prompt WE wrote states the truth about that turn — there
/// is no user text in it — and every reader that already speaks this convention (the
/// session title, `user_turns` below, the extraction seam) classifies it correctly.
///
/// It also closes the class STRUCTURALLY rather than by a list: the hygiene fix
/// recognises two prompt HEADS, and the wiki PAGE WRITER prompt is not one of them,
/// which is how this channel was still reachable.
fn platform_prompt(prompt: &str) -> String {
    format!("{prompt}{}", crate::chat::USER_TEXT_SENTINEL)
}

/// The messages a distillation may read: the ones that carry something of the USER's.
///
/// ONE definition, applied ONCE before either tier reads the session, so the rules tier
/// and the ACP transcript cannot disagree about whose words these are —
/// `sessions::user_text` is the same split the session title and the extraction seam
/// use. A turn that carries only platform text (an injected block, or one of our own job
/// prompts, which `ask_agent` marks with `platform_prompt`) has nothing of the user's in
/// it and is dropped here. A user turn is untouched, with or without a marker.
fn user_turns(
    messages: Vec<crate::sessions::SessionMessage>,
) -> Vec<crate::sessions::SessionMessage> {
    messages
        .into_iter()
        .filter(|m| !crate::sessions::user_text(&m.text).trim().is_empty())
        .collect()
}

/// The transcript rendered as plain turns for the extraction prompt — the exact
/// string the ACP tier appends to its prompt and the free tier reads as text.
fn render_transcript(messages: &[crate::sessions::SessionMessage]) -> String {
    messages
        .iter()
        .map(|m| {
            format!(
                "[{}] {}: {}",
                chrono::DateTime::from_timestamp_millis(m.ts)
                    .map(|d| d.format("%m-%d %H:%M").to_string())
                    .unwrap_or_default(),
                if m.role == "user" { "User" } else { "Agent" },
                m.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The three usage columns of one attempt, from the bundle's own report.
///
/// ONE READER, so the fold's accessors cannot be interpreted two ways
/// (ruagent-issues-45-55 t5 / issue #55): `distill_once` and the test that proves
/// the columns are populated both come through here.
fn usage_columns(
    usage: &crate::extract_plane::UsageReport,
) -> (Option<i64>, Option<i64>, Option<f64>) {
    (usage.tokens_used(), usage.context_size(), usage.cost_usd())
}

/// Cut `text` to at most `budget` BYTES, at a char boundary.
///
/// `floor_char_boundary` is unstable, and slicing a `&str` inside a multi-byte
/// character PANICS: the cut has to walk back to the nearest boundary, which for
/// Chinese text is one byte at worst and never a half character.
fn cut_to_bytes(text: &str, budget: usize) -> &str {
    if text.len() <= budget {
        return text;
    }
    let mut end = budget;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Split the messages into windows of at most `budget_bytes` of rendered text,
/// at TURN boundaries (ruagent-issues-45-55 t5 / issue #48).
///
/// WHY WINDOWS AT ALL. `render_transcript` used to render the whole session into
/// one prompt: the ACP tier appended it unbounded (the model's own context limit
/// was the only bound, and hitting it is a hard failure, not a shorter read), and
/// the free tier silently kept only the newest turns its own byte window allowed.
/// A session longer than one input is therefore READ IN WINDOWS here — cut where
/// the conversation itself is cut, between turns — and extracted once per window.
///
/// WHAT IS GUARANTEED, in the order it matters:
///
/// 1. **A session that fits is ONE window whose text is byte-identical to
///    `render_transcript`'s.** Not "equivalent": the same call, so the single-window
///    prompt, its `prompt_hash` and every existing assertion about it are
///    unchanged. A caller that wants the old rendering still gets it from
///    `render_transcript`; this function is where a budget enters.
/// 2. **A session that does not fit is cut BETWEEN turns**, newest-last, and every
///    message is eventually extracted exactly once: the union of the ranges is
///    `0..messages.len()`.
/// 3. **A single message larger than the budget gets its own window, CUT at the
///    budget**, with `truncated = true`. That is the "cannot be windowed any
///    further" case, and it is REPORTED rather than silent: the caller ORs the flag
///    into `ExtractBundle::truncated` so the ledger's `truncated` column says so.
fn transcript_windows(
    messages: &[crate::sessions::SessionMessage],
    budget_bytes: usize,
) -> Vec<TranscriptWindow> {
    let budget = budget_bytes.max(1);
    let mut windows: Vec<TranscriptWindow> = Vec::new();
    let mut start = 0usize;
    let mut start_bytes = 0usize;
    for (i, m) in messages.iter().enumerate() {
        // The turn's own rendered size: the same rendering the window will carry,
        // so the packing cannot disagree with what it packs.
        let turn_bytes = render_transcript(std::slice::from_ref(m)).len();
        let would_be =
            start_bytes + turn_bytes + if start_bytes == 0 { 0 } else { SEPARATOR_BYTES };
        if i > start && would_be > budget {
            windows.push(window_of(messages, start, i, budget));
            start = i;
            start_bytes = turn_bytes;
        } else {
            start_bytes = would_be;
        }
    }
    if start < messages.len() {
        windows.push(window_of(messages, start, messages.len(), budget));
    }
    windows
}

/// The separator `render_transcript` joins turns with, as a byte count: counted
/// HERE rather than guessed, because the packing above is about bytes.
const SEPARATOR_BYTES: usize = 2;

/// One window's rendered text, CUT to the budget when a single turn is larger than
/// it (see `transcript_windows`).
fn window_of(
    messages: &[crate::sessions::SessionMessage],
    from: usize,
    to: usize,
    budget: usize,
) -> TranscriptWindow {
    let rendered = render_transcript(&messages[from..to]);
    if rendered.len() <= budget {
        return TranscriptWindow {
            range: from..to,
            text: rendered,
            truncated: false,
        };
    }
    TranscriptWindow {
        range: from..to,
        text: cut_to_bytes(&rendered, budget).to_string(),
        truncated: true,
    }
}

/// The same messages as the free tier's [`ruagent_extract::Turn`]s. The role is
/// normalised here, at the boundary (`user` / everything else is the agent),
/// because the extractor's rule table filters on the role.
fn turns_of(messages: &[crate::sessions::SessionMessage]) -> Vec<ruagent_extract::Turn> {
    messages
        .iter()
        .map(|m| ruagent_extract::Turn {
            role: if m.role == "user" {
                ruagent_extract::Role::User
            } else {
                ruagent_extract::Role::Assistant
            },
            text: m.text.clone(),
            ts_ms: m.ts,
        })
        .collect()
}

fn default_namespace(store: &str) -> &'static str {
    if store == "profile" { "user" } else { "global" }
}

/// Strip markdown fences / commentary the agent may have added.
pub(crate) fn parse_extraction(raw: &str) -> Result<Extraction> {
    let trimmed = raw.trim();
    let body = if let Some(start) = trimmed.find('{') {
        let end = trimmed
            .rfind('}')
            .context("extraction JSON has no closing brace")?;
        &trimmed[start..=end]
    } else {
        anyhow::bail!(
            "distillation agent returned no JSON: {}",
            &raw[..raw.len().min(120)]
        )
    };
    serde_json::from_str(body).with_context(|| "parsing extraction JSON".to_string())
}

/// Cheap near-duplicate check: high token overlap on normalized text.
/// (The full embedder-based check lands with memory embeddings; this
/// ships the dedup contract now.)
/// The built-in extraction prompt — read-only reference for the panel's
/// settings card, so users can see what `[distill] prompt` would replace.
pub fn builtin_extraction_prompt() -> &'static str {
    EXTRACTION_PROMPT
}

/// The full extraction prompt: base (built-in or `[distill] prompt`
/// override), optional language clause, transcript tail. Free function
/// so the composition is testable without a Distiller.
fn extraction_prompt(language: Option<&str>, prompt_override: Option<&str>) -> String {
    let base = prompt_override.unwrap_or(EXTRACTION_PROMPT);
    let mut out = base.to_string();
    if let Some(lang) = language {
        out.push_str(&format!(
            "\n\nWrite every `content` value, entity `summary`, and \
             relation `fact` in {lang}. JSON keys and the \
             `store`/`namespace` values stay exactly as specified above."
        ));
    }
    out.push_str("\n\nTRANSCRIPT:\n");
    out
}

/// Which existing row this content should SUPERSEDE, if any (t329).
///
/// The criterion lives in the memory crate (`ruagent_memory::dedupe`): ONE source
/// for the vocabulary and the fail-closed rule, so the write path and its tests
/// cannot drift apart. It compares characters, which is what makes it able to
/// see Chinese at all — the word-based check it replaces normalised a whole
/// sentence to a single token.
fn mergeable_target(content: &str, existing: &[(i64, String)]) -> Option<i64> {
    existing.iter().find_map(
        |(id, row)| match ruagent_memory::dedupe::judge(row, content) {
            ruagent_memory::dedupe::Verdict::Mergeable => Some(*id),
            _ => None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_composition_language_and_override() {
        // Bare: builtin prompt + transcript tail, no language clause.
        let bare = extraction_prompt(None, None);
        assert!(bare.starts_with("You are a memory distillation engine."));
        assert!(bare.ends_with("TRANSCRIPT:\n"));
        assert!(!bare.contains("Write every"));

        // Language clause rides between the base and the tail.
        let zh = extraction_prompt(Some("简体中文"), None);
        assert!(zh.contains("in 简体中文"));
        assert!(zh.ends_with("TRANSCRIPT:\n"));

        // Full override replaces the base entirely.
        let over = extraction_prompt(Some("简体中文"), Some("CUSTOM REGIME\n"));
        assert!(over.starts_with("CUSTOM REGIME"));
        assert!(!over.contains("memory distillation engine"));
        assert!(over.ends_with("TRANSCRIPT:\n"));
    }

    /// #59 / t11: the ATTRIBUTION RULE is part of the built-in prompt, and this is
    /// the pin that keeps a later rewrite from dropping it silently.
    ///
    /// WHY A PIN IS NEEDED AT ALL. The rule is the ONLY thing standing between an
    /// ACP distillation of an assistant-only session and a `profile`/`user`
    /// memory fabricated out of the agent's own words, and that failure is
    /// measured, not hypothetical (t8): two `Agent:` lines and no `User:` line
    /// produced `profile/user 「用户偏好：所有回答使用简体中文…」` at confidence 0.9,
    /// and the model's own reasoning said "this is a user instruction recorded by
    /// agent". A prompt is PROSE: prose is exactly what a rewrite drops without a
    /// compiler or a behaviour test noticing, and no other test in this crate
    /// asserted a single sentence of the built-in prompt's content.
    ///
    /// THE NEEDLES ARE WRITTEN OUT HERE INSTEAD OF SHARING A CONSTANT with the
    /// prompt. A constant used by both sides stays green after someone rewords the
    /// rule into something the model reads differently -- the pin has to be an
    /// INDEPENDENT copy of the intent, so that changing the prompt's wording
    /// forces a change here too.
    ///
    /// The rule must also sit BEFORE the transcript tail: everything after
    /// `TRANSCRIPT:` is the data the rule is supposed to govern.
    #[test]
    fn the_attribution_rule_is_in_the_extraction_prompt() {
        let built_in = extraction_prompt(None, None);
        for needle in [
            "Attribution rule",
            "`Agent:` line is the AGENT's own words",
            "It is NOT evidence of what the user said",
            "Never turn an `Agent:` line into a `profile`/`user` memory",
            "ONLY when a `User:` line in the SAME transcript states that fact",
            "may be recorded as `observation` (never `profile`/`user`)",
            "no `User:` line at all can therefore never yield a `profile`/`user` memory",
        ] {
            assert!(
                built_in.contains(needle),
                "the built-in extraction prompt no longer states the attribution rule \
                 ({needle:?}); dropping it silently re-opens the t8 defect"
            );
        }
        let rule = built_in
            .find("Attribution rule")
            .expect("the rule is in the prompt");
        let tail = built_in
            .rfind("TRANSCRIPT:")
            .expect("the prompt ends with the transcript marker");
        assert!(
            rule < tail,
            "the attribution rule must be an INSTRUCTION (before the transcript \
             marker), not part of the data it governs"
        );

        // ... and it reaches the request the ACP tier actually sends: the language
        // clause (the live policy sets 简体中文) is appended AFTER the rule and
        // before the transcript, so it cannot displace it.
        let with_language = extraction_prompt(Some("简体中文"), None);
        assert!(with_language.contains("Attribution rule"));
        assert!(with_language.contains("Never turn an `Agent:` line"));

        // A FULL OPERATOR OVERRIDE still replaces the base -- including this rule.
        // Recorded here rather than discovered later: `prompt_override` is the
        // operator owning the extraction semantics, and an operator who writes
        // their own prompt owes the same attribution discipline (the built-in one
        // is what production runs: policy.toml's `[distill]` sets no `prompt`).
        let overridden = extraction_prompt(None, Some("CUSTOM REGIME\n"));
        assert!(
            !overridden.contains("Attribution rule"),
            "an override must keep replacing the base; if this ever changes, the \
             override contract changed and the doc comment on it must say so"
        );
    }

    #[test]
    fn extraction_json_parses_with_fences() {
        let raw = "Here you go:\n```json\n{\"memories\":[{\"store\":\"profile\",\"namespace\":\"user\",\"content\":\"prefers Rust\"}],\"entities\":[{\"name\":\"ruagent\",\"kind\":\"project\"}],\"relations\":[]}\n```\n";
        let ex = parse_extraction(raw).unwrap();
        assert_eq!(ex.memories.len(), 1);
        assert_eq!(ex.memories[0].content, "prefers Rust");
        assert_eq!(ex.entities[0].name, "ruagent");
        assert!(ex.relations.is_empty());
    }

    /// English, so both languages stay covered: a difference made of filler
    /// merges, a difference that carries content does not.
    ///
    /// The second assertion is where the old rule and this one disagree, and
    /// the disagreement is the point: the word-overlap rule called that pair a
    /// duplicate (3 of its 4 long words matched), while dropping "timezone
    /// UTC+8" plainly changes the fact. Fail-closed refuses (t329).
    #[test]
    fn near_duplicate_is_a_character_decision_now() {
        let existing = vec![(
            7i64,
            "the user prefers Rust and dislikes Java timezone UTC+8".to_string(),
        )];
        assert_eq!(
            mergeable_target(
                "user prefers Rust and dislikes Java timezone UTC+8",
                &existing
            ),
            Some(7),
            "only the filler word `the` differs"
        );
        assert_eq!(
            mergeable_target("User prefers Rust; dislikes Java (UTC+8)", &existing),
            None,
            "the timezone is content, so this must not merge"
        );
    }
}

#[cfg(test)]
mod t329_tests {
    use super::*;

    async fn count(db: &ruagent_store::Db, sql: &str) -> i64 {
        let sql = sql.to_string();
        db.call(move |conn| conn.query_row(&sql, [], |r| r.get::<_, i64>(0)))
            .await
            .unwrap()
            .unwrap()
    }

    /// The three graph tables one distillation's graph half touches, as one
    /// comparable value (t81): "the failed attempt changed the graph" and "the
    /// successful attempt wrote it" are both single comparisons of this.
    async fn graph_counts(d: &Distiller) -> (i64, i64, i64) {
        (
            count(&d.db, "SELECT count(*) FROM entities").await,
            count(&d.db, "SELECT count(*) FROM entity_aliases").await,
            count(&d.db, "SELECT count(*) FROM entity_edges").await,
        )
    }

    fn root(tag: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("ruagent-t329-{tag}-{}-{n}", std::process::id()));
        // A FRESH ROOT MEANS AN EMPTY ONE (t154). The name is `pid + seq`, and
        // pids are REUSED: when the OS hands this process a pid an earlier run
        // had, this path already exists, `Db::open` opens the OLD database, and
        // the write under test is a duplicate BY CONSTRUCTION -- `written == 0`,
        // the signature `t149`'s F2 reported (`left: 0 / right: 1`), reddening
        // the WHOLE module at once because one reused pid hits every test's
        // directory. Nothing ever removed these: 6202 leaked `ruagent-t329-*`
        // dirs, every one of them holding a `ruagent.db` (measured 2026-10-04).
        // So the wipe is not hygiene, it IS the fixture.
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn distiller(root: &std::path::Path) -> Distiller {
        Distiller {
            db: ruagent_store::Db::open(root.join("ruagent.db")).unwrap(),
            root: root.to_path_buf(),
            embedder: None,
            registry: crate::distill::AgentRegistry::default(),
            language: None,
            prompt_override: None,
            graph: false,
        }
    }

    fn mem(content: &str) -> ruagent_extract::MemoryCandidate {
        ruagent_extract::MemoryCandidate {
            store: ruagent_extract::CandidateStore::Profile,
            namespace: "user".to_string(),
            content: content.to_string(),
            confidence: ruagent_extract::CandidateConfidence::Unconfirmed,
            rule: "acp",
            origin: Default::default(),
            source: String::new(),
        }
    }

    fn msg(role: &str, text: &str) -> crate::sessions::SessionMessage {
        crate::sessions::SessionMessage {
            role: role.to_string(),
            text: text.to_string(),
            ts: 0,
        }
    }

    /// t21: a job prompt's EMBEDDED render must not become a memory in the session where
    /// the daemon ran the job. The prompt is a MACHINE COPY of another session's text --
    /// and the wiki PAGE WRITER prompt is not one of the two heads the hygiene fix
    /// knows, so this is the channel that was still open when the defect was measured.
    ///
    /// THE ASSERTION THAT FAILS IF THE ATTRIBUTION RETURNS: the third one, `no candidate
    /// mentions the embedded sentence`. It goes red the moment a marked job prompt
    /// reaches either tier again: drop `platform_prompt` from `ask_agent`'s send, or
    /// stop filtering with `user_turns`, and the sentence comes back as a candidate of
    /// the JOB session.
    #[test]
    fn a_marked_job_prompt_is_not_the_users_words() {
        let user_sentence = "记住：发布流程统一使用 scripts/release.sh 这一步，不要再手动打 tag。";
        let writer_prompt = format!(
            "WIKI PAGE WRITER (slug: t21)\n\nWrite ONE wiki page.\n\n<sources>\n\
             <document name=\"release-notes\">\n<chunk id=\"1\">\n{user_sentence}\n</chunk>\n\
             </document>\n</sources>\n"
        );
        // What the agent CLI logs for the job: the marked prompt, then the agent's reply.
        let job = vec![
            msg("user", &platform_prompt(&writer_prompt)),
            msg("assistant", "好的，已按这个约定记录。"),
        ];
        // 1. the marker states the truth about that turn, in the daemon's own vocabulary.
        assert!(
            crate::sessions::user_text(&job[0].text).trim().is_empty(),
            "a job prompt must carry no user text"
        );
        // 2. neither tier sees the embedded render.
        let kept = user_turns(job);
        let transcript = render_transcript(&kept);
        assert!(
            !transcript.contains(user_sentence),
            "the embedded render reached the ACP transcript: {transcript}"
        );
        let turns = turns_of(&kept);
        let cx = crate::extract_plane::ExtractCtx {
            session_key: "claude-code:t21-writer-job",
            transcript: &transcript,
            turns: &turns,
            limits: ruagent_extract::ExtractLimits::default(),
        };
        let bundle = crate::extract_plane::extract_rules(&cx);
        // 3. THE PIN: no candidate carries the machine copy's text.
        assert!(
            bundle
                .memories
                .iter()
                .all(|m| !m.content.contains("release.sh")),
            "a candidate was extracted from a machine copy: {:?}",
            bundle.memories
        );
        // CONTROL: the same sentence on the USER's own turn is still extracted, so the
        // filter drops platform text and not the user.
        let own = user_turns(vec![msg("user", user_sentence)]);
        let own_transcript = render_transcript(&own);
        let own_turns = turns_of(&own);
        let own_cx = crate::extract_plane::ExtractCtx {
            session_key: "claude-code:t21-user",
            transcript: &own_transcript,
            turns: &own_turns,
            limits: ruagent_extract::ExtractLimits::default(),
        };
        assert!(
            !crate::extract_plane::extract_rules(&own_cx)
                .memories
                .is_empty(),
            "the control must still extract the user's own sentence"
        );
    }

    /// t347 acceptance 1: what a distillation WRITES must not carry the
    /// provenance marker in the body any more. The fact it used to encode is
    /// read from the row's source_episode instead -- asserted here too, because
    /// removing the marker without moving the fact would lose it.
    #[tokio::test]
    async fn a_distilled_body_carries_no_provenance_marker() {
        let root = root("t347-prefix");
        let d = distiller(&root).await;
        // THE PREMISE, ASSERTED (t149 shape). This root is fresh, so the two
        // `written == 0` causes below have to be told apart BEFORE the write:
        // `write_memories` asks `merge_target` about the live rows in scope, and
        // a row already there would make the write below a duplicate BY
        // CONSTRUCTION rather than a defect. Without this assertion a leftover
        // row reads exactly like a refused write.
        let live_row = "SELECT count(*) FROM memories
                        WHERE store = 'profile' AND namespace = 'user'
                          AND superseded_at IS NULL";
        assert_eq!(
            count(&d.db, live_row).await,
            0,
            "the fresh root must start with no live `profile/user` row, or the \
             `written == 1` below is a duplicate by construction"
        );
        let out = d
            .write_memories(
                &[mem("用户偏好使用简体中文交流。")],
                Some(("ruagent:t347-test", "[t347] the marker moved to a field")),
            )
            .await
            .unwrap();
        let w = out.written;
        // The write is SYNCHRONOUS (`write_memories` awaits
        // `ruagent_memory::write::write_memory`, and `Db::call` awaits the
        // single-writer actor's reply), so a zero here is not a timing race --
        // but the two causes it CAN be are indistinguishable in the old message.
        // Name them: this is the field a future reader needs when the load shape
        // that reddened these six cases (`t149` F2) is reproduced again.
        assert_eq!(
            w,
            1,
            "the write must land, or the assertion below is vacuous: written={w} \
             skipped={} episode_recorded={} -- a non-zero `skipped` on an empty root means \
             `ruagent_memory::write::write_memory` judged the row a duplicate (content hash) \
             or refused the namespace; see `crates/memory/src/write.rs`",
            out.skipped,
            out.episode.is_some()
        );
        assert!(
            out.episode.is_some(),
            "the write pass must report the episode it attached the rows to: the graph \
             edges of the same distillation point at the same episode"
        );
        let rows: Vec<(i64, String, Option<i64>)> =
            d.db.call(
                |conn| -> Result<Vec<(i64, String, Option<i64>)>, rusqlite::Error> {
                    let mut st = conn
                        .prepare("SELECT id, content, source_episode FROM memories ORDER BY id")?;
                    let v: Vec<(i64, String, Option<i64>)> = st
                        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(v)
                },
            )
            .await
            .unwrap()
            .unwrap();
        let (id, content, episode) = rows.first().expect("one row").clone();
        println!("READING t347 write path: id={id} content={content:?} source_episode={episode:?}");
        assert!(
            !content.starts_with("[distilled] "),
            "the body still carries the provenance marker: {content:?}"
        );
        assert_eq!(content, "用户偏好使用简体中文交流。");
        assert!(
            episode.is_some(),
            "provenance must be in the field instead of the text"
        );
    }

    /// t329 acceptance 3: four phrasings of ONE fact must leave one live row —
    /// the earlier ones superseded — instead of the four rows t323 measured.
    #[tokio::test]
    async fn four_phrasings_leave_one_live_row() {
        let root = root("phrasings");
        let d = distiller(&root).await;
        let phrasings = [
            "用户偏好使用简体中文交流。",
            "用户使用简体中文交流。",
            "用户使用简体中文进行交流。",
            "偏好使用简体中文交流。",
        ];
        for (i, p) in phrasings.iter().enumerate() {
            let out = d
                .write_memories(
                    &[mem(p)],
                    Some(("ruagent:t329-test", "[t329] the user's language preference")),
                )
                .await
                .unwrap();
            println!(
                "READING write {i}: written={} skipped={} episode={:?}",
                out.written, out.skipped, out.episode
            );
        }
        let live = count(
            &d.db,
            "SELECT count(*) FROM memories WHERE superseded_at IS NULL",
        )
        .await;
        let dead = count(
            &d.db,
            "SELECT count(*) FROM memories WHERE superseded_at IS NOT NULL",
        )
        .await;
        let episodes = count(&d.db, "SELECT count(*) FROM episodes").await;
        let with_source = count(
            &d.db,
            "SELECT count(*) FROM memories WHERE source_episode IS NOT NULL",
        )
        .await;
        println!(
            "READING after four phrasings: live={live} superseded={dead} episodes={episodes} rows_with_source_episode={with_source}"
        );
        let rows: Vec<String> = d
            .db
            .call(|conn| {
                let mut st = conn.prepare(
                    "SELECT id, superseded_at IS NOT NULL, supersedes, content FROM memories ORDER BY id",
                )?;
                let v: Vec<String> = st
                    .query_map([], |r| {
                        Ok(format!(
                            "#{} superseded={} supersedes={:?} {}",
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, Option<i64>>(2)?,
                            r.get::<_, String>(3)?
                        ))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok::<_, rusqlite::Error>(v)
            })
            .await
            .unwrap()
            .unwrap();
        for r in &rows {
            println!("READING row: {r}");
        }
        assert_eq!(live, 1, "four phrasings of one fact leave one live row");
        assert_eq!(
            dead, 3,
            "the three earlier rows are superseded, never deleted"
        );
        assert!(episodes >= 1, "the transcript is recorded as an episode");
        assert_eq!(with_source, 4, "every row points at its episode");
        std::fs::remove_dir_all(&root).ok();
    }

    /// t329 acceptance 4: the polarity pair is refused — and it is refused even
    /// though the two strings are near neighbours in vector space, which is why
    /// the cosine cannot be the criterion.
    #[tokio::test]
    async fn a_polarity_pair_is_refused_even_though_the_cosine_is_high() {
        let a = "[distilled] 用户偏好简体中文";
        let b = "[distilled] 用户不使用简体中文";
        let verdict = ruagent_memory::dedupe::judge(a, b);
        let emb: std::sync::Arc<dyn ruagent_knowledge::embed::Embedder> =
            std::sync::Arc::new(ruagent_knowledge::embed::HashEmbedder::default());
        let v = emb.embed(&[a, b]).unwrap();
        let dot: f32 = v[0].iter().zip(&v[1]).map(|(x, y)| x * y).sum();
        let na: f32 = v[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = v[1].iter().map(|x| x * x).sum::<f32>().sqrt();
        let cos = dot / (na * nb);
        println!(
            "READING polarity pair: verdict={verdict:?} cosine({})={cos:.4}",
            emb.name()
        );
        assert!(matches!(
            verdict,
            ruagent_memory::dedupe::Verdict::Refused(_)
        ));
        assert!(cos > 0.5, "a neighbour in vector space, and still refused");
    }

    // -----------------------------------------------------------------------
    // gen2 (t9 / I-C): provenance, event-time source, distill three states,
    // and the confidence floor that made `< 0.5` unreachable.
    // -----------------------------------------------------------------------

    /// One row of `entity_edges` as this test reads it: the provenance fields a
    /// distillation must leave behind. Named because the four-column tuple is
    /// past what `clippy::type_complexity` accepts (t10 found it: the daemon's
    /// `--all-targets -D warnings` gate compiles this test target, so an inline
    /// tuple here turns into a red gate for whoever owns that command).
    type EdgeRow = (String, Option<i64>, Option<String>, Option<String>);

    /// The ACP wire shape, mapped through the extractor seam's own mapping — so
    /// these graph tests exercise the SAME conversion the production path uses
    /// (t4), instead of a second copy that could drift from it.
    fn extraction(
        entities: &[(&str, &[&str])],
        relations: &[(&str, &str, &str, &str, Option<&str>)],
    ) -> crate::extract_plane::ExtractBundle {
        crate::extract_plane::bundle_of_acp(
            &Extraction {
                memories: Vec::new(),
                entities: entities
                    .iter()
                    .map(|(name, aliases)| ExtractedEntity {
                        name: name.to_string(),
                        kind: Some("tool".to_string()),
                        summary: Some(format!("{name} summary")),
                        aliases: aliases.iter().map(|a| a.to_string()).collect(),
                    })
                    .collect(),
                relations: relations
                    .iter()
                    .map(|(src, relation, dst, fact, valid_at)| ExtractedRelation {
                        src: src.to_string(),
                        dst: dst.to_string(),
                        relation: relation.to_string(),
                        fact: fact.to_string(),
                        valid_at: valid_at.map(str::to_string),
                    })
                    .collect(),
            },
            "ruagent:test",
        )
    }

    /// G3/E5: the edges of one distillation point at the SAME episode its
    /// memories do; before this, `entity_edges.source_episode` was NULL on 0/67
    /// live rows, so no edge could answer "which session said this".
    #[tokio::test]
    async fn graph_edges_carry_the_episode_and_the_event_time_source() {
        let root = root("t9-provenance");
        let d = distiller(&root).await;
        let mems = d
            .write_memories(
                &[mem("用户在一个离线麒麟机上部署 Python。")],
                Some(("ruagent:t9-prov", "[t9] the session that knows this")),
            )
            .await
            .unwrap();
        let episode = mems.episode.expect("the pass reports its episode");

        let ex = extraction(
            &[("ruagent", &[]), ("麒麟 V10", &["银河麒麟"])],
            &[
                (
                    "ruagent",
                    "runs_on",
                    "麒麟 V10",
                    "ruagent runs on 麒麟",
                    Some("2026-08-01T00:00:00Z"),
                ),
                (
                    "ruagent",
                    "uses",
                    "麒麟 V10",
                    "ruagent uses 麒麟 without an event time",
                    None,
                ),
            ],
        );
        let report = d.write_graph(&ex, Some(episode)).await.unwrap();
        assert_eq!(report.entities, 2);
        assert_eq!(report.relations, 2);
        // The two counts issue #55 added: nothing here was a duplicate and nothing
        // was refused, and both are MEASURED zeros (the write ran).
        assert_eq!(report.duplicates, 0);
        assert_eq!(report.refused, 0);

        let rows: Vec<EdgeRow> =
            d.db.call(|conn| -> Result<Vec<EdgeRow>, rusqlite::Error> {
                let mut st = conn.prepare(
                    "SELECT relation, source_episode, event_time_source, fact_hash
                     FROM entity_edges ORDER BY id",
                )?;
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING edges: {rows:?}");
        for (relation, source, ets, hash) in &rows {
            assert_eq!(
                *source,
                Some(episode),
                "edge {relation} must point at the distillation's episode"
            );
            assert!(
                hash.as_deref().map(|h| h.len()) == Some(16),
                "edge {relation} must carry a stable fact identity"
            );
            let want = if relation == "runs_on" {
                "extracted"
            } else {
                "recorded"
            };
            assert_eq!(ets.as_deref(), Some(want), "edge {relation}");
        }
        // The alias list from the extraction resolved the second name onto the
        // first one (same real-world object) instead of creating a third row.
        let entities = count(&d.db, "SELECT count(*) FROM entities").await;
        let aliases = count(&d.db, "SELECT count(*) FROM entity_aliases").await;
        let edges = count(&d.db, "SELECT count(*) FROM entity_edges").await;
        println!(
            "READING entities={entities} aliases={aliases} edges={edges} (success path, t81 #1 control)"
        );
        assert_eq!(entities, 2);
        assert!(aliases >= 1, "the extracted alias was recorded");
        assert_eq!(edges, 2, "both relations landed in the same write");
    }

    /// D4/D5: all three outcomes leave a row, and a FAILED attempt never
    /// overwrites a recorded success (nor creates an episode).
    #[tokio::test]
    async fn the_log_records_three_states_and_a_failure_keeps_a_success() {
        let root = root("t9-states");
        let d = distiller(&root).await;
        // One real write first, so "the failure created no episode" is a
        // NON-vacuous assertion (there is already exactly one episode).
        d.write_memories(
            &[mem("用户偏好简体中文。")],
            Some(("ruagent:t9-ok", "[t9] raw")),
        )
        .await
        .unwrap();
        let episodes_before = count(&d.db, "SELECT count(*) FROM episodes").await;
        assert_eq!(episodes_before, 1);
        let ok = DistillOutcome {
            session_key: "ruagent:t9-ok".into(),
            memories_written: 3,
            memories_skipped: 0,
            entities_written: 2,
            relations_written: 1,
            agent: "dsh".into(),
            ..DistillOutcome::default()
        };
        d.log_outcome(&ok, "ok", "hash-a", None).await.unwrap();
        d.log_outcome(
            &DistillOutcome {
                session_key: "ruagent:t9-ok".into(),
                memories_written: 0,
                memories_skipped: 0,
                entities_written: 0,
                relations_written: 0,
                agent: "dsh".into(),
                ..DistillOutcome::default()
            },
            "failed",
            "hash-a",
            // THE REASON A REAL REFUSAL PRODUCES, used verbatim: this test is
            // about the ledger PRESERVING whatever the entry recorded, and this
            // sentence is what the entry records now that the missing-index-row
            // case answers in words instead of rusqlite's message (issue #45).
            Some(NOT_INDEXED_MESSAGE),
        )
        .await
        .unwrap();
        // ONE ROW PER ATTEMPT (0024): the "failure must not overwrite a success"
        // rule is now a row-level fact, so the succeeded attempt is still there
        // beside the failed one -- not a WHERE clause that had to choose.
        let ok_row: (String, Option<String>, i64) =
            d.db.call(|conn| {
                conn.query_row(
                    "SELECT COALESCE(status,'<null>'), failure_reason, memories_written
                     FROM distill_log WHERE session_key = 'ruagent:t9-ok'
                     ORDER BY id ASC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING the succeeded attempt after a later failure: {ok_row:?}");
        assert_eq!(
            ok_row.0, "ok",
            "a failure must not erase a recorded success"
        );
        assert_eq!(ok_row.2, 3);
        assert!(ok_row.1.is_none());
        let newest: String =
            d.db.call(|conn| {
                conn.query_row(
                    "SELECT COALESCE(status,'<null>') FROM distill_log
                     WHERE session_key = 'ruagent:t9-ok' ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING the newest attempt of that session: {newest}");
        assert_eq!(
            newest, "failed",
            "the newest attempt is the failure, and it is visible"
        );
        let attempts_of_ok: i64 = count(
            &d.db,
            "SELECT count(*) FROM distill_log WHERE session_key = 'ruagent:t9-ok'",
        )
        .await;
        assert_eq!(
            attempts_of_ok, 2,
            "two attempts, two rows: that is the whole fix"
        );
        // The contract's reading, verbatim: ONE success + ONE failure for the
        // same session must be TWO readable rows, each with its own status.
        let same_session: Vec<(i64, String, Option<String>, Option<String>)> =
            d.db.call(|conn| {
                let mut st = conn.prepare(
                    "SELECT id, COALESCE(status,'<null>'), failure_reason, prompt_hash
                     FROM distill_log WHERE session_key = 'ruagent:t9-ok' ORDER BY id",
                )?;
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING one session, two attempts: {same_session:?}");
        assert_eq!(same_session.len(), 2);
        assert_eq!(same_session[0].1, "ok");
        assert!(
            same_session[0].2.is_none(),
            "the success carries no failure reason"
        );
        assert_eq!(same_session[1].1, "failed");
        assert_eq!(same_session[1].2.as_deref(), Some(NOT_INDEXED_MESSAGE));
        assert_eq!(
            same_session[1].3.as_deref(),
            Some("hash-a"),
            "the failed attempt is attributable to the prompt it ran with"
        );

        // A session that never succeeded keeps its failure, with a reason.
        d.log_outcome(
            &DistillOutcome {
                session_key: "ruagent:t9-fail".into(),
                memories_written: 0,
                memories_skipped: 0,
                entities_written: 0,
                relations_written: 0,
                agent: "dsh".into(),
                ..DistillOutcome::default()
            },
            "failed",
            "hash-b",
            Some(NOT_INDEXED_MESSAGE),
        )
        .await
        .unwrap();
        let failed: (String, Option<String>, Option<String>) =
            d.db.call(|conn| {
                conn.query_row(
                    "SELECT status, failure_reason, prompt_hash FROM distill_log
                     WHERE session_key = 'ruagent:t9-fail'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING a first-attempt failure: {failed:?}");
        assert_eq!(failed.0, "failed");
        assert_eq!(failed.1.as_deref(), Some(NOT_INDEXED_MESSAGE));
        assert_eq!(
            failed.2.as_deref(),
            Some("hash-b"),
            "the prompt is attributable"
        );
        // The failure path must not have produced an episode.
        assert_eq!(
            count(&d.db, "SELECT count(*) FROM episodes").await,
            episodes_before,
            "a failed attempt must not create an episode"
        );

        // empty is its own state, distinct from a dedup-skip.
        d.log_outcome(
            &DistillOutcome {
                session_key: "ruagent:t9-empty".into(),
                memories_written: 0,
                memories_skipped: 0,
                entities_written: 0,
                relations_written: 0,
                agent: "dsh".into(),
                ..DistillOutcome::default()
            },
            "empty",
            "hash-c",
            None,
        )
        .await
        .unwrap();
        let by_state: Vec<(Option<String>, i64)> =
            d.db.call(|conn| {
                let mut st =
                    conn.prepare("SELECT status, COUNT(*) FROM distill_log GROUP BY 1 ORDER BY 1")?;
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING distill_log by status: {by_state:?}");
        let states: Vec<Option<String>> = by_state.iter().map(|(s, _)| s.clone()).collect();
        assert!(states.contains(&Some("ok".to_string())));
        assert!(states.contains(&Some("failed".to_string())));
        assert!(states.contains(&Some("empty".to_string())));

        // R-2's ACCEPTANCE READING: for this window, rows in the library MINUS
        // attempts made = 0. Before 0024 the same four attempts left ONE row.
        let attempts = 4i64;
        let rows = count(&d.db, "SELECT count(*) FROM distill_log").await;
        let recorded = count(&d.db, "SELECT count(*) FROM distill_recorded_outcomes").await;
        println!(
            "READING R-2: attempts={attempts} rows={rows} recorded_outcomes={recorded} | rows - attempts = {}",
            rows - attempts
        );
        assert_eq!(
            rows, attempts,
            "one row per attempt: 库行数 − 日志尝试数 = 0"
        );
        assert_eq!(
            recorded, attempts,
            "every attempt in this window recorded its outcome (no NULL status)"
        );
    }

    /// RVC-3: the compensation must run on the REAL failure path of the graph
    /// write, not on a direct call to the log writer. A trigger aborts every
    /// `entity_edges` insert, so `write_extraction` fails exactly the way a
    /// constraint violation would.
    #[tokio::test]
    async fn a_graph_write_failure_leaves_no_run_turn_episode() {
        let root = root("t27-void");
        let d = distiller(&root).await;
        let mems = d
            .write_memories(
                &[mem("用户在一个离线麒麟机上部署 Python。")],
                Some(("ruagent:t27-void", "[t27] the transcript of the attempt")),
            )
            .await
            .unwrap();
        let episode = mems.episode.expect("the write pass reports its episode");
        let episodes_before = count(&d.db, "SELECT count(*) FROM episodes").await;
        let run_turn_before = count(
            &d.db,
            "SELECT count(*) FROM episodes WHERE kind = 'run_turn'",
        )
        .await;
        let graph_before = graph_counts(&d).await;
        assert_eq!(episodes_before, 1);
        assert_eq!(
            run_turn_before, 1,
            "the attempt starts with a run_turn episode"
        );
        assert_eq!(
            graph_before,
            (0, 0, 0),
            "the graph is empty before the graph write"
        );

        // Poison the real write path.
        d.db.call(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER t27_boom BEFORE INSERT ON entity_edges
                 BEGIN SELECT RAISE(ABORT, 't27 injected failure'); END;",
            )
        })
        .await
        .unwrap()
        .unwrap();

        let ex = extraction(
            &[("ruagent", &[]), ("麒麟 V10", &["银河麒麟"])],
            &[(
                "ruagent",
                "runs_on",
                "麒麟 V10",
                "ruagent runs on 麒麟",
                None,
            )],
        );
        let err = d
            .write_extraction(&ex, Some(episode))
            .await
            .expect_err("the poisoned graph write must fail");
        println!("the real failure: {err:#}");

        let episodes_after = count(&d.db, "SELECT count(*) FROM episodes").await;
        let run_turn_after = count(
            &d.db,
            "SELECT count(*) FROM episodes WHERE kind = 'run_turn'",
        )
        .await;
        let kind: String =
            d.db.call(move |conn| {
                conn.query_row("SELECT kind FROM episodes WHERE id = ?1", [episode], |r| {
                    r.get(0)
                })
            })
            .await
            .unwrap()
            .unwrap();
        let meta: Option<String> =
            d.db.call(move |conn| {
                conn.query_row("SELECT meta FROM episodes WHERE id = ?1", [episode], |r| {
                    r.get(0)
                })
            })
            .await
            .unwrap()
            .unwrap();
        let with_source = count(
            &d.db,
            "SELECT count(*) FROM memories WHERE source_episode IS NOT NULL",
        )
        .await;
        // t81 (audit #1): BEFORE the fix this graph half was N autocommit writes,
        // so a failure at the relation left `entities 2 … aliases 1` behind. The
        // graph part of the attempt must now read exactly as it did before it.
        let graph_after = graph_counts(&d).await;
        println!(
            "READING RVC-3: episodes {episodes_before} -> {episodes_after} | kind=run_turn {run_turn_before} -> {run_turn_after} | episode kind={kind:?} | meta={meta:?} | memories keeping their provenance {with_source}"
        );
        println!(
            "READING t81 #1: failed graph write -> entities {}/{} aliases {}/{} edges {}/{} \
             (before/after; 0 residue is the invariant)",
            graph_before.0,
            graph_after.0,
            graph_before.1,
            graph_after.1,
            graph_before.2,
            graph_after.2
        );
        assert_eq!(
            graph_after, graph_before,
            "a failed distillation must leave the graph exactly as it was: entities, aliases and \
             edges all unchanged (the audit measured 2 entities + 1 alias surviving here)"
        );
        assert_eq!(
            episodes_after, episodes_before,
            "the failed attempt must not ADD or DELETE an episode row"
        );
        assert_eq!(
            run_turn_after, 0,
            "a failed graph write must leave NO run_turn episode: that is what the panel's badge reads"
        );
        assert_eq!(
            kind, "run_turn_failed",
            "the episode is MARKED, not deleted"
        );
        assert!(
            meta.unwrap_or_default().contains("voided_by"),
            "the marking says who voided it"
        );
        assert_eq!(with_source, 1, "the memory keeps pointing at its episode");
    }

    /// A deterministic, NON-fallback test embedder: the trait's `is_fallback`
    /// defaults to false, which is what makes `merge_target` take the
    /// `judge_merge` branch (the hash fallback's cosine is not evidence).
    struct TestEmbedder;

    impl ruagent_knowledge::embed::Embedder for TestEmbedder {
        fn embed(
            &self,
            texts: &[&str],
        ) -> Result<Vec<Vec<f32>>, ruagent_knowledge::embed::EmbedError> {
            Ok(texts
                .iter()
                .map(|t| {
                    let mut v = vec![0f32; 8];
                    let chars: Vec<char> = t.chars().collect();
                    for pair in chars.windows(2) {
                        let mut h = 0u32;
                        for c in pair {
                            h = h.wrapping_mul(31).wrapping_add(*c as u32);
                        }
                        v[(h % 8) as usize] += 1.0;
                    }
                    for c in &chars {
                        v[(*c as usize) % 8] += 0.5;
                    }
                    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if norm > 0.0 {
                        for x in &mut v {
                            *x /= norm;
                        }
                    }
                    v
                })
                .collect())
        }
        fn name(&self) -> &'static str {
            "t27-test-embedder"
        }
        fn dim(&self) -> usize {
            8
        }
    }

    /// R-3: `memembed::judge_merge` had NO production caller (live
    /// `op='merge_judged'` = 0 rows). This drives a REAL write path with a real
    /// (non-fallback) embedder and asserts the audit rows appear, with the
    /// bounded candidate count and the verdict visible in the reason.
    #[tokio::test]
    async fn the_write_path_asks_the_bounded_merge_judge_and_audits_the_decision() {
        let root = root("t27-judge");
        let mut d = distiller(&root).await;
        d.embedder = Some(std::sync::Arc::new(TestEmbedder));
        let before = count(
            &d.db,
            "SELECT count(*) FROM memory_diffs WHERE op = 'merge_judged'",
        )
        .await;
        let out = d
            .write_memories(
                &[
                    mem("用户偏好使用简体中文交流。"),
                    mem("用户偏好使用简体中文进行交流。"),
                ],
                Some(("ruagent:t27-judge", "[t27] the transcript")),
            )
            .await
            .unwrap();
        let after = count(
            &d.db,
            "SELECT count(*) FROM memory_diffs WHERE op = 'merge_judged'",
        )
        .await;
        let rows: Vec<(Option<String>, Option<String>)> =
            d.db.call(|conn| {
                let mut st = conn.prepare(
                    "SELECT before, reason FROM memory_diffs WHERE op = 'merge_judged' ORDER BY id",
                )?;
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await
            .unwrap()
            .unwrap();
        let live = count(
            &d.db,
            "SELECT count(*) FROM memories WHERE superseded_at IS NULL",
        )
        .await;
        let superseded = count(
            &d.db,
            "SELECT count(*) FROM memories WHERE superseded_at IS NOT NULL",
        )
        .await;
        println!(
            "READING R-3: merge_judged rows {before} -> {after} | written={} skipped={} | live={live} superseded={superseded}",
            out.written, out.skipped
        );
        for r in &rows {
            println!("   audit: before={:?} reason={:?}", r.0, r.1);
        }
        assert!(
            after > before,
            "the write path must call judge_merge: op='merge_judged' was {before} and is {after}"
        );
        assert_eq!(after - before, 2, "one decision per extracted memory");
        assert!(
            rows.iter().all(|(_, reason)| {
                let r = reason.as_deref().unwrap_or("");
                // mem-core's audited shape (t31): the candidate count, WHERE the
                // candidate came from, WHICH row the decision followed, and the
                // verdict. Asserting only `rule=`/`verdict=` would let a decision
                // that followed the wrong anchor pass.
                r.contains("candidates=")
                    && r.contains("source=")
                    && r.contains("anchor=")
                    && r.contains("rule=")
                    && r.contains("verdict=")
            }),
            "every audit row names the candidate count, its source, the anchor and the verdict: {rows:?}"
        );
    }

    /// C1 (asked by mem-core): the agent's confidence is kept verbatim, so a
    /// hedged value below the 0.5 floor can finally reach the database.
    #[tokio::test]
    async fn a_hedged_confidence_is_not_silently_raised_to_the_floor() {
        let root = root("t9-confidence");
        let d = distiller(&root).await;
        let mut hedged = mem("用户可能偏好简体中文。");
        hedged.confidence = ruagent_extract::CandidateConfidence::Explicit(0.4);
        let mut unstated = mem("用户使用双屏显示器。");
        unstated.confidence = ruagent_extract::CandidateConfidence::Unconfirmed;
        let out = d
            .write_memories(
                &[hedged, unstated],
                Some(("ruagent:t9-conf", "[t9] signals")),
            )
            .await
            .unwrap();
        assert_eq!(out.written, 2);
        let rows: Vec<(String, f64)> =
            d.db.call(|conn| {
                let mut st =
                    conn.prepare("SELECT content, confidence FROM memories ORDER BY id")?;
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await
            .unwrap()
            .unwrap();
        for (content, conf) in &rows {
            println!("READING confidence({content}) = {conf}");
        }
        let below = rows.iter().filter(|(_, c)| *c < 0.5).count();
        assert_eq!(
            below, 1,
            "the hedged row keeps 0.4; `clamp(0.5, 1.0)` made this unreachable (0/157 live rows)"
        );
        assert_eq!(
            rows.iter()
                .find(|(c, _)| c.contains("双屏"))
                .map(|(_, c)| *c),
            Some(ruagent_memory::confidence::CONF_UNCONFIRMED),
            "an unstated confidence is the unconfirmed default, from the single source"
        );
    }

    /// D3 gate 1: the MEMORIES half of the extraction prompt is byte-identical
    /// to what shipped before gen2 -- adding fields to the graph half must not
    /// move the bytes the memory extraction reads.
    #[test]
    fn the_memories_half_of_the_prompt_is_byte_identical() {
        let p = extraction_prompt(None, None);
        let memories_bullet = "1. memories: durable facts about the user (profile), observations, procedures (how-to knowledge), and lessons learned. Only things that generalize beyond this single session. Each memory carries a confidence (0.5\u{2013}1.0) from the signals above.";
        let memories_json = "  \"memories\": [{\"store\": \"profile|observation|procedure|lesson\", \"namespace\": \"user|global|project:<name>\", \"content\": \"...\", \"confidence\": 0.8}],";
        assert!(
            p.contains(memories_bullet),
            "the memories bullet moved: gate 1 of mem-core's D3 review"
        );
        assert!(
            p.contains(memories_json),
            "the memories JSON example moved: gate 1 of mem-core's D3 review"
        );
        // Gate 2: the event-time rule is stated, and it says null explicitly.
        assert!(p.contains("MUST be null"), "the null rule is in the prompt");
        assert!(p.contains("NEVER put the current time there"));
        // The graph half gained the two new fields.
        assert!(p.contains("\"aliases\": [\"...\"]"));
        assert!(p.contains("\"valid_at\":"));
    }
}

/// The distillation ENTRY's own contract.
///
/// ruagent-issues-45-55 t4 / issue #45: index before you read, refuse in sentences
/// a person can act on, and retry a failure a BOUNDED number of times while
/// recording how many retries it took.
///
/// ruagent-issues-45-55 t5 / issues #48 and #55: read a long session in WINDOWS and
/// record the coverage and whether anything was cut; carry the agent's own usage
/// into the ledger; carry the graph writer's duplicate/refused counts the same way.
/// The layout, distiller and ledger helpers above are shared by both halves on
/// purpose: they are the same entry, and two fixtures would be two chances for the
/// readings to disagree.
#[cfg(test)]
mod entry_tests {
    use super::*;

    /// (home, root) in the daemon's OWN layout: root is <home>/.ruagent,
    /// transcripts live under <root>/data/transcripts, and the session index is
    /// derived from the PARENT (see sessions::SessionIndexer::for_daemon_root). A
    /// distiller built on this root can therefore only ever index the fixture: the
    /// parent is a private temporary directory, so no pass can reach the real
    /// machine's ~/.claude, ~/.dsh or ~/.codex stores.
    ///
    /// The wipe is the fixture and not hygiene (t154): the name carries the pid,
    /// pids are reused, and a database left under a reused name makes every
    /// assertion below a reading of somebody else's run.
    fn daemon_layout(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let home =
            std::env::temp_dir().join(format!("ruagent-t45-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let root = home.join(".ruagent");
        std::fs::create_dir_all(root.join("data").join("transcripts")).unwrap();
        (home, root)
    }

    /// The transcript a real chat leaves behind, with a PREF marker so the
    /// DETERMINISTIC tier has something to extract. Every pass in this module runs
    /// the free tier: a plan with the ACP tier in it would spawn a real harness and
    /// spend a real turn.
    const TRANSCRIPT: &str = "{\"ts\":\"2026-09-26T14:00:01Z\",\"seq\":1,\"event\":{\"type\":\"user_message\",\"text\":\"记住：以后回答都要简洁，不要长篇大论。\"}}\n{\"ts\":\"2026-09-26T14:00:02Z\",\"seq\":2,\"event\":{\"type\":\"agent_message_chunk\",\"text\":\"understood\"}}\n";

    /// A chat opened and closed inside ten seconds: the harness wrote its session
    /// header and no turn was ever exchanged. The index refuses such a file a row
    /// ON PURPOSE (a spawn that never got an answer is not a session), which is the
    /// shape issue #45's second acceptance names.
    const TEN_SECOND_TRANSCRIPT: &str =
        "{\"ts\":\"2026-09-26T14:00:00Z\",\"seq\":0,\"event\":{\"type\":\"session_start\"}}\n";

    fn rules_plan() -> crate::extract_plane::ExtractPlan {
        crate::extract_plane::ExtractPlan::explicit(crate::extract_plane::Extractor::Rules)
    }

    /// The same plan, with the transcript's byte budget moved down so a NORMAL
    /// fixture is longer than one input: the windowing path is otherwise
    /// unreachable without a megabyte of text.
    fn rules_plan_with_budget(bytes: usize) -> crate::extract_plane::ExtractPlan {
        let mut plan = rules_plan();
        plan.limits.max_input_bytes = bytes;
        plan
    }

    /// A rules-tier candidate set for a graph write, built here because the free
    /// tier produces entities and relations only for the knowledge path.
    fn bundle_with(
        entities: &[(&str, &[&str])],
        relations: &[(&str, &str, &str, &str)],
    ) -> crate::extract_plane::ExtractBundle {
        crate::extract_plane::ExtractBundle {
            memories: Vec::new(),
            entities: entities
                .iter()
                .map(|(name, aliases)| ruagent_extract::EntityCandidate {
                    name: (*name).to_string(),
                    kind: Some("tool"),
                    summary: None,
                    aliases: aliases.iter().map(|a| (*a).to_string()).collect(),
                    score: 1.0,
                    rule: "acp",
                    origin: Default::default(),
                    source: String::new(),
                })
                .collect(),
            relations: relations
                .iter()
                .map(
                    |(src, dst, name, fact)| ruagent_extract::RelationCandidate {
                        src: (*src).to_string(),
                        dst: (*dst).to_string(),
                        relation: ruagent_extract::RelationName::Model((*name).to_string()),
                        fact: (*fact).to_string(),
                        valid_at: None,
                        score: 1.0,
                        rule: "acp",
                        origin: Default::default(),
                        source: String::new(),
                    },
                )
                .collect(),
            source: Some(crate::extract_plane::ExtractSource::Acp),
            truncated: false,
            usage: crate::extract_plane::UsageReport::default(),
        }
    }

    /// One user turn of a transcript fixture, at second `i` of a fixed minute.
    fn user_turn(i: usize, text: &str) -> String {
        format!(
            "{{\"ts\":\"2026-09-26T14:{:02}:00Z\",\"seq\":{},\"event\":{{\"type\":\"user_message\",\"text\":\"{}\"}}}}\n",
            i % 60,
            i + 1,
            text
        )
    }

    fn distiller_at(root: &std::path::Path) -> Distiller {
        Distiller {
            db: ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap(),
            root: root.to_path_buf(),
            embedder: None,
            // Empty on purpose and never consulted: the rules tier needs no agent.
            registry: crate::distill::AgentRegistry::default(),
            language: None,
            prompt_override: None,
            // Memories only. The graph half has its own tests, and a graph failure
            // here would redden a test about retries for an unrelated reason.
            graph: false,
        }
    }

    fn write_transcript(root: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = root.join("data").join("transcripts").join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    fn indexer_for(d: &Distiller) -> crate::sessions::SessionIndexer {
        crate::sessions::SessionIndexer::for_daemon_root(d.db.clone(), &d.root)
    }

    /// The NEWEST attempt's (status, reason, retries) for one session key.
    /// retries is read through a sentinel, so a NULL (a row written before 0028)
    /// cannot pass for a measured 0.
    async fn newest_attempt(db: &ruagent_store::Db, key: &str) -> (String, String, i64) {
        let key = key.to_string();
        db.call_flat(move |conn| {
            conn.query_row(
                "SELECT COALESCE(status,'<null>'), COALESCE(failure_reason,''),
                        COALESCE(retries,-1)
                 FROM distill_log WHERE session_key = ?1 ORDER BY id DESC LIMIT 1",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .await
        .unwrap()
    }

    /// The coverage range of the NEWEST attempt for one session, read through
    /// SENTINELS (`-1`) so a NULL can never pass for a measured 0 -- the same
    /// shape `newest_attempt` uses for `retries`, and the reason this helper
    /// exists at all rather than each test writing its own `COALESCE`.
    async fn newest_coverage(db: &ruagent_store::Db, key: &str) -> (i64, i64, i64) {
        let key = key.to_string();
        db.call_flat(move |conn| {
            conn.query_row(
                "SELECT COALESCE(from_seq,-1), COALESCE(to_seq,-1),
                        COALESCE(total_messages,-1)
                 FROM distill_log WHERE session_key = ?1 ORDER BY id DESC LIMIT 1",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .await
        .unwrap()
    }

    async fn attempts_of(db: &ruagent_store::Db, key: &str) -> i64 {
        let key = key.to_string();
        db.call_flat(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM distill_log WHERE session_key = ?1",
                [&key],
                |r| r.get(0),
            )
        })
        .await
        .unwrap()
    }

    /// The acceptance's own reading, in one place: none of the storage layer's
    /// vocabulary reaches a text a user (or the panel) reads.
    fn assert_sql_free(text: &str) {
        for fragment in [
            "SELECT",
            "INSERT",
            "UPDATE",
            "DELETE",
            "CREATE TABLE",
            "sqlite",
            "pragma",
            "no such table",
            "no such column",
            "Query returned no rows",
            "query_row",
        ] {
            assert!(
                !text.contains(fragment),
                "{text:?} must not carry the storage layer's {fragment:?}"
            );
        }
    }

    /// The user-visible vocabulary of a refusal is two sentences and nothing else,
    /// and which LEDGER STATUS each one gets is part of the contract: an empty
    /// session is not a failure, and a session we could not read is not empty.
    #[test]
    fn the_refusals_are_sentences_and_their_statuses_are_not_interchangeable() {
        assert_eq!(NO_CONTENT_MESSAGE, "没有可蒸馏的内容");
        assert_eq!(NOT_INDEXED_MESSAGE, "该会话尚未进入索引，正在同步");
        assert_sql_free(NO_CONTENT_MESSAGE);
        assert_sql_free(NOT_INDEXED_MESSAGE);
        assert_eq!(DistillRefusal::NoContent.status(), "empty");
        assert_eq!(DistillRefusal::NotIndexed.status(), "failed");
        // A refusal is terminal or transient BY KIND, which is why it is a type:
        // re-reading an empty conversation cannot change the answer, while the index
        // pass every attempt re-runs is exactly what can heal a missing row.
        assert!(!is_retryable(&anyhow::Error::new(
            DistillRefusal::NoContent
        )));
        assert!(is_retryable(&anyhow::Error::new(
            DistillRefusal::NotIndexed
        )));
    }

    /// The backoff grows, and the budget is the one the documentation claims.
    #[test]
    fn the_retry_waits_grow_and_the_budget_is_bounded() {
        assert_eq!(retry_wait(1), RETRY_FIRST_WAIT);
        assert_eq!(retry_wait(2), RETRY_FIRST_WAIT * 2u32);
        assert!(
            retry_wait(2) > retry_wait(1),
            "the waits must increase: {:?} then {:?}",
            retry_wait(1),
            retry_wait(2)
        );
        println!(
            "READING t45 retry budget: {RETRY_LIMIT} retries, waits {:?} then {:?}",
            retry_wait(1),
            retry_wait(2)
        );
    }

    /// ACCEPTANCE (issue #45): the entry INDEXES the session before it reads it.
    ///
    /// The fixture is a transcript on disk with NO sessions row and no scan -- the
    /// state a chat is in the moment it closes, and the state that used to answer
    /// "Query returned no rows". The pass must succeed, the index row must exist
    /// afterwards, and the ledger must record the retries it actually took (0: the
    /// first attempt found the file, because the entry indexed it).
    #[tokio::test]
    async fn the_entry_indexes_the_session_before_reading_it() {
        let (home, root) = daemon_layout("index-first");
        let d = distiller_at(&root);
        let path = write_transcript(&root, "run-t45-a.jsonl", TRANSCRIPT);
        let key = crate::sessions::session_key_of(&path);

        // The PREMISE, asserted rather than assumed: no row yet, so a reader that
        // only looked would have to report a session that does not exist.
        let indexer = indexer_for(&d);
        assert!(
            indexer.locate(&key).await.unwrap().is_none(),
            "the fixture must start UNINDEXED, or this test proves nothing"
        );

        let out = d
            .distill_plan(&key, None, rules_plan(), false)
            .await
            .expect("the entry must index the session instead of failing on a missing row");
        assert!(
            out.memories_written > 0,
            "the marked sentence must have been extracted, so this is a REAL pass: {out:?}"
        );
        assert_eq!(
            out.retries, 0,
            "the first attempt found the file: the entry indexed it before reading"
        );

        // The row the distiller needed is there now, written by its own pass.
        let located = indexer.locate(&key).await.unwrap();
        assert_eq!(
            located.map(|(source, _)| source).as_deref(),
            Some("ruagent"),
            "the pass must have indexed the transcript it read"
        );
        let (status, reason, retries) = newest_attempt(&d.db, &key).await;
        println!("READING unindexed-then-distilled: {status} {reason:?} retries={retries}");
        assert_eq!(status, "ok");
        assert_eq!(reason, "", "a success carries no reason");
        assert_eq!(retries, 0);
        assert_eq!(attempts_of(&d.db, &key).await, 1, "one pass, one row");
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #45): the ten-second chat is RECORDED as "nothing to
    /// distil", and the text is a sentence rather than the storage layer's.
    ///
    /// The refusal is terminal, so retries is 0: three attempts would read the same
    /// empty conversation, and recording 2 would claim a transient fault that never
    /// happened.
    #[tokio::test]
    async fn a_session_with_nothing_in_it_is_recorded_empty_not_failed() {
        let (home, root) = daemon_layout("ten-seconds");
        let d = distiller_at(&root);
        let path = write_transcript(&root, "run-t45-empty.jsonl", TEN_SECOND_TRANSCRIPT);
        let key = crate::sessions::session_key_of(&path);

        let err = d
            .distill_plan(&key, None, rules_plan(), false)
            .await
            .expect_err("a session with no turn in it is refused, not silently cleared");
        println!("READING the ten-second chat: {err}");
        assert_eq!(err.to_string(), NO_CONTENT_MESSAGE);
        assert_sql_free(&err.to_string());

        let (status, reason, retries) = newest_attempt(&d.db, &key).await;
        assert_eq!(
            status, "empty",
            "nothing to distil is the EMPTY state, not a failure: {reason:?}"
        );
        assert_eq!(reason, NO_CONTENT_MESSAGE);
        assert_sql_free(&reason);
        assert_eq!(retries, 0, "a terminal refusal is not retried");
        assert_eq!(attempts_of(&d.db, &key).await, 1);
        // t3 / issue #58: a REFUSAL read nothing, so the coverage columns stay
        // NULL. The `-1` sentinels are what make that a reading about NULL rather
        // than about `COALESCE(...,0)` -- a 0 here would be a fabricated range
        // ("read message 0 to message 0") for a pass that never looked.
        assert_eq!(
            newest_coverage(&d.db, &key).await,
            (-1, -1, -1),
            "a refused pass must leave all three coverage columns NULL, not 0"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #45): a failure is RETRIED, the ledger says how many
    /// retries it took, and the reason is a sentence -- not "Query returned no
    /// rows", which is what this exact case used to record.
    #[tokio::test]
    async fn a_doomed_session_is_retried_to_the_bound_and_reads_sql_free() {
        let (home, root) = daemon_layout("doomed");
        let d = distiller_at(&root);
        let key = "ruagent:t45-never-indexed";

        let started = std::time::Instant::now();
        let err = d
            .distill_plan(key, None, rules_plan(), false)
            .await
            .expect_err("a key no pass can resolve must not be reported as a success");
        let elapsed = started.elapsed();
        println!("READING a doomed pass: {err} (after {elapsed:?})");
        assert_eq!(err.to_string(), NOT_INDEXED_MESSAGE);

        let (status, reason, retries) = newest_attempt(&d.db, key).await;
        assert_eq!(status, "failed");
        assert_eq!(
            reason, NOT_INDEXED_MESSAGE,
            "the ledger must carry the actionable sentence, not the SQL machine's"
        );
        assert_sql_free(&reason);
        assert_eq!(
            retries, RETRY_LIMIT as i64,
            "the first attempt plus {RETRY_LIMIT} retries"
        );
        assert!(
            elapsed >= retry_wait(1) + retry_wait(2),
            "the waits must have been TAKEN, not merely counted: {elapsed:?} for {:?} + {:?}",
            retry_wait(1),
            retry_wait(2)
        );
        assert_eq!(
            attempts_of(&d.db, key).await,
            1,
            "retries are attempts INSIDE one pass, not one ledger row each"
        );
        // t3 / issue #58: a FAILURE read nothing either, so the three coverage
        // columns are NULL on its row as well -- `retries = 2` beside them is the
        // point: the pass tried three times and still never observed a range.
        assert_eq!(
            newest_coverage(&d.db, key).await,
            (-1, -1, -1),
            "a failed pass must leave all three coverage columns NULL, not 0"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #45): the retry loop carries a failure that CLEARS, does
    /// NOT retry a refusal, and stops at the bound on a failure that never clears.
    ///
    /// THE ATTEMPTS ARE COUNTED BY THE CLOSURE, so this is a reading of the loop
    /// itself: no clock, no database, no ordering assumption. It drives the real
    /// \`with_retries\` -- the function every pass in \`distill_plan\` goes through --
    /// rather than a copy of it.
    ///
    /// WHY THERE IS NO END-TO-END "healed pass" TEST BESIDE IT: a one-shot failure
    /// cannot be injected through SQL here. \`RAISE(ABORT)\` rolls back the
    /// transaction it fired in, so a trigger that decrements its own counter does
    /// not decrement it; and an arm/disarm pair needs a statement that runs in the
    /// SECOND attempt and not the first, which by construction does not exist --
    /// attempt 2 finds the session row and the episode row attempt 1 wrote, so it
    /// skips exactly the writes that could disarm it. The remaining option, a
    /// spawned task that clears the fault after a sleep, would make this test's
    /// outcome depend on how long it waited, which is the shape this repo refuses.
    #[tokio::test]
    async fn the_loop_retries_a_transient_failure_and_stops_at_the_bound() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // (1) A failure that CLEARS: one retry, then the value.
        let calls = AtomicU32::new(0);
        let (out, retries) = with_retries("ruagent:t45-loop", || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                if n == 0 {
                    Err(anyhow::anyhow!("transient"))
                } else {
                    Ok(n)
                }
            }
        })
        .await;
        assert_eq!(out.unwrap(), 1, "the second attempt's value");
        assert_eq!(retries, 1, "one failure, one retry, then success");
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        // (2) A REFUSAL is not retried: re-reading an empty conversation cannot
        //     change the answer, and a count of 2 would claim a transient fault
        //     that never existed.
        let calls = AtomicU32::new(0);
        let (out, retries) = with_retries("ruagent:t45-loop", || {
            let _ = calls.fetch_add(1, Ordering::SeqCst);
            async move { Err::<(), _>(anyhow::Error::new(DistillRefusal::NoContent)) }
        })
        .await;
        assert!(out.is_err());
        assert_eq!(retries, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no second attempt");

        // (3) A failure that never clears stops AT THE BOUND, and the waits are
        //     taken. The elapsed check is a LOWER bound on sleeps the loop owes,
        //     which is the only kind of timing assertion that cannot flake.
        let calls = AtomicU32::new(0);
        let started = std::time::Instant::now();
        let (out, retries) = with_retries("ruagent:t45-loop", || {
            let _ = calls.fetch_add(1, Ordering::SeqCst);
            async move { Err::<(), _>(anyhow::anyhow!("permanent")) }
        })
        .await;
        assert!(out.is_err());
        assert_eq!(retries, RETRY_LIMIT);
        assert_eq!(calls.load(Ordering::SeqCst), RETRY_LIMIT + 1);
        assert!(
            started.elapsed() >= retry_wait(1) + retry_wait(2),
            "the waits must have been taken, not merely counted: {:?}",
            started.elapsed()
        );
    }

    /// t23 (from t14-F1): an ASSISTANT-ONLY session is DISTILLED, not refused -- and
    /// that is pinned here rather than left incidental.
    ///
    /// The note this test was written for used to claim the refusal covered "the
    /// agent's own output". It does not: `user_turns` drops only a turn whose text
    /// after the daemon's sentinel is empty, and `sessions::user_text` returns a
    /// message with no sentinel untouched -- so an assistant turn is KEPT and read as
    /// `Agent: ...`. What must hold is the whole reading, not just the absence of an
    /// error: the entry answers Ok, the ledger says `empty` with NO reason (an empty
    /// pass is an answer, not a failure), and the pass reports the window it read.
    ///
    /// The two turns say `understood` and `done, nothing to do` ON PURPOSE: neither
    /// carries a rule marker (PREF/CORRECT/CONFIRM/...), so the zero memories below is
    /// a reading about the FILTER, not about the rules tier declining one sentence.
    /// Whether such a session SHOULD be refused is an open product question registered
    /// at the refusal site; this test freezes the behaviour that exists so a later
    /// decision changes it on purpose.
    #[tokio::test]
    async fn an_assistant_only_session_is_distilled_and_answers_empty() {
        let (home, root) = daemon_layout("assistant-only");
        let d = distiller_at(&root);
        // The REAL transcript shape for an agent reply: chunks carry a `content`
        // BLOCK ARRAY and are flushed into ONE assistant message by `stopped`
        // (`sessions::parse_ruagent`). A chunk written with a bare `text` field is
        // silently dropped by that parser, so a fixture spelled that way would prove
        // nothing about an assistant turn -- it would produce a session with no
        // messages at all.
        let body = r#"{"ts":"2026-09-26T15:00:01Z","seq":1,"event":{"type":"agent_message_chunk","content":[{"type":"text","text":"understood"}]}}
{"ts":"2026-09-26T15:00:02Z","seq":2,"event":{"type":"stopped"}}
"#;
        let path = write_transcript(&root, "run-t23-assistant-only.jsonl", body);
        let key = crate::sessions::session_key_of(&path);

        let out = d
            .distill_plan(&key, None, rules_plan(), false)
            .await
            .expect(
                "an assistant-only session is NOT refused (t23): user_turns keeps the                  agent's own turns",
            );
        println!("READING assistant-only pass: {out:?}");
        assert_eq!(
            out.memories_written, 0,
            "the agent's turns carry no durable statement of the user's: {out:?}"
        );
        assert_eq!(
            out.windows,
            Some(1),
            "the pass DID read the session -- one window -- so this is a pass, not a              refusal: {out:?}"
        );
        assert!(!out.truncated, "and nothing was cut: {out:?}");

        let (status, reason, retries) = newest_attempt(&d.db, &key).await;
        println!("READING assistant-only ledger: {status} {reason:?} retries={retries}");
        assert_eq!(
            status, "empty",
            "read, and nothing worth keeping in it -- 0024's third state"
        );
        assert_eq!(
            reason, "",
            "an empty pass carries NO failure reason: a refusal would have recorded one              (the two are different rows with the same status)"
        );
        assert_eq!(retries, 0, "nothing failed, so nothing was retried");
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (t3 / issue #58): the RANGE the pass read reaches the ledger,
    /// beside the numeric half of the same measurement — which must not move.
    ///
    /// Before 0030 the range existed only in this pass's `DistillOutcome` and died
    /// with the HTTP response, so the ledger could say `windows = 1,
    /// truncated = 0` and nothing about WHICH MESSAGES that window held. What is
    /// asserted here is the pair: the three new columns equal the coverage the
    /// pass MEASURED (`Some` on both sides — a `None` compared to `None` would
    /// prove nothing), and `windows`/`truncated` still read exactly what they read
    /// before the columns existed.
    ///
    /// The fixture is three user turns on the free tier: no agent, no model call,
    /// no spend. `from_message` is INCLUSIVE and `to_message` EXCLUSIVE, so a
    /// session read end to end has `(0, total, total)`.
    #[tokio::test]
    async fn the_coverage_range_reaches_the_ledger_beside_the_numeric_half() {
        let (home, root) = daemon_layout("coverage-columns");
        let d = distiller_at(&root);
        let mut body = String::new();
        for i in 0..3 {
            // PREF markers, so the rules tier has something to keep and this is a
            // pass with a NON-empty result -- the shape a reader meets in
            // production.
            body.push_str(&user_turn(
                i,
                &format!("记住：约定 {i} — 值班表编号 {i} 用值班人 operator-{i}。"),
            ));
        }
        let path = write_transcript(&root, "run-t3-coverage.jsonl", &body);
        let key = crate::sessions::session_key_of(&path);

        let out = d
            .distill_plan(&key, None, rules_plan(), false)
            .await
            .expect("a three-turn session must distil");
        let coverage = out
            .coverage
            .expect("an extraction loop ran, so the coverage is a MEASUREMENT");
        println!("READING coverage pass: status-worthy outcome {out:?} coverage {coverage:?}");
        assert_eq!(
            (out.windows, out.truncated),
            (Some(1), false),
            "this fixture fits one window and loses nothing: {out:?}"
        );
        assert_eq!(
            coverage.total_messages, 3,
            "three user turns, and the filter kept all three: {coverage:?}"
        );
        assert_eq!(
            (coverage.from_message, coverage.to_message),
            (0, 3),
            "inclusive start, exclusive end, over the whole list: {coverage:?}"
        );

        let (status, _, _) = newest_attempt(&d.db, &key).await;
        assert_eq!(status, "ok", "the ledger's own reading of the pass");
        // THE THREE COLUMNS, read back INDIVIDUALLY and compared to the coverage
        // the pass reported -- not to a literal, so this cannot pass by both sides
        // being wrong in the same way.
        assert_eq!(
            newest_coverage(&d.db, &key).await,
            (
                coverage.from_message as i64,
                coverage.to_message as i64,
                coverage.total_messages as i64
            ),
            "the ledger's range must be the range the pass MEASURED"
        );
        // ... and the numeric half is unchanged. Read from the SAME row, so the
        // two halves are a reading of one attempt rather than of two.
        let (w, t): (Option<i64>, Option<i64>) =
            d.db.call_flat({
                let key = key.clone();
                move |conn| {
                    conn.query_row(
                        "SELECT windows, truncated FROM distill_log WHERE session_key = ?1 \
                         ORDER BY id DESC LIMIT 1",
                        [&key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                }
            })
            .await
            .unwrap();
        assert_eq!(w, Some(1), "windows is the count that ran, unchanged");
        assert_eq!(
            t,
            Some(0),
            "a MEASURED lossless read is 0, not NULL -- unchanged by 0030"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #48): a session larger than one input is read IN WINDOWS,
    /// and the ledger says how many and whether any text was lost.
    ///
    /// The fixture is 40 user turns (~4 KB) against a 1 KiB budget, so the split is
    /// the SUBJECT rather than an accident. What is asserted is the whole reading:
    /// more than one window ran, the windows' union is the whole session, the bytes
    /// read exceed one budget (so it really was cut into several), nothing was LOST,
    /// and the ledger carries the count with a MEASURED truncated = 0.
    #[tokio::test]
    async fn a_session_over_the_budget_is_read_in_windows_and_the_ledger_says_so() {
        let (home, root) = daemon_layout("windows");
        let d = distiller_at(&root);
        let mut body = String::new();
        for i in 0..40 {
            // Distinct sentences on purpose: the writer's near-duplicate rule must
            // not be what decides the count below.
            body.push_str(&user_turn(
                i,
                &format!(
                    "记住：约定 {i} — 服务器 server-{i} 的负责人是 operator-{i}，值班表编号 {i}。"
                ),
            ));
        }
        let path = write_transcript(&root, "run-t5-windows.jsonl", &body);
        let key = crate::sessions::session_key_of(&path);

        let out = d
            .distill_plan(&key, None, rules_plan_with_budget(1024), false)
            .await
            .expect("a session over the budget must distil, in windows");
        let windows = out
            .windows
            .expect("an extraction loop ran, so the window count is a reading");
        let coverage = out.coverage.expect("and so is the coverage");
        println!("READING windowed pass: windows={windows} coverage={coverage:?}");
        assert!(windows > 1, "the fixture must exceed one window: {windows}");
        assert_eq!(
            coverage.from_message, 0,
            "the range starts at the first turn"
        );
        assert_eq!(
            coverage.to_message, coverage.total_messages,
            "and ends at the last one: the union of the windows is the session"
        );
        assert!(
            coverage.total_messages >= 40,
            "the fixture's own turns: {coverage:?}"
        );
        assert_eq!(coverage.budget_bytes, 1024);
        assert!(
            coverage.bytes > 1024,
            "the pass read MORE than one budget's worth, which is what windowing              means: {coverage:?}"
        );
        assert!(
            !out.truncated,
            "windowing is LOSSLESS: every turn went into a window, so nothing was cut"
        );
        assert!(
            out.memories_written >= 2,
            "several windows' candidates were written: {out:?}"
        );

        let (status, _, _) = newest_attempt(&d.db, &key).await;
        assert_eq!(status, "ok");
        let (w, t): (Option<i64>, Option<i64>) =
            d.db.call_flat({
                let key = key.clone();
                move |conn| {
                    conn.query_row(
                        "SELECT windows, truncated FROM distill_log WHERE session_key = ?1 \
                         ORDER BY id DESC LIMIT 1",
                        [&key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                }
            })
            .await
            .unwrap();
        assert_eq!(w, Some(windows as i64), "windows is the count that ran");
        assert_eq!(
            t,
            Some(0),
            "a MEASURED lossless read is 0, not NULL: the pass looked and nothing was cut"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #48): a turn too large for one window CANNOT be windowed
    /// further, so it is CUT — and the cut is recorded rather than silent.
    #[tokio::test]
    async fn a_turn_larger_than_the_budget_is_cut_and_the_ledger_says_so() {
        let (home, root) = daemon_layout("cut");
        let d = distiller_at(&root);
        let long = format!(
            "记住：{}",
            "这是一个很长的句子，用来超过字节预算。".repeat(80)
        );
        let body = user_turn(0, &long);
        let path = write_transcript(&root, "run-t5-cut.jsonl", &body);
        let key = crate::sessions::session_key_of(&path);

        let out = d
            .distill_plan(&key, None, rules_plan_with_budget(512), false)
            .await
            .expect("a session that cannot be windowed is still distilled");
        let coverage = out.coverage.expect("the coverage is a reading");
        println!("READING a cut pass: {out:?}");
        assert_eq!(
            out.windows,
            Some(1),
            "one turn is one window, however large"
        );
        assert!(
            out.truncated,
            "the text did not fit one window, so the tail was dropped -- and saying so              is the point"
        );
        assert!(
            coverage.bytes <= 512 && coverage.bytes > 0,
            "the window carries at most the budget: {coverage:?}"
        );
        assert_eq!(coverage.total_messages, 1);

        let (status, _, _) = newest_attempt(&d.db, &key).await;
        assert!(
            status == "ok" || status == "empty",
            "a cut is not a failure: {status}"
        );
        let (w, t): (Option<i64>, Option<i64>) =
            d.db.call_flat({
                let key = key.clone();
                move |conn| {
                    conn.query_row(
                        "SELECT windows, truncated FROM distill_log WHERE session_key = ?1 \
                         ORDER BY id DESC LIMIT 1",
                        [&key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                }
            })
            .await
            .unwrap();
        assert_eq!(w, Some(1));
        assert_eq!(
            t,
            Some(1),
            "the ledger must say text was dropped, not report a clean read"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #48 + B2): a failure raised BY THE FIRST WINDOW is retried
    /// with the same bounded backoff, and the ledger says how many retries it took.
    ///
    /// The instrument is the seam's own refusal to run the ACP tier without an
    /// extractor: a plan with BOTH tiers enabled and no card fails on the first
    /// window of every attempt, which is exactly the shape "the first window failed"
    /// takes. What must hold is that the failure is not recorded as one attempt, that
    /// the retries are REAL (the elapsed check is a lower bound on the waits the loop
    /// owes), and that nothing measured is claimed: windows/truncated are NULL,
    /// because no pass ever completed a read.
    #[tokio::test]
    async fn a_first_window_failure_is_retried_and_the_ledger_says_how_often() {
        let (home, root) = daemon_layout("first-window");
        let d = distiller_at(&root);
        let mut body = String::new();
        for i in 0..10 {
            body.push_str(&user_turn(i, &format!("记住：第 {i} 条约定。")));
        }
        let path = write_transcript(&root, "run-t5-fail.jsonl", &body);
        let key = crate::sessions::session_key_of(&path);
        // Both tiers, and NO card: the seam refuses window 1 of every attempt.
        let mut plan = rules_plan_with_budget(256);
        plan.acp = true;

        let started = std::time::Instant::now();
        let err = d
            .distill_plan(&key, None, plan, false)
            .await
            .expect_err("a plan that enables the ACP tier with no extractor must fail");
        let elapsed = started.elapsed();
        println!("READING a first-window failure: {err} (after {elapsed:?})");

        let (status, reason, retries) = newest_attempt(&d.db, &key).await;
        assert_eq!(status, "failed");
        assert!(
            reason.contains("no extractor was supplied"),
            "the reason must name the seam's own refusal: {reason:?}"
        );
        assert_eq!(
            retries, RETRY_LIMIT as i64,
            "the first attempt plus {RETRY_LIMIT} retries"
        );
        assert!(
            elapsed >= retry_wait(1) + retry_wait(2),
            "the waits must have been TAKEN: {elapsed:?}"
        );
        assert_eq!(
            attempts_of(&d.db, &key).await,
            1,
            "retries are attempts inside ONE pass, not one ledger row each"
        );
        let measured: (Option<i64>, Option<i64>) =
            d.db.call_flat({
                let key = key.clone();
                move |conn| {
                    conn.query_row(
                        "SELECT windows, truncated FROM distill_log WHERE session_key = ?1 \
                         ORDER BY id DESC LIMIT 1",
                        [&key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                }
            })
            .await
            .unwrap();
        assert_eq!(
            measured,
            (None, None),
            "no pass completed a read, so the windowing was never measured -- NULL, not 0"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #55): the agent's own usage events reach the ledger, folded
    /// the way the RunTimeline folds them.
    ///
    /// TWO HALVES, and the second is why the first is worth anything:
    /// (a) ReplyCollector -- the fold the ACP turn runs, driven with the events an
    ///     agent sends (the mock agent's own numbers 120/200_000/0.001, the values
    ///     mock-agent/tests/integration.rs:104-110 reads off the same event);
    /// (b) distill_log -- the columns those numbers land in, read through the SAME
    ///     expression distill_once uses (usage_columns), plus the counter-case that
    ///     a pass which measured nothing says NULL rather than 0.
    #[tokio::test]
    async fn the_agents_usage_events_reach_the_ledger_columns() {
        let (home, root) = daemon_layout("usage");
        let d = distiller_at(&root);

        let mut collector = ReplyCollector::default();
        assert!(
            collector.observe(&RunEvent::UsageUpdate {
                usage: ruagent_core::ContextUsage {
                    used: 120,
                    size: 200_000,
                    cost_usd: Some(0.001),
                },
            }),
            "a usage event is not the end of the turn"
        );
        collector.observe(&RunEvent::AgentMessageChunk {
            content: vec![ruagent_core::ContentBlock::Text {
                text: "{\"memories\":[]}".into(),
            }],
        });
        collector.observe(&RunEvent::UsageUpdate {
            usage: ruagent_core::ContextUsage {
                used: 150,
                size: 200_000,
                cost_usd: None,
            },
        });
        assert!(
            !collector.observe(&RunEvent::Stopped {
                stop_reason: ruagent_core::StopReason::EndTurn,
            }),
            "a stop event ends the turn"
        );
        assert_eq!(collector.text, "{\"memories\":[]}");
        assert_eq!(
            collector.usage.tokens_used(),
            Some(150),
            "the LAST occupancy is the reading (a snapshot, not a sum)"
        );
        assert_eq!(collector.usage.context_size(), Some(200_000));
        assert_eq!(
            collector.usage.cost_usd(),
            Some(0.001),
            "a later event without a cost must not erase the cost that was reported"
        );

        let (tokens, context, cost) = usage_columns(&collector.usage);
        let measured = DistillOutcome {
            session_key: "ruagent:t5-usage".into(),
            agent: "mock".into(),
            windows: Some(1),
            tokens_used: tokens,
            context_size: context,
            cost_usd: cost,
            ..DistillOutcome::default()
        };
        d.log_outcome(&measured, "ok", "hash-u", None)
            .await
            .unwrap();
        let quiet = DistillOutcome {
            session_key: "ruagent:t5-rules-only".into(),
            windows: Some(1),
            ..DistillOutcome::default()
        };
        d.log_outcome(&quiet, "empty", "hash-u", None)
            .await
            .unwrap();

        let read = |key: &'static str| {
            let db = d.db.clone();
            async move {
                db.call_flat(move |conn| {
                    conn.query_row(
                        "SELECT tokens_used, context_size, cost_usd FROM distill_log \
                         WHERE session_key = ?1 ORDER BY id DESC LIMIT 1",
                        [key],
                        |r| {
                            Ok((
                                r.get::<_, Option<i64>>(0)?,
                                r.get::<_, Option<i64>>(1)?,
                                r.get::<_, Option<f64>>(2)?,
                            ))
                        },
                    )
                })
                .await
                .unwrap()
            }
        };
        let got = read("ruagent:t5-usage").await;
        println!("READING the usage row: {got:?}");
        assert_eq!(got, (Some(150), Some(200_000), Some(0.001)));
        assert_eq!(
            read("ruagent:t5-rules-only").await,
            (None, None, None),
            "a pass no agent reported usage for says NULL -- not 0 tokens and not $0.00"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #55): the graph writer's two loss counts reach the ledger.
    ///
    /// The fixture states three relations between one pair: the first is written, the
    /// second is the SAME claim under another name of the same family (the write-side
    /// dedupe, G7), and the third is not a relation at all (the must_ prefix). Before
    /// this, those two counts existed only in an INFO log line.
    #[tokio::test]
    async fn the_graph_writers_duplicate_and_refused_counts_reach_the_ledger() {
        let (home, root) = daemon_layout("graph-counts");
        let mut d = distiller_at(&root);
        d.graph = true;
        let ex = bundle_with(
            &[("Rust", &[]), ("Cargo", &[])],
            &[
                ("Rust", "Cargo", "uses", "Rust uses Cargo."),
                ("Rust", "Cargo", "uses_protocol", "Rust depends on Cargo."),
                (
                    "Rust",
                    "Cargo",
                    "must_never_drop",
                    "Rust must never drop Cargo.",
                ),
            ],
        );
        let report = d
            .write_extraction(&ex, None)
            .await
            .expect("the graph write");
        println!("READING the graph report: {report:?}");
        assert_eq!(report.relations, 1, "the first claim is the one written");
        assert_eq!(
            report.duplicates, 1,
            "the same-family re-naming is a duplicate"
        );
        assert_eq!(report.refused, 1, "must_never_drop states no relation");

        let outcome = DistillOutcome {
            session_key: "ruagent:t5-graph".into(),
            relations_written: report.relations,
            relations_duplicate: Some(report.duplicates),
            relations_refused: Some(report.refused),
            windows: Some(1),
            ..DistillOutcome::default()
        };
        d.log_outcome(&outcome, "ok", "hash-g", None).await.unwrap();
        let graph_off = DistillOutcome {
            session_key: "ruagent:t5-nograph".into(),
            windows: Some(1),
            ..DistillOutcome::default()
        };
        d.log_outcome(&graph_off, "empty", "hash-g", None)
            .await
            .unwrap();

        let read = |key: &'static str| {
            let db = d.db.clone();
            async move {
                db.call_flat(move |conn| {
                    conn.query_row(
                        "SELECT relations_written, relations_duplicate, relations_refused \
                         FROM distill_log WHERE session_key = ?1 ORDER BY id DESC LIMIT 1",
                        [key],
                        |r| {
                            Ok((
                                r.get::<_, i64>(0)?,
                                r.get::<_, Option<i64>>(1)?,
                                r.get::<_, Option<i64>>(2)?,
                            ))
                        },
                    )
                })
                .await
                .unwrap()
            }
        };
        let got = read("ruagent:t5-graph").await;
        println!("READING the graph counts row: {got:?}");
        assert_eq!(got, (1, Some(1), Some(1)));
        assert_eq!(
            read("ruagent:t5-nograph").await,
            (0, None, None),
            "with the graph half off nothing counted these, so they are NULL -- not 0"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// ACCEPTANCE (issue #48, unchanged-behaviour half): a session that FITS is one
    /// window whose text is byte-identical to the old rendering, and every window is
    /// at most the budget. Both properties are asserted on the function the entry
    /// calls, so the prompt, its prompt_hash and every existing assertion about the
    /// single-window path are covered by construction.
    #[test]
    fn a_session_that_fits_renders_exactly_as_before() {
        let messages: Vec<crate::sessions::SessionMessage> = (0..5)
            .map(|i| crate::sessions::SessionMessage {
                role: "user".into(),
                text: format!("turn {i}"),
                ts: 0,
            })
            .collect();

        let roomy = transcript_windows(&messages, 1024 * 1024);
        assert_eq!(roomy.len(), 1, "a session that fits is ONE window");
        assert_eq!(roomy[0].range, 0..5);
        assert_eq!(
            roomy[0].text,
            render_transcript(&messages),
            "the single-window prompt must be BYTE-IDENTICAL to the old rendering"
        );
        assert!(!roomy[0].truncated);

        let tight = transcript_windows(&messages, 8);
        assert!(tight.len() > 1, "{tight:?}");
        assert_eq!(tight.first().map(|w| w.range.start), Some(0));
        assert_eq!(tight.last().map(|w| w.range.end), Some(messages.len()));
        for w in &tight {
            assert!(
                w.text.len() <= 8,
                "no window may exceed the budget: {} bytes",
                w.text.len()
            );
        }
        let mut expected = 0usize;
        for w in &tight {
            assert_eq!(w.range.start, expected, "windows must be contiguous");
            expected = w.range.end;
        }
        assert_eq!(expected, messages.len(), "and they must cover every turn");
    }

    /// A DRY RUN still writes NOTHING, failures included -- asserted for the retry
    /// loop's sake: the loop changes how many times a pass tries, not what a priced
    /// query leaves behind (design 14.3).
    #[tokio::test]
    async fn a_dry_run_leaves_no_row_even_when_it_fails() {
        let (home, root) = daemon_layout("dry-run");
        let d = distiller_at(&root);
        let err = d
            .distill_plan("ruagent:t45-dry", None, rules_plan(), true)
            .await
            .expect_err("a key no pass can resolve is refused for a dry run too");
        assert_eq!(err.to_string(), NOT_INDEXED_MESSAGE);
        assert_eq!(
            attempts_of(&d.db, "ruagent:t45-dry").await,
            0,
            "a dry run must not write a distill_log row, not even a failure"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}
