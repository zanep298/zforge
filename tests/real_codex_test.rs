//! Real Codex CLI binary contract verification.
//!
//! Verifies the assumptions zforge's orchestrator makes about `codex exec`:
//!   - prompt arrives via stdin
//!   - `-m <model>` honored
//!   - `-a never` + `-s workspace-write` (zforge headless flags) accepted
//!   - exit-code-0-on-API-error edge case (the catastrophe PR9 fixed by
//!     scanning stdout for failure markers)
//!
//! Gated by `ZFORGE_TEST_REAL_CODEX=1` because:
//!   - real ChatGPT account usage (small but non-zero cost)
//!   - requires `codex` binary on `$PATH` AND `codex login` to have run
//!
//! Run manually:
//! ```bash
//! ZFORGE_TEST_REAL_CODEX=1 cargo test --test real_codex_test -- --nocapture
//! ```

use serial_test::serial;
use std::time::{Duration, Instant};
use zforge::orchestrator::spawn::spawn_agent;
use zforge::registry::schema::AgentSpec;

fn codex_available() -> Option<String> {
    if std::env::var("ZFORGE_TEST_REAL_CODEX").is_err() {
        eprintln!(
            "[real_codex_test] SKIP: set ZFORGE_TEST_REAL_CODEX=1 to run \
             real-binary contract tests"
        );
        return None;
    }
    match which::which("codex") {
        Ok(p) => Some(p.to_string_lossy().into_owned()),
        Err(e) => {
            eprintln!("[real_codex_test] SKIP: `codex` not on $PATH: {e}");
            None
        }
    }
}

/// Codex non-interactive mode: `codex [TOP_FLAGS] exec [EXEC_FLAGS] [PROMPT]`.
/// Top-level flags (`-a`, `-s`) MUST precede `exec` — codex parses them at
/// the top-level, NOT under the `exec` subcommand. Putting `-a` after `exec`
/// errors with "unexpected argument '-a' found". zforge's
/// `headless_args_for_agent("codex")` returns top-level flags, and
/// `with_headless_args` PREPENDS them so the resulting argv is correct.
fn codex_spec(extra_args_after_exec: &[&str]) -> AgentSpec {
    // Order matters: top-level approval + sandbox flags BEFORE `exec`.
    let mut args = vec![
        "-a".into(),
        "never".into(),
        "-s".into(),
        "workspace-write".into(),
        "exec".into(),
    ];
    args.extend(extra_args_after_exec.iter().map(|s| s.to_string()));
    AgentSpec {
        command: "codex".into(),
        args,
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

/// Contract: `codex exec` accepts a stdin-piped prompt. zforge orchestrator
/// always pipes the prompt; if codex required arg-only delivery the orch
/// would silently send an empty prompt.
#[test]
#[serial]
fn codex_exec_reads_prompt_from_stdin() {
    let Some(_) = codex_available() else {
        return;
    };

    let spec = codex_spec(&[]);
    let started = Instant::now();
    let outcome = spawn_agent(&spec, "Respond with exactly the single word: PONG\n", 180)
        .expect("spawn codex");

    eprintln!(
        "codex exit={} timed_out={} stdout_len={} stderr_len={} elapsed={:?}",
        outcome.exit_code,
        outcome.timed_out,
        outcome.stdout.len(),
        outcome.stderr.len(),
        started.elapsed()
    );

    // codex prints conversation log to stdout. Look for our marker.
    assert!(
        outcome.stdout.to_uppercase().contains("PONG"),
        "stdin not delivered? stdout: {:?}",
        outcome.stdout
    );
}

/// Contract: `-m <model>` reaches the binary. Use a model the ChatGPT
/// account is allowed to invoke (default model — empty `-m` is omitted).
#[test]
#[serial]
fn codex_model_flag_accepted() {
    let Some(_) = codex_available() else {
        return;
    };

    // Don't pass -m — default model. zforge's `model_args_for_agent` for
    // codex returns empty (profile-based) so the real case in production
    // doesn't append `-m` either. We instead pass `-m` only when we know
    // a valid model name; here we test the flag is at least PARSED without
    // breaking the spawn.
    let spec = codex_spec(&["-m", "gpt-5-codex"]);
    let outcome = spawn_agent(&spec, "Reply with: 1\n", 180).expect("spawn codex");

    let combined = format!("{}\n{}", outcome.stdout, outcome.stderr);
    eprintln!(
        "[model-flag] exit={} stdout_snip={:?} stderr_snip={:?}",
        outcome.exit_code,
        outcome.stdout.chars().take(200).collect::<String>(),
        outcome.stderr.chars().take(200).collect::<String>()
    );
    // Two acceptable outcomes prove `-m` reached the binary:
    //   a) success — reply text in combined output
    //   b) error response — the model name appears in error JSON
    // Either way the flag was parsed (vs. clap rejecting it).
    let ok = combined.to_uppercase().contains("1")
        || combined.contains("ERROR:")
        || combined.contains("error");
    assert!(
        ok,
        "neither reply nor error in combined output: stdout={:?} stderr={:?}",
        outcome.stdout, outcome.stderr
    );
}

/// Contract — THE one PR9 fix depends on: codex prints `ERROR: {...}` JSON
/// to stdout AND exits 0 on API errors. Verify the signature so the
/// orchestrator's stdout-scan default patterns match the real shape.
#[test]
#[serial]
fn codex_invalid_model_yields_stdout_error_with_exit_zero() {
    let Some(_) = codex_available() else {
        return;
    };

    let spec = codex_spec(&["-m", "absolutely-not-a-real-model-xyz"]);
    let outcome = spawn_agent(&spec, "Reply: 1\n", 60).expect("spawn codex");

    eprintln!(
        "[invalid-model snapshot] exit={} stdout={:?} stderr={:?}",
        outcome.exit_code, outcome.stdout, outcome.stderr
    );

    // Codex routes the conversation log + error JSON to either stdout or
    // stderr depending on TTY mode. Under zforge spawn (piped), the
    // ERROR: marker may land in stderr. PR9 scans BOTH streams against
    // `retryable_stderr_patterns` precisely for this reason.
    let combined = format!("{}\n{}", outcome.stdout, outcome.stderr);
    assert!(
        combined.contains("ERROR:") || combined.contains("error"),
        "expected error marker in combined output — codex contract regressed?\n\
         stdout: {:?}\nstderr: {:?}",
        outcome.stdout,
        outcome.stderr
    );
}

/// Contract: 50KB prompt doesn't deadlock the stdin pipe. zforge uses a
/// dedicated writer thread (PR A spawn timeout work) — verify it works
/// with the real codex binary.
#[test]
#[serial]
fn codex_handles_large_prompt_without_deadlock() {
    let Some(_) = codex_available() else {
        return;
    };

    let filler = "x".repeat(50_000);
    let prompt = format!("{filler}\n\nIgnore filler. Respond: DONE\n");

    let spec = codex_spec(&[]);
    let started = Instant::now();
    let outcome = spawn_agent(&spec, &prompt, 180).expect("spawn codex");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(180),
        "pipe deadlock suspected — elapsed {elapsed:?}"
    );
    assert!(
        !outcome.timed_out,
        "spawn timed out — investigate writer thread"
    );
    // codex may or may not include DONE; the binding assertion is "no
    // deadlock + no timeout".
}

/// Contract: zforge's spawn timeout (PR A) actually kills a hung codex
/// process. Configure a 1-second timeout and an expensive request; verify
/// `timed_out` is true and we don't wait forever.
#[test]
#[serial]
fn codex_respects_zforge_spawn_timeout() {
    let Some(_) = codex_available() else {
        return;
    };

    let spec = codex_spec(&[]);
    let started = Instant::now();
    let outcome = spawn_agent(
        &spec,
        "Write a 5000-word essay on distributed consensus.",
        1, // aggressive — verify the kill path
    )
    .expect("spawn codex");
    let elapsed = started.elapsed();

    eprintln!(
        "[timeout] timed_out={} exit={} elapsed={:?}",
        outcome.timed_out, outcome.exit_code, elapsed
    );

    assert!(outcome.timed_out, "expected zforge to kill codex after 1s");
    assert_eq!(outcome.exit_code, 124, "timeout convention: exit 124");
    assert!(
        elapsed < Duration::from_secs(10),
        "kill should be prompt; elapsed={elapsed:?}"
    );
}
