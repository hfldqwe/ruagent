//! V-A F-5 / t30: `ScoreKind` has TWO wire paths and they must agree.
//!
//! `ScoreKind::as_str()` returns the frozen literal (`"rrf_rank"` is frozen by
//! R-B D.7: two generations of consumers read that exact string). But the enum
//! also derives `serde::Serialize`, and until t30 that derive had no rename
//! attribute -- so serde emitted the RUST VARIANT NAME (`"RrfRank"`) while
//! everything hand-written in the daemon emitted `"rrf_rank"`. Same value, two
//! names, no compile error, no test failure: a consumer that serde-serialized
//! `RankedHit`/`SearchPage` (the H-1 wiring in t19) would have silently changed
//! the vocabulary of the wire.
//!
//! V-A measured the pre-fix output (`"RrfRank"`); the first run of this test on
//! the pre-fix revision is that reading, re-taken with the real serializer:
//!
//!   left: `"RrfRank"`  right: `"\"rrf_rank\""`
//!
//! WHY EVERY VARIANT IS RENAMED EXPLICITLY, and not with `rename_all`: no single
//! rule produces these five literals. `snake_case` gives `semantic_distance` and
//! `keyword_bm25`, which are WRONG (the frozen literals are `semantic_l2sq` and
//! `bm25`) -- so `rename_all` alone would have replaced one silent wrong name
//! with another. Two of the five literals are decimals/abbreviations with no
//! derivable spelling, which is exactly why the wire form has to be written next
//! to each variant rather than derived.
//!
//! This pin is the DEFINITION side (this crate). The consumption side is pinned
//! separately in t19, so a rename on either side becomes visible where it
//! happens.

use ruagent_knowledge::ScoreKind;

/// Every variant, with its frozen wire literal. Kept as a list (not a `match`)
/// so a NEW variant cannot be added without this test noticing: the size
/// assertion below fails and the reader sees the list they must extend.
const FROZEN: [(ScoreKind, &str); 5] = [
    (ScoreKind::RrfRank, "rrf_rank"),
    (ScoreKind::SemanticDistance, "semantic_l2sq"),
    (ScoreKind::KeywordBm25, "bm25"),
    (ScoreKind::Cosine, "cosine"),
    (ScoreKind::Calibrated, "calibrated"),
];

#[test]
fn every_score_kind_serialises_as_its_frozen_literal() {
    for (kind, literal) in FROZEN {
        let via_serde = serde_json::to_string(&kind).unwrap();
        let via_as_str = format!("\"{}\"", kind.as_str());
        println!(
            "[t30] {kind:?}: serde={via_serde} as_str={via_as_str} frozen=\"{literal}\" {}",
            if via_serde == via_as_str && via_serde == format!("\"{literal}\"") {
                "AGREE"
            } else {
                "DISAGREE"
            }
        );
        assert_eq!(
            via_serde, via_as_str,
            "serde and as_str() must produce the same literal for {kind:?}; a consumer that \
             serialises the enum and one that writes as_str() would otherwise disagree"
        );
        assert_eq!(
            via_serde,
            format!("\"{literal}\""),
            "{kind:?} must serialise as its FROZEN literal"
        );
    }
}

/// The set of literals, so adding a variant is a visible decision.
#[test]
fn the_literal_set_is_frozen() {
    let mut literals: Vec<String> = FROZEN
        .iter()
        .map(|(kind, _)| serde_json::to_string(kind).unwrap())
        .collect();
    literals.sort();
    assert_eq!(
        literals,
        vec![
            "\"bm25\"".to_string(),
            "\"calibrated\"".to_string(),
            "\"cosine\"".to_string(),
            "\"rrf_rank\"".to_string(),
            "\"semantic_l2sq\"".to_string(),
        ],
        "the wire vocabulary of ScoreKind changed: panel/MCP/injection read these exact strings \
         (R-B D.7 freezes \"rrf_rank\"); a new scale gets a NEW literal, never a rename"
    );
}
