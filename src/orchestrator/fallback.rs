use crate::registry::schema::FallbackPolicy;
use anyhow::{anyhow, Result};
use regex::Regex;

/// Why a fallback fired. Logged into `fallback_history[].reason`.
#[derive(Debug, Clone)]
pub enum FallbackReason {
    ExitCode(i32),
    /// Pattern match against combined stdout+stderr. Renamed from
    /// `StderrMatch` in PR9 — real binaries (claude, codex) print failure
    /// messages to **stdout**, so the orchestrator scans both streams.
    OutputMatch(String),
}

impl FallbackReason {
    pub fn as_log_str(&self) -> String {
        match self {
            FallbackReason::ExitCode(c) => format!("exit_code:{c}"),
            FallbackReason::OutputMatch(p) => format!("output_match:{p}"),
        }
    }
}

/// Pre-compiled fallback policy. Regex compilation happens once at the start
/// of `run_phase`; the retry loop reuses this struct without paying recompile
/// cost on every attempt.
pub struct CompiledPolicy {
    exit_codes: Vec<i32>,
    patterns: Vec<Regex>,
    pub max_retries: u32,
    pub cooldown_ms: u64,
}

impl CompiledPolicy {
    pub fn compile(policy: &FallbackPolicy) -> Result<Self> {
        let patterns = policy
            .retryable_stderr_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("invalid output regex: {e}"))?;
        Ok(Self {
            exit_codes: policy.retryable_exit_codes.clone(),
            patterns,
            max_retries: policy.max_retries,
            cooldown_ms: policy.cooldown_seconds.saturating_mul(1000),
        })
    }

    /// Decide whether the just-finished attempt should trigger a fallback.
    /// Exit code precedence over pattern match: if both fire, return the
    /// exit code reason. `output` is the concatenation of stderr + stdout —
    /// callers must combine before calling. Real-world reason: claude
    /// prints failure messages to stdout, codex prints `ERROR: {...}` JSON
    /// to stdout with exit 0. Scanning stderr alone misses both.
    pub fn should_fallback(&self, exit: i32, output: &str) -> Option<FallbackReason> {
        if self.exit_codes.contains(&exit) {
            return Some(FallbackReason::ExitCode(exit));
        }
        for re in &self.patterns {
            if re.is_match(output) {
                return Some(FallbackReason::OutputMatch(re.as_str().to_string()));
            }
        }
        None
    }
}

/// The text a run's retryable patterns are matched against.
///
/// Plain-text output (and codex's `ERROR: {json}` line) is matched whole,
/// stderr first (PR9). A Claude `stream-json` run is not: its event framing
/// always contains a `rate_limit_event`, and its tool results hold whatever
/// the agent read, so matching the raw stream made every successful run look
/// rate-limited. For a stream only stderr and non-JSON lines count, plus —
/// when the final `result` reports an error — its message, subtype and API
/// status. A run that finished with a successful result has nothing else to
/// match.
pub fn scan_text(stdout: &str, stderr: &str) -> String {
    let events: Vec<(usize, serde_json::Value)> = stdout
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let v: serde_json::Value = serde_json::from_str(l.trim()).ok()?;
            v.get("type")?.as_str()?;
            Some((i, v))
        })
        .collect();
    let is_stream = events.iter().any(|(_, v)| {
        let kind = v["type"].as_str();
        kind == Some("result") || (kind == Some("system") && v["subtype"] == "init")
    });
    if !is_stream {
        return format!("{stderr}\n{stdout}");
    }

    let json_lines: std::collections::HashSet<usize> = events.iter().map(|(i, _)| *i).collect();
    let mut text = String::from(stderr);
    for (i, line) in stdout.lines().enumerate() {
        if !json_lines.contains(&i) {
            text.push('\n');
            text.push_str(line);
        }
    }
    if let Some((_, result)) = events.iter().rev().find(|(_, v)| v["type"] == "result") {
        let failed =
            result["is_error"].as_bool().unwrap_or(false) || result["subtype"] != "success";
        if failed {
            for key in ["subtype", "result", "api_error_status"] {
                match &result[key] {
                    serde_json::Value::Null => {}
                    serde_json::Value::String(s) => {
                        text.push('\n');
                        text.push_str(s);
                    }
                    other => {
                        text.push('\n');
                        text.push_str(&other.to_string());
                    }
                }
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes from a real `claude -p --output-format stream-json --verbose`
    /// run: every run carries a `rate_limit_event`, and tool results carry
    /// whatever the agent read.
    const INIT: &str = r#"{"type":"system","subtype":"init","session_id":"s"}"#;
    const RATE_EVENT: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour"}}"#;
    const TOOL_RESULT: &str = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"// retry on rate limit (429)"}]}}"#;
    const OK: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"Handled the rate limit case."}"#;

    fn stream(lines: &[&str]) -> String {
        lines.iter().map(|l| format!("{l}\n")).collect()
    }

    fn policy() -> CompiledPolicy {
        CompiledPolicy::compile(&default_policy()).unwrap()
    }

    /// The benchmark failure: a successful run was treated as rate-limited
    /// because the stream's own `rate_limit_event` matched the pattern.
    #[test]
    fn a_successful_claude_stream_is_not_retryable() {
        let out = stream(&[INIT, RATE_EVENT, TOOL_RESULT, OK]);
        let text = scan_text(&out, "");
        assert!(!text.to_lowercase().contains("rate"), "{text}");
        assert!(policy().should_fallback(0, &text).is_none());
    }

    #[test]
    fn a_failed_claude_stream_is_matched_on_its_error() {
        let err = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"api_error_status":429,"result":"API Error: Rate limit reached"}"#;
        let text = scan_text(&stream(&[INIT, RATE_EVENT, err]), "");
        assert!(text.contains("Rate limit reached"), "{text}");
        assert!(text.contains("429"), "{text}");
        assert!(policy().should_fallback(0, &text).is_some());
    }

    /// Killed mid-run: no result; whatever plain text and stderr exist are
    /// still matched, the JSON framing is not.
    #[test]
    fn a_stream_without_result_is_matched_on_plain_text_and_stderr() {
        let out = format!("{}overloaded_error\n", stream(&[INIT, RATE_EVENT]));
        let text = scan_text(&out, "boom");
        assert!(
            text.contains("overloaded_error") && text.contains("boom"),
            "{text}"
        );
        assert!(!text.contains("rate_limit_event"), "{text}");
    }

    /// Plain-text runners, and codex's `ERROR: {json}` line, are matched on
    /// everything as before (PR9).
    #[test]
    fn non_stream_output_is_scanned_whole() {
        let codex = r#"ERROR: {"type":"error","status":429,"message":"rate limit"}"#;
        assert_eq!(scan_text(codex, "e"), format!("e\n{codex}"));
        assert_eq!(scan_text("rate limited", ""), "\nrate limited");
    }

    fn default_policy() -> FallbackPolicy {
        FallbackPolicy::default()
    }

    #[test]
    fn success_does_not_fall_back() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        assert!(p.should_fallback(0, "").is_none());
    }

    #[test]
    fn unmatched_test_failure_does_not_fall_back() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        assert!(p.should_fallback(1, "test failed").is_none());
    }

    #[test]
    fn retryable_exit_code_124_falls_back() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        let r = p.should_fallback(124, "").unwrap();
        assert!(matches!(r, FallbackReason::ExitCode(124)));
        assert_eq!(r.as_log_str(), "exit_code:124");
    }

    #[test]
    fn retryable_exit_code_137_falls_back() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        assert!(matches!(
            p.should_fallback(137, "").unwrap(),
            FallbackReason::ExitCode(137)
        ));
    }

    #[test]
    fn rate_limit_in_combined_output_falls_back() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        let r = p.should_fallback(1, "rate limit exceeded").unwrap();
        assert!(matches!(r, FallbackReason::OutputMatch(_)));
    }

    #[test]
    fn http_429_in_combined_output_falls_back() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        let r = p.should_fallback(1, "got HTTP 429 from upstream").unwrap();
        match r {
            FallbackReason::OutputMatch(pat) => assert!(pat.contains("429")),
            other => panic!("expected OutputMatch, got {other:?}"),
        }
    }

    #[test]
    fn signal_kill_not_retryable_by_default() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        assert!(p.should_fallback(-1, "").is_none());
    }

    #[test]
    fn exit_code_precedence_over_output() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        let r = p.should_fallback(124, "rate limit exceeded").unwrap();
        assert!(matches!(r, FallbackReason::ExitCode(124)));
    }

    #[test]
    fn invalid_regex_rejected_at_compile() {
        let mut bad = default_policy();
        bad.retryable_stderr_patterns.push("(unclosed".into());
        assert!(CompiledPolicy::compile(&bad).is_err());
    }

    #[test]
    fn case_insensitive_patterns_match_mixed_case() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        assert!(p.should_fallback(1, "RATE LIMIT EXCEEDED").is_some());
    }

    /// PR9: codex prints `ERROR: {"type":"error","status":429,...}` to
    /// stdout even when rate-limited. Default pattern set must catch the
    /// `429` substring inside the JSON.
    #[test]
    fn codex_stdout_429_json_falls_back() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        let codex_stdout =
            r#"ERROR: {"type":"error","status":429,"error":{"type":"rate_limit_error"}}"#;
        let r = p.should_fallback(0, codex_stdout);
        assert!(
            r.is_some(),
            "codex 429 JSON should trigger fallback even at exit 0"
        );
    }

    /// PR9: claude prints model errors to stdout. Default pattern set
    /// must not falsely trigger on those (they are user errors, non-retryable).
    #[test]
    fn claude_invalid_model_output_does_not_trigger_fallback() {
        let p = CompiledPolicy::compile(&default_policy()).unwrap();
        let claude_stdout =
            "There's an issue with the selected model (totally-invalid-model-name). \
             It may not exist or you may not have access to it.";
        let r = p.should_fallback(1, claude_stdout);
        assert!(
            r.is_none(),
            "invalid model is a user error, NOT retryable: got {r:?}"
        );
    }
}
