//! Verifier-driven retry loop. SWE-bench-style: regenerate code on failed
//! tests, feeding the failure back into the next code attempt as context.
//!
//! Pure control flow — knows nothing about how code is generated or how tests
//! are run. Callers inject both via closures so this module is unit-testable
//! without spawning real agents.

use crate::cli::verify::VerifyOutcome;
use anyhow::{anyhow, Result};

/// Result of a completed verifier loop. `iterations` is 1-based: 1 means
/// "passed on the first attempt", `max_iterations` means "passed on the
/// final allowed attempt".
#[derive(Debug, Clone)]
pub struct LoopOutcome {
    pub iterations: u32,
    pub final_verify: VerifyOutcome,
}

/// Drive the code → verify → (if fail) code → ... cycle.
///
/// `code_runner(attempt, prev_failure)` writes/edits source files. When
/// `prev_failure` is `Some`, the runner should inject that failure as
/// feedback into the next code prompt so the agent knows what to fix.
///
/// `verify_runner()` runs the test suite and returns a `VerifyOutcome`.
///
/// Returns the first passing outcome along with the iteration count. Bails
/// when `max_iterations` is exhausted without a pass — the final failure
/// outcome is included in the error message so the caller (or operator)
/// can see which tests held out.
pub fn iterate<C, V>(
    max_iterations: u32,
    mut code_runner: C,
    mut verify_runner: V,
) -> Result<LoopOutcome>
where
    C: FnMut(u32, Option<&VerifyOutcome>) -> Result<()>,
    V: FnMut() -> Result<VerifyOutcome>,
{
    if max_iterations == 0 {
        return Err(anyhow!("max_iterations must be >= 1"));
    }

    let mut prev_failure: Option<VerifyOutcome> = None;
    for attempt in 1..=max_iterations {
        code_runner(attempt, prev_failure.as_ref())?;
        let outcome = verify_runner()?;
        if outcome.passed {
            return Ok(LoopOutcome {
                iterations: attempt,
                final_verify: outcome,
            });
        }
        if attempt == max_iterations {
            return Err(anyhow!(
                "verifier budget exhausted after {} iteration(s); {} test(s) still failing: {}",
                max_iterations,
                outcome.failed_tests,
                outcome.failed_names.join(", "),
            ));
        }
        prev_failure = Some(outcome);
    }
    unreachable!("loop returned without exit; range was 1..={max_iterations}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn outcome(passed: bool, failed_names: &[&str]) -> VerifyOutcome {
        VerifyOutcome {
            passed,
            total_tests: 10,
            passed_tests: if passed { 10 } else { 10 - failed_names.len() },
            failed_tests: failed_names.len(),
            failed_names: failed_names.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn passes_on_first_attempt() {
        let code_calls = RefCell::new(Vec::new());
        let verify_calls = RefCell::new(0);

        let result = iterate(
            3,
            |attempt, prev| {
                code_calls.borrow_mut().push((attempt, prev.cloned()));
                Ok(())
            },
            || {
                *verify_calls.borrow_mut() += 1;
                Ok(outcome(true, &[]))
            },
        )
        .unwrap();

        assert_eq!(result.iterations, 1);
        assert_eq!(code_calls.borrow().len(), 1);
        // First attempt has no prior failure feedback.
        assert!(code_calls.borrow()[0].1.is_none());
        assert_eq!(*verify_calls.borrow(), 1);
    }

    #[test]
    fn passes_on_second_attempt_with_feedback() {
        let code_calls = RefCell::new(Vec::new());
        let verify_calls = RefCell::new(0);

        let result = iterate(
            3,
            |attempt, prev| {
                code_calls.borrow_mut().push((attempt, prev.cloned()));
                Ok(())
            },
            || {
                let mut v = verify_calls.borrow_mut();
                *v += 1;
                if *v == 1 {
                    Ok(outcome(false, &["test_a", "test_b"]))
                } else {
                    Ok(outcome(true, &[]))
                }
            },
        )
        .unwrap();

        assert_eq!(result.iterations, 2);
        let calls = code_calls.borrow();
        assert_eq!(calls.len(), 2);
        // Second attempt receives the first failure as feedback.
        let prev = calls[1].1.as_ref().expect("retry must carry prev failure");
        assert_eq!(prev.failed_names, vec!["test_a", "test_b"]);
    }

    #[test]
    fn exhausts_budget_when_never_passes() {
        let code_calls = RefCell::new(0);

        let err = iterate(
            2,
            |_, _| {
                *code_calls.borrow_mut() += 1;
                Ok(())
            },
            || Ok(outcome(false, &["stubborn_test"])),
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("budget exhausted"), "{msg}");
        assert!(msg.contains("stubborn_test"), "{msg}");
        // Code ran exactly max_iterations times — no off-by-one extra spawn.
        assert_eq!(*code_calls.borrow(), 2);
    }

    #[test]
    fn rejects_zero_max_iterations() {
        let err = iterate::<_, _>(
            0,
            |_, _| Ok(()),
            || Ok(outcome(true, &[])),
        )
        .unwrap_err();
        assert!(err.to_string().contains("max_iterations"));
    }

    #[test]
    fn code_runner_error_propagates_immediately() {
        let verify_calls = RefCell::new(0);
        let err = iterate(
            3,
            |_, _| Err(anyhow!("agent crashed")),
            || {
                *verify_calls.borrow_mut() += 1;
                Ok(outcome(true, &[]))
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("agent crashed"));
        // Verify must not run when code generation itself failed.
        assert_eq!(*verify_calls.borrow(), 0);
    }

    #[test]
    fn code_template_renders_feedback_section_when_failed_tests_set() {
        use crate::prompt::{render_template, PromptContext};

        let tmpl = include_str!("../../templates/code.tmpl");

        // No failure → feedback block hidden.
        let ctx_clean = PromptContext {
            task_id: "T1".into(),
            language: "rust".into(),
            test_command: "cargo test".into(),
            ..Default::default()
        };
        let clean = render_template(tmpl, &ctx_clean);
        assert!(!clean.contains("Verifier Feedback"));
        assert!(!clean.contains("Failing tests:"));

        // With failure → feedback block fires.
        let ctx_retry = PromptContext {
            task_id: "T1".into(),
            language: "rust".into(),
            test_command: "cargo test".into(),
            failed_tests: "auth::test_login\nauth::test_logout".into(),
            verify_ref: "/file .zforge/tasks/T1/verify.md".into(),
            ..Default::default()
        };
        let retry = render_template(tmpl, &ctx_retry);
        assert!(retry.contains("Verifier Feedback"));
        assert!(retry.contains("auth::test_login"));
        assert!(retry.contains("auth::test_logout"));
        assert!(retry.contains("/file .zforge/tasks/T1/verify.md"));
    }

    #[test]
    fn feedback_chains_across_attempts() {
        let code_calls = RefCell::new(Vec::new());

        let _ = iterate(
            3,
            |attempt, prev| {
                code_calls
                    .borrow_mut()
                    .push((attempt, prev.map(|p| p.failed_names.clone())));
                Ok(())
            },
            || Ok(outcome(false, &["a", "b"])),
        );

        let calls = code_calls.borrow();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].1.is_none(), "attempt 1: no prior");
        assert!(calls[1].1.is_some(), "attempt 2: has feedback");
        assert!(calls[2].1.is_some(), "attempt 3: has feedback");
    }
}
