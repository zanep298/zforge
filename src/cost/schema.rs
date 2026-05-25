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

    // Derived. Token counts are estimates (~4 chars/token) unless the agent
    // emits parseable usage lines (currently only codex's `tokens used N`).
    pub est_input_tokens: usize,
    pub est_output_tokens: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_total_tokens: Option<u64>,
    pub est_cost_usd: f64,
}

/// Rough character → token conversion. Vendor tokenizers vary; this is a
/// cheap upper-bound proxy good for spending trends, not exact billing.
pub const CHARS_PER_TOKEN: usize = 4;

pub fn estimate_tokens(chars: usize) -> usize {
    chars.div_ceil(CHARS_PER_TOKEN)
}
