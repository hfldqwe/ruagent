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

#[cfg(test)]
mod tests {
    use super::*;

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
