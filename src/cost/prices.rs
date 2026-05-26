//! Static price table per (agent, model). USD per 1M tokens.
//!
//! Source of truth: vendor pricing pages. Update when pricing changes —
//! keep this file as the canonical reference.
//!
//! Aliases: versionless rows (`claude/opus`, `claude/sonnet`, `claude/haiku`)
//! map to the latest known list price for that tier and act as a fallback
//! for unknown version suffixes. Pinned rows (`claude-opus-4-7`, etc.) let
//! reports stay accurate even if the alias rate changes later.
//!
//! Codex via ChatGPT Plus = flat subscription. Per-token cost = $0. The
//! table still contains a row so `lookup` finds something; the cost field
//! is zero. API-billed codex (`codex-api`) needs separate entries — the
//! ChatGPT-Plus row is the default to avoid silently overcharging users
//! on the more common subscription plan.

/// `(agent, model)` → `(input_usd_per_1m, output_usd_per_1m)`.
const PRICES: &[(&str, &str, f64, f64)] = &[
    // Anthropic Claude (API list price as of 2026-05).
    ("claude", "haiku", 1.00, 5.00),
    ("claude", "claude-haiku-4-5", 1.00, 5.00),
    ("claude", "claude-haiku-4-5-20251001", 1.00, 5.00),
    ("claude", "sonnet", 3.00, 15.00),
    ("claude", "claude-sonnet-4-6", 3.00, 15.00),
    ("claude", "claude-sonnet-4-7", 3.00, 15.00),
    ("claude", "opus", 15.00, 75.00),
    ("claude", "claude-opus-4-5", 15.00, 75.00),
    ("claude", "claude-opus-4-6", 15.00, 75.00),
    ("claude", "claude-opus-4-7", 15.00, 75.00),
    // OpenAI Codex via ChatGPT Plus subscription — no per-token billing.
    // For API-billed users, register an explicit `codex-api` agent in
    // registry.yaml and add matching rows here.
    ("codex", "default", 0.0, 0.0),
    ("codex", "gpt-5-codex", 0.0, 0.0),
    ("codex", "gpt-5", 0.0, 0.0),
    // OpenCode + agy: provider-routed. User must update this table when
    // adopting and verify against vendor invoice.
];

/// Anthropic cache pricing multipliers vs. base input rate. Cache reads
/// are discounted; cache writes (creation) are surcharged.
const CACHE_READ_MULTIPLIER: f64 = 0.10;
const CACHE_WRITE_MULTIPLIER: f64 = 1.25;

/// Returns `(input_per_1m, output_per_1m)` if known. Falls back to the
/// alias row (`haiku`/`sonnet`/`opus`) when an exact version is unknown
/// but the prefix matches. None when both lookups miss — log entry still
/// written with cost 0; callers treat 0 as "missing data" not "free".
pub fn lookup(agent: &str, model: Option<&str>) -> Option<(f64, f64)> {
    let model = model.unwrap_or("default");
    if let Some(row) = PRICES
        .iter()
        .find(|(a, m, _, _)| *a == agent && *m == model)
    {
        return Some((row.2, row.3));
    }
    // Alias fallback: extract tier name from model string. Covers
    // unreleased version suffixes the table doesn't list yet.
    let alias = if agent == "claude" {
        if model.contains("haiku") {
            Some("haiku")
        } else if model.contains("sonnet") {
            Some("sonnet")
        } else if model.contains("opus") {
            Some("opus")
        } else {
            None
        }
    } else {
        None
    };
    alias.and_then(|tier| {
        PRICES
            .iter()
            .find(|(a, m, _, _)| *a == agent && *m == tier)
            .map(|(_, _, i, o)| (*i, *o))
    })
}

/// Compute USD cost from token counts. Returns 0.0 when prices unknown.
///
/// When cache token counts are provided (Anthropic prompt caching), the
/// `input_tokens` argument is treated as **fresh** (non-cached) input;
/// cache reads + writes are billed separately at their adjusted rates.
pub fn cost_usd(
    agent: &str,
    model: Option<&str>,
    input_tokens: usize,
    output_tokens: usize,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
) -> f64 {
    let Some((input_per_1m, output_per_1m)) = lookup(agent, model) else {
        return 0.0;
    };
    let fresh_input_cost = input_tokens as f64 * input_per_1m;
    let cache_read_cost = cache_read_tokens as f64 * input_per_1m * CACHE_READ_MULTIPLIER;
    let cache_write_cost = cache_creation_tokens as f64 * input_per_1m * CACHE_WRITE_MULTIPLIER;
    let output_cost = output_tokens as f64 * output_per_1m;
    (fresh_input_cost + cache_read_cost + cache_write_cost + output_cost) / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_claude_haiku_costs_calculate() {
        // 1M input + 1M output haiku at current $1/$5 list = $6.00.
        let cost = cost_usd("claude", Some("haiku"), 1_000_000, 1_000_000, 0, 0);
        assert!((cost - 6.00).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn unknown_agent_returns_zero() {
        assert_eq!(cost_usd("mystery", Some("opus"), 1000, 1000, 0, 0), 0.0);
    }

    #[test]
    fn codex_chatgpt_is_zero_per_token() {
        assert_eq!(
            cost_usd("codex", Some("default"), 1_000_000, 1_000_000, 0, 0),
            0.0
        );
    }

    #[test]
    fn missing_model_falls_back_to_default_lookup() {
        let (i, o) = lookup("codex", None).expect("codex default");
        assert_eq!((i, o), (0.0, 0.0));
    }

    #[test]
    fn unknown_version_falls_back_to_alias() {
        // Hypothetical future version not in the explicit table.
        let (i, o) = lookup("claude", Some("claude-sonnet-9-0")).expect("alias hit");
        assert_eq!((i, o), (3.00, 15.00));
    }

    #[test]
    fn cache_reads_billed_at_one_tenth_input() {
        // 1M cache-read tokens for haiku = 0.10 * $1 = $0.10.
        let cost = cost_usd("claude", Some("haiku"), 0, 0, 1_000_000, 0);
        assert!((cost - 0.10).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn cache_writes_billed_at_one_and_quarter_input() {
        // 1M cache-creation tokens for haiku = 1.25 * $1 = $1.25.
        let cost = cost_usd("claude", Some("haiku"), 0, 0, 0, 1_000_000);
        assert!((cost - 1.25).abs() < 1e-9, "got {cost}");
    }
}
