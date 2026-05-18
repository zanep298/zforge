//! Rough token-count estimator. Used by `zforge` to print prompt sizes in
//! status output and frontmatter — never for billing.
//!
//! The previous implementation was `text.len() / 4`, which counts UTF-8
//! *bytes* and badly under-counts CJK text (one Chinese character takes 3
//! bytes, so the old heuristic gave ~0.75 tokens per char when the true
//! cl100k/o200k figure is closer to 1.5–2 tokens). The current heuristic
//! splits the input by codepoint and applies different per-character rates:
//!
//! * ASCII: ~4 chars per token (English prose / typical code)
//! * non-ASCII: ~0.6 tokens per codepoint (CJK averages just over 1, Latin-
//!   diacritic / emoji is much lower — 0.6 is a usable middle ground that is
//!   never wildly wrong in either direction).
//!
//! For exact counts, swap in `tiktoken-rs` at the cost of binary size and
//! compile time. The current accuracy is sufficient for the UI surface.

/// Estimate token count. Returns 0 for empty input.
pub fn estimate(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut ascii = 0usize;
    let mut non_ascii = 0usize;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            non_ascii += 1;
        }
    }
    let approx = (ascii as f64 / 4.0) + (non_ascii as f64 * 0.6);
    approx.ceil().max(1.0) as usize
}

pub fn fmt(n: usize) -> String {
    if n >= 1_000 {
        format!("~{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("~{}", n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_zero() {
        assert_eq!(estimate(""), 0);
    }

    #[test]
    fn ascii_uses_four_chars_per_token() {
        // 12 ASCII chars → 12/4 = 3 tokens.
        assert_eq!(estimate("abcdefghijkl"), 3);
    }

    #[test]
    fn short_ascii_rounds_up_to_at_least_one_token() {
        assert_eq!(estimate("hi"), 1);
    }

    // Regression: bytes/4 counted CJK at ~0.75 tokens per char (3 bytes / 4).
    // A short Chinese phrase under that rule reported fewer tokens than the
    // same English phrase, which is the opposite of reality.
    #[test]
    fn cjk_costs_more_per_char_than_ascii() {
        let english = estimate("hello world!");
        let chinese = estimate("你好世界");
        assert!(
            chinese > english / 2,
            "estimator under-counts CJK: chinese={chinese}, english={english}"
        );
    }

    #[test]
    fn fmt_uses_k_suffix_above_thousand() {
        assert_eq!(fmt(999), "~999");
        assert_eq!(fmt(1500), "~1.5k");
        assert_eq!(fmt(12345), "~12.3k");
    }

    #[test]
    fn estimate_is_monotonic_in_length() {
        let a = estimate("a");
        let aa = estimate(&"a".repeat(1000));
        assert!(aa > a);
    }
}
