# PR3-03 — Fallback decision logic

## Goal

Pure function that, given a spawn outcome and the policy, returns whether to fall back and why.

## Files touched

- `src/orchestrator/fallback.rs` (new)
- `src/orchestrator/mod.rs` — `pub mod fallback;`

## Implementation

```rust
use crate::registry::schema::FallbackPolicy;
use anyhow::Result;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub enum FallbackReason {
    ExitCode(i32),
    StderrMatch(String),
}

impl FallbackReason {
    pub fn as_log_str(&self) -> String {
        match self {
            FallbackReason::ExitCode(c) => format!("exit_code:{c}"),
            FallbackReason::StderrMatch(p) => format!("stderr_match:{p}"),
        }
    }
}

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
            .map_err(|e| anyhow::anyhow!("invalid stderr regex: {e}"))?;
        Ok(Self {
            exit_codes: policy.retryable_exit_codes.clone(),
            patterns,
            max_retries: policy.max_retries,
            cooldown_ms: policy.cooldown_seconds.saturating_mul(1000),
        })
    }

    pub fn should_fallback(&self, exit: i32, stderr: &str) -> Option<FallbackReason> {
        if self.exit_codes.contains(&exit) {
            return Some(FallbackReason::ExitCode(exit));
        }
        for re in &self.patterns {
            if re.is_match(stderr) {
                return Some(FallbackReason::StderrMatch(re.as_str().to_string()));
            }
        }
        None
    }
}
```

## Acceptance criteria

- [ ] `should_fallback(0, "")` returns `None`.
- [ ] `should_fallback(1, "test failed")` with default policy returns `None` (1 is not in `retryable_exit_codes`).
- [ ] `should_fallback(124, "")` with default policy returns `Some(ExitCode(124))`.
- [ ] `should_fallback(137, "")` returns `Some(ExitCode(137))` — SIGKILL semantic.
- [ ] `should_fallback(1, "rate limit exceeded")` with default policy returns `Some(StderrMatch(...))`.
- [ ] `should_fallback(1, "HTTP 429")` returns `Some(StderrMatch(r"\b429\b"))`.
- [ ] `should_fallback(-1, "")` (signal kill) returns `None` unless `-1` is explicitly added to the policy.
- [ ] Invalid regex in policy → `CompiledPolicy::compile` returns an `Err`.
- [ ] Patterns are case-insensitive where the regex uses `(?i)`.

## Depends on

- PR1-02 (FallbackPolicy in registry).

## Notes

- Exit code precedence over stderr match — if both fire, log the exit code reason. Either path triggers fallback regardless.
- The retryable check is per-attempt. Whether `max_retries` is exceeded is decided in PR3-05, not here.
- `cooldown_ms` exposed here so `run_phase` can sleep without re-reading policy.

---

```yaml
status: done
owner: ""
pr: ""
```
