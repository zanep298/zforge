use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One agent invocation's cost telemetry. Serialized one-per-line into
/// `.zforge/cost-log.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub phase: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    // Raw measurements.
    pub prompt_chars: usize,
    pub stdout_chars: usize,
    pub stderr_chars: usize,
    pub duration_ms: u128,
    pub exit_code: i32,
    #[serde(default)]
    pub timed_out: bool,

    // Token accounting. When the agent emits a parseable usage report
    // (claude `--output-format json`'s `usage` block, codex's
    // `tokens used N` line), we record real counts here AND mark
    // `tokens_source = "reported"`. Otherwise we estimate from byte
    // length and mark `tokens_source = "estimated"`.
    pub est_input_tokens: usize,
    pub est_output_tokens: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_total_tokens: Option<u64>,
    #[serde(default = "default_tokens_source")]
    pub tokens_source: String,
    pub est_cost_usd: f64,
}

fn default_tokens_source() -> String {
    "estimated".into()
}

/// Rough byte → token conversion. Vendor tokenizers vary; this is a cheap
/// upper-bound proxy good for spending trends, not exact billing.
///
/// Uses BYTES (not chars). For non-ASCII (Vietnamese, CJK), a single
/// `char` can map to 2-4 tokens because each char often becomes multiple
/// UTF-8 bytes which BPE tokenizers fragment further. Byte-based estimates
/// land closer to reality for mixed-language prompts; ASCII-only prompts
/// are unaffected (1 char == 1 byte).
pub const BYTES_PER_TOKEN: usize = 4;

pub fn estimate_tokens(bytes: usize) -> usize {
    bytes.div_ceil(BYTES_PER_TOKEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_bytes_match_old_char_count() {
        // "hello world" = 11 bytes = 11 chars = ceil(11/4) = 3 tokens.
        assert_eq!(estimate_tokens(11), 3);
    }

    #[test]
    fn multibyte_bytes_estimate_higher_than_chars() {
        // "xin chào" — 'à' is 2 bytes in UTF-8. byte_len = 9, char_count = 8.
        let s = "xin chào";
        assert!(s.len() > s.chars().count());
        assert_eq!(estimate_tokens(s.len()), 3);
    }
}
