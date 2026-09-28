//! Reciprocal Rank Fusion — the zero-LLM retrieval fusion (design §6.6
//! #3). Pure, deterministic, unit-tested.

/// Fuse ranked id lists with RRF: `score(d) = Σ 1/(k + rank_i(d))`.
/// Returns ids sorted by fused score (desc), ties by smallest id.
pub fn rrf(rankings: &[Vec<i64>], k: u32) -> Vec<(i64, f32)> {
    let mut scores: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
    for ranking in rankings {
        for (rank, id) in ranking.iter().enumerate() {
            *scores.entry(*id).or_insert(0.0) += 1.0 / (k as f32 + rank as f32 + 1.0);
        }
    }
    let mut out: Vec<(i64, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    out
}

/// Weighted Reciprocal Rank Fusion: `score(d) = Σ w_i/(k + rank_i(d))`.
///
/// WHY A NEW FUNCTION INSTEAD OF A PARAMETER ON `rrf` (t7 / R-A D.3 B-3):
/// `rrf` is frozen — `crates/daemon/src/memembed.rs:310` calls it for the
/// memory two-leg fusion, so changing its signature would be a cross-crate
/// break for a caller that must keep the unweighted form. The spec's rule is
/// explicit: the weights go through a new function, and `rrf(x, k)` must stay
/// bit-identical (pinned by `unweighted_is_the_1_1_special_case` below).
///
/// The weights are RELATIVE, not normalised: what matters is `Σ w_i/(k+rank)`.
/// Measured on the live gold set (R-A C1/C2/C9): `w_sem:w_kw = 2:1` gives
/// recall@1 0.4667→0.7333, MRR 0.6889→0.8167, nDCG@10 0.7682→0.8421, and
/// `{2,3,4}:1` are a plateau — the direction (semantic matters more) is what
/// the reading supports, the exact value is not proven optimal.
///
/// Returns ids sorted by fused score (desc), ties by smallest id — the same
/// determinism rule as `rrf`, so a caller can diff two runs byte for byte.
pub fn rrf_weighted(rankings: &[(&[i64], f32)], k: u32) -> Vec<(i64, f32)> {
    let mut scores: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
    for (ranking, weight) in rankings {
        for (rank, id) in ranking.iter().enumerate() {
            *scores.entry(*id).or_insert(0.0) += weight / (k as f32 + rank as f32 + 1.0);
        }
    }
    let mut out: Vec<(i64, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unweighted_is_the_1_1_special_case() {
        // The bridge between the frozen function and the new one: same input,
        // same output. If `rrf_weighted` ever drifts from `rrf` at w=1, the
        // memory crate's fusion and the knowledge crate's fusion disagree
        // silently -- which is the drift t247 measured in the tokenizer.
        let ann = vec![3, 1, 2];
        let fts = vec![2, 3];
        assert_eq!(
            rrf(&[ann.clone(), fts.clone()], 60),
            rrf_weighted(&[(&ann, 1.0), (&fts, 1.0)], 60)
        );
    }

    #[test]
    fn weight_moves_a_document_that_only_one_leg_found() {
        // The measured live failure mode, in miniature: the distractor is
        // keyword-rank 0 (a hub page whose text contains the query verbatim)
        // and semantic-rank 1, the gold is semantic-rank 0 (the specific page)
        // and keyword-rank 2. Unweighted fusion prefers the distractor
        // (1/62 + 1/61 > 1/61 + 1/63); at 2:1 the semantic leg decides
        // (2/61 + 1/63 > 2/62 + 1/61) and the gold wins. This is WHY the
        // weights exist, and the assertion fails on the unweighted form.
        let sem = vec![100, 200]; // gold 100 at rank 0, distractor 200 at rank 1
        let kw = vec![200, 999, 100]; // distractor at 0, gold at 2
        let unweighted = rrf(&[sem.clone(), kw.clone()], 60);
        assert_eq!(unweighted[0].0, 200, "unweighted fusion prefers the hub");
        let weighted = rrf_weighted(&[(&sem, 2.0), (&kw, 1.0)], 60);
        assert_eq!(weighted[0].0, 100, "2:1 fusion prefers the specific page");
    }

    #[test]
    fn zero_weight_drops_a_leg_without_changing_the_math() {
        let a = vec![1, 2];
        let b = vec![3];
        let fused = rrf_weighted(&[(&a, 1.0), (&b, 0.0)], 60);
        // A zero-weight leg contributes nothing: its document scores exactly
        // 0.0 and therefore sorts LAST, behind documents a real leg found. (The
        // first draft of this test asserted the opposite; the suite caught it,
        // which is the point of asserting the arithmetic and not the shape.)
        assert_eq!(fused[0].0, 1, "the zero-weight leg must not lift id 3");
        assert_eq!(fused.last().unwrap().0, 3);
        assert_eq!(
            fused.last().unwrap().1,
            0.0,
            "zero weight must contribute exactly nothing"
        );
    }

    #[test]
    fn weighted_empty_input() {
        assert!(rrf_weighted(&[], 60).is_empty());
        assert!(rrf_weighted(&[(&[], 2.0)], 60).is_empty());
    }

    #[test]
    fn both_legs_win() {
        // ANN ranks: [1, 2, 3]; FTS ranks: [2, 1].
        let fused = rrf(&[vec![1, 2, 3], vec![2, 1]], 60);
        // Doc 1: 1/61 + 1/62; doc 2: 1/62 + 1/61 -> tie, but doc 2 is
        // rank-2 in ANN and rank-1 in FTS vs doc 1 rank-1 + rank-2 ->
        // actually identical sums; tie broken by id.
        assert_eq!(fused[0].0, 1);
        assert_eq!(fused[1].0, 2);
        // Doc 3 only in one leg: lower score than both-legs docs.
        assert_eq!(fused[2].0, 3);
        assert!(fused[2].1 < fused[0].1);
    }

    #[test]
    fn single_leg_still_works() {
        let fused = rrf(&[vec![7, 8]], 60);
        assert_eq!(fused[0].0, 7);
        assert_eq!(
            fused.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![7, 8]
        );
    }

    #[test]
    fn empty_input() {
        assert!(rrf(&[], 60).is_empty());
        assert!(rrf(&[vec![]], 60).is_empty());
    }
}
