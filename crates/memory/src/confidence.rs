//! Confidence: THE ONE PLACE that turns evidence into a number (R-B C1).
//!
//! Why this module exists. Before it, three write paths disagreed about the same
//! quantity: the HTTP write endpoint hard-coded `0.9`, the MCP tool had no
//! parameter at all, and distillation took whatever the extractor said and
//! clamped it to `[0.5, 1.0]`. The clamp is the load-bearing part: the panel and
//! the injection contract both have a notion of "low confidence" at 0.5, and
//! **no production writer could produce a value below it** — 0 of 157 live rows
//! were below 0.5 when this was measured (2026-09-27T21:39+08:00, R-B A.3).
//!
//! So the rule is stated here, once, with named thresholds, and the write paths
//! pass EVIDENCE rather than a number they made up:
//!
//!   user confirmed it      -> 1.0  (the user said so)
//!   the user corrected the agent -> 1.0  (the corrected fact is the user's words)
//!   the agent hedged       -> 0.4  (below LOW_CONFIDENCE, so it renders as unsure)
//!   nobody said anything   -> 0.8  (a normal extraction, not a confirmed fact)
//!
//! `explicit` is for callers that already have a value (a re-write of an older
//! row, a migration): it is used verbatim after clamping, and it is the ONLY way
//! to get a value this module did not choose. Nothing here clamps to a floor
//! above zero: a floor at 0.5 is exactly the defect being removed.

/// At or below this, a memory is rendered as unsure (`[unverified]` in the
/// injection contract, "low" in the panel). ONE constant, two consumers.
pub const LOW_CONFIDENCE: f64 = 0.5;

/// The user explicitly confirmed the fact ("对", "就是这样", "perfect").
pub const CONF_CONFIRMED: f64 = 1.0;

/// The user corrected the agent: the corrected fact comes from the user.
pub const CONF_CORRECTED: f64 = 1.0;

/// The agent hedged ("可能", "I think", "not sure").
pub const CONF_HEDGED: f64 = 0.4;

/// Nothing was said about it: an ordinary extraction. NOT a confirmed fact.
pub const CONF_UNCONFIRMED: f64 = 0.8;

/// The evidence a writer actually has about one extracted fact.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ConfidenceSignals {
    /// The user explicitly confirmed it.
    pub user_confirmed: bool,
    /// The user corrected the agent (so this fact is the user's own statement).
    pub user_corrected: bool,
    /// The agent hedged while stating it.
    pub hedged: bool,
    /// An explicit value the caller already owns (used verbatim, clamped).
    /// `None` = this module decides. This is the escape hatch that keeps the
    /// three-signal rule from lying about a value that was measured elsewhere.
    pub explicit: Option<f64>,
}

impl ConfidenceSignals {
    /// The ordinary case: nothing was said about the fact.
    pub fn unconfirmed() -> Self {
        Self::default()
    }

    /// The user explicitly confirmed the fact.
    pub fn confirmed() -> Self {
        Self {
            user_confirmed: true,
            ..Self::default()
        }
    }

    /// The user corrected the agent about this fact.
    pub fn corrected() -> Self {
        Self {
            user_corrected: true,
            ..Self::default()
        }
    }

    /// The agent hedged while stating it.
    pub fn hedged() -> Self {
        Self {
            hedged: true,
            ..Self::default()
        }
    }

    /// An explicit value the caller owns.
    pub fn explicit(value: f64) -> Self {
        Self {
            explicit: Some(value),
            ..Self::default()
        }
    }
}

/// THE RULE. Precedence, highest first:
/// `explicit` > `user_confirmed` > `user_corrected` > `hedged` > unconfirmed.
///
/// A confirmed fact beats a hedge: the user's own "对" is stronger evidence than
/// the agent's uncertainty about the same statement. A correction beats a hedge
/// for the same reason — the fact being written is the user's, not the agent's.
pub fn confidence(signals: &ConfidenceSignals) -> f64 {
    if let Some(v) = signals.explicit {
        return v.clamp(0.0, 1.0);
    }
    if signals.user_confirmed {
        return CONF_CONFIRMED;
    }
    if signals.user_corrected {
        return CONF_CORRECTED;
    }
    if signals.hedged {
        return CONF_HEDGED;
    }
    CONF_UNCONFIRMED
}

/// Whether a value is in the band the contract renders as unsure.
pub fn is_low(value: f64) -> bool {
    value < LOW_CONFIDENCE
}

/// Every value this rule can produce, for tests that must not hard-code them.
/// Sorted ascending, so a test can print the spread instead of listing numbers.
pub fn rule_values() -> Vec<f64> {
    let mut v = vec![
        CONF_HEDGED,
        CONF_UNCONFIRMED,
        CONF_CORRECTED,
        CONF_CONFIRMED,
    ];
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C1: the low band must be REACHABLE, and the rule must produce more than
    /// one value. Twelve facts, one per evidence combination that matters.
    #[test]
    fn the_low_band_is_reachable_and_the_rule_is_not_constant() {
        let cases = [
            ConfidenceSignals::confirmed(),
            ConfidenceSignals::confirmed(),
            ConfidenceSignals::confirmed(),
            ConfidenceSignals::corrected(),
            ConfidenceSignals::corrected(),
            ConfidenceSignals::unconfirmed(),
            ConfidenceSignals::unconfirmed(),
            ConfidenceSignals::unconfirmed(),
            ConfidenceSignals::unconfirmed(),
            ConfidenceSignals::hedged(),
            ConfidenceSignals::hedged(),
            ConfidenceSignals::explicit(0.6),
        ];
        let values: Vec<f64> = cases.iter().map(confidence).collect();
        let mut distinct: Vec<f64> = values.clone();
        distinct.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        distinct.dedup();
        println!("READING C1 12-fact scenario: values={values:?} distinct={distinct:?}");
        assert!(
            distinct.len() >= 3,
            "the rule must produce at least three distinct values, got {distinct:?}"
        );
        assert!(
            values.iter().any(|v| is_low(*v)),
            "at least one hedged fact must land in the low band: {values:?}"
        );
        assert!(
            !values.iter().all(|v| *v >= LOW_CONFIDENCE),
            "the low band must be reachable"
        );
    }

    /// The floor that made the band unreachable is gone: no value is clamped up
    /// to LOW_CONFIDENCE.
    #[test]
    fn nothing_is_clamped_up_to_the_low_threshold() {
        let v = confidence(&ConfidenceSignals::hedged());
        println!("READING C1 hedged -> {v}");
        assert!(v < LOW_CONFIDENCE, "hedged must be below {LOW_CONFIDENCE}");
        assert_eq!(v, CONF_HEDGED);
        // and the explicit escape hatch may go lower still
        assert_eq!(confidence(&ConfidenceSignals::explicit(0.2)), 0.2);
        assert!(is_low(confidence(&ConfidenceSignals::explicit(0.2))));
    }

    /// Precedence is a decision, so it is asserted: a confirmed fact is 1.0 even
    /// if the same extraction also hedged.
    #[test]
    fn confirmed_beats_hedged() {
        let s = ConfidenceSignals {
            user_confirmed: true,
            hedged: true,
            ..ConfidenceSignals::default()
        };
        assert_eq!(confidence(&s), CONF_CONFIRMED);
        let h = ConfidenceSignals {
            user_corrected: true,
            hedged: true,
            ..ConfidenceSignals::default()
        };
        assert_eq!(confidence(&h), CONF_CORRECTED);
    }

    /// The thresholds live in ONE place: a probe reads them from here.
    #[test]
    fn rule_values_come_from_this_module() {
        let v = rule_values();
        println!("READING C1 rule_values={v:?} LOW_CONFIDENCE={LOW_CONFIDENCE}");
        assert!(v.contains(&CONF_HEDGED));
        assert!(v.contains(&CONF_UNCONFIRMED));
        assert!(v.contains(&CONF_CONFIRMED));
        assert_eq!(v.iter().filter(|x| is_low(**x)).count(), 1);
    }
}
