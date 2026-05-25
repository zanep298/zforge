//! Real Claude Code binary contract verification.
//!
//! These tests spawn the actual `claude` binary to confirm zforge's
//! orchestrator delivers prompts the way Claude Code expects (stdin pipe vs
//! `-p` arg), parses the right exit codes, and recognizes real-world
//! rate-limit / quota stderr patterns.
//!
//! **All tests in this file are gated by `ZFORGE_TEST_REAL_CLAUDE=1`** because:
//!   1. They burn real API quota.
//!   2. They require the `claude` binary on `$PATH`.
//!   3. CI defaults skip them — opt-in locally when wiring a new agent.
//!
//! Run manually:
//! ```bash
//! ZFORGE_TEST_REAL_CLAUDE=1 cargo test --test real_claude_test -- --nocapture
//! ```
//!
//! Each test prefers the cheapest available model (`haiku`) and asks for the
//! shortest possible response to minimize cost and wall time.

use serial_test::serial;
use std::time::{Duration, Instant};
use zforge::orchestrator::spawn::spawn_agent;
use zforge::registry::schema::AgentSpec;

// ─── gate ─────────────────────────────────────────────────────────────────────

fn claude_available() -> Option<String> {
    if std::env::var("ZFORGE_TEST_REAL_CLAUDE").is_err() {
        eprintln!(
            "[real_claude_test] SKIP: set ZFORGE_TEST_REAL_CLAUDE=1 to run \
             real-binary contract tests"
        );
        return None;
    }
    match which::which("claude") {
        Ok(p) => Some(p.to_string_lossy().into_owned()),
        Err(e) => {
            eprintln!("[real_claude_test] SKIP: `claude` not on $PATH: {e}");
            None
        }
    }
}

fn claude_spec(extra_args: &[&str]) -> AgentSpec {
    let mut args = vec!["-p".to_string()];
    args.extend(extra_args.iter().map(|s| s.to_string()));
    AgentSpec {
        command: "claude".into(),
        args,
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

/// Contract Q1: does `claude -p` accept the prompt via stdin?
///
/// zforge's orchestrator pipes the rendered prompt to the child's stdin.
/// If Claude only reads `-p <text>` as an arg, every spawn through the
/// orchestrator silently sends an empty prompt — catastrophic. This test
/// is the canary.
#[test]
#[serial]
fn claude_reads_prompt_from_stdin() {
    let Some(_bin) = claude_available() else { return; };

    let spec = claude_spec(&["--model", "haiku"]);
    let started = Instant::now();
    let outcome = spawn_agent(
        &spec,
        "Respond with exactly the single word: PONG\n",
        120,
    )
    .expect("spawn claude");

    let elapsed = started.elapsed();
    eprintln!(
        "claude exit={} stdout_len={} stderr_len={} elapsed={:?}",
        outcome.exit_code,
        outcome.stdout.len(),
        outcome.stderr.len(),
        elapsed
    );

    assert_eq!(
        outcome.exit_code, 0,
        "claude failed; stderr: {}",
        outcome.stderr
    );
    let stdout = outcome.stdout.to_uppercase();
    assert!(
        stdout.contains("PONG"),
        "stdout does not echo PONG (stdin not delivered?): {:?}",
        outcome.stdout
    );
}

/// Contract Q2: `--model` flag is honored.
///
/// zforge's `model_args_for_agent` appends `--model <X>` for claude. If the
/// real CLI rejects or ignores the flag, per-phase model routing is broken.
#[test]
#[serial]
fn claude_accepts_model_flag() {
    let Some(_) = claude_available() else { return; };

    let spec = claude_spec(&["--model", "haiku"]);
    let outcome = spawn_agent(&spec, "Reply with only the digit 1.\n", 120)
        .expect("spawn claude");

    assert_eq!(
        outcome.exit_code, 0,
        "claude rejected --model haiku; stderr: {}",
        outcome.stderr
    );
    assert!(
        !outcome.stdout.is_empty(),
        "empty stdout suggests model flag silently dropped or invalid"
    );
}

/// Contract Q3: spawning a non-existent binary returns a typed error (no
/// panic). Regression guard — zforge already enforces this in
/// `spawn_agent` but real-binary tests catch platform-specific surprises.
#[test]
#[serial]
fn claude_missing_binary_returns_error_not_panic() {
    let Some(_) = claude_available() else { return; };

    let spec = AgentSpec {
        command: "/definitely/does/not/exist/zforge_claude_ghost".into(),
        args: vec!["-p".into()],
    };
    let err = spawn_agent(&spec, "prompt", 10).unwrap_err();
    assert!(err.to_string().contains("spawn"), "{err}");
}

/// Contract Q4: large prompt does NOT deadlock the stdin pipe.
///
/// `wait_with_output` reads stdout/stderr after writing all of stdin. If
/// Claude's pipe buffer is smaller than our prompt AND Claude prints output
/// before draining stdin, the write blocks forever. This test feeds ~50KB
/// to surface the bug if present.
#[test]
#[serial]
fn claude_handles_large_prompt_without_pipe_deadlock() {
    let Some(_) = claude_available() else { return; };

    // ~50KB of context + a tiny instruction at the end so the response stays
    // short and cheap.
    let filler = "x".repeat(50_000);
    let prompt = format!(
        "{filler}\n\nIgnore the filler above. Respond with: DONE"
    );

    let spec = claude_spec(&["--model", "haiku"]);
    let started = Instant::now();
    let outcome = spawn_agent(&spec, &prompt, 120).expect("spawn claude");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(120),
        "pipe deadlock suspected: {:?} elapsed",
        elapsed
    );
    assert_eq!(outcome.exit_code, 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stdout.to_uppercase().contains("DONE"),
        "stdout: {:?}",
        outcome.stdout
    );
}

/// Contract Q5: failure case emits an identifiable signature the
/// orchestrator's fallback policy can match.
///
/// Originally this test set `ANTHROPIC_API_KEY` to a bogus value. On
/// macOS/Linux Claude Code stores OAuth credentials separately from the
/// env var, so a bogus key is silently ignored and the call succeeds —
/// the env var is NOT a reliable failure trigger.
///
/// Use an invalid model name instead. Verified live (2026-05-25):
///   - exit code: 1
///   - stdout:    "There's an issue with the selected model (...)..."
///   - stderr:    ""
///
/// IMPORTANT IMPLICATION: Claude prints its failure message to STDOUT, not
/// stderr. `FallbackPolicy.retryable_stderr_patterns` matches stderr only —
/// won't catch this on its own. Either:
///   (a) the orchestrator must also scan stdout, or
///   (b) failure modes that matter (rate limit, quota) emit to stderr.
/// We default to exit-code-based fallback for claude; this test documents
/// the gap.
#[test]
#[serial]
fn claude_failure_emits_identifiable_signature() {
    let Some(_) = claude_available() else { return; };

    let spec = claude_spec(&["--model", "totally-invalid-model-name"]);
    let outcome = spawn_agent(&spec, "Respond with: AUTH\n", 60)
        .expect("spawn must not panic on failure");

    eprintln!(
        "[claude-failure snapshot] exit={} stdout={:?} stderr={:?}",
        outcome.exit_code, outcome.stdout, outcome.stderr
    );

    assert_ne!(outcome.exit_code, 0, "expected non-zero exit on invalid model");
    let combined = format!("{} {}", outcome.stdout, outcome.stderr);
    assert!(
        combined.to_lowercase().contains("model"),
        "failure output should mention `model`; got stdout={:?} stderr={:?}",
        outcome.stdout,
        outcome.stderr
    );
}

/// Contract Q5b: bogus `ANTHROPIC_API_KEY` env does NOT invalidate auth on
/// machines with Claude Code OAuth credentials cached. This locks the
/// surprising behavior so future readers don't waste time on it.
#[test]
#[serial]
fn claude_oauth_credentials_override_env_api_key() {
    let Some(_) = claude_available() else { return; };

    let prev = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-INVALID-FOR-TEST");

    let spec = claude_spec(&["--model", "haiku"]);
    let outcome = spawn_agent(&spec, "Reply with: 1", 60);

    match prev {
        Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
        None => std::env::remove_var("ANTHROPIC_API_KEY"),
    }
    let outcome = outcome.expect("spawn must not panic");
    eprintln!(
        "[oauth-precedence] exit={} stdout_len={}",
        outcome.exit_code,
        outcome.stdout.len()
    );

    // The interesting assertion: bogus env var is IGNORED — call succeeds.
    // If this ever flips (Anthropic changes auth precedence), the change
    // breaks many integration scripts; we want the canary loud.
    assert_eq!(
        outcome.exit_code, 0,
        "OAuth credentials should override bogus ANTHROPIC_API_KEY; \
         if this changes, update integration docs"
    );
}

/// Contract Q6: long-running task does NOT hang past a reasonable wall
/// clock. Currently `spawn_agent` has no timeout — this test documents the
/// max real-world duration so we know whether to add one.
///
/// Asks for a moderately verbose response (200 words). Should finish in
/// well under 60s on `haiku`.
#[test]
#[serial]
fn claude_moderate_response_under_60s() {
    let Some(_) = claude_available() else { return; };

    let spec = claude_spec(&["--model", "haiku"]);
    let started = Instant::now();
    let outcome = spawn_agent(
        &spec,
        "Write exactly 200 words explaining what HTTP is. Keep it concise.\n",
        120,
    )
    .expect("spawn claude");
    let elapsed = started.elapsed();

    eprintln!("[duration] {elapsed:?}");
    assert_eq!(outcome.exit_code, 0, "{}", outcome.stderr);
    assert!(
        elapsed < Duration::from_secs(60),
        "took {elapsed:?} — consider adding spawn timeout"
    );
    // Loose word count check — just to confirm we got real output, not a
    // truncated stub.
    let word_count = outcome.stdout.split_whitespace().count();
    assert!(word_count > 50, "suspiciously short output: {word_count} words");
}
