//! Context usage reporting. See design §8.1.
//!
//! ACP v1 `usage_update` reports context-window occupancy (`used`/`size`)
//! plus an optional cumulative session cost — not per-request token billing.

use serde::{Deserialize, Serialize};

/// Snapshot of context-window usage as reported by ACP `usage_update`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContextUsage {
    /// Tokens currently in context.
    pub used: u64,
    /// Total context window size in tokens.
    pub size: u64,
    /// Cumulative session cost in USD, when the agent reports it.
    pub cost_usd: Option<f64>,
}

impl ContextUsage {
    /// Fraction of the context window in use, clamped to 0.0–1.0.
    pub fn fraction(&self) -> f64 {
        if self.size == 0 {
            0.0
        } else {
            (self.used as f64 / self.size as f64).clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_clamps() {
        let u = ContextUsage {
            used: 300,
            size: 0,
            cost_usd: None,
        };
        assert_eq!(u.fraction(), 0.0);
        let u = ContextUsage {
            used: 300,
            size: 100,
            cost_usd: None,
        };
        assert_eq!(u.fraction(), 1.0);
        let u = ContextUsage {
            used: 50,
            size: 100,
            cost_usd: Some(0.25),
        };
        assert_eq!(u.fraction(), 0.5);
    }
}
