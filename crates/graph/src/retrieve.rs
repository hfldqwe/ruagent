//! Gen2 graph retrieval (design: docs/design/reviews/gen2-graph-spec.md §D).
//!
//! Three things this module exists to make answerable, none of which the
//! pre-gen2 crate could answer:
//!
//! 1. WHY did a seed resolve? `SeedLeg` names the leg (exact name / alias table /
//!    name token / summary FTS), and every leg's count appears in the evidence
//!    even when it is 0. The pre-gen2 entity leg was one opaque FTS phrase.
//! 2. WHY are these two hops connected? A path is returned as a first-class
//!    object (`EvidencePath`): the nodes, the edges in traversal order, the
//!    per-hop score factors so the score can be recomputed by a reader, and the
//!    bi-temporal status + provenance of every edge.
//! 3. WHY is the result empty/short? `RetrievalStats` carries the frontier sizes,
//!    which budget truncated the walk, and a named `EmptyReason`. The pre-gen2
//!    recall path could only say "no entities" -- "no seed", "seeds but no edge"
//!    and "the walk hit its ceiling" were indistinguishable.
//!
//! CJK: this module never segments text itself. It asks `ruagent_store::fts` for
//! the vocabulary (`terms`, `han_bigrams`), for the recall forms
//! (`match_any_prefix`), for the substring patterns (`like_patterns`) and for the
//! recall floor (`MIN_RECALL_ASCII`). `match_bigrams` is NOT used here and the
//! reason is mechanical: it builds a phrase over bigram tokens, which only exists
//! in `chunks_fts_cjk`; `entities_fts` has no bigram column, so a bigram phrase
//! could never match it.

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use ruagent_store::Db;

use crate::{DbError, Entity};

/// Parse one stored instant into a TIME, not a string (RVC-1).
///
/// WHY this exists: the first version compared `valid_at <= as_of` as **text**.
/// Two spellings of the SAME instant then sort differently, so
/// `2026-09-13T18:38:15.3506+08:00`, `2026-09-13T10:38:15.3506Z` and the
/// canonical `.350600000+00:00` produced **different evidence sets** (measured by
/// RV-C: at one instant the canonical form was 0 wrong, the `Z` form judged 16
/// rows backwards, the `+08:00` form 21; at another instant 0 / 19 / 32 — and
/// the SETS differed too: `银河麒麟 V10` canonical [2,3,4,5,10,11,14,26] vs the
/// `Z` form which also returned 39). The instant is the same moment; the answer
/// must be the same answer.
///
/// Accepted shapes (the live table has all of them, plus date-only rows that the
/// pre-gen2 writer produced):
///   * RFC3339 with `Z` or an offset, seconds optional, nanoseconds 0-9 digits;
///   * `YYYY-MM-DD` (midnight UTC);
///   * `YYYY-MM-DD HH:MM:SS` and `YYYY-MM-DDTHH:MM:SS` with no zone (UTC).
pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc());
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt.and_utc());
        }
    }
    None
}

/// Default lookahead, and the ceiling the walk will not cross even if asked to.
pub const DEFAULT_HOPS: u32 = 2;
pub const MAX_HOPS: u32 = 3;
pub const DEFAULT_BEAM: u32 = 8;
pub const DEFAULT_MAX_PATHS: u32 = 12;
pub const DEFAULT_MAX_FACTS: u32 = 24;
/// Personalized-propagation damping per hop (HippoRAG uses PPR; at this graph
/// size a bounded damped walk is the same ranking with a bound a reader can
/// recompute).
pub const DAMPING: f32 = 0.5;

/// One seeds-only budget so a caller cannot ask for an unbounded seed scan.
const SEED_CAP: u32 = 32;

/// A retrieval request. Budgets and the time view are data, not constants.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GraphQuery {
    /// The raw query string (user/task text). Seed resolution happens HERE.
    pub text: String,
    /// Lookahead. Values above `MAX_HOPS` are clamped and reported as truncated.
    pub hops: u32,
    /// Frontier width kept per hop.
    pub beam: u32,
    /// Paths returned.
    pub max_paths: u32,
    /// Distinct edges returned across all paths.
    pub max_facts: u32,
    /// Bi-temporal view: "what was true as of X" (RFC3339). None = now.
    pub as_of: Option<String>,
    /// Also return superseded edges as history. Then every such edge MUST carry
    /// `TemporalStatus::Superseded` (a reader cannot mistake it for current).
    pub include_superseded: bool,
}

impl Default for GraphQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            hops: DEFAULT_HOPS,
            beam: DEFAULT_BEAM,
            max_paths: DEFAULT_MAX_PATHS,
            max_facts: DEFAULT_MAX_FACTS,
            as_of: None,
            include_superseded: false,
        }
    }
}

impl GraphQuery {
    /// The common case: a query string with every other field at its default.
    pub fn for_text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

/// Which leg found a seed. A seed that two legs could both claim keeps the
/// STRONGER leg, so `seeds_by_leg` sums to the seed count.
///
/// `Hash` is derived on top of the spec's list so a caller can index leg counts
/// by leg (the panel/recall wiring does exactly that).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedLeg {
    ExactName,
    AliasTable,
    NameToken,
    SummaryFts,
    VectorNearest,
}

impl SeedLeg {
    /// Stable name for logs and assertions.
    pub fn as_str(self) -> &'static str {
        match self {
            SeedLeg::ExactName => "exact_name",
            SeedLeg::AliasTable => "alias_table",
            SeedLeg::NameToken => "name_token",
            SeedLeg::SummaryFts => "summary_fts",
            SeedLeg::VectorNearest => "vector_nearest",
        }
    }
    /// The substrate score of a leg. Frozen: these are the spec's §D.1 constants.
    fn base_score(self) -> f32 {
        match self {
            SeedLeg::ExactName => 1.0,
            SeedLeg::AliasTable => 0.9,
            SeedLeg::NameToken => 0.7,
            SeedLeg::SummaryFts => 0.5,
            SeedLeg::VectorNearest => 0.4,
        }
    }
    /// Every leg, in strength order -- the list a reader sees counts for.
    pub const ALL: [SeedLeg; 5] = [
        SeedLeg::ExactName,
        SeedLeg::AliasTable,
        SeedLeg::NameToken,
        SeedLeg::SummaryFts,
        SeedLeg::VectorNearest,
    ];
}

/// One resolved seed.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SeedHit {
    pub entity: Entity,
    pub leg: SeedLeg,
    pub score: f32,
}

/// An edge's state in the requested time view. `RecordedAtOnly` is the one that
/// matters: it says the timestamp is the write clock, NOT an event time, so a
/// reader must not treat it as "when this was true".
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum TemporalStatus {
    /// Currently valid and the event time came from extraction.
    Current,
    /// Currently valid, but `valid_at` is the write clock (66/67 live edges).
    RecordedAtOnly,
    /// True as of the requested instant, superseded since.
    TrueAsOf {
        valid_at: String,
        invalid_at: String,
    },
    /// Not true any more (only with `include_superseded`).
    Superseded { invalid_at: String },
}

/// Where an edge came from. All three parts can be `None`: "unknown" is a fact,
/// and nothing here backfills it with a plausible-looking value.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EdgeSource {
    /// `entity_edges.source_episode` (FK to `episodes(id)`; never a sentinel).
    pub episode: Option<i64>,
    /// The episode's `source_run` (the session key), when there is an episode.
    pub session_key: Option<String>,
    /// `entity_edges.created_at` -- the record time (T').
    pub recorded_at: String,
}

/// One edge as evidence.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EvidenceEdge {
    pub edge_id: i64,
    pub src: i64,
    pub dst: i64,
    pub src_name: String,
    pub dst_name: String,
    pub relation: String,
    pub fact_text: String,
    /// T: the event time when extraction supplied one, else the write clock.
    pub valid_at: String,
    pub invalid_at: Option<String>,
    pub temporal: TemporalStatus,
    pub source: EdgeSource,
}

/// Why this path was kept: the factors a reader can multiply to recompute the
/// score. `factors[0]` is the seed score; the rest are per-hop weights.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PathRationale {
    pub seed_leg: SeedLeg,
    pub factors: Vec<f32>,
}

/// A path IS the evidence: `nodes[0]` is the seed, `edges[i]` joins
/// `nodes[i]` and `nodes[i+1]`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EvidencePath {
    pub hops: u32,
    pub score: f32,
    pub nodes: Vec<Entity>,
    pub edges: Vec<EvidenceEdge>,
    pub why: PathRationale,
}

/// Which budget stopped the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncationBudget {
    Hops,
    Beam,
    MaxPaths,
    MaxFacts,
}

/// Why the result is empty. Exactly one of these is reported, and it is never
/// inferred from "the answer is short".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyReason {
    /// Nothing in the query resolves to a graph object.
    NoSeed,
    /// Seeds exist but no edge incident to them (an orphan entity: 20/63 live).
    NoEdgeFromSeeds,
    /// `as_of` precedes every edge's valid_at.
    AsOfBeforeAnyFact,
    /// The frontier was still non-empty at the hop ceiling.
    HopsCap,
    /// Every hop's frontier was cut by the beam and no path survived.
    BeamCap,
}

/// The evidence's own account of itself.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RetrievalStats {
    /// Every leg, including the ones that found nothing.
    pub seeds_by_leg: Vec<(SeedLeg, u32)>,
    /// Frontier size AFTER each hop.
    pub frontier_sizes: Vec<u32>,
    pub paths_considered: u32,
    pub paths_emitted: u32,
    pub truncated_by: Option<TruncationBudget>,
    pub empty_reason: Option<EmptyReason>,
    /// Edge count of the whole graph at this time view, so a reader can tell
    /// "found little" from "there is little".
    pub graph_edges: u32,
}

/// What `retrieve` returns.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GraphEvidence {
    pub seeds: Vec<SeedHit>,
    pub paths: Vec<EvidencePath>,
    pub stats: RetrievalStats,
}

impl GraphEvidence {
    /// Distinct current facts carried by the returned paths.
    pub fn fact_count(&self) -> u32 {
        let mut ids = BTreeSet::new();
        for p in &self.paths {
            for e in &p.edges {
                ids.insert(e.edge_id);
            }
        }
        ids.len() as u32
    }
    /// The rendered shape the injection contract consumes: one line per path,
    /// carrying path + hops + temporal + source in the SAME line (§D.2).
    ///
    /// RVC-5: the walk is UNDIRECTED (`neighbors` semantics), so roughly half of
    /// all hops traverse an edge backwards. The first version always rendered
    /// `nodes[i] -relation-> nodes[i+1]`, which printed `微信 -uses-> 用户19410`
    /// for an edge stored as `用户19410 -uses-> 微信` -- a line destined for
    /// injection into an agent, asserting the REVERSE of the stored fact. Now the
    /// arrow is derived from each edge's own `src`/`dst`: a forward hop renders
    /// `-rel->`, a reverse hop renders `<-rel-`, and the two endpoints are always
    /// named in the direction the edge is stored in.
    pub fn lines(&self) -> Vec<String> {
        self.paths
            .iter()
            .map(|p| {
                let name = |id: i64| -> String {
                    p.nodes
                        .iter()
                        .find(|n| n.id == id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| format!("#{id}"))
                };
                let mut s = p.nodes.first().map(|n| n.name.clone()).unwrap_or_default();
                for (i, e) in p.edges.iter().enumerate() {
                    let here = p.nodes.get(i).map(|n| n.id);
                    let next = p.nodes.get(i + 1).map(|n| n.id);
                    if Some(e.src) == here && Some(e.dst) == next {
                        s.push_str(&format!(" -{}-> {}", e.relation, name(e.dst)));
                    } else if Some(e.src) == next && Some(e.dst) == here {
                        s.push_str(&format!(" <-{}- {}", e.relation, name(e.src)));
                    } else {
                        // Cannot happen for a path built from connecting edges; if
                        // it ever does, name both endpoints from the EDGE so the
                        // line is still true rather than silently chained.
                        s.push_str(&format!(
                            " ({} -{}-> {})",
                            name(e.src),
                            e.relation,
                            name(e.dst)
                        ));
                    }
                }
                let newest = p
                    .edges
                    .iter()
                    .map(|e| e.valid_at.as_str())
                    .max()
                    .unwrap_or("");
                let state = p
                    .edges
                    .iter()
                    .map(|e| match &e.temporal {
                        TemporalStatus::Current => "current",
                        TemporalStatus::RecordedAtOnly => "recorded_at_only",
                        TemporalStatus::TrueAsOf { .. } => "true_as_of",
                        TemporalStatus::Superseded { .. } => "superseded",
                    })
                    .collect::<Vec<_>>()
                    .join("+");
                let src = p
                    .edges
                    .iter()
                    .filter_map(|e| e.source.session_key.as_deref())
                    .next()
                    .unwrap_or("-");
                let ids = p
                    .edges
                    .iter()
                    .map(|e| e.edge_id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                s.push_str(&format!(
                    " [hops={} valid_at={} state={} edges={} source={}]",
                    p.hops, newest, state, ids, src
                ));
                s
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Seed resolution
// ---------------------------------------------------------------------------

/// Everything the entity legs need, in ONE read transaction: the candidate
/// entity rows plus which leg claimed each one.
#[derive(Debug, Default)]
struct SeedBucket {
    hits: HashMap<i64, SeedHit>,
}

impl SeedBucket {
    /// Keep the stronger leg; on a tie keep the higher score.
    fn offer(&mut self, entity: Entity, leg: SeedLeg, score: f32) {
        let better = match self.hits.get(&entity.id) {
            None => true,
            Some(old) => (leg, f32_key(score)) < (old.leg, f32_key(old.score)),
        };
        if better {
            let score = score.max(leg.base_score());
            self.hits.insert(entity.id, SeedHit { entity, leg, score });
        }
    }
    fn into_sorted(self, limit: u32) -> Vec<SeedHit> {
        let mut v: Vec<SeedHit> = self.hits.into_values().collect();
        v.sort_by(|a, b| {
            a.leg
                .cmp(&b.leg)
                .then_with(|| f32_key(b.score).cmp(&f32_key(a.score)))
                .then_with(|| a.entity.id.cmp(&b.entity.id))
        });
        v.truncate(limit as usize);
        v
    }
}

/// `f32` cannot be an `Ord` key, and NaN has no place in a ranking: total-order
/// the bits so a comparison is never a surprise.
fn f32_key(x: f32) -> i64 {
    let bits = if x.is_nan() { 0.0f32 } else { x }.to_bits() as i64;
    if bits < 0 { i64::MIN - bits } else { bits }
}

fn quote_phrase(t: &str) -> String {
    format!("\"{}\"", t.replace('"', "\"\""))
}

/// Resolve a query string to graph objects, naming the leg for each.
///
/// Legs, in the order they run (the first two are precision, the rest recall):
/// 1. `ExactName`   -- `norm_name = norm(text)`: the whole query is the name.
/// 2. `AliasTable`  -- a term (or the whole query) is a registered alias.
/// 3. `NameToken`   -- FTS prefix over the NAME column, an entity name contained
///    in the query, and a Han substring of a name (LIKE).
/// 4. `SummaryFts`  -- FTS prefix over the SUMMARY column. Lowest, because a
///    summary hit is the entity's description mentioning the word, not the
///    entity being named: `ruagent` returns 10 entities this way (live reading).
/// 5. `VectorNearest` -- never populated: no embedder is wired into this crate
///    (the entity vector column exists since t6; wiring one is E10, P2).
pub async fn resolve_seeds(db: &Db, text: &str, limit: u32) -> Result<Vec<SeedHit>, DbError> {
    let text = text.to_string();
    let limit = limit.clamp(1, SEED_CAP);
    db.call_flat(move |conn| -> Result<Vec<SeedHit>, rusqlite::Error> {
        let mut bucket = SeedBucket::default();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let terms = ruagent_store::fts::terms(trimmed);
        let bigram_words: Vec<String> = ruagent_store::fts::han_bigrams(trimmed)
            .split_whitespace()
            .map(str::to_string)
            .collect();

        let read_entity = |row: &rusqlite::Row<'_>| -> rusqlite::Result<Entity> {
            Ok(Entity {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                summary: row.get(3)?,
            })
        };

        // 1. Exact name.
        let norm_text = crate::norm(trimmed);
        {
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, summary FROM entities WHERE norm_name = ?1",
            )?;
            let rows = stmt.query_map([&norm_text], read_entity)?;
            for r in rows {
                bucket.offer(r?, SeedLeg::ExactName, 1.0);
            }
        }

        // 2. Alias table (query as a whole, and each term).
        {
            let mut probes: Vec<String> = vec![norm_text.clone()];
            probes.extend(terms.iter().cloned());
            probes.sort();
            probes.dedup();
            for p in probes {
                let mut stmt = conn.prepare(
                    "SELECT e.id, e.name, e.kind, e.summary
                     FROM entity_aliases a JOIN entities e ON e.id = a.entity_id
                     WHERE a.norm_alias = ?1 ORDER BY e.id",
                )?;
                let rows = stmt.query_map([&p], read_entity)?;
                for r in rows {
                    bucket.offer(r?, SeedLeg::AliasTable, 0.9);
                }
            }
        }

        // 3a. FTS prefix over the NAME column only (store builds the form).
        let prefix_form = ruagent_store::fts::match_any_prefix(&terms);
        if !prefix_form.is_empty() {
            let expr = format!("name : ({prefix_form})");
            let mut stmt = conn.prepare(
                "SELECT e.id, e.name, e.kind, e.summary
                 FROM entities_fts f JOIN entities e ON e.id = f.rowid
                 WHERE entities_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![expr, limit], read_entity)?;
            for r in rows {
                bucket.offer(r?, SeedLeg::NameToken, 0.7);
            }
        }

        // 3b. An entity NAME contained in the query text ("蓝鲸潜艇是什么" names
        //     蓝鲸潜艇). Not expressible as an FTS query, and `instr` is exact.
        {
            let floor = ruagent_store::fts::MIN_RECALL_ASCII;
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, summary FROM entities
                 WHERE instr(lower(?1), norm_name) > 0 ORDER BY length(norm_name) DESC, id LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![trimmed.to_lowercase(), limit], read_entity)?;
            for r in rows {
                let e = r?;
                // The same floor the store applies to recall forms: a 1-2 char
                // ASCII name is not evidence that the query is ABOUT it.
                let n = crate::norm(&e.name);
                if n.chars().count() < floor && n.is_ascii() {
                    continue;
                }
                bucket.offer(e, SeedLeg::NameToken, 0.7);
            }
        }

        // 3c. Han substring of a NAME: the only path FTS5 cannot express
        //     (unicode61 keeps a Han run as one token). The probe set comes from
        //     the store's bigram vocabulary; the pattern comes from the store's
        //     LIKE builder.
        {
            let mut probes: Vec<String> = Vec::new();
            for t in terms.iter().chain(bigram_words.iter()) {
                if t.chars().any(is_han) {
                    probes.push(t.clone());
                    probes.push(quote_phrase(t));
                }
            }
            probes.sort();
            probes.dedup();
            for pat in ruagent_store::fts::like_patterns(&probes) {
                let mut stmt = conn.prepare(
                    "SELECT id, name, kind, summary FROM entities
                     WHERE name LIKE ?1 ESCAPE '\\' ORDER BY id LIMIT ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![pat, limit], read_entity)?;
                for r in rows {
                    bucket.offer(r?, SeedLeg::NameToken, 0.7);
                }
            }
        }

        // 4. Summary FTS: recall, and the noisiest leg.
        if !prefix_form.is_empty() {
            let expr = format!("summary : ({prefix_form})");
            let mut stmt = conn.prepare(
                "SELECT e.id, e.name, e.kind, e.summary
                 FROM entities_fts f JOIN entities e ON e.id = f.rowid
                 WHERE entities_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![expr, limit], read_entity)?;
            for r in rows {
                bucket.offer(r?, SeedLeg::SummaryFts, 0.5);
            }
        }

        // The one reportable form of "no vector leg": the leg ran and found
        // nothing because no embedder is wired (never silently dropped).
        Ok(bucket.into_sorted(limit))
    })
    .await
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

// ---------------------------------------------------------------------------
// Multi-hop retrieval
// ---------------------------------------------------------------------------

/// One raw edge row plus the two things `Edge` (frozen) does not carry.
#[derive(Debug, Clone)]
struct RawEdge {
    id: i64,
    src: i64,
    dst: i64,
    relation: String,
    fact_text: String,
    valid_at: String,
    invalid_at: Option<String>,
    /// The two instants above, PARSED ONCE (RVC-1). `None` = the stored text is
    /// not a shape `parse_ts` understands; the comparison then falls back to the
    /// raw string so no row silently disappears, and that fallback is the only
    /// place text order is still used.
    valid_at_t: Option<DateTime<Utc>>,
    invalid_at_t: Option<DateTime<Utc>>,
    created_at: String,
    source_episode: Option<i64>,
    session_key: Option<String>,
    event_time_source: Option<String>,
}

/// A path under construction.
#[derive(Debug, Clone)]
struct PathState {
    nodes: Vec<i64>,
    edges: Vec<RawEdge>,
    factors: Vec<f32>,
    leg: SeedLeg,
}

impl PathState {
    fn score(&self) -> f32 {
        self.factors.iter().product::<f32>()
    }
}

/// The walk's own view of the graph, computed once per call.
struct GraphIndex {
    /// incident current edges per entity, with the entity's degree
    incident: HashMap<i64, Vec<RawEdge>>,
    /// how many current edges carry each relation name
    rel_count: HashMap<String, u32>,
    /// current-edge degree per entity
    degree: HashMap<i64, u32>,
    total_current: u32,
}

impl GraphIndex {
    fn rel_idf(&self, relation: &str) -> f32 {
        let n = *self.rel_count.get(relation).unwrap_or(&0) as f32;
        1.0 / (1.0 + (1.0 + n).ln())
    }
    fn degree_norm(&self, e: &RawEdge) -> f32 {
        let ds = *self.degree.get(&e.src).unwrap_or(&0) as f32;
        let dd = *self.degree.get(&e.dst).unwrap_or(&0) as f32;
        1.0 / (1.0 + (1.0 + ds + dd).ln())
    }
    /// Frozen §D.1 weight: damping x relation rarirty x degree penalty.
    fn hop_weight(&self, e: &RawEdge) -> f32 {
        DAMPING * self.rel_idf(&e.relation) * self.degree_norm(e)
    }
}

/// Was this edge true at instant `t`? Chronological when both stored instants
/// parse; the raw-string form survives ONLY as a per-row fallback for a shape
/// `parse_ts` does not know, so an unparsable row is still judged rather than
/// dropped. `t` itself is always a parsed instant (the entry point refuses to
/// run otherwise).
fn true_then_at(e: &RawEdge, t: DateTime<Utc>) -> bool {
    let starts = match e.valid_at_t {
        Some(v) => v <= t,
        None => e.valid_at.as_str() <= t.to_rfc3339().as_str(),
    };
    if !starts {
        return false;
    }
    match e.invalid_at_t {
        Some(inv) => inv > t,
        None => match e.invalid_at.as_deref() {
            Some(raw) => raw > t.to_rfc3339().as_str(),
            None => true,
        },
    }
}

fn temporal_status(e: &RawEdge, as_of: Option<DateTime<Utc>>) -> TemporalStatus {
    match as_of {
        None => {
            if e.invalid_at.is_some() {
                TemporalStatus::Superseded {
                    invalid_at: e.invalid_at.clone().unwrap_or_default(),
                }
            } else if e.event_time_source.as_deref() == Some("extracted") {
                TemporalStatus::Current
            } else {
                TemporalStatus::RecordedAtOnly
            }
        }
        Some(t) => {
            let true_then = true_then_at(e, t);
            if true_then && e.invalid_at.is_some() {
                TemporalStatus::TrueAsOf {
                    valid_at: e.valid_at.clone(),
                    invalid_at: e.invalid_at.clone().unwrap_or_default(),
                }
            } else if true_then {
                if e.event_time_source.as_deref() == Some("extracted") {
                    TemporalStatus::Current
                } else {
                    TemporalStatus::RecordedAtOnly
                }
            } else {
                TemporalStatus::Superseded {
                    invalid_at: e.invalid_at.clone().unwrap_or_default(),
                }
            }
        }
    }
}

/// Multi-hop retrieval. The ONLY entry point for graph-assisted recall.
///
/// Bounded by construction: `hops <= MAX_HOPS`, at most `beam` paths per hop,
/// at most `max_paths` paths and `max_facts` distinct edges returned. Every cut
/// is REPORTED (`truncated_by`), never silent.
pub async fn retrieve(db: &Db, q: &GraphQuery) -> Result<GraphEvidence, DbError> {
    let hops = q.hops.clamp(1, MAX_HOPS);
    let beam = q.beam.max(1);
    let max_paths = q.max_paths.max(1);
    let max_facts = q.max_facts.max(1);
    let as_of_text = q.as_of.clone();
    // RVC-1: the instant is parsed ONCE, here, at the entrance. A text that is
    // not an instant is refused loudly instead of being compared as a string --
    // 2026-09-13T10:38:15Z and 2026-09-13T18:38:15+08:00 are the same moment and
    // must not be two different answers.
    let as_of: Option<DateTime<Utc>> = match as_of_text.as_deref() {
        None => None,
        Some(raw) => match parse_ts(raw) {
            Some(t) => Some(t),
            None => {
                return Err(DbError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "as_of is not an instant: {raw:?} (accepted: RFC3339 with Z or an offset, \
                         with or without nanoseconds; YYYY-MM-DD; %Y-%m-%d %H:%M:%S)"
                    ),
                )));
            }
        },
    };
    let include_superseded = q.include_superseded;

    let seeds = resolve_seeds(db, &q.text, SEED_CAP).await?;
    let mut stats = RetrievalStats {
        seeds_by_leg: SeedLeg::ALL
            .iter()
            .map(|leg| (*leg, seeds.iter().filter(|s| s.leg == *leg).count() as u32))
            .collect(),
        frontier_sizes: Vec::new(),
        paths_considered: 0,
        paths_emitted: 0,
        truncated_by: None,
        empty_reason: None,
        graph_edges: 0,
    };
    if seeds.is_empty() {
        stats.empty_reason = Some(EmptyReason::NoSeed);
        return Ok(GraphEvidence {
            seeds,
            paths: Vec::new(),
            stats,
        });
    }

    // One read of the graph: the walk needs degrees, relation rarity and the
    // temporal filter, and all three are properties of the whole edge set.
    let (index, traversable_edges) = {
        let as_of_f = as_of;
        let incl = include_superseded;
        db.call_flat(move |conn| -> Result<(GraphIndex, bool), rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT e.id, e.src, e.dst, e.relation, e.fact_text, e.valid_at, e.invalid_at,
                        e.created_at, e.source_episode, ep.source_run, e.event_time_source
                 FROM entity_edges e
                 LEFT JOIN episodes ep ON ep.id = e.source_episode
                 ORDER BY e.id",
            )?;
            let raw = stmt
                .query_map([], |row| {
                    let valid_at: String = row.get(5)?;
                    let invalid_at: Option<String> = row.get(6)?;
                    Ok(RawEdge {
                        id: row.get(0)?,
                        src: row.get(1)?,
                        dst: row.get(2)?,
                        relation: row.get(3)?,
                        fact_text: row.get(4)?,
                        valid_at_t: parse_ts(&valid_at),
                        invalid_at_t: invalid_at.as_deref().and_then(parse_ts),
                        valid_at,
                        invalid_at,
                        created_at: row.get(7)?,
                        source_episode: row.get(8)?,
                        session_key: row.get(9)?,
                        event_time_source: row.get(10)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut rel_count: HashMap<String, u32> = HashMap::new();
            let mut degree: HashMap<i64, u32> = HashMap::new();
            let mut total_current = 0u32;
            // Degrees and relation rarity are properties of the CURRENT graph
            // even when the walk runs as-of: a superseded edge must not make the
            // hub look rarer today than it is.
            for e in &raw {
                if e.invalid_at.is_none() {
                    total_current += 1;
                    *rel_count.entry(e.relation.clone()).or_insert(0) += 1;
                    *degree.entry(e.src).or_insert(0) += 1;
                    *degree.entry(e.dst).or_insert(0) += 1;
                }
            }

            // Which edges the walk may traverse under the requested time view.
            let mut incident: HashMap<i64, Vec<RawEdge>> = HashMap::new();
            let mut traversable = 0u32;
            for e in raw {
                let live = e.invalid_at.is_none();
                let ok = match as_of_f {
                    None => live || incl,
                    Some(t) => true_then_at(&e, t) || incl,
                };
                if !ok {
                    continue;
                }
                traversable += 1;
                incident.entry(e.src).or_default().push(e.clone());
                incident.entry(e.dst).or_default().push(e);
            }
            Ok((
                GraphIndex {
                    incident,
                    rel_count,
                    degree,
                    total_current,
                },
                traversable > 0,
            ))
        })
        .await?
    };
    stats.graph_edges = index.total_current;
    let traversable_any = traversable_edges;

    // The walk: paths (not nodes) are the frontier, so a returned path can
    // always say how it got there.
    let mut frontier: Vec<PathState> = seeds
        .iter()
        .map(|s| PathState {
            nodes: vec![s.entity.id],
            edges: Vec::new(),
            factors: vec![s.score],
            leg: s.leg,
        })
        .collect();
    let mut pool: Vec<PathState> = Vec::new();
    let mut cuts: Vec<TruncationBudget> = Vec::new();
    let mut seeded_no_edge = false;

    for _hop in 1..=hops {
        // Keyed by the EDGE-id sequence, not by the node sequence: two different
        // relations between the same pair are two different pieces of evidence.
        // (Keying by nodes silently dropped one of them.)
        let mut next: HashMap<Vec<i64>, PathState> = HashMap::new();
        let mut any_edge = false;
        for p in &frontier {
            let last = *p.nodes.last().unwrap_or(&0);
            let Some(cands) = index.incident.get(&last) else {
                continue;
            };
            for e in cands {
                let other = if e.src == last { e.dst } else { e.src };
                if p.nodes.contains(&other) {
                    continue; // a walk, not a cycle
                }
                any_edge = true;
                let mut nodes = p.nodes.clone();
                nodes.push(other);
                let mut factors = p.factors.clone();
                factors.push(index.hop_weight(e));
                let mut edges = p.edges.clone();
                edges.push(e.clone());
                let key: Vec<i64> = edges.iter().map(|x| x.id).collect();
                let cand = PathState {
                    nodes: nodes.clone(),
                    edges,
                    factors,
                    leg: p.leg,
                };
                next.entry(key)
                    .and_modify(|cur| {
                        if f32_key(cand.score()) > f32_key(cur.score()) {
                            *cur = cand.clone();
                        }
                    })
                    .or_insert(cand);
            }
        }
        if !any_edge && pool.is_empty() {
            seeded_no_edge = true;
        }
        let mut level: Vec<PathState> = next.into_values().collect();
        // RVC-4: the tie-break key must be a TOTAL order before any truncation.
        // `next` is a HashMap, so its iteration order varies per instance, and a
        // stable sort preserves that order for exactly-equal keys -- which is how
        // the SAME query returned 6 different JSON documents, swapping the slot
        // between the three parallel edges 49/52/53 (same endpoints, same score,
        // same node sequence). §D.1's `(score, hops, node ids)` cannot separate
        // parallel edges, so the EDGE ID SEQUENCE is appended.
        level.sort_by(|a, b| {
            f32_key(b.score())
                .cmp(&f32_key(a.score()))
                .then_with(|| a.edges.len().cmp(&b.edges.len()))
                .then_with(|| a.nodes.cmp(&b.nodes))
                .then_with(|| edge_seq(a).cmp(&edge_seq(b)))
        });
        if level.len() as u32 > beam {
            cuts.push(TruncationBudget::Beam);
            level.truncate(beam as usize);
        }
        stats.frontier_sizes.push(level.len() as u32);
        pool.extend(level.iter().cloned());
        stats.paths_considered = pool.len() as u32;
        let non_empty = !level.is_empty();
        frontier = level;
        if !non_empty {
            break;
        }
        if _hop == hops {
            // Ceiling reached with a live frontier: say so.
            cuts.push(TruncationBudget::Hops);
        }
    }

    // Rank the pool, dedupe by path shape, then apply the output budgets.
    pool.sort_by(|a, b| {
        f32_key(b.score())
            .cmp(&f32_key(a.score()))
            .then_with(|| a.edges.len().cmp(&b.edges.len()))
            .then_with(|| a.nodes.cmp(&b.nodes))
            .then_with(|| edge_seq(a).cmp(&edge_seq(b)))
    });
    let mut paths: Vec<PathState> = Vec::new();
    let mut seen_shapes: HashSet<Vec<i64>> = HashSet::new();
    for p in pool {
        let key: Vec<i64> = p.edges.iter().map(|e| e.id).collect();
        if seen_shapes.insert(key) {
            paths.push(p);
        }
    }
    let mut truncated = cuts.first().copied();
    if paths.len() as u32 > max_paths {
        paths.truncate(max_paths as usize);
        truncated = Some(TruncationBudget::MaxPaths);
    }
    // Distinct-edge budget: a path is dropped whole, never half.
    let mut used: HashSet<i64> = HashSet::new();
    let mut kept: Vec<PathState> = Vec::new();
    for p in paths {
        let new_edges = p.edges.iter().filter(|e| !used.contains(&e.id)).count() as u32;
        if used.len() as u32 + new_edges > max_facts && !kept.is_empty() {
            truncated = Some(TruncationBudget::MaxFacts);
            continue;
        }
        for e in &p.edges {
            used.insert(e.id);
        }
        kept.push(p);
    }

    let out = build_evidence(db, kept, as_of).await?;
    stats.paths_emitted = out.len() as u32;
    stats.truncated_by = if out.is_empty() { None } else { truncated };
    if out.is_empty() {
        // Order matters, and each test below is a DIFFERENT fact:
        //  * no seed at all             -> the query names nothing in the graph;
        //  * as_of before every fact    -> the walk is fine, the instant is early
        //    (checked BEFORE the traversable test, because "nothing is valid then"
        //    also makes the traversable set empty);
        //  * no edge from the seeds     -> the seeds are orphans (20/63 live);
        //  * a budget stopped the walk  -> the frontier was still growing.
        stats.empty_reason = Some(if seeds.is_empty() {
            EmptyReason::NoSeed
        } else if as_of.is_some() && index.total_current > 0 && !traversable_any {
            EmptyReason::AsOfBeforeAnyFact
        } else if seeded_no_edge || !traversable_any {
            EmptyReason::NoEdgeFromSeeds
        } else if truncated == Some(TruncationBudget::Hops) {
            EmptyReason::HopsCap
        } else {
            EmptyReason::BeamCap
        });
    }
    Ok(GraphEvidence {
        seeds,
        paths: out,
        stats,
    })
}

/// The edge-id sequence of a path: the last key of the ranking, and the only one
/// that separates parallel edges (RVC-4). Also the identity used to dedupe path
/// shapes, because two different relations between the same pair are two
/// different pieces of evidence.
fn edge_seq(p: &PathState) -> Vec<i64> {
    p.edges.iter().map(|e| e.id).collect()
}

/// Turn internal path states into the frozen evidence shape, reading the node
/// rows and each edge's endpoint names.
async fn build_evidence(
    db: &Db,
    paths: Vec<PathState>,
    as_of: Option<DateTime<Utc>>,
) -> Result<Vec<EvidencePath>, DbError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids: Vec<i64> = Vec::new();
    for p in &paths {
        ids.extend(p.nodes.iter().copied());
    }
    ids.sort();
    ids.dedup();
    let out = db
        .call_flat(move |conn| -> Result<Vec<EvidencePath>, rusqlite::Error> {
            let mut names: HashMap<i64, Entity> = HashMap::new();
            {
                let mut stmt =
                    conn.prepare("SELECT id, name, kind, summary FROM entities WHERE id = ?1")?;
                for id in &ids {
                    let e = stmt.query_row([id], |row| {
                        Ok(Entity {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            kind: row.get(2)?,
                            summary: row.get(3)?,
                        })
                    })?;
                    names.insert(*id, e);
                }
            }
            let mut out = Vec::with_capacity(paths.len());
            for p in paths {
                let nodes: Vec<Entity> = p
                    .nodes
                    .iter()
                    .map(|id| {
                        names.get(id).cloned().unwrap_or(Entity {
                            id: *id,
                            name: format!("#{id}"),
                            kind: None,
                            summary: None,
                        })
                    })
                    .collect();
                let edges = p
                    .edges
                    .iter()
                    .map(|e| EvidenceEdge {
                        edge_id: e.id,
                        src: e.src,
                        dst: e.dst,
                        src_name: names
                            .get(&e.src)
                            .map(|n| n.name.clone())
                            .unwrap_or_else(|| format!("#{}", e.src)),
                        dst_name: names
                            .get(&e.dst)
                            .map(|n| n.name.clone())
                            .unwrap_or_else(|| format!("#{}", e.dst)),
                        relation: e.relation.clone(),
                        fact_text: e.fact_text.clone(),
                        valid_at: e.valid_at.clone(),
                        invalid_at: e.invalid_at.clone(),
                        temporal: temporal_status(e, as_of),
                        source: EdgeSource {
                            episode: e.source_episode,
                            session_key: e.session_key.clone(),
                            recorded_at: e.created_at.clone(),
                        },
                    })
                    .collect();
                out.push(EvidencePath {
                    hops: p.edges.len() as u32,
                    score: p.score(),
                    nodes,
                    edges,
                    why: PathRationale {
                        seed_leg: p.leg,
                        factors: p.factors.clone(),
                    },
                });
            }
            Ok(out)
        })
        .await?;
    Ok(out)
}
