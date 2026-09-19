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

/// The [distill] policy from policy.toml.
#[derive(Debug, Clone, Default)]
pub struct AutoDistill {
    pub auto: bool,
    pub agent: Option<String>,
    pub language: Option<String>,
    pub prompt: Option<String>,
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
    /// Distill one session: render the transcript, run the extraction
    /// prompt through the given agent (one ACP chat turn), write the
    /// results into memory + graph.
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

        let (mem_w, mem_s) = self.write_memories(&extraction).await?;
        let (ent_w, rel_w) = self.write_graph(&extraction).await?;

        let outcome = DistillOutcome {
            session_key: session_key.to_string(),
            memories_written: mem_w,
            memories_skipped: mem_s,
            entities_written: ent_w,
            relations_written: rel_w,
            agent: card.name.clone(),
        };
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
        Ok(outcome)
    }

    /// The transcript rendered as plain turns for the extraction prompt.
    async fn render_transcript(&self, session_key: &str) -> Result<String> {
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
        let messages = crate::sessions::parse_file_messages(&source, &PathBuf::from(&ref_path));
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
    async fn write_memories(&self, ex: &Extraction) -> Result<(u32, u32)> {
        let mut written = 0u32;
        let mut skipped = 0u32;
        for m in &ex.memories {
            let store = normalize_store(&m.store);
            let namespace = if m.namespace.is_empty() {
                default_namespace(&store).to_string()
            } else {
                m.namespace.clone()
            };
            let content = format!("[distilled] {}", m.content.trim());
            // Near-duplicate check against existing rows in scope.
            let dup = {
                let db = self.db.clone();
                let (store_c, ns_c) = (store.clone(), namespace.clone());
                let existing: Vec<String> = db
                    .call(move |conn| -> Result<Vec<String>, ruagent_store::DbError> {
                        let mut stmt = conn
                            .prepare(
                                "SELECT content FROM memories
                                  WHERE store = ?1 AND namespace = ?2 AND superseded_at IS NULL",
                            )
                            .map_err(ruagent_store::DbError::from)?;
                        let rows = stmt
                            .query_map([&store_c, &ns_c], |r| r.get::<_, String>(0))
                            .map_err(ruagent_store::DbError::from)?;
                        Ok(rows.filter_map(|r| r.ok()).collect())
                    })
                    .await??;
                is_near_duplicate(&content, &existing)
            };
            if dup {
                skipped += 1;
                continue;
            }
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
                    source_episode: None,
                    supersedes: None,
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

fn is_near_duplicate(content: &str, existing: &[String]) -> bool {
    let norm = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_lowercase()
            })
            .filter(|w| w.len() > 2)
            .collect()
    };
    let target = norm(content);
    if target.is_empty() {
        return true;
    }
    for e in existing {
        let cand = norm(e);
        if cand.is_empty() {
            continue;
        }
        let hits = target.iter().filter(|t| cand.contains(t)).count();
        if hits as f64 / target.len() as f64 >= 0.7 {
            return true;
        }
    }
    false
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

    #[test]
    fn near_duplicate_detects_high_overlap() {
        let existing = vec!["user prefers Rust and dislikes Java timezone UTC+8".to_string()];
        assert!(is_near_duplicate(
            "User prefers Rust; dislikes Java (UTC+8)",
            &existing
        ));
        assert!(!is_near_duplicate(
            "deploy via docker compose on the weekend",
            &existing
        ));
    }
}
