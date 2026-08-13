// Static model pricing for budget ENFORCEMENT, not billing. The audit's
// finding was that max_cost_cents was recorded on receipts while zero
// pricing code existed anywhere in the live path - the system could not
// state its own spend. This table is deliberately a conservative static
// rate card: enforcement needs an upper-bound estimate that exists, not a
// perfect invoice. Rates are cents per million tokens, matched by
// substring on the provider-reported model id; unknown models price at
// the default tier so an unpriced model can never mean unmetered spend.
//
// Cost math stays in integer MICRO-cents (1/1,000,000 cent) so per-turn
// rounding can never leak spend under a budget.

/// (input cents per MTok, output cents per MTok). Exact current flagship ids
/// come first; broad family rows remain conservative enforcement fallbacks.
const RATE_TABLE: [(&str, u64, u64); 13] = [
    ("claude-opus-4-8", 500, 2500),
    ("gpt-5.6-sol", 500, 3000),
    ("gpt-5.6", 500, 3000),
    ("grok-4.5", 200, 600),
    ("kimi-k3", 300, 1500),
    ("opus", 1500, 7500),
    ("sonnet", 300, 1500),
    ("haiku", 100, 500),
    ("grok", 300, 1500),
    ("gpt", 250, 1000),
    ("codex", 250, 1000),
    ("gemini", 250, 1000),
    ("mini", 60, 240),
];
const DEFAULT_RATE: (u64, u64) = (1500, 7500);

fn model_rate_cents_per_mtok(model_id: &str) -> (u64, u64) {
    let id = model_id.to_ascii_lowercase();
    // "mini" must not shadow a stronger family name appearing earlier in
    // the id (gpt-5-mini should price as mini, grok-mini as mini), so the
    // MOST SPECIFIC (cheapest qualifying) match wins only when present;
    // otherwise the first family hit applies.
    if id.contains("mini") {
        let (_, input, output) = RATE_TABLE[12];
        return (input, output);
    }
    for (pattern, input, output) in RATE_TABLE {
        if id.contains(pattern) {
            return (input, output);
        }
    }
    DEFAULT_RATE
}

/// The cost of one model turn in micro-cents.
pub(crate) fn turn_cost_microcents(model_id: &str, input_tokens: u64, output_tokens: u64) -> u64 {
    let (input_rate, output_rate) = model_rate_cents_per_mtok(model_id);
    // cents/MTok * tokens = cents * tokens / 1e6; micro-cents multiplies
    // by 1e6, so the units cancel: microcents = tokens * rate.
    let cost =
        input_tokens as u128 * input_rate as u128 + output_tokens as u128 * output_rate as u128;
    u128::min(cost, u64::MAX as u128) as u64
}

pub(crate) fn microcents_to_cents_rounded_up(microcents: u64) -> u64 {
    microcents.div_ceil(1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_families_price_by_their_tier_and_unknown_prices_conservatively() {
        // Exact flagship ids use their current published rate before the
        // conservative family fallback.
        let opus = turn_cost_microcents("claude-opus-4-8", 1_000_000, 1_000_000);
        assert_eq!(microcents_to_cents_rounded_up(opus), 3000);
        let gpt = turn_cost_microcents("gpt-5.6-sol", 1_000_000, 1_000_000);
        assert_eq!(microcents_to_cents_rounded_up(gpt), 3500);
        let grok = turn_cost_microcents("grok-4.5", 1_000_000, 1_000_000);
        assert_eq!(microcents_to_cents_rounded_up(grok), 800);
        let kimi = turn_cost_microcents("kimi-k3", 1_000_000, 1_000_000);
        assert_eq!(microcents_to_cents_rounded_up(kimi), 1800);
        let mini = turn_cost_microcents("gpt-5-mini", 1_000_000, 1_000_000);
        assert_eq!(microcents_to_cents_rounded_up(mini), 300);
        let unknown = turn_cost_microcents("mystery-model-x", 1_000_000, 1_000_000);
        assert_eq!(
            microcents_to_cents_rounded_up(unknown),
            9000,
            "an unknown model must price at the conservative default, never free"
        );
    }

    #[test]
    fn rounding_always_rounds_spend_up_never_down() {
        // One single token of sonnet input = 300 micro-cents, far below a
        // cent - but it must still count as 1 cent when converted, so many
        // small turns cannot hide under a budget.
        let tiny = turn_cost_microcents("claude-sonnet-5", 1, 0);
        assert_eq!(tiny, 300);
        assert_eq!(microcents_to_cents_rounded_up(tiny), 1);
        assert_eq!(microcents_to_cents_rounded_up(0), 0);
    }
}
