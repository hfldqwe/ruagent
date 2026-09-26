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
}

impl ContextItem {
    /// An item whose line is prefixed with the date part of an RFC3339 stamp.
    pub fn dated(tag: &'static str, content: impl Into<String>, ts: &str) -> Self {
        Self {
            tag,
            content: content.into(),
            date: date_of(ts),
        }
    }

    /// An item with no date (knowledge chunks have no per-chunk date).
    pub fn undated(tag: &'static str, content: impl Into<String>) -> Self {
        Self {
            tag,
            content: content.into(),
            date: String::new(),
        }
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
        TAG_WIKI => 3,
        TAG_PROJECT_CONTEXT => 4,
        _ => 5,
    }
}

/// How many retrieval hits reach the injection, per kind (t260).
pub const KNOWLEDGE_SOURCES: usize = 3;
pub const WIKI_PAGES: usize = 2;

/// One retrieval hit, in the shape the contract needs. The knowledge crate is
/// deliberately NOT a dependency of this crate: the caller hands over the
/// fields, and the selection rule below stays testable without a vector store.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalHit {
    /// Source document name (rendered, so the reader knows where it came from).
    pub document: String,
    pub content: String,
    /// The retrieval's own score (the knowledge base's fused rank score).
    /// Higher is better.
    pub score: f32,
    /// True for a generated wiki page, false for a source document.
    pub wiki: bool,
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
    let pick = |want_wiki: bool, take: usize| -> Vec<&RetrievalHit> {
        let mut v: Vec<&RetrievalHit> = hits.iter().filter(|h| h.wiki == want_wiki).collect();
        // Deterministic order: score desc, then document name, then content.
        v.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.document.cmp(&b.document))
                .then_with(|| a.content.cmp(&b.content))
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
            format!("{}: {}", h.document, h.content.trim()),
        ));
    }
    for h in pick(true, wiki) {
        out.push(ContextItem::undated(
            TAG_WIKI,
            format!("{}: {}", h.document, h.content.trim()),
        ));
    }
    out
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
    let mut dropped = 0usize;

    for tag in order {
        let group = &merged[tag];
        let body: String = group
            .iter()
            .map(|m| {
                if m.date.is_empty() {
                    format!("{}\n", m.content.trim())
                } else {
                    format!("[{}] {}\n", m.date, m.content.trim())
                }
            })
            .collect();

        let header = format!("<{tag}>\n");
        let footer = format!("</{tag}>\n");

        // Per-block truncation with a visible marker.
        let body = if body.chars().count() > budget.per_block {
            let cut: String = body.chars().take(budget.per_block).collect();
            let remaining = body.chars().count() - budget.per_block;
            format!("{cut}\n… [+{remaining} chars truncated]\n")
        } else {
            body
        };

        let block = format!("{header}{body}{footer}");
        // Total budget: drop whole blocks (never silently).
        if out.chars().count() + block.chars().count() > budget.total {
            dropped += group.len();
            continue;
        }
        out.push_str(&block);
    }

    // The drop notice itself obeys the budget — the hard bound wins.
    if dropped > 0 {
        let full = format!(
            "<context_budget>\n… [+{dropped} items dropped: context budget reached]\n</context_budget>\n"
        );
        if out.chars().count() + full.chars().count() <= budget.total {
            out.push_str(&full);
        } else {
            let minimal = format!("… [+{dropped} dropped]");
            if out.chars().count() + minimal.chars().count() <= budget.total {
                out.push_str(&minimal);
            }
        }
    }
    debug_assert!(out.chars().count() <= budget.total);
    out
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
        assert!(tag_rank(TAG_KNOWLEDGE) < tag_rank(TAG_WIKI));
        assert!(tag_rank(TAG_WIKI) < tag_rank(TAG_PROJECT_CONTEXT));
    }
}
