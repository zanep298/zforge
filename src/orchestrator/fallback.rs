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

#[cfg(test)]
mod tests {
    use super::*;

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
