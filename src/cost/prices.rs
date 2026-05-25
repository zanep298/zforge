//! Static price table per (agent, model). USD per 1M tokens.
//!
//! Source of truth: vendor pricing pages, current as of 2026-05-25. Update
//! when pricing changes — keep this file as the canonical reference.
//!
//! Codex via ChatGPT Plus = flat subscription. Per-token cost = $0. The
//! table still contains a row so `lookup` finds something; the cost field
//! is zero. API-billed codex usage would need a separate model entry.

/// `(agent, model)` → `(input_usd_per_1m, output_usd_per_1m)`.
const PRICES: &[(&str, &str, f64, f64)] = &[
    // Anthropic Claude (API pricing, public list price)
    ("claude", "haiku", 0.80, 4.00),
    ("claude", "claude-haiku-4-5", 0.80, 4.00),
    ("claude", "sonnet", 3.00, 15.00),
    ("claude", "claude-sonnet-4-6", 3.00, 15.00),
    ("claude", "opus", 15.00, 75.00),
    ("claude", "claude-opus-4-5", 15.00, 75.00),
    // OpenAI Codex via ChatGPT Plus subscription — no per-token billing.
    ("codex", "default", 0.0, 0.0),
    ("codex", "gpt-5-codex", 0.0, 0.0),
    // OpenCode + agy: provider-routed. User must update this table when
    // adopting and verify against vendor invoice.
];

/// Returns `(input_per_1m, output_per_1m)` if known. None when:
///   - Unknown (agent, model) — log entry still written with cost 0,
///     callers should treat 0 as "missing data" not "free".
pub fn lookup(agent: &str, model: Option<&str>) -> Option<(f64, f64)> {
    let model = model.unwrap_or("default");
    PRICES
        .iter()
        .find(|(a, m, _, _)| *a == agent && *m == model)
        .map(|(_, _, i, o)| (*i, *o))
}

/// Compute USD cost from token counts. Returns 0.0 when prices unknown —
/// caller may want to flag the entry for manual review.
pub fn cost_usd(agent: &str, model: Option<&str>, input_tokens: usize, output_tokens: usize) -> f64 {
    let Some((input_per_1m, output_per_1m)) = lookup(agent, model) else {
        return 0.0;
    };
    (input_tokens as f64 * input_per_1m + output_tokens as f64 * output_per_1m) / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_claude_haiku_costs_calculate() {
        // 1M input + 1M output haiku should be $0.80 + $4.00 = $4.80
        let cost = cost_usd("claude", Some("haiku"), 1_000_000, 1_000_000);
        assert!((cost - 4.80).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn unknown_agent_returns_zero() {
        assert_eq!(cost_usd("mystery", Some("opus"), 1000, 1000), 0.0);
    }

    #[test]
    fn codex_chatgpt_is_zero_per_token() {
        // ChatGPT plan is flat subscription; token cost = 0 is correct.
        assert_eq!(cost_usd("codex", Some("default"), 1_000_000, 1_000_000), 0.0);
    }

    #[test]
    fn missing_model_falls_back_to_default_lookup() {
        // codex default row exists; explicit None resolves to "default".
        let (i, o) = lookup("codex", None).expect("codex default");
        assert_eq!((i, o), (0.0, 0.0));
    }
}
