//! Token usage and cost accounting. See design §8.1.

use serde::{Deserialize, Serialize};

/// Cumulative token usage for a run, updated from ACP `usage_update` events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl UsageTotals {
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    /// Add a usage update into the running totals. ACP agents report
    /// either cumulative totals or deltas; callers pass the right one and
    /// we simply accumulate.
    pub fn add(&mut self, other: UsageTotals) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(other.cache_write_tokens);
    }
}

/// Per-model price in USD per million tokens, for cost estimation.
/// Integer micro-USD arithmetic avoids float drift in aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_per_mtok_micro_usd: u64,
    pub output_per_mtok_micro_usd: u64,
}

impl ModelPrice {
    /// Estimate cost in micro-USD (1e-6 USD) for a usage snapshot.
    pub fn estimate_micro_usd(&self, usage: &UsageTotals) -> u64 {
        let input =
            (usage.input_tokens as u128 * self.input_per_mtok_micro_usd as u128) / 1_000_000;
        let output =
            (usage.output_tokens as u128 * self.output_per_mtok_micro_usd as u128) / 1_000_000;
        (input + output) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_saturating() {
        let mut u = UsageTotals {
            input_tokens: u64::MAX - 1,
            ..Default::default()
        };
        u.add(UsageTotals {
            input_tokens: 10,
            ..Default::default()
        });
        assert_eq!(u.input_tokens, u64::MAX);
    }

    #[test]
    fn cost_estimation_math() {
        // $3/M input, $15/M output (Opus-class pricing as reference).
        let price = ModelPrice {
            input_per_mtok_micro_usd: 3_000_000,
            output_per_mtok_micro_usd: 15_000_000,
        };
        let usage = UsageTotals {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        assert_eq!(price.estimate_micro_usd(&usage), 18_000_000); // $18
    }
}
