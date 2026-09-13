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

Extract:
1. memories: durable facts about the user (profile), observations, procedures (how-to knowledge), and lessons learned. Only include things that generalize beyond this single session.
2. entities: real-world objects mentioned (people, projects, tools, organizations, products). Entity identity = the real-world object, NOT its category. Merge only explicit aliases of the same object.
3. relations: durable facts between entities (src/dst by entity name).

Respond with ONLY a JSON object, no markdown fences, no commentary:
{
  "memories": [{"store": "profile|observation|procedure|lesson", "namespace": "user|global|project:<name>", "content": "..."}],
  "entities": [{"name": "...", "kind": "person|project|tool|org|product|concept", "summary": "one line"}],
  "relations": [{"src": "...", "dst": "...", "relation": "snake_case", "fact": "one sentence"}]
}
Empty arrays are valid. Quality over quantity.

TRANSCRIPT:
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

pub struct Distiller {
    pub db: Db,
    pub root: PathBuf, // ruagent home (~/.ruagent)
}

impl Distiller {
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

        let raw = self
            .ask_agent(card, &format!("{EXTRACTION_PROMPT}{transcript}"))
            .await
            .context("distillation agent run failed")?;
        let extraction = parse_extraction(&raw)?;

        let (mem_w, mem_s) = self.write_memories(&extraction, card).await?;
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
    async fn ask_agent(&self, card: &ruagent_core::AgentCard, prompt: &str) -> Result<String> {
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

        session.send(ChatCommand::Prompt {
            text: prompt.to_string(),
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
    async fn write_memories(
        &self,
        ex: &Extraction,
        card: &ruagent_core::AgentCard,
    ) -> Result<(u32, u32)> {
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
                    confidence: 0.8,
                    source_episode: None,
                    supersedes: None,
                },
            )
            .await
            .with_context(|| format!("writing distilled memory to {store}/{namespace}"))?;
            use ruagent_memory::write::WriteOutcome as W;
            match outcome {
                W::Inserted(_) | W::Superseded { .. } => written += 1,
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
