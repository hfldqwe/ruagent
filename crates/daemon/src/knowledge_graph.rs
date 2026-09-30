//! Knowledge base → knowledge graph ingestion (t4,
//! docs/plans/capability-plugins-design.md §13).
//!
//! WHY THIS MODULE EXISTS: today `POST /api/v1/knowledge/ingest` writes the
//! markdown file, indexes its chunks and stops — no document, and no knowledge
//! chunk, has ever written an entity or a relation
//! (`ruagent_graph::apply_extraction`'s only production caller is
//! `distill.rs`, the session path). This module closes that gap: a knowledge
//! document's text goes through the SAME extractor seam the session
//! distillation uses and lands in `crates/graph` — with zero tokens on this
//! path (the free tier: `ruagent_extract::graph_candidates` is a pure
//! function) and OFF by default.
//!
//! THE CAPABILITY GATE IS NOT HERE. Callers check
//! `CapabilityId::KnowledgeIngestGraph` (`knowledge_ingest_graph`, a NEW free
//! capability whose registry default is OFF, so with no `[capabilities]` table
//! this code is never reached — law L1: the default configuration behaves
//! exactly as before). This module is a library of two operations, so the gate
//! is one `if` at each of the two call sites instead of a hidden flag inside
//! the function.
//!
//! COST CONTROL, IN THE FOUR PLACES THE DESIGN NAMES (§13.2):
//!   1. zero tokens by construction — nothing here calls a model;
//!   2. off by default (the registry row, not this file);
//!   3. only changed documents — the ledger below remembers the
//!      `(document_name, content_hash)` already written, so an unchanged
//!      document is never re-extracted, however often the scan runs;
//!   4. bounded catch-up — `max_docs_per_pass` documents per sweep (default
//!      20), so enabling the capability on a 6 000-file tree ingests 20 per
//!      minute instead of all at once.
//!
//! THE LEDGER (migration 0026, `knowledge_graph_ingest`) is the memory that
//! makes 1) idempotence, 2) the revision rule below, and 3) the status route
//! possible at all. It holds ONE ROW PER DOCUMENT NAME (the current revision)
//! plus the entity names and fact hashes that revision wrote, because the graph
//! does not record which document a fact came from and the new revision's text
//! no longer mentions what the old one contributed.
//!
//! THE REVISION RULE (what happens when a document changes) — the design
//! document fixes the ledger but says nothing about removals, so it is stated
//! in full here, once, and implemented exactly once:
//!
//!   * FACTS the previous revision stated and the current one does not are
//!     RETRACTED, never deleted: `invalid_at` and `expired_at` are closed
//!     together, mirroring `insert_fact_in` (graph/src/lib.rs:656-661), because
//!     `0005_graph.sql`'s rule is "edges are invalidated, never deleted: what
//!     was true as of X is always answerable".
//!   * A fact that ANOTHER document's ledger row still states is left alone —
//!     one document dropping a claim must not retract another's.
//!   * An ENTITY the previous revision contributed and the current one does not
//!     is DELETED only when ALL of these hold: nothing else's ledger row claims
//!     it (by name or by one of its aliases), it has no currently-valid edge
//!     (another source — a distilled session, say — may still use it), and
//!     every edge row it still has was stated by the very revision being
//!     replaced (so the history this deletes is only the history this revision
//!     is retracting). Otherwise it is kept: an entity that other material still
//!     references is not an orphan.
//!   * A document that changes to something with nothing to extract retracts
//!     everything it contributed before (the empty-current case falls out of
//!     the same rule).
//!
//! ORDER: extract → WRITE → prune → ledger. Writing before pruning means a
//! reader never sees a window in which the document's facts have vanished; the
//! prune sees the new revision's own edge rows in place, which is what makes
//! the "all its edges are mine" test of the entity rule possible.
//!
//! WINDOWS/UNIX: nothing here is platform-specific — the only I/O is
//! `Knowledge::read_raw`, which resolves `<root>/knowledge/<name>.md` with
//! `std::path::Path::join` (files.rs:239-246), and SQLite.
//!
//! WIRED (t4): `lib.rs` declares the module and `api.rs` serves
//! `POST /api/v1/knowledge/graph/ingest` (+ its `/status` sibling), where the
//! `knowledge_ingest_graph` gate is checked. The scan-loop sweep that rides the
//! 60 s knowledge scan is the integration task's line (§17.8, `lib.rs`); this
//! module is called from it with no other change.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use ruagent_graph::{EventTimeSource, ExtractEntity, ExtractFact};
use ruagent_knowledge::Knowledge;
use ruagent_store::Db;
use rusqlite::OptionalExtension as _;
use serde::Serialize;

/// The `source` column value every row written by this path carries. Session
/// distillation writes behind its own source; the two must stay distinguishable
/// in the graph.
pub const GRAPH_SOURCE: &str = "knowledge";

/// What one ingestion pass may do. Every field is resolved by the CALLER from
/// the `knowledge_ingest_graph` capability's options (§4.2: `max_per_input`
/// default 96, `max_docs_per_pass` default 20; `min_confidence` is not a
/// declared option of that capability today, so it stays at its default).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IngestOptions {
    /// Candidate cap per document, handed to the extractor as
    /// `ExtractLimits::max_per_input`.
    pub max_per_input: usize,
    /// Drop candidates the extractor scored below this. `0.0` keeps everything
    /// the rules produced (§9.3).
    pub min_confidence: f32,
    /// Documents per sweep — the bounded catch-up (§13.2 tooth 4).
    pub max_docs_per_pass: u32,
    /// Extract and count, write nothing and record nothing (§13.4).
    pub dry_run: bool,
}

/// The registry defaults, so a caller that resolves nothing still behaves like
/// the shipped configuration.
pub const DEFAULT_MAX_PER_INPUT: usize = 96;
pub const DEFAULT_MAX_DOCS_PER_PASS: u32 = 20;

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            max_per_input: DEFAULT_MAX_PER_INPUT,
            min_confidence: 0.0,
            max_docs_per_pass: DEFAULT_MAX_DOCS_PER_PASS,
            dry_run: false,
        }
    }
}

/// What one ingestion pass (or a whole sweep) did.
///
/// HONEST ABOUT `dry_run`: in a dry run `documents` is 0 (nothing was ingested
/// and no ledger row was written) and `entities`/`relations` are what WOULD be
/// written — the candidate counts. The HTTP layer labels the response
/// `"dry_run": true` so a reader is never left to guess which reading applies.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct IngestReport {
    /// Documents that were extracted and written in this pass.
    pub documents: u32,
    /// Documents the operation could not ingest: no markdown file behind the
    /// row, or a read/extraction error. Never a silent omission — each one is
    /// logged with its document name.
    pub skipped: u32,
    /// Distinct entity names the write resolved (created, merged or queued).
    pub entities: u32,
    /// Relations written.
    pub relations: u32,
    /// Relations the graph's write-side dedupe recognised as already stated.
    pub duplicates: u32,
    /// Relations refused because the name states no relation.
    pub refused: u32,
    /// Documents whose input was cut to the extractor's text bound.
    pub truncated: u32,
    /// Candidates the extractor produced (entities + relations, after its caps).
    pub candidates: u32,
    /// Documents skipped because the ledger already holds this content hash.
    pub ledger_hits: u32,
}

impl IngestReport {
    /// Fold one pass's report into a sweep's.
    fn merge(&mut self, other: IngestReport) {
        self.documents += other.documents;
        self.skipped += other.skipped;
        self.entities += other.entities;
        self.relations += other.relations;
        self.duplicates += other.duplicates;
        self.refused += other.refused;
        self.truncated += other.truncated;
        self.candidates += other.candidates;
        self.ledger_hits += other.ledger_hits;
    }
}

/// One ledger row, as the status route reports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LedgerRow {
    pub document_name: String,
    pub document_id: i64,
    pub content_hash: String,
    pub entities: i64,
    pub relations: i64,
    pub candidates: i64,
    pub ingested_at: String,
}

/// The answer to "is my knowledge base in the graph?" (§16).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LedgerStatus {
    /// Ledger rows = documents whose current revision is in the graph.
    pub documents: i64,
    /// Sum of the entity counts across those rows.
    pub entities: i64,
    /// Sum of the relation counts across those rows.
    pub relations: i64,
    /// The most recent rows, newest first.
    pub last: Vec<LedgerRow>,
}

/// Ingest ONE document into the graph. Idempotent per content hash: a document
/// whose text is already recorded in the ledger does nothing and reports
/// `ledger_hits == 1`.
///
/// The caller checks the capability first (§13, `knowledge_ingest_graph`);
/// this function does not know what a capability is.
pub async fn ingest_document(
    db: &Db,
    kb: &Knowledge,
    name: &str,
    opts: &IngestOptions,
) -> Result<IngestReport> {
    let wanted = canonical_doc_name(name);
    let docs = kb
        .list_documents()
        .await
        .context("list knowledge documents")?;
    let doc = docs.iter().find(|d| d.name == wanted).with_context(|| {
        format!(
            "knowledge document `{name}` is not indexed ({} documents; a name is the markdown \
                 path under <root>/knowledge with the .md suffix removed)",
            docs.len()
        )
    })?;
    let text = kb
        .read_raw(&doc.name)
        .with_context(|| format!("read knowledge document `{}`", doc.name))?
        .with_context(|| {
            format!(
                "knowledge document `{}` has no markdown file (the file is the source of truth)",
                doc.name
            )
        })?;

    // The hash of the TEXT THIS PASS EXTRACTS FROM, not of the index row: if a
    // file changed between two scans, the ledger must notice.
    let hash = ruagent_knowledge::sha256_hex(text.as_bytes());
    let ledger = load_ledger(db).await?;
    let previous = ledger.get(&doc.name).cloned();
    if let Some(prev) = &previous
        && prev.content_hash == hash
    {
        return Ok(IngestReport {
            ledger_hits: 1,
            ..IngestReport::default()
        });
    }

    let limits = ruagent_extract::ExtractLimits {
        max_per_input: opts.max_per_input,
        min_score: opts.min_confidence,
        ..ruagent_extract::ExtractLimits::graph_default()
    };
    let candidates = ruagent_extract::graph_candidates(&text, &limits);
    let entities: Vec<ExtractEntity> = candidates
        .entities
        .iter()
        .map(|e| ExtractEntity {
            name: e.name.clone(),
            kind: e.kind.map(str::to_string),
            summary: e.summary.clone(),
            aliases: e.aliases.clone(),
        })
        .collect();
    let facts: Vec<ExtractFact> = candidates
        .relations
        .iter()
        .map(|r| ExtractFact {
            src: r.src.clone(),
            dst: r.dst.clone(),
            relation: r.relation.relation_literal().to_string(),
            fact_text: r.fact.clone(),
            // A markdown document states no event time for its relations; the
            // graph then writes `valid_at` from the clock with
            // `EventTimeSource::Recorded` (graph/src/lib.rs:644-651), exactly
            // as a session fact with `valid_at: null` does.
            valid_at: None,
            event_time_source: EventTimeSource::Recorded,
        })
        .collect();
    // Claims are what the revision STATES, not what the write accepted: a fact
    // the write deduped as already-present is still this document's claim, and
    // over-claiming only makes the prune more conservative.
    let claims = RevisionClaims {
        entities: candidates.entities.iter().map(|e| e.name.clone()).collect(),
        facts: candidates
            .relations
            .iter()
            .map(|r| ruagent_graph::fact_hash(&r.fact))
            .collect(),
    };
    let candidate_count = (entities.len() + facts.len()) as u32;

    if opts.dry_run {
        return Ok(IngestReport {
            entities: entities.len() as u32,
            relations: facts.len() as u32,
            candidates: candidate_count,
            truncated: u32::from(candidates.truncated),
            ..IngestReport::default()
        });
    }

    let write = ruagent_graph::apply_extraction(db, &entities, &facts, GRAPH_SOURCE, None)
        .await
        .context("write the extraction into the graph")?;

    // The revision rule: what the previous revision contributed and this one no
    // longer states leaves the graph — after the write, so nothing is missing
    // in between.
    if let Some(prev) = &previous {
        let (other_entities, other_facts) = other_claims(&ledger, &doc.name);
        let (facts_to_retract, entities_to_delete) =
            retractions(&prev.claims, &claims, &other_entities, &other_facts);
        if !facts_to_retract.is_empty() || !entities_to_delete.is_empty() {
            let outcome = prune_previous_revision(
                db,
                facts_to_retract,
                entities_to_delete,
                other_entities,
                prev.claims.facts.iter().cloned().collect(),
            )
            .await
            .context("retract the previous revision from the graph")?;
            tracing::debug!(
                document = %doc.name,
                facts_retracted = outcome.facts_retracted,
                entities_deleted = outcome.entities_deleted,
                entities_kept = outcome.entities_kept,
                "knowledge graph ingest: previous revision retracted"
            );
        }
    }

    write_ledger_row(
        db,
        &doc.name,
        doc.id,
        &hash,
        LedgerCounts {
            entities: write.entities,
            relations: write.relations,
            candidates: candidate_count,
        },
        &claims,
    )
    .await
    .context("record the ingestion in the ledger")?;

    Ok(IngestReport {
        documents: 1,
        entities: write.entities,
        relations: write.relations,
        duplicates: write.duplicates,
        refused: write.refused,
        truncated: u32::from(candidates.truncated),
        candidates: candidate_count,
        ..IngestReport::default()
    })
}

/// Sweep up to `max_docs_per_pass` documents the ledger has not seen.
///
/// Deterministic order: `(created_at DESC, id ASC)` — `list_documents` orders by
/// `created_at DESC` alone (store.rs:1304), and two documents indexed in the
/// same second must not swap places between runs, so the id breaks the tie here.
///
/// One document that cannot be read or extracted does not stop the sweep: it is
/// logged, counted in `skipped`, and the pass moves on (a 6 000-document tree
/// must not be stopped by one unreadable file).
pub async fn sweep(db: &Db, kb: &Knowledge, opts: &IngestOptions) -> Result<IngestReport> {
    let mut docs = kb
        .list_documents()
        .await
        .context("list knowledge documents")?;
    docs.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    let ledger = load_ledger(db).await?;

    let mut report = IngestReport::default();
    let mut selected = 0u32;
    for doc in docs {
        let seen = ledger
            .get(&doc.name)
            .is_some_and(|row| row.content_hash == doc.content_hash);
        if seen {
            report.ledger_hits += 1;
            continue;
        }
        if selected >= opts.max_docs_per_pass {
            break;
        }
        selected += 1;
        match ingest_document(db, kb, &doc.name, opts).await {
            Ok(one) => report.merge(one),
            Err(e) => {
                tracing::warn!(
                    document = %doc.name,
                    error = %e,
                    "knowledge graph ingest: document skipped"
                );
                report.skipped += 1;
            }
        }
    }
    Ok(report)
}

/// The options the `knowledge_ingest_graph` capability resolves to, from the
/// registry defaults when the file declares nothing (§4.4's totality).
pub fn options_of(plane: &crate::capability::CapabilityPlane) -> IngestOptions {
    let o = plane.options(crate::capability::CapabilityId::KnowledgeIngestGraph);
    IngestOptions {
        max_per_input: o.max_per_input.unwrap_or(DEFAULT_MAX_PER_INPUT as u32) as usize,
        // The capability does not declare `min_confidence`, so §9.3's default
        // (0.0 — keep everything the rules produced) is the only value it can
        // have.
        min_confidence: 0.0,
        max_docs_per_pass: o.max_docs_per_pass.unwrap_or(DEFAULT_MAX_DOCS_PER_PASS),
        dry_run: false,
    }
}

/// The sweep that rides the existing 60 s knowledge scan (§13.1).
///
/// With `knowledge_ingest_graph` off — the DEFAULT — this returns immediately
/// and touches neither the graph nor the ledger: that is what keeps the shipped
/// configuration byte-for-byte equivalent to today. With it on, one bounded pass
/// runs per scan (`max_docs_per_pass`).
pub async fn sweep_if_enabled(
    db: &Db,
    kb: &Knowledge,
    plane: &std::sync::Arc<std::sync::RwLock<crate::capability::CapabilityPlane>>,
) {
    let enabled = {
        // Read, clone, drop the guard — never held across an await.
        let plane = plane.read().expect("capability plane").clone();
        plane.enabled(crate::capability::CapabilityId::KnowledgeIngestGraph)
    };
    if !enabled {
        return;
    }
    let opts = {
        let plane = plane.read().expect("capability plane").clone();
        options_of(&plane)
    };
    match sweep(db, kb, &opts).await {
        Ok(report) if report.documents > 0 => tracing::info!(
            documents = report.documents,
            entities = report.entities,
            relations = report.relations,
            ledger_hits = report.ledger_hits,
            "knowledge graph ingest sweep"
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "knowledge graph ingest sweep failed"),
    }
}

/// The ledger, newest row first, plus its totals (§16's status route). Read-only
/// and safe to call whether or not the capability is enabled: it triggers no
/// extraction.
pub async fn ledger_status(db: &Db, limit: u32) -> Result<LedgerStatus> {
    let mut rows = load_ledger(db).await?.into_values().collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.ingested_at
            .cmp(&a.ingested_at)
            .then_with(|| a.document_name.cmp(&b.document_name))
    });
    let mut status = LedgerStatus {
        documents: rows.len() as i64,
        ..LedgerStatus::default()
    };
    for row in &rows {
        status.entities += row.entities;
        status.relations += row.relations;
    }
    status.last = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| LedgerRow {
            document_name: row.document_name,
            document_id: row.document_id,
            content_hash: row.content_hash,
            entities: row.entities,
            relations: row.relations,
            candidates: row.candidates,
            ingested_at: row.ingested_at,
        })
        .collect();
    Ok(status)
}

// ---------------------------------------------------------------------------
// The revision rule's pure part.
// ---------------------------------------------------------------------------

/// What one revision of one document contributed: entity names as written, and
/// the `fact_hash` of every fact statement. The two are what the prune reasons
/// about, and the only things the ledger needs to remember.
#[derive(Debug, Clone, Default, PartialEq)]
struct RevisionClaims {
    entities: Vec<String>,
    facts: Vec<String>,
}

impl RevisionClaims {
    fn entity_norms(&self) -> BTreeSet<String> {
        self.entities.iter().map(|e| norm_key(e)).collect()
    }

    fn fact_set(&self) -> BTreeSet<String> {
        self.facts.iter().cloned().collect()
    }
}

/// The graph's entity identity (`entities.norm_name`, 0005_graph.sql:10).
fn norm_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The pure core of the revision rule: which of the previous revision's claims
/// must be retracted, given what the current revision still states and what
/// other documents' ledger rows claim.
///
/// `claimed_elsewhere` holds the NORMED entity names and the fact hashes other
/// documents still state; anything in there is never touched. Returned in
/// deterministic order (the previous revision's own order), so a test and a log
/// read the same both times.
fn retractions(
    previous: &RevisionClaims,
    current: &RevisionClaims,
    claimed_elsewhere_entities: &BTreeSet<String>,
    claimed_elsewhere_facts: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let now_facts = current.fact_set();
    let now_entities = current.entity_norms();
    let facts = previous
        .facts
        .iter()
        .filter(|f| !now_facts.contains(*f) && !claimed_elsewhere_facts.contains(*f))
        .cloned()
        .collect::<Vec<_>>();
    let entities = previous
        .entities
        .iter()
        .filter(|e| {
            let key = norm_key(e);
            !now_entities.contains(&key) && !claimed_elsewhere_entities.contains(&key)
        })
        .cloned()
        .collect::<Vec<_>>();
    (facts, entities)
}

/// Every claim made by a ledger row OTHER than `except`'s: the normed entity
/// names and the fact hashes — in the SAME ORDER [`retractions`] takes them
/// (entities, then facts), because both are `BTreeSet<String>` and swapping them
/// would compile while checking entity names against fact hashes.
///
/// A row whose JSON does not parse contributes nothing rather than everything:
/// the failure direction that cannot retract someone else's material.
fn other_claims(
    ledger: &BTreeMap<String, LedgerEntry>,
    except: &str,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut entities = BTreeSet::new();
    let mut facts = BTreeSet::new();
    for (name, row) in ledger {
        if name == except {
            continue;
        }
        facts.extend(row.claims.facts.iter().cloned());
        entities.extend(row.claims.entities.iter().map(|e| norm_key(e)));
    }
    (entities, facts)
}

// ---------------------------------------------------------------------------
// SQL.
// ---------------------------------------------------------------------------

/// One ledger row in memory.
#[derive(Debug, Clone, PartialEq)]
struct LedgerEntry {
    document_name: String,
    document_id: i64,
    content_hash: String,
    entities: i64,
    relations: i64,
    candidates: i64,
    ingested_at: String,
    claims: RevisionClaims,
}

/// What the prune did, for the log line.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PruneOutcome {
    facts_retracted: u64,
    entities_deleted: u64,
    entities_kept: u64,
}

/// The three counts a ledger row carries alongside the claims.
struct LedgerCounts {
    entities: u32,
    relations: u32,
    candidates: u32,
}

async fn load_ledger(db: &Db) -> Result<BTreeMap<String, LedgerEntry>> {
    let rows = db
        .call_flat(
            |conn| -> std::result::Result<Vec<LedgerEntry>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT document_name, document_id, content_hash, entities, relations,
                        candidates, entities_json, facts_json, ingested_at
                   FROM knowledge_graph_ingest",
                )?;
                stmt.query_map([], |row| {
                    let entities_json: String = row.get(6)?;
                    let facts_json: String = row.get(7)?;
                    Ok(LedgerEntry {
                        document_name: row.get(0)?,
                        document_id: row.get(1)?,
                        content_hash: row.get(2)?,
                        entities: row.get(3)?,
                        relations: row.get(4)?,
                        candidates: row.get(5)?,
                        ingested_at: row.get(8)?,
                        claims: RevisionClaims {
                            entities: json_list(&entities_json),
                            facts: json_list(&facts_json),
                        },
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()
            },
        )
        .await
        .context("read the knowledge graph ingestion ledger")?;
    Ok(rows
        .into_iter()
        .map(|row| (row.document_name.clone(), row))
        .collect())
}

async fn write_ledger_row(
    db: &Db,
    document_name: &str,
    document_id: i64,
    content_hash: &str,
    counts: LedgerCounts,
    claims: &RevisionClaims,
) -> Result<()> {
    let name = document_name.to_string();
    let hash = content_hash.to_string();
    let entities_json = json_of(&claims.entities);
    let facts_json = json_of(&claims.facts);
    let ingested_at = chrono::Utc::now().to_rfc3339();
    db.call_flat(move |conn| -> std::result::Result<(), rusqlite::Error> {
        conn.execute(
            "INSERT INTO knowledge_graph_ingest
                 (document_name, document_id, content_hash, entities, relations, candidates,
                  entities_json, facts_json, ingested_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(document_name) DO UPDATE SET
                 document_id   = excluded.document_id,
                 content_hash  = excluded.content_hash,
                 entities      = excluded.entities,
                 relations     = excluded.relations,
                 candidates    = excluded.candidates,
                 entities_json = excluded.entities_json,
                 facts_json    = excluded.facts_json,
                 ingested_at   = excluded.ingested_at",
            rusqlite::params![
                name,
                document_id,
                hash,
                counts.entities as i64,
                counts.relations as i64,
                counts.candidates as i64,
                entities_json,
                facts_json,
                ingested_at
            ],
        )?;
        Ok(())
    })
    .await
    .context("write the knowledge graph ingestion ledger")?;
    Ok(())
}

/// Retract one document's previous revision, in ONE transaction.
///
/// * `facts_to_retract` — fact hashes no live claim still states: their edges
///   are closed on both axes (never deleted, 0005_graph.sql:4-5).
/// * `entities_to_delete` — entity names the current revision dropped. Each is
///   deleted only if it has no currently-valid edge, no alias another ledger
///   row claims (`claimed_elsewhere_entities`), and no edge row whose fact hash
///   came from somewhere other than `own_fact_hashes` (the revision being
///   replaced). Deleting an entity needs its edge rows gone first: they carry a
///   foreign key (0005_graph.sql:26-27), and the ones removed here are exactly
///   the retracted history of the revision being replaced.
#[allow(clippy::too_many_arguments)]
async fn prune_previous_revision(
    db: &Db,
    facts_to_retract: Vec<String>,
    entities_to_delete: Vec<String>,
    claimed_elsewhere_entities: BTreeSet<String>,
    own_fact_hashes: BTreeSet<String>,
) -> Result<PruneOutcome> {
    let now = chrono::Utc::now().to_rfc3339();
    db.call_flat(
        move |conn| -> std::result::Result<PruneOutcome, rusqlite::Error> {
            let tx = conn.transaction()?;
            let mut out = PruneOutcome::default();

            for hash in &facts_to_retract {
                out.facts_retracted += tx.execute(
                    "UPDATE entity_edges
                    SET expired_at = COALESCE(expired_at, ?1),
                        invalid_at = COALESCE(invalid_at, ?1)
                  WHERE fact_hash = ?2 AND invalid_at IS NULL",
                    rusqlite::params![now, hash],
                )? as u64;
            }

            for name in &entities_to_delete {
                let id: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM entities WHERE norm_name = ?1",
                        [norm_key(name)],
                        |r| r.get(0),
                    )
                    .optional()?;
                let Some(id) = id else {
                    continue;
                };
                let live: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM entity_edges
                  WHERE (src = ?1 OR dst = ?1) AND invalid_at IS NULL",
                    [id],
                    |r| r.get(0),
                )?;
                if live > 0 {
                    out.entities_kept += 1;
                    continue;
                }
                let aliases: Vec<String> = {
                    let mut stmt =
                        tx.prepare("SELECT norm_alias FROM entity_aliases WHERE entity_id = ?1")?;
                    stmt.query_map([id], |r| r.get(0))?
                        .collect::<std::result::Result<Vec<_>, _>>()?
                };
                if aliases
                    .iter()
                    .any(|a| claimed_elsewhere_entities.contains(a))
                {
                    out.entities_kept += 1;
                    continue;
                }
                let hashes: Vec<Option<String>> = {
                    let mut stmt = tx
                        .prepare("SELECT fact_hash FROM entity_edges WHERE src = ?1 OR dst = ?1")?;
                    stmt.query_map([id], |r| r.get(0))?
                        .collect::<std::result::Result<Vec<_>, _>>()?
                };
                let foreign_history = hashes.iter().any(|h| match h.as_deref() {
                    Some(h) => !own_fact_hashes.contains(h),
                    // A pre-0021 edge has no hash: never ours to delete.
                    None => true,
                });
                if foreign_history {
                    out.entities_kept += 1;
                    continue;
                }
                tx.execute("DELETE FROM entity_edges WHERE src = ?1 OR dst = ?1", [id])?;
                tx.execute("DELETE FROM entity_aliases WHERE entity_id = ?1", [id])?;
                tx.execute(
                    "DELETE FROM resolution_pending WHERE entity_a = ?1 OR entity_b = ?1",
                    [id],
                )?;
                tx.execute("DELETE FROM community_entities WHERE entity_id = ?1", [id])?;
                tx.execute("DELETE FROM entities WHERE id = ?1", [id])?;
                out.entities_deleted += 1;
            }

            tx.commit()?;
            Ok(out)
        },
    )
    .await
    .context("prune the previous revision")
}

/// A document's name is its markdown path under `<root>/knowledge` with the
/// `.md` suffix removed (`Knowledge::save` stores exactly that,
/// files.rs:180-185, 230). Accepting either spelling here is what lets the
/// HTTP route take `notes`, `notes.md` or `wiki/notes`.
fn canonical_doc_name(name: &str) -> String {
    name.trim()
        .strip_suffix(".md")
        .unwrap_or(name.trim())
        .to_string()
}

/// A JSON string array out of a ledger column. A malformed value yields an
/// empty list: the ledger then claims nothing, which can only make the prune
/// more conservative, never destructive.
fn json_list(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn json_of(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(entities: &[&str], facts: &[&str]) -> RevisionClaims {
        RevisionClaims {
            entities: entities.iter().map(|e| e.to_string()).collect(),
            facts: facts.iter().map(|f| f.to_string()).collect(),
        }
    }

    async fn scalar(db: &Db, sql: &'static str) -> i64 {
        db.call_flat(move |conn| conn.query_row(sql, [], |r| r.get(0)))
            .await
            .unwrap()
    }

    /// The 60 s scan loop's sweep (lib.rs calls `sweep_if_enabled`): a no-op
    /// until the capability is on, then one bounded pass per scan, and never
    /// twice for the same content.
    #[tokio::test]
    async fn the_scan_loop_sweep_is_a_no_op_until_the_capability_is_on() {
        let dir = std::env::temp_dir().join(format!(
            "ruagent-t4-sweep-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open_in_memory().unwrap();
        let kb = ruagent_knowledge::Knowledge::open(&dir, db.clone())
            .await
            .unwrap();
        kb.save("doc-a", "# Alpha\n\n# Beta\n\nAlpha is a Beta.\n")
            .await
            .unwrap();

        // DEFAULT configuration: no `[capabilities]` table at all.
        let legacy = std::sync::Arc::new(std::sync::RwLock::new(
            crate::capability::CapabilityPlane::legacy(),
        ));
        sweep_if_enabled(&db, &kb, &legacy).await;
        assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, 0);
        assert_eq!(
            scalar(&db, "SELECT COUNT(*) FROM knowledge_graph_ingest").await,
            0,
            "the ledger is untouched while the capability is off"
        );

        // Turning it on takes effect at the NEXT pass: no restart, no new job.
        let policy = ruagent_policy::PolicyConfig::parse(
            "[capabilities.knowledge_ingest_graph]\nenabled = true\n",
        )
        .unwrap();
        let on = std::sync::Arc::new(std::sync::RwLock::new(
            crate::capability::CapabilityPlane::from_policy(&policy).unwrap(),
        ));
        sweep_if_enabled(&db, &kb, &on).await;
        assert!(scalar(&db, "SELECT COUNT(*) FROM entities").await >= 2);
        assert_eq!(
            scalar(&db, "SELECT COUNT(*) FROM knowledge_graph_ingest").await,
            1
        );

        // A second pass over unchanged content writes nothing (that is the whole
        // point of the ledger: the sweep runs every 60 s).
        let before = scalar(&db, "SELECT COUNT(*) FROM entities").await;
        sweep_if_enabled(&db, &kb, &on).await;
        assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, before);
    }

    #[test]
    fn an_unchanged_revision_retracts_nothing() {
        let prev = claims(&["Alpha", "Beta"], &["h1", "h2"]);
        let (facts, entities) = retractions(&prev, &prev, &BTreeSet::new(), &BTreeSet::new());
        assert!(facts.is_empty(), "{facts:?}");
        assert!(entities.is_empty(), "{entities:?}");
    }

    #[test]
    fn a_dropped_claim_is_retracted_and_a_moved_one_is_not() {
        let prev = claims(&["Alpha", "Beta"], &["h1", "h2"]);
        let now = claims(&["beta"], &["h2"]);
        let (facts, entities) = retractions(&prev, &now, &BTreeSet::new(), &BTreeSet::new());
        // identity is the graph's norm_name, so `Beta`/`beta` is the same node
        assert_eq!(facts, vec!["h1".to_string()]);
        assert_eq!(entities, vec!["Alpha".to_string()]);
    }

    #[test]
    fn another_documents_claim_is_never_retracted() {
        let prev = claims(&["Alpha", "Beta"], &["h1", "h2"]);
        let now = claims(&[], &[]);
        let elsewhere_entities = BTreeSet::from(["alpha".to_string()]);
        let elsewhere_facts = BTreeSet::from(["h1".to_string()]);
        let (facts, entities) = retractions(&prev, &now, &elsewhere_entities, &elsewhere_facts);
        assert_eq!(facts, vec!["h2".to_string()]);
        assert_eq!(entities, vec!["Beta".to_string()]);
    }

    #[test]
    fn a_document_that_gains_content_retracts_only_what_it_dropped() {
        let prev = claims(&["Alpha"], &["h1"]);
        let now = claims(&["Alpha", "Gamma"], &["h1", "h3"]);
        let (facts, entities) = retractions(&prev, &now, &BTreeSet::new(), &BTreeSet::new());
        assert!(facts.is_empty(), "{facts:?}");
        assert!(entities.is_empty(), "{entities:?}");
    }

    #[test]
    fn document_names_accept_both_spellings() {
        assert_eq!(canonical_doc_name("notes"), "notes");
        assert_eq!(canonical_doc_name("notes.md"), "notes");
        assert_eq!(canonical_doc_name(" wiki/notes.md "), "wiki/notes");
        assert_eq!(canonical_doc_name("wiki/notes"), "wiki/notes");
    }

    #[test]
    fn a_malformed_ledger_cell_claims_nothing() {
        assert_eq!(json_list("not json"), Vec::<String>::new());
        assert_eq!(json_list("[\"a\"]"), vec!["a".to_string()]);
        assert_eq!(json_of(&["a".to_string()]), "[\"a\"]");
    }
}
