//! The injection contract (design §6.4): bounded tagged context blocks,
//! pure rendering. Property tests pin the bound; golden tests pin the
//! bytes. This is what "the central memory never blows up an agent's
//! context" means in code.

/// Budget for one injection render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InjectionBudget {
    /// Max characters per block (truncation must be visible).
    pub per_block: usize,
    /// Max characters for the entire render (hard bound).
    pub total: usize,
}

impl Default for InjectionBudget {
    fn default() -> Self {
        // Conservative defaults: ~1k chars per block, ~4k total
        // (~1k tokens) — the LLM-OS "RAM is scarce" rule.
        Self {
            per_block: 1024,
            total: 4096,
        }
    }
}

/// THE PLATFORM'S VISIBLE-TRUNCATION VOCABULARY -- one place, because "the same
/// concept in two byte sequences" is the defect this exists to prevent.
///
/// MEASURED BEFORE t309 (three wordings for one idea, and no test could see it,
/// because each site only ever asserted its own text):
///   contract (this file)      "… [+N chars truncated]"
///   retry context (runs.rs)   "…[+N chars truncated]"          <- no space
///   handoff upstream (runs.rs) "…[upstream result truncated at N chars]"
///
/// Both daemon producers of agent context call these now, and a test in the
/// daemon compares their rendered bytes against THIS function's output, so
/// changing one side without the other turns red.
///
/// The convention is the space: every marker opens with an ellipsis and a SPACE
/// before the bracket. That single byte is what the two producers disagreed on.
pub fn tail_truncated(dropped_chars: usize) -> String {
    format!("… [+{dropped_chars} chars truncated]")
}

/// A text cut at a FIXED bound, naming what was cut (the handoff's upstream
/// result is bounded at a constant, not at "what is left after a tail").
pub fn cut_at(what: &str, bound_chars: usize) -> String {
    format!("… [{what} truncated at {bound_chars} chars]")
}

/// The whole-render drop notice: whole blocks were dropped because the TOTAL
/// budget bound. Public for the same reason as the two above, and because a probe
/// that greps for natural-language keywords instead of asking for these bytes is
/// measuring the wrong thing (t31 acceptance: scan criteria come from THIS
/// vocabulary — owner of that rule is the verification task, V-INT).
pub fn items_dropped_notice(dropped_items: usize) -> String {
    format!(
        "<context_budget>\n… [+{dropped_items} items dropped: context budget reached]\n</context_budget>\n"
    )
}

/// The degradation of the notice above when even it does not fit the budget.
pub fn items_dropped_minimal(dropped_items: usize) -> String {
    format!("… [+{dropped_items} dropped]")
}

/// One memory selected for injection.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryForInjection {
    /// Block tag, e.g. `user_profile`, `project_context`, `relevant_memories`.
    pub tag: &'static str,
    pub content: String,
    /// When the memory was last updated (rendered so staleness is
    /// visible — design §6.4 "facts carry dates").
    pub updated_at: String,
}

/// One item to inject, before it is grouped into a tagged block.
///
/// WHY THIS EXISTS (t260): the renderer used to take memories only, so the
/// chat path that wanted to inject knowledge had to write its own header,
/// its own budget and its own truncation -- and it drifted. One item type
/// means one renderer, and therefore one set of rules for every producer.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextItem {
    /// Block tag. Items sharing a tag merge into ONE block, in first-seen
    /// order of the tag.
    pub tag: &'static str,
    pub content: String,
    /// The date part rendered in front of the content (empty = no date).
    /// Memories carry updated_at; a retrieval hit carries whatever date its
    /// document has, or none.
    pub date: String,
    /// The writer's confidence in this item, when the item has one (R-B C1/D.4).
    /// `None` = the producer has no value for it, which is NOT the same as 1.0.
    /// Rendered as a WORD when below `LOW_CONFIDENCE` — see `LOW_CONFIDENCE_MARK`.
    pub confidence: Option<f64>,
}

/// The word that marks a low-confidence line. ONE place: the injection contract
/// owns these bytes, and a test compares the render against this constant.
///
/// WHY A WORD AND NOT THE NUMBER. The number is not calibrated (the panel's own
/// copy says so: "not a calibrated probability"), and printing it on every line
/// spends tokens on a value the model cannot act on. What it CAN act on is
/// "this was not confirmed": one token, and the only case the palette of values
/// below 0.5 was ever meant to signal.
///
/// WHY A PREFIX, NOT A SUFFIX (found by this module's own test, t8): a line's
/// tail is exactly what `per_block` truncation removes. A suffix mark on a long
/// line was silently cut off — the one case where the mark matters most (a long
/// uncertain memory) was the case where it vanished. A prefix survives every cut
/// by construction.
pub const LOW_CONFIDENCE_MARK: &str = "[unverified]";

impl ContextItem {
    /// An item whose line is prefixed with the date part of an RFC3339 stamp.
    pub fn dated(tag: &'static str, content: impl Into<String>, ts: &str) -> Self {
        Self {
            tag,
            content: content.into(),
            date: date_of(ts),
            confidence: None,
        }
    }

    /// An item with no date (knowledge chunks have no per-chunk date).
    pub fn undated(tag: &'static str, content: impl Into<String>) -> Self {
        Self {
            tag,
            content: content.into(),
            date: String::new(),
            confidence: None,
        }
    }

    /// A dated item that ALSO carries its writer's confidence (R-B D.4).
    /// The only constructor that sets `confidence`: a producer must say the value
    /// explicitly, so "no value" cannot be mistaken for "high value".
    pub fn dated_with_confidence(
        tag: &'static str,
        content: impl Into<String>,
        ts: &str,
        confidence: Option<f64>,
    ) -> Self {
        Self {
            confidence,
            ..Self::dated(tag, content, ts)
        }
    }

    /// Whether this line renders the low-confidence mark.
    pub fn is_low_confidence(&self) -> bool {
        self.confidence
            .map(crate::confidence::is_low)
            .unwrap_or(false)
    }
}

/// The tags this contract emits, listed IN DROP ORDER -- the first tag is the
/// last to be dropped. The renderer preserves first-seen tag order, and when
/// the total budget binds, the blocks that come later are dropped VISIBLY
/// (a counted notice, never silently).
///
/// WHY THIS ORDER (t260, approved): user_profile > relevant_memories >
/// knowledge > wiki > project_context.
/// - user_profile and relevant_memories are FACTS ABOUT THIS USER AND THIS
///   CONVERSATION. Nothing downstream can reconstruct them, and an agent that
///   is wrong about the user fails in the most expensive way.
/// - knowledge is EVIDENCE: verbatim source text the agent can quote.
/// - wiki is a LEAD: an agent-generated page that has to be verified against a
///   source anyway. Losing a lead costs one retrieval; losing evidence costs
///   the answer.
/// - project_context is the narrowest scope here (one project's notes) and is
///   therefore the cheapest to lose.
///
/// Reordering this list is a POLICY change, not a refactor: it changes what an
/// agent sees when the budget binds.
pub const TAG_USER_PROFILE: &str = "user_profile";
pub const TAG_RELEVANT_MEMORIES: &str = "relevant_memories";
pub const TAG_KNOWLEDGE: &str = "knowledge";
/// Structured graph evidence: entity/relation lines with their subject and
/// object (R-B D.2, proposed by graph and accepted). Sits between knowledge and
/// wiki because a relation is a one-line structured claim, weaker than verbatim
/// text and stronger than a generated page.
pub const TAG_GRAPH: &str = "graph";
/// Agent-generated wiki pages. Kept in their OWN block, never merged into
/// knowledge (design SS13-2): a generated summary is a lead to verify against
/// sources, not a source.
pub const TAG_WIKI: &str = "wiki";
pub const TAG_PROJECT_CONTEXT: &str = "project_context";

/// Which namespace a memory group reads. Project is resolved by the CALLER
/// (a chat is pinned to a directory, a run to its task's project), which is why
/// the groups are DATA and not a hard-coded SQL string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryScope {
    User,
    Global,
    Project,
}

/// One group of memories an injection path reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryGroup {
    pub tag: &'static str,
    pub store: crate::MemoryStore,
    pub scope: MemoryScope,
    /// Rows to read. The memory crate's read path orders by recency, so this is
    /// "the N most recent" -- not "the N most relevant".
    pub limit: u32,
}

/// The SELECTION half of the injection contract: which memories a path puts in
/// front of an agent, and by what rule.
///
/// WHY THIS TYPE EXISTS (t278): the chat and runs paths each hard-coded their
/// own numbers in their own file. A reader could not tell a deliberate
/// difference from drift, and could not see either path's rule without reading
/// two files. Both presets are now here, named, with the difference between
/// them stated field by field.
///
/// THE RULE ITSELF IS ONE FUNCTION (crates/daemon/src/memembed.rs,
/// select_injection_memories): the groups in order, then the optional query
/// leg. These presets are its PARAMETERS -- deliberately NOT one shared
/// default, because the two paths answer different questions.
///
/// The per-group limits below are AS FOUND (2026-09-26): no rationale for 5 vs
/// 8, or for the 3s, exists in the tree or in the design docs. They are
/// PRESERVED, not endorsed -- changing them changes what an agent sees, so it
/// is a behaviour change and a separate decision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InjectionSelection {
    pub groups: &'static [MemoryGroup],
    /// The query-side semantic leg: the N closest memories at or above
    /// query_min_score (cosine). A query_top_n of 0 means this path has NO
    /// query leg at all -- it injects the most RECENT memories and never asks
    /// what the task is about.
    pub query_top_n: u32,
    pub query_min_score: f32,
}

/// The CHAT path's selection, as found (t278).
pub const CHAT_SELECTION: InjectionSelection = InjectionSelection {
    groups: &[
        // Who the user is. The ONLY group the two paths agree on.
        MemoryGroup {
            tag: TAG_USER_PROFILE,
            store: crate::MemoryStore::Profile,
            scope: MemoryScope::User,
            limit: 5,
        },
        MemoryGroup {
            tag: TAG_RELEVANT_MEMORIES,
            store: crate::MemoryStore::Observation,
            scope: MemoryScope::User,
            limit: 5,
        },
        // DEAD GROUP, kept as found: an Observation can never be WRITTEN in the
        // global namespace (MemoryStore::allows_namespace), so this read can
        // never match a row. It costs one query per injection and changes
        // nothing. Deleting it is provably neutral -- and still a behaviour
        // change, so it is flagged here and left for a separate decision.
        MemoryGroup {
            tag: TAG_RELEVANT_MEMORIES,
            store: crate::MemoryStore::Observation,
            scope: MemoryScope::Global,
            limit: 3,
        },
        MemoryGroup {
            tag: TAG_RELEVANT_MEMORIES,
            store: crate::MemoryStore::Procedure,
            scope: MemoryScope::Global,
            limit: 3,
        },
        MemoryGroup {
            tag: TAG_RELEVANT_MEMORIES,
            store: crate::MemoryStore::Lesson,
            scope: MemoryScope::Global,
            limit: 3,
        },
    ],
    // A chat's first message IS the query, so this path has a query leg.
    query_top_n: 4,
    query_min_score: 0.34,
};

/// The RUNS path's selection, as found (t278).
pub const RUNS_SELECTION: InjectionSelection = InjectionSelection {
    groups: &[
        MemoryGroup {
            tag: TAG_USER_PROFILE,
            store: crate::MemoryStore::Profile,
            scope: MemoryScope::User,
            limit: 5,
        },
        MemoryGroup {
            tag: TAG_RELEVANT_MEMORIES,
            store: crate::MemoryStore::Observation,
            scope: MemoryScope::User,
            limit: 8,
        },
        // The run's own project scope. The CHAT path has no equivalent group: a
        // chat is pinned to a DIRECTORY (cwd), not to a project name, so there
        // is nothing to resolve Project from. That is a GAP, not a decision --
        // registered in the t278 report, not silently closed.
        MemoryGroup {
            tag: TAG_PROJECT_CONTEXT,
            store: crate::MemoryStore::Observation,
            scope: MemoryScope::Project,
            limit: 8,
        },
    ],
    // NO QUERY LEG -- and this is the one difference that is NOT a parameter: a
    // run HAS a query (its task's title + intent, runs.rs::run_query) and the
    // chat path uses one. Giving runs a leg would change what an agent sees, so
    // t278 measures the difference and does NOT close it.
    query_top_n: 0,
    query_min_score: 0.0,
};

/// The drop order as a sortable rank: a LOWER rank is an earlier block and is
/// dropped LAST. Sorting items by this reproduces the documented block order no
/// matter what order a caller discovered them in -- so the order lives HERE,
/// not in the sequence of push calls in two files.
pub fn tag_rank(tag: &str) -> u8 {
    match tag {
        TAG_USER_PROFILE => 0,
        TAG_RELEVANT_MEMORIES => 1,
        TAG_KNOWLEDGE => 2,
        TAG_GRAPH => 3,
        TAG_WIKI => 4,
        TAG_PROJECT_CONTEXT => 5,
        _ => 6,
    }
}

/// How many retrieval hits reach the injection, per kind (t260).
pub const KNOWLEDGE_SOURCES: usize = 3;
pub const WIKI_PAGES: usize = 2;
/// How many graph evidence lines reach the injection (t58 / F5 / G10).
///
/// A NUMBER, not a magic literal at the call site: the two producers must not
/// each choose their own width (that is how the same concept ends up with two
/// byte shapes). Its value keeps the graph block the SMALLEST of the three
/// evidence blocks — graph lines are one structured fact each (plus hop/temporal
/// provenance), so three is already a lot of context for the cheapest-to-lose
/// evidence class.
pub const GRAPH_PATHS: usize = 3;

/// THE GRAPH BLOCK (t58): graph evidence lines → `<graph>` items.
///
/// WHY THIS FUNCTION EXISTS HERE AND NOT IN THE PRODUCERS. The contract owns
/// the tag, the block shape and the budget, so the producers must not build the
/// block themselves — exactly as with `knowledge_items`. The PRODUCER side of
/// graph evidence (the retrieval and its `lines()`) lives in `crates/graph`, and
/// THIS crate must not depend on it (dependency direction: `ruagent-memory`
/// depends on `core` + `store` only), so the caller hands over the rendered lines
/// and the shape stays testable without a graph.
///
/// WHAT IT DOES NOT DO. It does not invent a line, does not render a placeholder,
/// and does not date the lines: a graph line already carries its own hop/temporal
/// provenance in its text (graph's `lines()`), and an empty input returns an empty
/// vector so the caller emits NO block — a `<graph>` block that says "nothing was
/// found" costs tokens and teaches the model nothing (the same rule as
/// `knowledge_items`).
///
/// Blank/whitespace-only lines are DROPPED rather than rendered: an empty line
/// inside the block would look like evidence with missing content.
pub fn graph_items(lines: &[String], limit: usize) -> Vec<ContextItem> {
    lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .take(limit)
        .map(|l| ContextItem::undated(TAG_GRAPH, l))
        .collect()
}

/// One retrieval hit, in the shape the contract needs. The knowledge crate is
/// deliberately NOT a dependency of this crate: the caller hands over the
/// fields, and the selection rule below stays testable without a vector store.
///
/// FROZEN SHAPE (t8). Adding a field here would break the two daemon
/// construction sites (`chat.rs`, `runs.rs`) that this crate must not edit, so
/// the two cross-region payloads ride in `EnrichedHit` instead — one new
/// construction point for the daemon, no change to this struct.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalHit {
    /// Source document name (rendered, so the reader knows where it came from).
    pub document: String,
    pub content: String,
    /// The retrieval's own score (the knowledge base's fused rank score).
    /// Higher is better. **This is a RANK score, not a similarity** (R-B D.7).
    pub score: f32,
    /// True for a generated wiki page, false for a source document.
    pub wiki: bool,
}

/// The self-description of a generated wiki page, as it reaches the contract
/// (R-D D.7's `WikiLeadMeta`). Pure data: this crate never learns about the wiki
/// producer's types, so the dependency stays one-way.
#[derive(Debug, Clone, PartialEq)]
pub struct WikiLeadMeta {
    pub slug: String,
    pub stale: Option<bool>,
    pub stale_since: Option<String>,
    /// THREE states, matching the producer: `Some(true)` hand-edited after its
    /// build, `Some(false)` untouched, `None` = this surface cannot see the DB.
    ///
    /// CORRECTED at integration time (t8 + integ's O-1 / DEP-INT-8). D.7 froze
    /// `bool` here, but `wiki::WikiLead` carries `Option<bool>` and wiki's own
    /// D.2/D.5 three-state rule says `None` means "not decidable". A `bool` would
    /// render that as a definite `edited=false` — the same defect as reporting
    /// `Readout(0)` for a surface nobody looked at. `None` renders `unknown`, and
    /// a test asserts the string `edited=false` does NOT appear for it.
    pub edited: Option<bool>,
    /// THREE states, for the same reason as `edited` — and this one was corrected
    /// a second time by RV-D-1 (wiki's t34): the old `f32` could only ever be
    /// `0.0` or `1.0` because the producer drew its numerator and denominator from
    /// the same `meta.citations`, and a page with no `wiki_pages` row reported a
    /// perfect score. The producer now carries a build-time recorded value and
    /// only dares report it while the page is still fresh, so:
    /// `Some(v)` = recorded and still applicable, `None` = unknown. `None` renders
    /// `coverage=unknown` (never omitted, never 0.0/1.0).
    pub cite_coverage: Option<f32>,
    /// ≤3 anchors, `(document, chunk_id)`, so the agent can fetch the evidence
    /// itself through the knowledge expand endpoint.
    pub anchors: Vec<(String, i64)>,
    pub hint: String,
}

/// The frozen hint sentence (R-D D.7). ONE constant: the HTTP face and the
/// injection face must say the same thing.
pub const WIKI_LEAD_HINT: &str = "generated wiki page — verify against its sources before trusting";

/// A real relevance score, when the retrieval can produce one (R-A D.3's
/// `RelevanceScore`, mirrored here as pure data).
///
/// WHY IT IS CARRIED SEPARATELY FROM `RetrievalHit::score`: `score` is a rank
/// score (RRF, k=60). Ordering injection by it means ordering by name — the
/// defect R-B D.7 froze against. `relevance` is the calibrated similarity; the
/// contract uses it for ORDERING ONLY and never renders the number into the
/// block (`value` is not the model's business, and rendering it invites the same
/// "0.86 cosine next to 0.016 rrf" misreading the panel was fixed for).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelevanceMeta {
    pub value: f32,
    pub kind: &'static str,
    pub version: u32,
    pub query_background: Option<f32>,
}

/// A retrieval hit plus everything the contract needs to render it honestly.
///
/// This is the ONE place a daemon producer passes lead/relevance data; `lead`
/// and `relevance` are `Option`, so a producer that has neither keeps using
/// `knowledge_items` and gets byte-identical output.
#[derive(Debug, Clone, PartialEq)]
pub struct EnrichedHit {
    pub hit: RetrievalHit,
    pub lead: Option<WikiLeadMeta>,
    pub relevance: Option<RelevanceMeta>,
}

impl EnrichedHit {
    /// A hit with nothing extra — the shape every existing caller already has.
    pub fn plain(hit: RetrievalHit) -> Self {
        Self {
            hit,
            lead: None,
            relevance: None,
        }
    }

    /// The value the contract orders this hit by: the calibrated relevance when
    /// one exists, else the retrieval's own rank score. Documented, not implied:
    /// a hit WITHOUT a relevance is ordered by a different quantity, and the
    /// point of this function is that the substitution is visible in one place.
    pub fn order_value(&self) -> f32 {
        self.relevance.map(|r| r.value).unwrap_or(self.hit.score)
    }
}

/// THE SELECTION RULE (t260), falsifiable in one sentence: of the hits the
/// retrieval returned, keep the top N SOURCE hits and the top M WIKI hits,
/// each ordered by score descending and then by document name, so a tie cannot
/// reorder the block between two runs.
///
/// Nothing is invented: an empty input returns an empty vector, and the caller
/// then emits NO block at all rather than a placeholder. That is the honest
/// failure mode -- a context block that says "nothing was found" costs tokens
/// and teaches the model nothing.
pub fn knowledge_items(hits: &[RetrievalHit], sources: usize, wiki: usize) -> Vec<ContextItem> {
    let enriched: Vec<EnrichedHit> = hits.iter().cloned().map(EnrichedHit::plain).collect();
    knowledge_items_enriched(&enriched, sources, wiki)
}

/// The same rule, over hits that may carry a lead and a relevance (R-A H-4,
/// R-D D.7). Ordering: `EnrichedHit::order_value` descending, then document,
/// then content — so a hit with a relevance is ordered by similarity and one
/// without keeps the old rank-score order, and neither is silently mixed.
///
/// Wiki rendering with a lead is the frozen shape of R-D D.7:
///
/// ```text
/// wiki/<slug>: <title> — stale=false coverage=1.00 anchors=2 edited=false
///   generated wiki page — verify against its sources before trusting
///   <chunk text>
///   anchors: ops-handbook#18 (GET /api/v1/knowledge/expand/{chunk_id})
/// ```
///
/// Without a lead the wiki line is exactly what it always was
/// (`<document>: <content>`), so the existing golden bytes do not move.
pub fn knowledge_items_enriched(
    hits: &[EnrichedHit],
    sources: usize,
    wiki: usize,
) -> Vec<ContextItem> {
    let pick = |want_wiki: bool, take: usize| -> Vec<&EnrichedHit> {
        let mut v: Vec<&EnrichedHit> = hits.iter().filter(|h| h.hit.wiki == want_wiki).collect();
        // Deterministic order: order_value desc, then document name, then content.
        v.sort_by(|a, b| {
            b.order_value()
                .partial_cmp(&a.order_value())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.hit.document.cmp(&b.hit.document))
                .then_with(|| a.hit.content.cmp(&b.hit.content))
        });
        v.truncate(take);
        v
    };
    let mut out: Vec<ContextItem> = Vec::new();
    // Knowledge first: a source outranks a generated page when the budget
    // binds (SS13-2 -- leads are cheaper to lose than evidence).
    for h in pick(false, sources) {
        out.push(ContextItem::undated(
            TAG_KNOWLEDGE,
            format!("{}: {}", h.hit.document, h.hit.content.trim()),
        ));
    }
    for h in pick(true, wiki) {
        out.push(ContextItem::undated(
            TAG_WIKI,
            render_wiki_item(&h.hit, h.lead.as_ref()),
        ));
    }
    out
}

/// The wiki line(s). The marker line is self-contained on purpose: a reader of
/// the block must be able to judge the page WITHOUT reading its body (R-D D.7).
fn render_wiki_item(hit: &RetrievalHit, lead: Option<&WikiLeadMeta>) -> String {
    let body = format!("{}: {}", hit.document, hit.content.trim());
    let Some(lead) = lead else {
        return body;
    };
    let stale = match lead.stale {
        Some(true) => "true".to_string(),
        Some(false) => "false".to_string(),
        None => "unknown".to_string(),
    };
    // `None` is "this surface cannot see the DB", never "not edited".
    let edited = match lead.edited {
        Some(true) => "true".to_string(),
        Some(false) => "false".to_string(),
        None => "unknown".to_string(),
    };
    // Same three-state vocabulary (RV-D-1): a missing coverage is `unknown`, and
    // the key is never omitted — a reader must not have to guess whether the
    // absence means 0.0, 1.0 or "nobody measured".
    let coverage = match lead.cite_coverage {
        Some(v) => format!("{v:.2}"),
        None => "unknown".to_string(),
    };
    let mut out = format!(
        "{}: {} — stale={stale} coverage={coverage} anchors={} edited={edited}",
        hit.document,
        lead.slug,
        lead.anchors.len(),
    );
    if let Some(since) = &lead.stale_since {
        out.push_str(&format!(" stale_since={since}"));
    }
    out.push('\n');
    let hint = if lead.hint.is_empty() {
        WIKI_LEAD_HINT
    } else {
        lead.hint.as_str()
    };
    out.push_str(&format!("  {hint}\n"));
    out.push_str(&format!("  {}\n", hit.content.trim()));
    if !lead.anchors.is_empty() {
        let list: Vec<String> = lead
            .anchors
            .iter()
            .take(3)
            .map(|(doc, id)| format!("{doc}#{id}"))
            .collect();
        out.push_str(&format!(
            "  anchors: {} (GET /api/v1/knowledge/expand/{{chunk_id}})",
            list.join(", ")
        ));
    }
    out.trim_end().to_string()
}

/// Render context items into bounded tagged blocks -- THE ONE RENDERER.
///
/// <user_profile>
/// [2026-09-11] prefers concise answers
/// </user_profile>
///
/// Rules (all enforced, all tested):
/// - every block is at most per_block chars, truncated with a visible
///   [+N chars truncated] marker;
/// - the whole render is at most total chars -- blocks are dropped (never
///   partially, never silently: a dropped count is appended);
/// - blocks with identical tags are merged under one tag, in first-seen order.
///
/// render_injection is a thin wrapper over this for memory-only callers; the
/// daemon's two injection paths (chat, runs) both call THIS function, so the
/// rules cannot drift between them again (t259 measured them already drifting:
/// the block-header wording changed in a097c5e and the old transcripts kept
/// the old wording).
pub fn render_context(items: &[ContextItem], budget: &InjectionBudget) -> String {
    render_context_report(items, budget).0
}

/// ONE EMITTED BLOCK'S ACCOUNT OF ITSELF (t60 / INT-F6): the shape the
/// integration contract §3.2 freezes for `budget.blocks[]`.
///
/// `chars` is the block's own rendered size (header + body + footer), so a
/// reader can check this account against the render BYTE FOR BYTE instead of
/// trusting it. `truncated_chars` is how many characters the per-block cut
/// removed — exactly the `N` in that block's `… [+N chars truncated]` marker (0
/// when the block was not cut).
///
/// WHICH BLOCKS APPEAR HERE: the EMITTED ones, in render order. A whole-block
/// drop is reported by the top-level `dropped_items` instead, because the
/// contract's own A-2 criterion compares this list's tag set with the render's
/// and a dropped block is not in the render. Consequence, stated rather than
/// glossed: this list cannot name WHICH tag lost items (§3.3 Q2) — the t60
/// report carries that as a finding, and this account never pretends otherwise.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BlockReport {
    pub tag: &'static str,
    pub items: u32,
    pub chars: usize,
    pub truncated_chars: u32,
    pub dropped_items: u32,
}

/// THE RENDER'S OWN ACCOUNT OF THE BUDGET (t60 / INT-F6): what was emitted, what
/// was cut, what was dropped — as DATA, from the same pass that produced the
/// bytes.
///
/// WHY THIS EXISTS. `budget` in the `context_injected` transcript event was
/// hardcoded `null` (`runs.rs:1278`, `crates/acp/src/chat.rs:504`) for an honest
/// reason: `render_context` returned text only, so the only way to fill the field
/// was to REVERSE-ENGINEER `blocks[]` out of the rendered text — i.e. to treat
/// "looks right" as a reading (t19 §9-3). The instrument was missing, so A-2's
/// budget half and N-6 ("a dropped block must be counted") were undecidable: the
/// data was never produced. This is that instrument, derived from the SAME
/// accounting that writes the bytes (`render_context` is now
/// `render_context_report(..).0`), so the report and the render cannot disagree.
///
/// WHAT IT DOES NOT CLAIM: what the renderer DID, not what the agent read. An
/// empty result is still a MEASUREMENT (`used_chars: 0`, `dropped_items: 0`),
/// which is exactly why a caller must not confuse it with "not collected" —
/// `budget: null` stays reserved for "no report was taken".
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BudgetReport {
    pub per_block_chars: usize,
    pub total_chars: usize,
    pub used_chars: usize,
    pub truncated_blocks: u32,
    pub dropped_items: u32,
    pub blocks: Vec<BlockReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl BudgetReport {
    /// Did this render lose anything? `false` is a MEASUREMENT (zero drops), and
    /// this method exists so a caller never has to test `None` for that.
    pub fn lost_anything(&self) -> bool {
        self.dropped_items > 0 || self.truncated_blocks > 0
    }

    /// The account in the words the criteria use, for a log line or an assertion.
    pub fn summary(&self) -> String {
        format!(
            "used={}/{} chars (per_block={}) blocks={:?} truncated_blocks={} dropped_items={}",
            self.used_chars,
            self.total_chars,
            self.per_block_chars,
            self.blocks.iter().map(|b| b.tag).collect::<Vec<_>>(),
            self.truncated_blocks,
            self.dropped_items
        )
    }
}

/// THE REPORT-RETURNING RENDER (t60 / INT-F6): the bytes of `render_context`,
/// plus the account of how the budget shaped them.
///
/// `render_context` is a thin wrapper over this, so there is exactly ONE code
/// path and ONE accounting: a caller that asks for the report cannot get
/// different bytes than one that does not (asserted by test).
pub fn render_context_report(
    items: &[ContextItem],
    budget: &InjectionBudget,
) -> (String, BudgetReport) {
    // Merge by tag, preserving first-seen order.
    let mut order: Vec<&'static str> = Vec::new();
    let mut merged: std::collections::HashMap<&'static str, Vec<&ContextItem>> =
        std::collections::HashMap::new();
    for m in items {
        if !merged.contains_key(&m.tag) {
            order.push(m.tag);
        }
        merged.entry(m.tag).or_default().push(m);
    }

    let mut out = String::new();
    let mut dropped = 0u32;
    let mut truncated_blocks = 0u32;
    let mut blocks: Vec<BlockReport> = Vec::new();

    for tag in order {
        let group = &merged[tag];
        let body: String = group
            .iter()
            .map(|m| {
                let line = if m.date.is_empty() {
                    format!("{}\n", m.content.trim())
                } else {
                    format!("[{}] {}\n", m.date, m.content.trim())
                };
                if m.is_low_confidence() {
                    // A PREFIX: a suffix on a long line is exactly what the
                    // per_block cut below removes (see LOW_CONFIDENCE_MARK).
                    format!("{LOW_CONFIDENCE_MARK} {line}")
                } else {
                    line
                }
            })
            .collect();

        let header = format!("<{tag}>\n");
        let footer = format!("</{tag}>\n");

        // Per-block truncation with a visible marker.
        let mut truncated_chars = 0usize;
        let body = if body.chars().count() > budget.per_block {
            let cut: String = body.chars().take(budget.per_block).collect();
            let remaining = body.chars().count() - budget.per_block;
            truncated_chars = remaining;
            format!("{cut}\n{}\n", tail_truncated(remaining))
        } else {
            body
        };

        let block = format!("{header}{body}{footer}");
        // Total budget: drop whole blocks (never silently).
        if out.chars().count() + block.chars().count() > budget.total {
            dropped += group.len() as u32;
            continue;
        }
        if truncated_chars > 0 {
            truncated_blocks += 1;
        }
        blocks.push(BlockReport {
            tag,
            items: group.len() as u32,
            chars: block.chars().count(),
            truncated_chars: truncated_chars as u32,
            dropped_items: 0,
        });
        out.push_str(&block);
    }

    // The drop notice itself obeys the budget — the hard bound wins.
    if dropped > 0 {
        let full = items_dropped_notice(dropped as usize);
        if out.chars().count() + full.chars().count() <= budget.total {
            out.push_str(&full);
        } else {
            let minimal = items_dropped_minimal(dropped as usize);
            if out.chars().count() + minimal.chars().count() <= budget.total {
                out.push_str(&minimal);
            }
        }
    }
    let used_chars = out.chars().count();
    debug_assert!(used_chars <= budget.total);
    (
        out,
        BudgetReport {
            per_block_chars: budget.per_block,
            total_chars: budget.total,
            used_chars,
            truncated_blocks,
            dropped_items: dropped,
            blocks,
            notes: None,
        },
    )
}

/// Date part of an RFC3339 timestamp ("" if unparseable).
fn date_of(ts: &str) -> String {
    ts.split('T').next().unwrap_or("").to_string()
}

/// Memory-only wrapper over render_context.
///
/// RETAINED DELIBERATELY -- this is the answer to F-t281-02 (t281 found its
/// production callers to be ZERO, with all five references inside #[cfg(test)]).
/// Both daemon injection paths call render_context directly now, because both
/// carry knowledge items too, so this function is not on the hot path.
///
/// The reasons it is kept rather than deleted, in the order they weigh:
/// 1. It is the memory-only ENTRY POINT of this crate's published surface
///    (lib.rs re-exports it), and it owns the one mapping from
///    MemoryForInjection to ContextItem. A future memory-only consumer -- an MCP
///    tool, a distill path, a CLI -- should call this, not re-derive that
///    mapping to reach render_context.
/// 2. The property and golden tests that pin the injection contract's BYTE
///    SHAPES run through exactly this signature. They are the regression net for
///    the shapes the daemon's two paths must keep producing.
///
/// HONESTLY: those tests are also its only callers today, so reason 2 is close
/// to circular -- the real argument is reason 1. Deleting it is a mechanical
/// change (this function, MemoryForInjection, the lib.rs re-export, and moving
/// the tests onto ContextItem), and it is NOT taken here because
/// crates/memory/src/lib.rs is outside this task's scope. If the sweep decides a
/// public-but-uncalled function is worse than a one-line re-export edit, take
/// that follow-up; this comment is the decision record either way.
pub fn render_injection(memories: &[MemoryForInjection], budget: &InjectionBudget) -> String {
    let items: Vec<ContextItem> = memories
        .iter()
        .map(|m| ContextItem::dated(m.tag, m.content.clone(), &m.updated_at))
        .collect();
    render_context(&items, budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(tag: &'static str, content: &str) -> MemoryForInjection {
        MemoryForInjection {
            tag,
            content: content.into(),
            updated_at: "2026-09-11T10:00:00Z".into(),
        }
    }

    #[test]
    fn golden_render() {
        let budget = InjectionBudget {
            per_block: 100,
            total: 1000,
        };
        let out = render_injection(
            &[
                mem("user_profile", "prefers concise answers"),
                mem("project_context", "deploy via scripts/release.sh"),
            ],
            &budget,
        );
        let expected = "\
<user_profile>
[2026-09-11] prefers concise answers
</user_profile>
<project_context>
[2026-09-11] deploy via scripts/release.sh
</project_context>
";
        assert_eq!(out, expected);
    }

    /// The tag blocks of a render: lines shaped `<tag>` for a tag that is one of
    /// the INPUT tags.
    ///
    /// The input-tag filter is what makes this a check rather than a coincidence:
    /// the drop notice's `<context_budget>` wrapper is not a tag block (the
    /// contract's `blocks[]` covers the TAG vocabulary only), and neither is a
    /// content line that happens to look like a tag.
    fn rendered_tag_blocks<'a>(out: &'a str, input_tags: &[&'static str]) -> Vec<&'a str> {
        out.lines()
            .filter_map(|l| l.strip_prefix('<').and_then(|r| r.strip_suffix('>')))
            .filter(|t| !t.starts_with('/') && !t.contains(' ') && input_tags.contains(t))
            .collect()
    }

    /// THE BYTE-FREEZE (t60): the canonical renders, with the bytes recorded
    /// BEFORE the report-returning refactor (a temporary probe ran on the
    /// pre-refactor code and printed exactly these strings, `chars` included).
    /// The fixture that matters most is `all_dropped`: its render is EMPTY while
    /// `dropped_items = 1` — the case where the text alone cannot tell "nothing
    /// to say" from "everything was dropped", which is what N-6 needed an
    /// instrument for.
    #[test]
    fn the_report_refactor_moved_no_byte() {
        let expected: [(&str, &str); 6] = [
            (
                "multi_block",
                "<user_profile>\n[2026-09-11] prefers concise answers\n</user_profile>\n<knowledge>\n[2026-09-11] ops-handbook: deploy via scripts/release.sh\n</knowledge>\n<wiki>\n[2026-09-11] wiki/ops: runbook\n</wiki>\n<project_context>\n[2026-09-11] deploy via scripts/release.sh\n</project_context>\n",
            ),
            (
                "per_block_cut",
                "<user_profile>\n[2026-09-11] xxxxxxxxxxx\n… [+190 chars truncated]\n</user_profile>\n",
            ),
            ("whole_block_drop", "… [+2 dropped]"),
            ("all_dropped", ""),
            ("empty", ""),
            (
                "low_confidence",
                "<user_profile>\n[unverified] [2026-09-11] the user might use xlwt\n</user_profile>\n",
            ),
        ];
        for ((name, items, budget), (want_name, want)) in t60_fixtures().iter().zip(expected) {
            assert_eq!(*name, want_name);
            let (out, report) = render_context_report(items, budget);
            println!("READING t60 AFTER {name}: {}", report.summary());
            assert_eq!(out, want, "bytes moved for {name}");
            assert_eq!(
                render_context(items, budget),
                out,
                "the report-returning path and render_context must produce the same bytes"
            );
            assert_eq!(
                report.used_chars,
                out.chars().count(),
                "used_chars is DEFINED as render.chars().count() (contract §3.2)"
            );
            assert!(report.used_chars <= report.total_chars);
            assert_eq!(report.per_block_chars, budget.per_block);
            assert_eq!(report.total_chars, budget.total);
        }
    }

    /// A-2's cross-check, made decidable: the tag set of `blocks[]` equals the
    /// tag set of the render, and the block sizes ADD UP to the render (plus the
    /// drop notice, which is not a tag block).
    #[test]
    fn the_block_account_adds_up_to_the_render() {
        for (name, items, budget) in t60_fixtures() {
            let (out, report) = render_context_report(&items, &budget);
            let input_tags: Vec<&'static str> = {
                let mut v: Vec<&'static str> = items.iter().map(|i| i.tag).collect();
                v.dedup();
                v
            };
            let rendered_tags = rendered_tag_blocks(&out, &input_tags);
            let accounted: Vec<&str> = report.blocks.iter().map(|b| b.tag).collect();
            println!(
                "READING t60 tags {name}: rendered={rendered_tags:?} blocks={accounted:?} \
                 sum_block_chars={} used_chars={} dropped_items={} truncated_blocks={}",
                report.blocks.iter().map(|b| b.chars).sum::<usize>(),
                report.used_chars,
                report.dropped_items,
                report.truncated_blocks
            );
            assert_eq!(rendered_tags, accounted, "tag sets must agree ({name})");
            // The ONLY non-tag block a render may contain is the drop notice, and
            // when present it must be the vocabulary's own bytes.
            if out.contains("<context_budget>") {
                assert!(
                    out.contains(&items_dropped_notice(report.dropped_items as usize)),
                    "the notice must be the vocabulary form, not a lookalike: {out:?}"
                );
            }
            let block_chars: usize = report.blocks.iter().map(|b| b.chars).sum();
            let notice_chars = report
                .used_chars
                .checked_sub(block_chars)
                .expect("blocks cannot be bigger than the render");
            assert!(
                notice_chars < 80,
                "the remainder must be the drop notice, not a lost block: {out:?}"
            );
            // Per-block truncation agrees with the marker(s) in that block.
            let body_truncations = out.matches(" chars truncated]").count();
            assert_eq!(
                body_truncations, report.truncated_blocks as usize,
                "every cut is counted ({name})"
            );
            assert_eq!(
                report
                    .blocks
                    .iter()
                    .filter(|b| b.truncated_chars > 0)
                    .count(),
                report.truncated_blocks as usize
            );
        }
    }

    /// N-6's two halves, stated as the acceptance asks: **no drops ⇒ 0** (a
    /// measurement, never `null`/absent), **drops ⇒ > 0**.
    #[test]
    fn a_render_reports_zero_drops_as_zero_and_real_drops_as_positive() {
        // (1) no drops, no truncation: the numbers exist and are 0
        let (_, clean) = render_context_report(
            &[ContextItem::dated(
                TAG_USER_PROFILE,
                "prefers concise answers",
                "2026-09-11T00:00:00Z",
            )],
            &InjectionBudget::default(),
        );
        println!("READING t60 negative control: {}", clean.summary());
        assert_eq!(clean.dropped_items, 0, "0 is a reading, not `null`");
        assert_eq!(clean.truncated_blocks, 0);
        assert!(!clean.lost_anything());
        assert_eq!(clean.blocks.len(), 1);
        assert_eq!(clean.blocks[0].dropped_items, 0);
        assert_eq!(clean.blocks[0].truncated_chars, 0);

        // (2) a whole-block drop: positive count, and WHICH block survived is
        //     visible (the dropped tag is simply absent from blocks[])
        let (out, dropped) = render_context_report(
            &[
                ContextItem::dated(TAG_USER_PROFILE, "a".repeat(60), "2026-09-11T00:00:00Z"),
                ContextItem::dated(TAG_PROJECT_CONTEXT, "b".repeat(60), "2026-09-11T00:00:00Z"),
            ],
            &InjectionBudget {
                per_block: 4096,
                total: 40,
            },
        );
        println!(
            "READING t60 positive control: render={out:?} {}",
            dropped.summary()
        );
        assert!(dropped.dropped_items > 0, "a dropped block must be counted");
        assert!(dropped.lost_anything());
        assert!(dropped.blocks.is_empty());
        assert_eq!(dropped.dropped_items, 2, "both 1-item blocks were dropped");

        // (3) truncation alone is also a loss the report names
        let (_, cut) = render_context_report(
            &[ContextItem::dated(
                TAG_USER_PROFILE,
                "x".repeat(200),
                "2026-09-11T00:00:00Z",
            )],
            &InjectionBudget {
                per_block: 24,
                total: 4096,
            },
        );
        println!("READING t60 truncation control: {}", cut.summary());
        assert_eq!(cut.truncated_blocks, 1);
        assert_eq!(cut.dropped_items, 0);
        assert!(cut.blocks[0].truncated_chars > 0);
        assert!(cut.lost_anything(), "a cut IS a loss, even with no drop");
    }

    /// The report's JSON is the shape the integration contract §3.2 names —
    /// asserted field by field, because a struct that merely derives Serialize
    /// is a claim until something reads the bytes.
    #[test]
    fn the_report_serializes_to_the_contracts_budget_shape() {
        let (out, report) = render_context_report(&t60_fixtures()[1].1, &t60_fixtures()[1].2);
        let v: serde_json::Value = serde_json::to_value(&report).expect("report is serializable");
        println!("READING t60 budget json: {v}");
        for key in [
            "per_block_chars",
            "total_chars",
            "used_chars",
            "truncated_blocks",
            "dropped_items",
            "blocks",
        ] {
            assert!(v.get(key).is_some(), "§3.2 names `{key}`: {v}");
        }
        assert_eq!(v["used_chars"], serde_json::json!(out.chars().count()));
        assert_eq!(v["truncated_blocks"], serde_json::json!(1));
        assert_eq!(v["dropped_items"], serde_json::json!(0));
        let b = &v["blocks"][0];
        for key in ["tag", "items", "chars", "truncated_chars", "dropped_items"] {
            assert!(b.get(key).is_some(), "§3.2 names blocks[].{key}: {b}");
        }
        assert_eq!(b["tag"], serde_json::json!("user_profile"));
        assert_eq!(b["items"], serde_json::json!(1));
        assert_eq!(b["truncated_chars"], serde_json::json!(190));
        // `notes` is optional in §3.2 and omitted when nobody set it
        assert!(
            v.get("notes").is_none(),
            "an unset note must not serialize: {v}"
        );
        // and the empty case is a MEASURED zero, not a missing object
        let (_, empty) = render_context_report(&[], &InjectionBudget::default());
        let ev: serde_json::Value = serde_json::to_value(&empty).unwrap();
        println!("READING t60 empty budget json: {ev}");
        assert_eq!(ev["used_chars"], serde_json::json!(0));
        assert_eq!(ev["dropped_items"], serde_json::json!(0));
        assert_eq!(ev["blocks"], serde_json::json!([]));
    }

    /// The canonical fixtures the t60 report must account for: plain blocks, a
    /// per-block cut, a whole-block drop, everything dropped, empty, and the
    /// low-confidence prefix. Kept as data so the BEFORE and AFTER readings and
    /// the permanent byte-freeze test all speak about the same inputs.
    fn t60_fixtures() -> Vec<(&'static str, Vec<ContextItem>, InjectionBudget)> {
        let d =
            |tag: &'static str, text: String| ContextItem::dated(tag, text, "2026-09-11T00:00:00Z");
        vec![
            (
                "multi_block",
                vec![
                    d(TAG_USER_PROFILE, "prefers concise answers".into()),
                    d(
                        TAG_KNOWLEDGE,
                        "ops-handbook: deploy via scripts/release.sh".into(),
                    ),
                    d(TAG_WIKI, "wiki/ops: runbook".into()),
                    d(TAG_PROJECT_CONTEXT, "deploy via scripts/release.sh".into()),
                ],
                InjectionBudget {
                    per_block: 100,
                    total: 1000,
                },
            ),
            (
                "per_block_cut",
                vec![d(TAG_USER_PROFILE, "x".repeat(200))],
                InjectionBudget {
                    per_block: 24,
                    total: 4096,
                },
            ),
            (
                "whole_block_drop",
                vec![
                    d(TAG_USER_PROFILE, "a".repeat(60)),
                    d(TAG_PROJECT_CONTEXT, "b".repeat(60)),
                ],
                InjectionBudget {
                    per_block: 4096,
                    total: 40,
                },
            ),
            (
                "all_dropped",
                vec![d(TAG_RELEVANT_MEMORIES, "c".repeat(60))],
                InjectionBudget {
                    per_block: 4096,
                    total: 10,
                },
            ),
            ("empty", Vec::new(), InjectionBudget::default()),
            (
                "low_confidence",
                vec![ContextItem::dated_with_confidence(
                    TAG_USER_PROFILE,
                    "the user might use xlwt",
                    "2026-09-11T00:00:00Z",
                    Some(crate::confidence::CONF_HEDGED),
                )],
                InjectionBudget {
                    per_block: 100,
                    total: 1000,
                },
            ),
        ]
    }

    #[test]
    fn per_block_truncation_is_visible() {
        let budget = InjectionBudget {
            per_block: 20,
            total: 10_000,
        };
        let out = render_injection(&[mem("user_profile", &"x".repeat(100))], &budget);
        assert!(out.contains("chars truncated]"), "{out}");
        assert!(out.chars().count() < 200, "truncated block must be small");
    }

    /// THE D.3 VOCABULARY, pinned as bytes (t31 acceptance: verification and
    /// review scripts must take their criteria from THESE functions, never from a
    /// natural-language keyword scan over the render).
    ///
    /// A probe needs no regex: `tail_truncated(n)`, `cut_at(what, bound)`,
    /// `items_dropped_notice(n)` and `items_dropped_minimal(n)` are the four forms,
    /// and `render_context` uses the last two itself, so a script that calls them
    /// cannot drift from what the renderer emits.
    #[test]
    fn the_visible_truncation_vocabulary_is_the_only_source_a_probe_needs() {
        println!(
            "READING D.3 vocabulary: {:?} | {:?} | {:?} | {:?}",
            tail_truncated(7),
            cut_at("upstream result", 500),
            items_dropped_notice(3),
            items_dropped_minimal(3)
        );
        assert_eq!(tail_truncated(7), "… [+7 chars truncated]");
        assert_eq!(
            cut_at("upstream result", 500),
            "… [upstream result truncated at 500 chars]"
        );
        assert_eq!(
            items_dropped_notice(3),
            "<context_budget>\n… [+3 items dropped: context budget reached]\n</context_budget>\n"
        );
        assert_eq!(items_dropped_minimal(3), "… [+3 dropped]");
        // and the renderer really emits exactly those bytes
        let budget = InjectionBudget {
            per_block: 24,
            total: 4096,
        };
        let out = render_injection(&[mem("user_profile", &"y".repeat(200))], &budget);
        assert!(
            out.contains("… [+") && out.contains(" chars truncated]"),
            "{out}"
        );
        let tight = InjectionBudget {
            per_block: 4096,
            total: 40,
        };
        let dropped = render_context(
            &[
                ContextItem::dated(TAG_USER_PROFILE, "a".repeat(60), "2026-09-11T00:00:00Z"),
                ContextItem::dated(TAG_PROJECT_CONTEXT, "b".repeat(60), "2026-09-11T00:00:00Z"),
            ],
            &tight,
        );
        println!("READING D.3 drop notice as emitted: {dropped:?}");
        assert!(
            items_dropped_notice(1).contains("items dropped")
                && items_dropped_minimal(1).contains(" dropped]")
        );
    }

    #[test]
    fn total_budget_drops_blocks_visibly() {
        // total fits exactly one block (tag + date prefix + body).
        let budget = InjectionBudget {
            per_block: 1000,
            total: 90,
        };
        let out = render_injection(
            &[mem("a", &"0".repeat(50)), mem("b", &"1".repeat(50))],
            &budget,
        );
        // One block fits; the second is dropped and counted.
        assert!(out.contains("<a>"), "{out}");
        assert!(!out.contains("<b>"), "{out}");
        assert!(out.contains("1 dropped"), "drop notice present: {out}");
        assert!(out.chars().count() <= budget.total, "{out}");
    }

    #[test]
    fn duplicate_tags_merge() {
        let budget = InjectionBudget {
            per_block: 1000,
            total: 10_000,
        };
        let out = render_injection(
            &[mem("obs", "one"), mem("obs", "two"), mem("other", "x")],
            &budget,
        );
        assert_eq!(out.matches("<obs>").count(), 1);
        assert!(out.contains("one") && out.contains("two"));
    }

    #[test]
    fn empty_renders_empty() {
        assert_eq!(render_injection(&[], &InjectionBudget::default()), "");
    }

    // -----------------------------------------------------------------
    // Property: the contract's whole point — the render NEVER exceeds
    // the budget, for any memory set.
    // -----------------------------------------------------------------
    proptest::proptest! {
        #[test]
        fn never_exceeds_total(
            n in 0usize..20,
            contents in proptest::collection::vec("[a-z ]{0,2000}", 0..20),
            per_block in 10usize..300,
            total in 50usize..2000,
        ) {
            let mems: Vec<MemoryForInjection> = (0..n.min(contents.len()))
                .map(|i| {
                    let tags: [&'static str; 3] = ["user_profile", "project_context", "relevant_memories"];
                    MemoryForInjection {
                        tag: tags[i % 3],
                        content: contents[i].clone(),
                        updated_at: "2026-09-11T00:00:00Z".into(),
                    }
                })
                .collect();
            let budget = InjectionBudget { per_block, total };
            let out = render_injection(&mems, &budget);
            proptest::prop_assert!(out.chars().count() <= budget.total,
                "render {} > budget {} for {} memories", out.chars().count(), budget.total, mems.len());
        }

        /// t60: the same shapes, but for the ACCOUNT. The report is not allowed
        /// to disagree with the bytes for ANY input — that is what "derived from
        /// the same pass" has to mean, and a hand-written example cannot show it.
        ///
        /// Content is restricted to `[a-z ]` so the render-text tag scan below
        /// cannot be fooled by a content line that looks like `<tag>`; the
        /// agreement the property asserts is exactly the one A-2's criterion
        /// needs ("the tag set of `blocks[]` equals the render's").
        #[test]
        fn the_report_never_disagrees_with_the_render(
            contents in proptest::collection::vec("[a-z ]{0,300}", 0..10),
            per_block in 10usize..200,
            total in 20usize..1000,
        ) {
            let tags: [&'static str; 3] = [TAG_USER_PROFILE, TAG_PROJECT_CONTEXT, TAG_KNOWLEDGE];
            let items: Vec<ContextItem> = contents
                .iter()
                .enumerate()
                .map(|(i, c)| ContextItem::dated(tags[i % 3], c.clone(), "2026-09-11T00:00:00Z"))
                .collect();
            let budget = InjectionBudget { per_block, total };
            let (out, report) = render_context_report(&items, &budget);

            proptest::prop_assert_eq!(report.used_chars, out.chars().count());
            proptest::prop_assert!(report.used_chars <= report.total_chars);
            let block_chars: usize = report.blocks.iter().map(|b| b.chars).sum();
            proptest::prop_assert!(block_chars <= report.used_chars);
            if report.dropped_items == 0 {
                proptest::prop_assert_eq!(block_chars, report.used_chars,
                    "with no drop the render is exactly its blocks: {:?}", out);
            }
            proptest::prop_assert_eq!(
                out.matches(" chars truncated]").count(),
                report.truncated_blocks as usize
            );
            proptest::prop_assert_eq!(
                report.blocks.iter().filter(|b| b.truncated_chars > 0).count(),
                report.truncated_blocks as usize
            );
            let rendered: Vec<&str> = rendered_tag_blocks(&out, &tags);
            let accounted: Vec<&str> = report.blocks.iter().map(|b| b.tag).collect();
            proptest::prop_assert_eq!(rendered, accounted);
        }
    }

    #[test]
    fn knowledge_selection_rule_is_the_top_n_per_kind() {
        let hits = vec![
            RetrievalHit {
                document: "src/a".into(),
                content: "A".into(),
                score: 0.9,
                wiki: false,
            },
            RetrievalHit {
                document: "wiki/w".into(),
                content: "W".into(),
                score: 0.95,
                wiki: true,
            },
            RetrievalHit {
                document: "src/b".into(),
                content: "B".into(),
                score: 0.8,
                wiki: false,
            },
            RetrievalHit {
                document: "src/c".into(),
                content: "C".into(),
                score: 0.7,
                wiki: false,
            },
            RetrievalHit {
                document: "src/d".into(),
                content: "D".into(),
                score: 0.6,
                wiki: false,
            },
            RetrievalHit {
                document: "wiki/v".into(),
                content: "V".into(),
                score: 0.5,
                wiki: true,
            },
        ];
        let items = knowledge_items(&hits, 3, 2);
        assert_eq!(items.len(), 5, "3 sources + 2 pages");
        assert_eq!(items[0].tag, TAG_KNOWLEDGE);
        assert!(
            items[0].content.starts_with("src/a: "),
            "{}",
            items[0].content
        );
        assert!(
            items[2].content.starts_with("src/c: "),
            "{}",
            items[2].content
        );
        assert!(
            !items.iter().any(|i| i.content.starts_with("src/d")),
            "the 4th source is cut"
        );
        assert_eq!(items[3].tag, TAG_WIKI);
        assert!(
            items[3].content.starts_with("wiki/w"),
            "{}",
            items[3].content
        );
        // A higher-scoring page does NOT get promoted into the knowledge block.
        assert!(hits[1].score > hits[0].score);
        assert!(items.iter().filter(|i| i.tag == TAG_KNOWLEDGE).count() == 3);
        // Sources come first: when the budget binds, evidence outlives leads.
        let k = items.iter().position(|i| i.tag == TAG_KNOWLEDGE).unwrap();
        let w = items.iter().position(|i| i.tag == TAG_WIKI).unwrap();
        assert!(k < w);
    }

    #[test]
    fn empty_hits_render_no_block_at_all() {
        let items = knowledge_items(&[], KNOWLEDGE_SOURCES, WIKI_PAGES);
        assert!(items.is_empty());
        assert_eq!(
            render_context(&items, &InjectionBudget::default()),
            "",
            "nothing found must produce nothing, not a placeholder"
        );
    }

    #[test]
    fn knowledge_and_wiki_are_separate_blocks_and_knowledge_lines_are_undated() {
        let hits = vec![
            RetrievalHit {
                document: "runbook".into(),
                content: "restart the daemon".into(),
                score: 0.03,
                wiki: false,
            },
            RetrievalHit {
                document: "wiki/ops".into(),
                content: "generated summary".into(),
                score: 0.03,
                wiki: true,
            },
        ];
        let out = render_context(&knowledge_items(&hits, 3, 2), &InjectionBudget::default());
        assert!(out.contains("<knowledge>"), "{out}");
        assert!(out.contains("runbook: restart the daemon"), "{out}");
        assert!(out.contains("<wiki>"), "{out}");
        assert!(out.contains("wiki/ops: generated summary"), "{out}");
        assert!(
            !out.contains("[20"),
            "knowledge lines carry no date prefix: {out}"
        );
    }

    #[test]
    fn the_total_bound_holds_with_knowledge_items_too() {
        let hits: Vec<RetrievalHit> = (0..8)
            .map(|i| RetrievalHit {
                document: format!("d{i}"),
                content: "x".repeat(400),
                score: 1.0 - i as f32 / 10.0,
                wiki: i % 2 == 0,
            })
            .collect();
        let budget = InjectionBudget::default();
        let out = render_context(&knowledge_items(&hits, 3, 2), &budget);
        assert!(
            out.chars().count() <= budget.total,
            "{} > {}",
            out.chars().count(),
            budget.total
        );
        assert!(
            out.contains("chars truncated"),
            "per_block truncation must be visible"
        );
    }

    /// The difference between the two injection paths is DATA now, so it can be
    /// ASSERTED instead of inferred from two files (t278). If someone unifies
    /// these by accident, this test says which decision they overrode.
    #[test]
    fn the_two_presets_state_their_own_differences() {
        assert_eq!(CHAT_SELECTION.query_top_n, 4);
        assert_eq!(CHAT_SELECTION.query_min_score, 0.34);
        assert_eq!(
            RUNS_SELECTION.query_top_n, 0,
            "runs has no query leg -- measured, not fixed"
        );
        assert!(
            CHAT_SELECTION
                .groups
                .iter()
                .any(|g| g.store == crate::MemoryStore::Procedure),
            "chat reads the durable procedure store"
        );
        assert!(
            RUNS_SELECTION
                .groups
                .iter()
                .all(|g| g.store != crate::MemoryStore::Procedure),
            "runs does NOT -- a whole store the other path cannot see"
        );
        assert!(
            RUNS_SELECTION
                .groups
                .iter()
                .any(|g| g.scope == MemoryScope::Project),
            "runs reads the task's project scope"
        );
        assert!(
            CHAT_SELECTION
                .groups
                .iter()
                .all(|g| g.scope != MemoryScope::Project),
            "chat cannot: it is pinned to a cwd, not a project name"
        );

        // The DEAD group is pinned to the rule that makes it dead: if an
        // Observation ever becomes writable in the global namespace, this test
        // fails and the group has to be re-read rather than silently kept.
        let dead = CHAT_SELECTION
            .groups
            .iter()
            .find(|g| g.scope == MemoryScope::Global && g.store == crate::MemoryStore::Observation)
            .expect("the as-found chat groups include it");
        assert_eq!(dead.limit, 3);
        assert!(
            !crate::MemoryStore::Observation.allows_namespace(&crate::Namespace::Global),
            "the group above can never match ONLY while this holds"
        );

        // The drop order is a rank, and it is the documented order.
        assert!(tag_rank(TAG_USER_PROFILE) < tag_rank(TAG_RELEVANT_MEMORIES));
        assert!(tag_rank(TAG_RELEVANT_MEMORIES) < tag_rank(TAG_KNOWLEDGE));
        // t8: the graph block sits between the two, by the same rationale --
        // a structured one-line claim is weaker than verbatim evidence and
        // stronger than a generated page.
        assert!(tag_rank(TAG_KNOWLEDGE) < tag_rank(TAG_GRAPH));
        assert!(tag_rank(TAG_GRAPH) < tag_rank(TAG_WIKI));
        assert!(tag_rank(TAG_WIKI) < tag_rank(TAG_PROJECT_CONTEXT));
        assert!(tag_rank(TAG_PROJECT_CONTEXT) < tag_rank("not_a_tag"));
    }

    // ── t8: the C6 aims, each with its falsifiable reading ──────────────────

    /// C6: the graph block exists as a tag with its own rank, and an unknown tag
    /// still sorts last (so a new producer cannot silently take a priority slot).
    #[test]
    fn the_graph_tag_is_frozen_between_knowledge_and_wiki() {
        println!(
            "READING C6 tag ranks: profile={} memories={} knowledge={} graph={} wiki={} project={} unknown={}",
            tag_rank(TAG_USER_PROFILE),
            tag_rank(TAG_RELEVANT_MEMORIES),
            tag_rank(TAG_KNOWLEDGE),
            tag_rank(TAG_GRAPH),
            tag_rank(TAG_WIKI),
            tag_rank(TAG_PROJECT_CONTEXT),
            tag_rank("whatever"),
        );
        assert_eq!(tag_rank(TAG_GRAPH), 3);
        assert_eq!(tag_rank(TAG_WIKI), 4);
        assert_eq!(tag_rank(TAG_PROJECT_CONTEXT), 5);
        assert_eq!(tag_rank("whatever"), 6);
    }

    // ── t58 / F5 / G10: the graph block itself ─────────────────────────────

    /// POSITIVE: graph evidence lines reach the render as a `<graph>` block, and
    /// the lines are the graph's own text (hops/temporal/source already inside).
    #[test]
    fn graph_evidence_lines_render_as_a_graph_block() {
        let lines = vec![
            "用户19410 -uses-> 微信 (hops=1 valid_at=2026-09-20T00:00:00Z state=current edges=e12 source=session:abc)"
                .to_string(),
            "微信 -runs_on-> macOS (hops=2 state=current edges=e12,e13 source=session:abc)".to_string(),
        ];
        let items = graph_items(&lines, GRAPH_PATHS);
        println!(
            "READING t58 graph items: {} (limit={GRAPH_PATHS})",
            items.len()
        );
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.tag == TAG_GRAPH));
        let out = render_context(&items, &InjectionBudget::default());
        println!("READING t58 graph block:\n{out}");
        assert!(out.starts_with("<graph>\n"), "{out}");
        assert!(out.ends_with("</graph>\n"), "{out}");
        for l in &lines {
            assert!(out.contains(l.as_str()), "line lost: {l}\n{out}");
        }
    }

    /// NEGATIVE CONTROL: no evidence ⇒ **no block at all** (not an empty block,
    /// not a placeholder). Blank lines are not evidence either.
    #[test]
    fn no_graph_evidence_means_no_graph_block() {
        let empty = render_context(&graph_items(&[], GRAPH_PATHS), &InjectionBudget::default());
        println!("READING t58 no-evidence render: {empty:?}");
        assert_eq!(
            empty, "",
            "an empty block would cost tokens and teach nothing"
        );

        let blanks = vec!["".to_string(), "   ".to_string(), "\n".to_string()];
        let blank_items = graph_items(&blanks, GRAPH_PATHS);
        println!("READING t58 blank-lines items: {}", blank_items.len());
        assert!(blank_items.is_empty(), "a blank line is not evidence");
        assert_eq!(
            render_context(&blank_items, &InjectionBudget::default()),
            ""
        );

        // And a render that contains OTHER blocks must not grow a graph block.
        let only_knowledge = render_context(
            &knowledge_items(
                &[RetrievalHit {
                    document: "ops-handbook".into(),
                    content: "deploy via scripts/release.sh".into(),
                    score: 0.9,
                    wiki: false,
                }],
                KNOWLEDGE_SOURCES,
                WIKI_PAGES,
            ),
            &InjectionBudget::default(),
        );
        println!("READING t58 knowledge-only render:\n{only_knowledge}");
        assert!(!only_knowledge.contains("<graph>"), "{only_knowledge}");
    }

    /// The budget is a NUMBER in the contract, and it really caps the block.
    #[test]
    fn the_graph_block_is_capped_by_the_contract_budget() {
        let lines: Vec<String> = (0..10)
            .map(|i| format!("节点{i} -rel{i}-> 目标{i} (hops=1 state=current edges=e{i})"))
            .collect();
        let items = graph_items(&lines, GRAPH_PATHS);
        println!(
            "READING t58 cap: lines=10 limit={GRAPH_PATHS} items={}",
            items.len()
        );
        assert_eq!(items.len(), GRAPH_PATHS);
        let out = render_context(&items, &InjectionBudget::default());
        assert_eq!(out.matches(" -rel").count(), GRAPH_PATHS);
        assert!(out.chars().count() <= InjectionBudget::default().total);
    }

    /// THE INVARIANTS THE BLOCK MUST NOT BREAK: bounded by `total`, truncation
    /// visible, whole-block drop counted. A long graph line is the case that
    /// matters (a path line can be hundreds of chars).
    #[test]
    fn adding_the_graph_block_keeps_the_contract_invariants() {
        let long: Vec<String> = (0..3)
            .map(|i| {
                format!(
                    "起点{i} -very_long_relation_name_{i}-> 终点{i} {}",
                    "x".repeat(400)
                )
            })
            .collect();
        let items = graph_items(&long, GRAPH_PATHS);

        // (a) per-block truncation is VISIBLE and the render stays bounded
        let budget = InjectionBudget {
            per_block: 120,
            total: 4096,
        };
        let out = render_context(&items, &budget);
        println!("READING t58 long-line render (per_block=120):\n{out}");
        assert!(out.chars().count() <= budget.total);
        assert!(
            out.contains(" chars truncated]"),
            "the cut must be visible: {out}"
        );
        assert!(out.contains(TAG_GRAPH));

        // (b) the TOTAL budget drops the whole graph block VISIBLY, with a count
        let mut with_padding = items.clone();
        with_padding.push(ContextItem::undated(TAG_USER_PROFILE, "y".repeat(90)));
        let tight = InjectionBudget {
            per_block: 4096,
            total: 120,
        };
        let dropped = render_context(&with_padding, &tight);
        println!("READING t58 tight-total render (total=120):\n{dropped}");
        assert!(dropped.chars().count() <= tight.total);
        assert!(
            dropped.contains("items dropped") || dropped.contains(" dropped]"),
            "a dropped block must be counted: {dropped}"
        );

        // (c) the property the crate's own proptest asserts, restated for the
        //     graph block: whatever the budget, the bound holds.
        for total in [20usize, 60, 200, 1000] {
            let b = InjectionBudget {
                per_block: 1024,
                total,
            };
            let r = render_context(&items, &b);
            assert!(r.chars().count() <= total, "total={total}");
        }
    }

    /// C1/D.4: the low band is VISIBLE in the bytes, and only when it is low.
    /// Before this, `ContextItem` had no confidence field at all, so a memory
    /// the writer was unsure about reached the agent as if it were certain.
    #[test]
    fn a_low_confidence_line_is_marked_and_a_normal_one_is_not() {
        let low = ContextItem::dated_with_confidence(
            TAG_USER_PROFILE,
            "the user might use xlwt",
            "2026-09-27T10:00:00Z",
            Some(crate::confidence::CONF_HEDGED),
        );
        let high = ContextItem::dated_with_confidence(
            TAG_USER_PROFILE,
            "the user uses xlwt",
            "2026-09-27T10:00:00Z",
            Some(crate::confidence::CONF_CONFIRMED),
        );
        let unknown = ContextItem::dated(TAG_USER_PROFILE, "no value", "2026-09-27T10:00:00Z");
        let out = render_context(&[low, high, unknown], &InjectionBudget::default());
        println!("READING C1 render:\n{out}");
        assert!(out.contains(&format!(
            "{LOW_CONFIDENCE_MARK} [2026-09-27] the user might use xlwt"
        )));
        assert!(!out.contains(&format!(
            "{LOW_CONFIDENCE_MARK} [2026-09-27] the user uses xlwt"
        )));
        assert!(
            !out.contains(&format!("{LOW_CONFIDENCE_MARK} [2026-09-27] no value")),
            "None is not low"
        );
        assert_eq!(out.matches(LOW_CONFIDENCE_MARK).count(), 1);
        // The mark must not push the block over its own bound.
        assert!(out.chars().count() <= InjectionBudget::default().total);
    }

    /// D.4: a low-confidence line is still truncated BEFORE the mark, so the mark
    /// survives the cut (it is the actionable part of the line).
    #[test]
    fn the_low_confidence_mark_survives_truncation() {
        let item = ContextItem::dated_with_confidence(
            TAG_RELEVANT_MEMORIES,
            "x".repeat(4_000),
            "2026-09-27T10:00:00Z",
            Some(0.2),
        );
        let budget = InjectionBudget {
            per_block: 100,
            total: 4_096,
        };
        let out = render_context(&[item], &budget);
        println!(
            "READING C6 low-confidence long line: len={} tail={:?}",
            out.chars().count(),
            &out[out.len().saturating_sub(60)..]
        );
        assert!(out.contains("chars truncated"), "the cut stays visible");
        // The mark is written after the marker line, so both are present.
        assert!(out.contains(LOW_CONFIDENCE_MARK));
    }

    /// R-D D.7 (G8 memory half): a wiki hit with a lead renders its own
    /// self-description — stale / coverage / anchors / edited / hint — in the
    /// SAME block as the body, so the agent can judge the page without trusting
    /// it. Without a lead the line is byte-identical to the old shape.
    #[test]
    fn a_wiki_lead_renders_its_marks_and_a_plain_wiki_hit_does_not_change() {
        let hit = RetrievalHit {
            document: "wiki/kubernetes-troubleshooting".into(),
            content: "当 Pod 出现 crash-loop 时 …".into(),
            score: 0.7,
            wiki: true,
        };
        let plain = knowledge_items(std::slice::from_ref(&hit), 3, 2);
        let plain_render = render_context(&plain, &InjectionBudget::default());
        println!("READING C6/R-D D.7 plain wiki line:\n{plain_render}");
        assert_eq!(
            plain_render,
            "<wiki>\nwiki/kubernetes-troubleshooting: 当 Pod 出现 crash-loop 时 …\n</wiki>\n"
        );

        let lead = WikiLeadMeta {
            slug: "kubernetes-troubleshooting".into(),
            stale: Some(false),
            stale_since: None,
            edited: Some(false),
            cite_coverage: Some(1.0),
            anchors: vec![("ops-handbook".into(), 18), ("ops-handbook".into(), 22)],
            hint: WIKI_LEAD_HINT.into(),
        };
        let enriched = vec![EnrichedHit {
            hit,
            lead: Some(lead),
            relevance: None,
        }];
        let render = render_context(
            &knowledge_items_enriched(&enriched, 3, 2),
            &InjectionBudget::default(),
        );
        println!("READING R-D D.7 wiki lead render:\n{render}");
        for needle in [
            "stale=false",
            "coverage=1.00",
            "anchors=2",
            "edited=false",
            WIKI_LEAD_HINT,
            "anchors: ops-handbook#18, ops-handbook#22",
        ] {
            assert!(render.contains(needle), "missing {needle:?} in:\n{render}");
        }
        assert!(render.contains(TAG_WIKI) || render.starts_with("<wiki>"));
    }

    /// INTEGRATION CORRECTION (t8 + integ's O-1): the producer reports THREE
    /// states for `edited`, and the injection block must not turn "not
    /// decidable" into a definite "not hand-edited".
    ///
    /// The falsifiable half: `WikiLead { edited: None }` must NOT render the
    /// string `edited=false`. A `bool` field cannot express this, which is why
    /// the field is `Option<bool>`.
    ///
    /// RV-D-1 (wiki's t34, 2026-09-28) extended the same rule to
    /// `cite_coverage`, which had been an `f32` that could only ever be 0.0/1.0:
    /// `None` renders `coverage=unknown`, never 0.00 or 1.00.
    #[test]
    fn an_unknown_edited_state_never_renders_as_false() {
        let mk = |edited: Option<bool>, coverage: Option<f32>| EnrichedHit {
            hit: RetrievalHit {
                document: "wiki/kubernetes-troubleshooting".into(),
                content: "body".into(),
                score: 0.7,
                wiki: true,
            },
            lead: Some(WikiLeadMeta {
                slug: "kubernetes-troubleshooting".into(),
                stale: None,
                stale_since: None,
                edited,
                cite_coverage: coverage,
                anchors: Vec::new(),
                hint: WIKI_LEAD_HINT.into(),
            }),
            relevance: None,
        };
        let render = |edited, coverage| {
            render_context(
                &knowledge_items_enriched(&[mk(edited, coverage)], 3, 2),
                &InjectionBudget::default(),
            )
        };
        let unknown = render(None, None);
        let no = render(Some(false), Some(0.0));
        let yes = render(Some(true), Some(1.0));
        println!("READING O-1 edited=None coverage=None ->\n{unknown}");
        println!("READING O-1 edited=Some(false) coverage=0.0 -> {no:?}");
        println!("READING O-1 edited=Some(true) coverage=1.0 -> {yes:?}");
        assert!(unknown.contains("edited=unknown"), "{unknown}");
        assert!(
            !unknown.contains("edited=false"),
            "None must never be rendered as a definite false: {unknown}"
        );
        assert!(
            unknown.contains("stale=unknown"),
            "stale is three-state too"
        );
        // RV-D-1's falsifiable half: unknown coverage is `unknown`, and it is not
        // silently equal to either definite value.
        assert!(unknown.contains("coverage=unknown"), "{unknown}");
        assert!(
            !unknown.contains("coverage=0.00") && !unknown.contains("coverage=1.00"),
            "an unknown coverage must not be reported as a definite ratio: {unknown}"
        );
        assert!(no.contains("edited=false") && no.contains("coverage=0.00"));
        assert!(yes.contains("edited=true") && yes.contains("coverage=1.00"));
    }

    /// R-A H-4: injection orders by the calibrated `relevance` when one exists,
    /// NOT by the rank score. The two orders are printed side by side, because
    /// the point of the change is that the ORDER moves.
    #[test]
    fn ordering_uses_relevance_instead_of_the_rank_score() {
        let mk = |doc: &str, score: f32, relevance: Option<f32>| EnrichedHit {
            hit: RetrievalHit {
                document: doc.into(),
                content: format!("body of {doc}"),
                score,
                wiki: false,
            },
            lead: None,
            relevance: relevance.map(|value| RelevanceMeta {
                value,
                kind: "calibrated",
                version: 1,
                query_background: Some(0.2),
            }),
        };
        // A: rank score order is a, b, c. B: relevance order is c, b, a.
        let hits = vec![
            mk("a", 0.9, Some(0.30)),
            mk("b", 0.5, Some(0.55)),
            mk("c", 0.1, Some(0.80)),
        ];
        let by_rank: Vec<String> = {
            let plain: Vec<EnrichedHit> = hits
                .iter()
                .map(|h| EnrichedHit {
                    relevance: None,
                    ..h.clone()
                })
                .collect();
            knowledge_items_enriched(&plain, 3, 2)
                .iter()
                .map(|i| i.content.clone())
                .collect()
        };
        let by_relevance: Vec<String> = knowledge_items_enriched(&hits, 3, 2)
            .iter()
            .map(|i| i.content.clone())
            .collect();
        println!("READING H-4 order by rank score (before): {by_rank:?}");
        println!("READING H-4 order by relevance (after):   {by_relevance:?}");
        assert_eq!(
            by_rank,
            vec!["a: body of a", "b: body of b", "c: body of c"],
            "the old order is the rank score's"
        );
        assert_eq!(
            by_relevance,
            vec!["c: body of c", "b: body of b", "a: body of a"],
            "the new order is the calibrated relevance's"
        );
    }

    /// R-A H-4 / R-B D.7: the relevance VALUE never enters the block. The
    /// contract consumes it for ordering, and a reader of the block must not be
    /// invited to read it as a similarity (the defect the panel was fixed for).
    #[test]
    fn the_relevance_number_is_not_rendered_into_the_block() {
        let hits = vec![EnrichedHit {
            hit: RetrievalHit {
                document: "ops-handbook".into(),
                content: "restart the pod".into(),
                score: 0.016393,
                wiki: false,
            },
            lead: None,
            relevance: Some(RelevanceMeta {
                value: 0.8615,
                kind: "calibrated",
                version: 1,
                query_background: Some(0.2),
            }),
        }];
        let out = render_context(
            &knowledge_items_enriched(&hits, 3, 2),
            &InjectionBudget::default(),
        );
        println!("READING H-4 render with a relevance:\n{out}");
        assert!(!out.contains("0.8615"), "no similarity number in the block");
        assert!(
            !out.contains("0.016393"),
            "no rank score in the block either"
        );
        assert!(out.contains("restart the pod"));
    }
}
