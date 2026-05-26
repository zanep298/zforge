//! Parse agent-reported token usage from captured stdout/stderr.
//!
//! Two known formats:
//! - **claude** with `--output-format json` or `stream-json`: emits a final
//!   line containing a `usage` object with `input_tokens`, `output_tokens`,
//!   `cache_read_input_tokens`, `cache_creation_input_tokens`.
//! - **codex** non-interactive: emits a `tokens used N` line (total only,
//!   no input/output split).
//!
//! Falls back to None on any parse failure — caller substitutes estimates.

use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct UsageReport {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

impl UsageReport {
    pub fn is_empty(&self) -> bool {
        self.input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.cache_read_input_tokens.is_none()
            && self.cache_creation_input_tokens.is_none()
            && self.total_tokens.is_none()
    }
}

/// Parse claude JSON output. Scans every line for a JSON object with a
/// `usage` field — handles both single-line `--output-format json` and
/// multi-line `--output-format stream-json` (the final `result` message
/// carries cumulative usage).
///
/// Returns the LAST seen usage block so streaming logs reflect cumulative
/// totals from the final message rather than the first partial one.
pub fn parse_claude_usage(stdout: &str) -> Option<UsageReport> {
    let mut latest: Option<UsageReport> = None;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if let Some(report) = extract_usage(&value) {
            latest = Some(report);
        }
    }
    latest
}

fn extract_usage(value: &Value) -> Option<UsageReport> {
    let usage = value.get("usage")?;
    let report = UsageReport {
        input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
        output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
        cache_read_input_tokens: usage.get("cache_read_input_tokens").and_then(Value::as_u64),
        cache_creation_input_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64),
        total_tokens: None,
    };
    if report.is_empty() {
        None
    } else {
        Some(report)
    }
}

/// Codex prints a line like `tokens used 9963` after the conversation.
/// Returns the total when found in either stream.
pub fn parse_codex_tokens(stdout: &str, stderr: &str) -> Option<u64> {
    for stream in [stdout, stderr] {
        for line in stream.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("tokens used ") {
                if let Ok(n) = rest.trim().parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_single_line_json_usage() {
        let stdout = r#"{"type":"result","usage":{"input_tokens":123,"output_tokens":456,"cache_read_input_tokens":1000,"cache_creation_input_tokens":50}}"#;
        let report = parse_claude_usage(stdout).expect("usage block");
        assert_eq!(report.input_tokens, Some(123));
        assert_eq!(report.output_tokens, Some(456));
        assert_eq!(report.cache_read_input_tokens, Some(1000));
        assert_eq!(report.cache_creation_input_tokens, Some(50));
    }

    #[test]
    fn parses_claude_stream_json_returns_last_usage() {
        let stdout = "\
{\"type\":\"start\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}\n\
{\"type\":\"chunk\"}\n\
{\"type\":\"result\",\"usage\":{\"input_tokens\":100,\"output_tokens\":200}}\n";
        let report = parse_claude_usage(stdout).expect("usage block");
        assert_eq!(report.input_tokens, Some(100));
        assert_eq!(report.output_tokens, Some(200));
    }

    #[test]
    fn ignores_non_json_lines_in_claude_output() {
        let stdout = "Banner line\n\
not json {bad}\n\
{\"type\":\"result\",\"usage\":{\"input_tokens\":7,\"output_tokens\":8}}\n";
        let report = parse_claude_usage(stdout).expect("usage block");
        assert_eq!(report.input_tokens, Some(7));
    }

    #[test]
    fn returns_none_when_no_usage_block() {
        assert!(parse_claude_usage("just a banner").is_none());
        assert!(parse_claude_usage("{\"type\":\"chunk\"}").is_none());
    }

    #[test]
    fn parses_codex_tokens_line() {
        assert_eq!(parse_codex_tokens("tokens used 9963\n", ""), Some(9963));
    }

    #[test]
    fn codex_tokens_searches_stderr_too() {
        assert_eq!(parse_codex_tokens("", "tokens used 42"), Some(42));
    }

    #[test]
    fn codex_tokens_none_when_missing() {
        assert_eq!(parse_codex_tokens("nothing here", "nope"), None);
    }
}
