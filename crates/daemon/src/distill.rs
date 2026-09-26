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
2. entities: real-world objects mentioned (people, projects, tools, organizations, products). Entity identity = the real-world object, NOT its category. Merge only explicit aliases of the same object.
3. relations: durable facts between entities (src/dst by entity name).

Respond with ONLY a JSON object, no markdown fences, no commentary:
{
  "memories": [{"store": "profile|observation|procedure|lesson", "namespace": "user|global|project:<name>", "content": "...", "confidence": 0.8}],
  "entities": [{"name": "...", "kind": "person|project|tool|org|product|concept", "summary": "one line"}],
  "relations": [{"src": "...", "dst": "...", "relation": "snake_case", "fact": "one sentence"}]
}
Empty arrays are valid. Quality over quantity.
"#;

#[derive(Debug, Default, Deserialize)]
struct Extraction {
    #[serde(default)]
    memories: Vec<ExtractedMemory>,
    #[serde(default)]
    entities: Vec<ExtractedEntity>,
    #[serde(default)]
    relations: Vec<ExtractedRelation>,
}

#[derive(Debug, Deserialize)]
struct ExtractedMemory {
    store: String,
    namespace: String,
    content: String,
    /// 0.5–1.0, from answer-quality signals in the transcript.
    #[serde(default)]
    confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ExtractedEntity {
    name: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtractedRelation {
    src: String,
    dst: String,
    relation: String,
    fact: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DistillOutcome {
    pub session_key: String,
    pub memories_written: u32,
    pub memories_skipped: u32,
    pub entities_written: u32,
    pub relations_written: u32,
    pub agent: String,
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
    fn compose_prompt(&self) -> String {
        extraction_prompt(self.language.as_deref(), self.prompt_override.as_deref())
    }
    /// Distill one session: run the extraction prompt through the given
    /// agent (one ACP chat turn), then write the results into memory
    /// and — unless graph extraction is off — the entity graph.
    pub async fn distill(
        &self,
        session_key: &str,
        card: &ruagent_core::AgentCard,
    ) -> Result<DistillOutcome> {
        let transcript = self.render_transcript(session_key).await?;
        if transcript.is_empty() {
            anyhow::bail!("session has no messages to distill");
        }

        let full_prompt = self.compose_prompt();
        let raw = self
            .ask_agent(card, &format!("{full_prompt}{transcript}"))
            .await
            .context("distillation agent run failed")?;
        let extraction = parse_extraction(&raw)?;

        let (mem_w, mem_s) = self
            .write_memories(&extraction.memories, Some((session_key, &transcript)))
            .await?;
        // graph = false: memories only; entities/relations count 0 and
        // the log still records the run.
        let (ent_w, rel_w) = if self.graph {
            self.write_graph(&extraction).await?
        } else {
            (0, 0)
        };

        let outcome = DistillOutcome {
            session_key: session_key.to_string(),
            memories_written: mem_w,
            memories_skipped: mem_s,
            entities_written: ent_w,
            relations_written: rel_w,
            agent: card.name.clone(),
        };
        self.log_outcome(&outcome).await?;
        Ok(outcome)
    }

    /// Record the outcome in distill_log (the per-session content
    /// dedup marker; re-distilling a session replaces its row).
    async fn log_outcome(&self, outcome: &DistillOutcome) -> Result<()> {
        let log = outcome.clone();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO distill_log
                         (session_key, distilled_at, memories_written, entities_written,
                          relations_written, agent)
                     VALUES (?1,?2,?3,?4,?5,?6)",
                    rusqlite::params![
                        log.session_key,
                        chrono::Utc::now().to_rfc3339(),
                        log.memories_written,
                        log.entities_written,
                        log.relations_written,
                        log.agent,
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

    /// The transcript rendered as plain turns for the extraction prompt.
    async fn render_transcript(&self, session_key: &str) -> Result<String> {
        let messages = self.load_messages(session_key).await?;
        Ok(messages
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
            .join("\n\n"))
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
        // layer back into itself.
        session.send(ChatCommand::Prompt {
            text: prompt.to_string(),
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
        memories: &[ExtractedMemory],
        provenance: Option<(&str, &str)>,
    ) -> Result<(u32, u32)> {
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
            let store = normalize_store(&m.store);
            let namespace = if m.namespace.is_empty() {
                default_namespace(&store).to_string()
            } else {
                m.namespace.clone()
            };
            let content = format!("[distilled] {}", m.content.trim());
            // Near-duplicate check against the live rows in scope. A hit is no
            // longer a SKIP: t323 measured 8 of 44 `profile` rows saying the same
            // thing in different words, because the check it had was word-based
            // and therefore blind to Chinese. A hit now SUPERSEDES that row, so
            // the store keeps one row per meaning instead of one per phrasing
            // (t329).
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
                mergeable_target(&content, &existing)
            };
            // The memory crate's write path handles the content hash,
            // exact-duplicate rejection and the audit trail.
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
            let outcome = ruagent_memory::write::write_memory(
                &self.db,
                &ruagent_memory::write::MemoryWrite {
                    store: store_t,
                    namespace: ns_t,
                    content: content.clone(),
                    confidence: m.confidence.unwrap_or(0.8).clamp(0.5, 1.0),
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
        Ok((written, skipped))
    }

    /// Find-or-create entities, then add relations — through the graph
    /// crate (norm_name resolution, deterministic supersession, FTS sync).
    async fn write_graph(&self, ex: &Extraction) -> Result<(u32, u32)> {
        use std::collections::HashMap;
        let mut ids: HashMap<String, i64> = HashMap::new();

        for e in &ex.entities {
            let name = e.name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let kind = e.kind.as_deref();
            let summary = e.summary.as_deref();
            let id = ruagent_graph::upsert_entity(&self.db, &name, kind, summary)
                .await
                .with_context(|| format!("upserting entity {name}"))?;
            ids.insert(name, id);
        }
        let entities_written = ids.len() as u32;

        let mut relations_written = 0u32;
        for r in &ex.relations {
            let (Some(src), Some(dst)) = (ids.get(r.src.trim()), ids.get(r.dst.trim())) else {
                continue; // relation to an unlisted entity — skip
            };
            ruagent_graph::add_fact(
                &self.db,
                *src,
                *dst,
                r.relation.trim(),
                r.fact.trim(),
                None,
                None,
            )
            .await
            .with_context(|| format!("adding relation {}", r.relation))?;
            relations_written += 1;
        }
        Ok((entities_written, relations_written))
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

fn default_namespace(store: &str) -> &'static str {
    if store == "profile" { "user" } else { "global" }
}

/// Strip markdown fences / commentary the agent may have added.
fn parse_extraction(raw: &str) -> Result<Extraction> {
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

    fn mem(content: &str) -> ExtractedMemory {
        ExtractedMemory {
            store: "profile".to_string(),
            namespace: "user".to_string(),
            content: content.to_string(),
            confidence: None,
        }
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
            let (w, s) = d
                .write_memories(
                    &[mem(p)],
                    Some(("ruagent:t329-test", "[t329] the user's language preference")),
                )
                .await
                .unwrap();
            println!("READING write {i}: written={w} skipped={s}");
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
}
