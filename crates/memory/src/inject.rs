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

/// Memory-only wrapper over render_context -- the shape the runs path used
/// before t260, and the shape the property/golden tests pin.
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
}
