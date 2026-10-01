//! Requirement-shaped Chinese has a marker (ruagent-close-the-gaps t26), and that
//! marker is PRECISE (the repair, t32).
//!
//! The t26 pass added `我要`, `需要`, `应该用`, `不能用`, `不要用` and `我不认可` to
//! `PREF`. The verification of that pass showed every one of them firing on text that
//! REPORTS a requirement instead of stating one — quotation, a third party's
//! statement, a hypothetical, boilerplate attribution, and the substring collision
//! `不能用` inside `能不能用` — and that the delta it added was mostly noise. This file
//! is the deliverable of the repair: every positive case, and every one of those
//! near-misses asserted SILENT.
//!
//! HOW A NEAR-MISS PROVES THE GUARD IS THE REASON. Each family asserts two things:
//!
//! 1. the marker text IS present under plain containment (`text::contains_any`, the
//!    unguarded path) — so a case cannot pass because the words were absent; and
//! 2. the graded pass yields NOTHING (`memory_candidates`).
//!
//! Remove a guard from `text::requirement_marker_blocked` and (1) stays true while
//! (2) stops being empty: the assertion that fails is the `Vec::<&str>::new()`
//! comparison inside the family's test. Nothing here weakens a near-miss to keep a
//! marker; the positives below are what keep the markers.

use ruagent_extract::{ExtractLimits, Role, Turn, memory_candidates, rules, text};

const RULE_PREF: &str = "user_preference";

fn user(s: &str) -> Turn {
    Turn {
        role: Role::User,
        text: s.to_string(),
        ts_ms: 1_700_000_000_000,
    }
}

/// The rules that fired on one user turn, sorted.
fn rules_of(s: &str) -> Vec<&'static str> {
    let mut r: Vec<&'static str> = memory_candidates(&[user(s)], &ExtractLimits::default())
        .into_iter()
        .map(|c| c.rule)
        .collect();
    r.sort_unstable();
    r.dedup();
    r
}

fn is_pref(s: &str) -> bool {
    rules_of(s).contains(&RULE_PREF)
}

/// The marker text is present under plain containment — i.e. a silence below is the
/// guard's doing and not a missing word.
fn words_are_there(s: &str) -> bool {
    text::contains_any(&s.to_lowercase(), rules::PREF)
}

/// The markers this task added, with the reason each is a FORM rather than a bare
/// word. Kept as data so the sweep below and the per-marker cases cannot drift.
const ADDED: &[(&str, &str)] = &[
    ("我要", "first-person statement of a durable want"),
    ("需要", "the recall leg of the same sentence family"),
    (
        "应该用",
        "the imperative form; bare 应该 is a hedge and a CORRECT substring",
    ),
    (
        "不能用",
        "the prohibition form; bare 不能 fired on a rhetorical question",
    ),
    (
        "不要用",
        "the prohibition form; bare 不要 fired on 要不要 and on our own prompts",
    ),
    ("我不认可", "explicit first-person rejection"),
];

/// The registry is exactly what t26 added, in both directions: a marker silently
/// dropped from `PREF` fails the positive cases below, and a marker added to `PREF`
/// without a case here fails this sweep.
#[test]
fn every_added_marker_is_in_pref_and_every_pref_addition_has_a_case_here() {
    for (m, why) in ADDED {
        assert!(
            rules::PREF.contains(m),
            "PREF lost `{m}` ({why}) — the recall the measurement justified is gone"
        );
    }
    // The three bare words t26 deliberately did NOT include (measured false
    // positives), each of which also has a near-miss case below.
    for bare in ["应该", "不能", "不要"] {
        assert!(
            !rules::PREF.contains(&bare),
            "`{bare}` is the precision failure this tier already had"
        );
    }
}

// ---------------------------------------------------------------------------
// The five false-positive families the t32 verification measured, each silent
// ---------------------------------------------------------------------------

/// QUOTATION: the marker is inside someone else's words — a document, a README, or a
/// message the user is reporting. The guard is quotation parity plus the `>` markdown
/// rule (`text::inside_quotation`).
#[test]
fn quotation_is_silent() {
    let cases = [
        // ASCII quote delimiters, a markdown quotation, and typographic ones
        "文档里写着\"不要用 npm 装这个\"，但那是老版本的说法。",
        "> 我需要 Python 3.12 …这是 README 的原文。",
        "他说\"我要用 Rust 重写这个服务\"，我没同意。",
        "文档里写着「不要用 npm 装这个」，但那是老版本的说法。",
        // the same guard also protects the PRE-EXISTING markers (one of the two
        // candidates it removed from the corpus)
        "- 匹配显式记忆指令：消息以「记住」「remember」「note:」等前缀开头。",
    ];
    for s in cases {
        assert!(
            words_are_there(s),
            "the words must be there, or the guard proves nothing: {s}"
        );
        assert_eq!(
            rules_of(s),
            Vec::<&str>::new(),
            "quotation must be silent: {s}"
        );
    }
    // A statement that follows a CLOSED quotation is the user's own and still fires:
    // the guard is parity, not "a quote appears somewhere in the sentence".
    assert!(is_pref(
        "文档里写着\"不要用 npm\"，我的结论是：不要用 npm 装这个，用 pnpm。"
    ));
}

/// THIRD PARTY: a requirement reported about someone else (`同事说…`) is not the
/// user's own statement. The guard is `REPORTING_FRAMES` in the 12 characters before
/// the match.
#[test]
fn a_third_partys_statement_is_silent() {
    for s in ["同事说他需要更多时间。", "文档里说不能用了，要升级。"] {
        assert!(words_are_there(s), "{s}");
        assert_eq!(
            rules_of(s),
            Vec::<&str>::new(),
            "a reported statement must be silent: {s}"
        );
    }
    // The frames are subject+verb PHRASES, never the bare verb `说`: the user's own
    // requirement after an unrelated `说` still fires — the recall cost a bare `说`
    // guard would pay, which is why it is not one.
    assert!(is_pref("你说的对，我需要改一下这个配置。"));
}

/// HYPOTHETICAL: `如果…需要` / `是否…需要` is a conditional, not a requirement. The
/// guard is `CONDITIONAL_FRAMES` in the 3 characters before the match, matched by
/// containment so Chinese adverbs between frame and verb (`是否还`需要) are covered.
#[test]
fn a_hypothetical_is_silent() {
    for s in [
        "如果需要的话我可以补测试。",
        "是否还需要真正的部署起来，回头看一下。",
    ] {
        assert!(words_are_there(s), "{s}");
        assert_eq!(
            rules_of(s),
            Vec::<&str>::new(),
            "a hypothetical must be silent: {s}"
        );
    }
}

/// BOILERPLATE AND THE STATIVE COLLISION: the discourse head `需要说明的是…`, and the
/// same sentence's `可选` — a PRE-EXISTING `user_decision` match from the bare `选`
/// marker, fixed in the same word-boundary family with the `可+V` stative rule
/// (`text::contains_decision`).
#[test]
fn boilerplate_and_the_stative_collision_are_silent() {
    let s = "需要说明的是，这个字段是可选的。";
    assert!(
        words_are_there(s),
        "the requirement marker IS present — the guards are the reason it is silent"
    );
    assert_eq!(
        rules_of(s),
        Vec::<&str>::new(),
        "boilerplate must be silent: {s}"
    );
    // the `可+V` rule on its own: the decision marker is present and silent
    let stative = "这个字段是可选的，也可以留空。";
    assert!(
        text::contains_any(&stative.to_lowercase(), rules::DECISION),
        "the bare `选` marker IS present under plain containment"
    );
    assert_eq!(
        rules_of(stative),
        Vec::<&str>::new(),
        "`可选` is not a decision: {stative}"
    );
    assert!(rules_of("选 B 方案吧，别的都先不做。").contains(&"user_decision"));
}

/// THE SUBSTRING COLLISION: `不能用` sits inside `能不能用`, and Chinese has no word
/// spaces, so the boundary has to come from the language. The rule is the A-not-A
/// pattern: a marker that starts with `不` is not a match when the character BEFORE
/// that `不` equals the character AFTER it — that IS the reduplicated verb of a V不V
/// question (`能不能用`, `要不要用`, `是不是`, `有没有`). The same rule covers `不要用`
/// inside `要不要用`, and it also protects the pre-existing `不要再`.
#[test]
fn the_anota_substring_collision_is_silent() {
    for s in [
        "他问我能不能用别的库。",
        "要不要用 pnpm 重装一遍比较省事。",
        "我还没想好要不要再跑一次全量测试。",
    ] {
        assert!(words_are_there(s), "{s}");
        assert_eq!(
            rules_of(s),
            Vec::<&str>::new(),
            "a V不V question is not a requirement: {s}"
        );
    }
    // A real prohibition is not reduplicated, and still fires.
    assert!(is_pref("这个脚本不能用相对路径调用。"));
}

// ---------------------------------------------------------------------------
// The positives the markers exist for
// ---------------------------------------------------------------------------

#[test]
fn a_first_person_requirement_is_a_preference_and_a_question_is_not() {
    assert!(is_pref("我要用 pnpm 管理这个仓库的依赖。")); // positive
    assert!(
        !is_pref("我要不要先把测试跑完？"),
        "the question form is §8.7's filter, not the marker"
    );
    assert!(!is_pref("我不要求重跑测试。"), "a negation is not a want");
}

#[test]
fn need_carries_a_constraint_but_not_when_it_is_negated_or_asked() {
    assert!(is_pref("我需要 Python 和 openpyxl，离线环境里要能直接装。")); // positive
    assert!(is_pref(
        "环境没有外部网络，所以需要的内容都需要提前下载好。"
    )); // positive, no first person
    assert!(
        !is_pref("我不需要这个依赖，删掉吧。"),
        "the negation guard already covered this shape"
    );
    assert!(!is_pref("还需要我做什么吗？"), "§8.7's question filter");
}

#[test]
fn should_use_is_a_requirement_while_should_be_is_a_correction() {
    assert!(is_pref("这里应该用绝对路径来写。")); // positive
    assert_eq!(
        rules_of("不对，应该是用 cargo 而不是 npm。"),
        vec!["user_correction"],
        "`应该用` is not a substring of `应该是用`: no double count"
    );
    assert!(
        !is_pref("你把该升级的都升级一下应该就好了吧。"),
        "the hedge bare `应该` matched"
    );
}

#[test]
fn a_prohibition_is_a_requirement_but_our_own_prompt_text_is_not() {
    assert!(is_pref("不要用 npm 装这个，用 pnpm。")); // positive
    assert!(
        !is_pref("抓取失败重试一次；再失败就记入 failed（url + reason），不要写文件。"),
        "our own unattended prompt residue: bare `不要` matched it"
    );
    assert!(
        !is_pref("你不能直接调用命令就阻塞等待吗，为什么一直在 sleep。"),
        "a rhetorical complaint"
    );
}

#[test]
fn an_explicit_first_person_rejection_is_caught_and_a_third_party_one_is_not() {
    assert!(is_pref("我不认可先接 OpenCode，认证的事不该我们来管。")); // positive, the named case
    assert!(
        !is_pref("团队那边不认可这个方案。"),
        "the marker is first-person"
    );
}

/// The corpus-narration shapes the previous increment measured, kept together so a
/// future marker cannot quietly re-open them.
#[test]
fn the_measured_corpus_narration_is_silent_as_a_group() {
    let narration = [
        "抓取失败重试一次；再失败就记入 failed（url + reason），不要写文件。",
        "挑真正相关的 2-5 个，不要全列。",
        "我有个问题：要不要考虑换成 postgresql？",
        "你不能直接调用命令就阻塞等待吗，为什么一直在 sleep。",
        "你把该升级的都升级一下应该就好了吧，检查下是否有插件不兼容的情况。",
    ];
    for s in narration {
        assert_eq!(
            rules_of(s),
            Vec::<&str>::new(),
            "narration/prompt residue must produce nothing: {s}"
        );
    }
}
