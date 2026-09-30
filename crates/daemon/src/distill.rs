//! Session distillation: turn a conversation into durable memory,
//! graph entities and relations — the OpenViking pattern (extract →
//! dedup → write), executed by one of the configured agent runtimes
//! through our own ACP machinery. See
//! docs/plans/2026-09-14-distillation-design.md.

use std::path::PathBuf;

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
            dry_run: false,
        }
    }
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
        if dry_run {
            // A price is not a purchase: no episode is created either, because a
            // `run_turn` episode is what the panel reads as "this session was
            // distilled".
            let (outcome, _status) = self.distill_once(session_key, card, plan, true).await?;
            return Ok(outcome);
        }
        match self.distill_once(session_key, card, plan, false).await {
            Ok((outcome, status)) => {
                self.log_outcome(&outcome, status, &prompt_hash, None)
                    .await?;
                Ok(outcome)
            }
            Err(e) => {
                let reason = e.to_string();
                // A failure MUST NOT create an episode: the panel's "distilled"
                // boolean is derived from episodes.kind = 'run_turn' (t350), so a
                // failure row that materialised one would claim a distillation
                // that did not happen.
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
                            ..DistillOutcome::default()
                        },
                        "failed",
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
                Err(e)
            }
        }
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
        let raw_turns = messages.len();
        let messages = user_turns(messages);
        if messages.is_empty() && raw_turns > 0 {
            anyhow::bail!(
                "session {session_key} carries no user text: every turn is platform \
                 text (an injected block, or one of our own job prompts) or the agent's \
                 own output — there is nothing of the user's in it to distil"
            );
        }
        let transcript = render_transcript(&messages);
        if transcript.is_empty() {
            anyhow::bail!("session has no messages to distill");
        }
        // The free tier reads turns; the ACP tier reads the rendered transcript.
        // Loading turns only when the plan asks for them keeps today's path free
        // of work it did not do before.
        let turns: Vec<ruagent_extract::Turn> = if plan.rules {
            turns_of(&messages)
        } else {
            Vec::new()
        };
        let cx = crate::extract_plane::ExtractCtx {
            session_key,
            transcript: &transcript,
            turns: &turns,
            limits: plan.limits,
        };
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
        let bundle = crate::extract_plane::extract(&cx, plan, acp)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // A dry run reports what WOULD be written and writes nothing: no
        // episode, no memory, no graph row.
        let (mem, ent_w, rel_w) = if dry_run {
            (
                MemoryWriteOutcome {
                    written: bundle.memories.len() as u32,
                    skipped: 0,
                    episode: None,
                },
                bundle.entities.len() as u32,
                bundle.relations.len() as u32,
            )
        } else {
            let mem = self
                .write_memories(&bundle.memories, Some((session_key, &transcript)))
                .await?;
            // graph = false: memories only; entities/relations count 0 and
            // the log still records the run.
            let (ent_w, rel_w) = if self.graph {
                self.write_extraction(&bundle, mem.episode).await?
            } else {
                (0, 0)
            };
            (mem, ent_w, rel_w)
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
                entities_written: ent_w,
                relations_written: rel_w,
                // Empty when no agent card was resolved, i.e. on a rules-only
                // pass (t14): the free tier has no agent, and `source` is the
                // field that says which implementation produced this row.
                agent: card.map(|c| c.name.clone()).unwrap_or_default(),
                source: plan.label().to_string(),
                truncated: bundle.truncated,
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
    ) -> Result<(u32, u32)> {
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
                          relations_written, agent, status, failure_reason, prompt_hash)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
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
                    ],
                )
            })
            .await??;
        Ok(())
    }

    /// Raw session messages: the sessions index holds the source and
    /// file path, the file is parsed on demand.
    async fn load_messages(
        &self,
        session_key: &str,
    ) -> Result<Vec<crate::sessions::SessionMessage>> {
        let key = session_key.to_string();
        let (source, ref_path): (String, String) = self
            .db
            .call(move |conn| {
                conn.query_row(
                    "SELECT source, ref_path FROM sessions WHERE key = ?1",
                    [&key],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .await
            .map_err(|_| anyhow::anyhow!("session not indexed: {session_key}"))??;
        Ok(crate::sessions::parse_file_messages(
            &source,
            &PathBuf::from(&ref_path),
        ))
    }

    /// One-shot ACP run: spawn the agent, ask, collect the reply.
    /// Shared with the wiki builder (design §6.1: same single-call
    /// discipline, no tool loops for unattended jobs).
    pub(crate) async fn ask_agent(
        &self,
        card: &ruagent_core::AgentCard,
        prompt: &str,
    ) -> Result<String> {
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

        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<String>();
        let (err_tx, mut err_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let mut sub = session.subscribe();
        tokio::spawn(async move {
            let mut text = String::new();
            let mut done = false;
            while !done {
                match sub.recv().await {
                    Ok(RunEvent::AgentMessageChunk { content }) => {
                        for block in content {
                            if let ruagent_core::ContentBlock::Text { text: t } = block {
                                text.push_str(&t);
                            }
                        }
                    }
                    Ok(RunEvent::Stopped { .. }) => {
                        done = true;
                    }
                    Ok(RunEvent::Error { message }) => {
                        let _ = err_tx.send(message);
                        done = true;
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => {
                        done = true;
                    }
                }
            }
            let _ = done_tx.send(text);
        });

        // No context payload: distillation must not feed the memory
        // layer back into itself. The prompt carries the platform marker (t21), so the
        // session the agent CLI logs for this job does not present our own text — a
        // machine copy of another session's transcript — as the user's words.
        session.send(ChatCommand::Prompt {
            text: platform_prompt(prompt),
            context: None,
        })?;
        let reply = match tokio::time::timeout(std::time::Duration::from_secs(300), done_rx).await {
            Ok(Ok(text)) => text,
            Ok(Err(_)) => anyhow::bail!("agent stream closed before reply"),
            Err(_) => anyhow::bail!("distillation agent timed out (5 min)"),
        };
        if let Some(err) = err_rx.recv().await
            && reply.trim().is_empty()
        {
            anyhow::bail!("distillation agent error: {err}");
        }
        let _ = session.send(ChatCommand::Shutdown);
        Ok(reply)
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
    ) -> Result<(u32, u32)> {
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

        let report =
            ruagent_graph::apply_extraction(&self.db, &entities, &facts, "extraction", episode)
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
        Ok((report.entities, report.relations))
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
        let out = d
            .write_memories(
                &[mem("用户偏好使用简体中文交流。")],
                Some(("ruagent:t347-test", "[t347] the marker moved to a field")),
            )
            .await
            .unwrap();
        let w = out.written;
        assert_eq!(
            w, 1,
            "the write must land, or the assertion below is vacuous"
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
        let (ent, rel) = d.write_graph(&ex, Some(episode)).await.unwrap();
        assert_eq!(ent, 2);
        assert_eq!(rel, 2);

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
            Some("Query returned no rows"),
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
        assert_eq!(same_session[1].2.as_deref(), Some("Query returned no rows"));
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
            Some("Query returned no rows"),
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
        assert_eq!(failed.1.as_deref(), Some("Query returned no rows"));
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
