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

/// Render memories into bounded tagged blocks:
///
/// ```text
/// <user_profile>
/// [2026-09-11] prefers concise answers
/// </user_profile>
/// ```
///
/// Rules (all enforced, all tested):
/// - every block is at most `per_block` chars, truncated with a visible
///   `… [+N chars truncated]` marker;
/// - the whole render is at most `total` chars — blocks are dropped
///   (never partially, never silently: a dropped count is appended);
/// - blocks with identical tags are merged under one tag.
pub fn render_injection(memories: &[MemoryForInjection], budget: &InjectionBudget) -> String {
    // Merge by tag, preserving first-seen order.
    let mut order: Vec<&'static str> = Vec::new();
    let mut merged: std::collections::HashMap<&'static str, Vec<&MemoryForInjection>> =
        std::collections::HashMap::new();
    for m in memories {
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
            .map(|m| format!("[{}] {}\n", date_of(&m.updated_at), m.content.trim()))
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
            "<context_budget>\n… [+{dropped} memories dropped: context budget reached]\n</context_budget>\n"
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
}
