//! Every marker table, every rule id, every stop list and every cap the free
//! tier uses — in ONE file, so the whole vocabulary of the deterministic rules
//! can be read (and argued with) without reading the algorithms.
//!
//! Where these strings come from. The memory marker sets are the words the LLM
//! extraction prompt already names (`crates/daemon/src/distill.rs:19-22`:
//! "不对" / "no, actually" / "应该是" / "对" / "就是这样" / "perfect" / "可能" /
//! "I think" / "not sure") and the words the confidence module documents
//! (`crates/memory/src/confidence.rs:14-17,28-38`). The two tiers must ask for
//! EVIDENCE about the same vocabulary, or the seam of
//! `docs/plans/capability-plugins-design.md` §10 would hand the applier two
//! different notions of "the user confirmed this".
//!
//! Marker matching is whole-marker containment over a lowercased copy of the
//! sentence (design §8.2), with one precision guard that the design's own gold
//! fixture forces: an occurrence preceded by a negation is not a match, because
//! `CORRECT` contains "不对" and `CONFIRM` contains "对", so bare containment
//! would read the correction "不对，应该是用 work-stealing 的 executor。" as a
//! CONFIRMATION (§8.6 case `correction_beats_the_agents_wrong_answer` expects
//! exactly one candidate, and it is the correction). The guard lives in
//! [`crate::text::contains_marker`].

// ---------------------------------------------------------------------------
// Memory rules (§8.2)
// ---------------------------------------------------------------------------

/// Rule ids. `&'static str` so they can travel in `MemoryCandidate::rule`, be
/// named in the dedup key (design §8.4) and be asserted by name in tests.
pub const RULE_USER_PREFERENCE: &str = "user_preference";
pub const RULE_USER_CORRECTION: &str = "user_correction";
pub const RULE_USER_DECISION: &str = "user_decision";
pub const RULE_PROCEDURE_NOTE: &str = "procedure_note";
pub const RULE_LESSON_LEARNED: &str = "lesson_learned";
pub const RULE_HEDGED_STATEMENT: &str = "hedged_statement";

/// The namespace the free tier writes into. It is the only namespace this crate
/// can justify without reading config: a deterministic rule produces facts about
/// the user. The applier still validates it through
/// `ruagent_memory::namespace::Namespace::parse` (used at distill.rs:683).
pub const NAMESPACE_USER: &str = "user";

/// `PREF` — the user stated a durable preference/instruction.
///
/// Design §8.2 spells one marker as `我的…是`; a marker table cannot express a
/// wildcard, so it is realised as the prefix `我的` (the first-person store test
/// below covers the rest of the shape).
///
/// REQUIREMENT-SHAPED STATEMENTS (ruagent-close-the-gaps t26). The hand read behind
/// the hygiene measurement found this one gap: Chinese requirement statements
/// (`我要…`, `我需要…`, `…应该用…`, `不要用…`) had NO marker at all, so the durable
/// statements the user actually makes were caught, when they were caught, by accident
/// in the wrong rule (`不是`/`应该是` → user_correction, `我觉得` → hedged_statement).
/// The six entries below close it, and each one is the FORM that survived a
/// precision measurement rather than the bare word:
///
/// * `我要` — first-person statement of a durable want (0 false positives in the
///   bounded read of 55 real user turns).
/// * `需要` — the recall leg of the same sentence family: it carries the constraint
///   statements ("…所以需要的内容都需要提前下载好") that the other markers miss. Kept
///   broad deliberately; the question filter (`text::is_question`, §8.7) already
///   blocks the "…吗" forms.
/// * `应该用`, NOT bare `应该`: bare 应该 fires on the HEDGE `应该就好了吧` — a measured
///   false positive, and the exact precision failure this tier already had — and it
///   is a substring of `应该是`, which is `CORRECT`'s marker, so it would double-count
///   every correction as a preference.
/// * `不能用` / `不要用`, NOT bare `不能` / `不要`: bare 不要 fires inside the QUESTION
///   `要不要考虑换成postgresql` (measured), and on the residue of our OWN unattended
///   prompts still on disk (`不要写文件`, `不要全列` — one prompt family, ~20+ candidates
///   across sessions), which the hygiene work exists to exclude. Bare 不能 fired on a
///   rhetorical complaint (`你不能直接调用命令就阻塞等待吗`).
/// * `我不认可` — explicit first-person rejection, the strongest requirement shape in
///   the measured set and the one the hand read named; it is the only marker here
///   whose every measured hit was a durable decision.
pub const PREF: &[&str] = &[
    "记住",
    "以后",
    "下次",
    "不要再",
    "别用",
    "我喜欢",
    "我不喜欢",
    "我偏好",
    "统一用",
    "我的",
    "我要",
    "需要",
    "应该用",
    "不能用",
    "不要用",
    "我不认可",
    "remember that",
    "from now on",
    "always use",
    "never use",
    "i prefer",
    "i like",
    "i don't like",
    "don't use",
    "call it",
];

/// `CORRECT` — the user corrected the agent.
pub const CORRECT: &[&str] = &[
    "不对",
    "不是",
    "不这样",
    "应该是",
    "纠正",
    "不是这样",
    "no, actually",
    "that's wrong",
    "actually,",
    "correction:",
];

/// `CONFIRM` — the user explicitly confirmed. Matching this set is what makes a
/// candidate `CandidateConfidence::Confirmed` (design §8.2) and what emits the
/// confirmation candidate itself when no `PREF` marker is present.
pub const CONFIRM: &[&str] = &[
    "对",
    "就是这样",
    "没错",
    "正确",
    "perfect",
    "exactly",
    "yes that's right",
];

/// `DECISION` — the user settled something.
pub const DECISION: &[&str] = &[
    "决定",
    "就用",
    "选",
    "定下来",
    "以后都用",
    "let's go with",
    "we'll use",
    "decided",
    "final answer",
];

/// `TOOLISH` — the vocabulary that decides `procedure` vs `observation` for a
/// decision. Design §8.2 names the set but does not enumerate it ("procedure if
/// the sentence names a marker from `TOOLISH`"), so the set is named here: a
/// decision is a PROCEDURE when it names something you operate.
pub const TOOLISH: &[&str] = &[
    "工具", "命令", "脚本", "配置", "安装", "部署", "插件", "服务", "框架", "tool", "command",
    "script", "config", "install", "deploy", "plugin", "service", "cli", "api", "sdk", "mcp",
    "skill", "cargo", "npm", "pnpm", "docker", "postgres", "sqlite",
];

/// `PROC` — the assistant stated how to do something.
pub const PROC: &[&str] = &[
    "步骤",
    "首先要",
    "然后",
    "否则",
    "需要先",
    "配置",
    "安装",
    "执行",
    "step 1",
    "first,",
    "then,",
    "otherwise,",
    "note that",
    "requires",
    "run the",
];

/// `LESSON` — the assistant stated a pitfall/lesson.
pub const LESSON: &[&str] = &[
    "坑",
    "踩坑",
    "注意",
    "很容易",
    "会导致",
    "失败是因为",
    "教训",
    "pitfall",
    "gotcha",
    "lesson",
    "because it fails",
];

/// `HEDGE` — the speaker expressed doubt. The single set behind both the
/// `hedged_statement` rule and the `Hedged` confidence of `procedure_note` /
/// `lesson_learned` (design §8.2).
pub const HEDGE: &[&str] = &[
    "可能",
    "我觉得",
    "好像",
    "不确定",
    "大概",
    "i think",
    "maybe",
    "not sure",
    "probably",
];

/// First-person markers: they decide `profile` vs `observation` for
/// `user_preference` (design §8.2).
pub const FIRST_PERSON: &[&str] = &["我", "我的", "我们", "my ", "i ", "i'"];

// ---------------------------------------------------------------------------
// The requirement-context guards (t32)
// ---------------------------------------------------------------------------
//
// `PREF` is matched through `text::contains_requirement` (`PREF` only;
// `CORRECT`/`CONFIRM`/`HEDGE` keep the plain containment they were designed with),
// which adds four context guards on top of the negation guard. Why: containment
// fires on text that REPORTS a requirement instead of stating one. Measured against
// the verification's five families (ruagent-close-the-gaps t32), every one of which
// fired `user_preference` before these guards existed. The three tables below are
// the word lists those guards use; the structural rules (the A-not-A adjacency rule
// and the quotation parity rule) live in `text.rs`, because they are about POSITION
// rather than about words.

/// Attribution frames: a requirement introduced by one of these is someone else's
/// statement being reported (`同事说他需要更多时间。`), not the user's own.
///
/// SUBJECT+VERB phrases, never the bare verb `说`: a bare `说` would silence
/// `你说的对，我需要改一下`, which IS the user's requirement — the recall regression
/// this table is shaped to avoid. The window is the 12 characters immediately before
/// the match (`text::attributed_before`).
pub const REPORTING_FRAMES: &[&str] = &[
    "他说",
    "她说",
    "他们说",
    "同事说",
    "对方说",
    "别人说",
    "文档里说",
    "文档说",
    "书里说",
    "文章说",
    "作者说",
    "原文",
    "readme",
    "据说",
    "引用",
];

/// Conditional frames immediately before the marker: a hypothetical is not a
/// requirement (`如果需要的话我可以补测试。`). `是否` belongs here because it is the
/// same shape — a yes/no frame (`是否还需要真正的部署起来`).
pub const CONDITIONAL_FRAMES: &[&str] = &["如果", "假如", "要是", "假设", "万一", "是否"];

/// Discourse heads immediately AFTER the marker: boilerplate that introduces an
/// explanation rather than stating a requirement (`需要说明的是…`).
pub const BOILERPLATE_HEADS: &[&str] = &["说明", "指出", "强调", "注明"];

// ---------------------------------------------------------------------------
// Graph rules (§9.2, §9.4)
// ---------------------------------------------------------------------------

pub const RULE_HEADING_ENTITY: &str = "heading_entity";
pub const RULE_WIKI_LINK: &str = "wiki_link";
pub const RULE_INLINE_CODE: &str = "inline_code_identifier";
pub const RULE_PROPER_NOUN: &str = "proper_noun_phrase";
pub const RULE_HAN_TERM: &str = "chinese_term_run";

/// Identity priority among the entity rules, highest first. The first rule that
/// names a term owns it: a heading owns a term it shares with an inline-code
/// span, and the emitted `rule`/`kind`/`aliases` are the owner's.
///
/// `wiki_link` sits directly after `heading_entity` because both are the
/// author's own terms rather than a coincidence — see the named addition in
/// `lib.rs`'s deviations list (§9.2's table has four sources; the task's
/// acceptance names wiki links among them).
pub const ENTITY_RULE_PRIORITY: &[&str] = &[
    RULE_HEADING_ENTITY,
    RULE_WIKI_LINK,
    RULE_INLINE_CODE,
    RULE_PROPER_NOUN,
    RULE_HAN_TERM,
];

/// Headings that carry no term (design §9.2).
pub const STOP_HEADINGS: &[&str] = &[
    "overview",
    "overview and background",
    "introduction",
    "intro",
    "介绍",
    "简介",
    "概览",
    "背景",
    "notes",
    "note",
    "summary",
    "总结",
    "参考",
    "reference",
    "附录",
    "appendix",
    "目录",
    "contents",
    "todo",
    "see also",
    "说明",
];

/// Backticked spans that are language keywords, not identifiers (§9.2).
/// Single letters are rejected structurally (`is_ident_shape` needs 3 chars).
pub const STOP_CODE: &[&str] = &[
    "true", "false", "null", "none", "let", "fn", "impl", "async", "await", "if", "else", "for",
    "while", "return", "mut", "pub", "use", "self", "match", "struct", "enum", "const", "var",
    "def", "int", "str", "bool", "void", "new", "type",
];

/// Words that make a capitalized phrase ordinary prose rather than a name
/// (§9.2: "not in `STOP_WORDS` (the/this/if/when/note/see/…)"). A phrase is
/// rejected when ANY of its words is in this list.
pub const STOP_WORDS: &[&str] = &[
    "the", "this", "that", "these", "those", "there", "here", "if", "when", "then", "note", "see",
    "also", "and", "or", "but", "for", "with", "from", "into", "onto", "over", "under", "not",
    "are", "was", "were", "is", "be", "been", "has", "have", "had", "will", "would", "can",
    "could", "should", "may", "might", "must", "all", "any", "each", "every", "some", "what",
    "which", "who", "how", "why", "where", "it", "its", "a", "an", "to", "of", "in", "on", "as",
    "at", "by", "we", "you", "they", "he", "she", "my", "our", "your", "their", "no", "yes",
    "only", "more", "most", "much", "many", "new", "used", "using", "uses", "first", "second",
    "third", "example", "both", "either", "neither", "one", "two", "three", "instead", "however",
];

/// Han runs that are function words, not terms (§9.2).
pub const STOP_HAN: &[&str] = &[
    "的", "是", "我们", "这个", "可以", "如果", "因为", "所以", "以及", "一个", "没有", "就是",
    "不是",
];

/// The vocabulary that makes an inline-code identifier a `tool` (§9.2).
pub const EXEC: &[&str] = &[
    "run", "install", "execute", "执行", "安装", "启动", "配置", "deploy", "部署", "build", "编译",
];

/// Extensions that make an inline-code identifier a `tool` (§9.2).
pub const FILE_EXTENSIONS: &[&str] = &[
    ".sh", ".md", ".json", ".toml", ".rs", ".ts", ".tsx", ".py", ".exe", ".dll",
];

/// One relation pattern of the closed table (design §9.4). `connectors` holds
/// the surface forms of the same relation; whitespace around a connector is
/// tolerated (Chinese prose writes both `X是Y` and `X 是 Y`).
pub struct RelationPattern {
    pub rule: &'static str,
    pub relation: &'static str,
    pub connectors: &'static [&'static str],
    /// Distinct sentences that must state the same `(src, relation, dst)`
    /// before it is emitted (§9.4). `is_a` is definitional, so support 1 is
    /// enough; the other three need a second, independent sentence.
    pub min_support: usize,
}

/// The only four relations the free tier may name. The extractor never invents
/// a relation literal (§9.4); the model tier carries its own string through
/// `RelationName::Model` and is deliberately not subject to this table.
pub const RELATION_PATTERNS: &[RelationPattern] = &[
    RelationPattern {
        rule: "relation_is_a",
        relation: "is_a",
        connectors: &["是一种", "是", " is an ", " is a "],
        min_support: 1,
    },
    RelationPattern {
        rule: "relation_uses",
        relation: "uses",
        connectors: &["使用", "依赖", " uses ", " depends on "],
        min_support: 2,
    },
    RelationPattern {
        rule: "relation_runs_on",
        relation: "runs_on",
        connectors: &["运行在", "部署在", " runs on ", " is deployed on "],
        min_support: 2,
    },
    RelationPattern {
        rule: "relation_includes",
        relation: "includes",
        connectors: &["包含", "由", "组成", " includes ", " consists of "],
        min_support: 2,
    },
];

// ---------------------------------------------------------------------------
// Caps and the bounds that keep the rules honest (§7.4.7, §8.5, §9.2, §9.5)
// ---------------------------------------------------------------------------

/// The distinct-term table is capped (first-occurrence order wins) so a
/// pathological input cannot grow it (design §9.2). Past the cap, NEW terms are
/// not counted; terms already in the table keep counting.
pub const MAX_DISTINCT_TERMS: usize = 4096;

/// Minimum length of an acronym alias.
///
/// The design asks for "its acronym — the same shape as `ruagent_graph::acronym`
/// (graph/src/lib.rs:930-944)". Taken literally, a two-word title yields a
/// two-letter alias: `DeepSeek Harness` -> `dh`, and the graph's own judge treats
/// an acronym as SAME as its multi-word form (`same_variant`,
/// graph/src/lib.rs:968-990), i.e. a two-letter acronym is a false-merge
/// generator. The bound: only acronyms of 3+ characters become alias
/// candidates. `Agent Client Protocol` -> `ACP` still qualifies; `DeepSeek
/// Harness (dsh)` gets its real abbreviation from the parenthetical, not from
/// `dh`. This is a named narrowing of §9.2, tested in
/// `tests/graph-gold.rs`.
pub const MIN_ACRONYM_CHARS: usize = 3;

/// A maximal Han run longer than this has no deterministic term boundary
/// without a segmenter, so it contributes no Chinese term (design §9.2 caps the
/// rule at 12 chars). Stated as a recall limit, not a bug: the rule does not
/// guess, and a clause longer than 12 Han characters is a clause, not a term.
pub const MAX_HAN_TERM_CHARS: usize = 12;

/// How many capitalized words a `proper_noun_phrase` may span (design §9.2's
/// `{0,3}`). A longer run is scanned in windows of this size.
pub const MAX_PHRASE_WORDS: usize = 4;
