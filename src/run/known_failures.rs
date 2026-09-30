//! Tolerating pinned known failures in a run's verification (ONBOARD
//! TASK-011; REQ-010, business rule 8): a non-zero exit still verifies when
//! at least one failing test name was read and every one of them is on the
//! handover's pinned known-failure list. Unreadable output — no failing
//! names at all — never passes this way (AC-03): a known list only
//! tolerates named failures, it never turns silence into a pass.
//!
//! Pure and side-effect free so the rules are easy to pin down in tests;
//! `run::execute` is the only caller.

use std::collections::BTreeSet;

/// What a known-failure list makes of one verification's raw result.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tolerance {
    /// The run passes: the test command exited 0, or every failing name it
    /// read was known.
    pub passes: bool,
    /// Failing names the known list did not cover — what still blocks a
    /// pass, and all the agent's feedback should name (AC-02).
    pub unknown: Vec<String>,
    /// Known tests that failed this time.
    pub tolerated: Vec<String>,
    /// Known tests that did not fail this time, or no longer appear in the
    /// output at all — `zforge status` suggests dropping these (AC-04).
    pub known_passing: Vec<String>,
}

/// `raw_passed` is the test command's own verdict (exit 0 and not timed
/// out); `failed_names` is what the runner read off a non-zero exit — empty
/// when it could not parse any.
pub fn evaluate(known: &[String], raw_passed: bool, failed_names: &[String]) -> Tolerance {
    let failed: BTreeSet<&str> = failed_names.iter().map(String::as_str).collect();
    let known_set: BTreeSet<&str> = known.iter().map(String::as_str).collect();
    let unknown: Vec<String> = failed_names
        .iter()
        .filter(|f| !known_set.contains(f.as_str()))
        .cloned()
        .collect();
    let tolerated: Vec<String> = known
        .iter()
        .filter(|k| failed.contains(k.as_str()))
        .cloned()
        .collect();
    let known_passing: Vec<String> = known
        .iter()
        .filter(|k| !failed.contains(k.as_str()))
        .cloned()
        .collect();
    let passes = raw_passed || (!failed_names.is_empty() && unknown.is_empty());
    Tolerance {
        passes,
        unknown,
        tolerated,
        known_passing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// AC-01: every failing name is known — the run passes, tolerating them.
    #[test]
    fn passes_when_every_failure_is_known() {
        let t = evaluate(&v(&["TestA", "TestB"]), false, &v(&["TestA", "TestB"]));
        assert!(t.passes);
        assert_eq!(t.tolerated, v(&["TestA", "TestB"]));
        assert!(t.known_passing.is_empty());
        assert!(t.unknown.is_empty());
    }

    /// AC-02: one failure is not known — the run fails, naming only that one.
    #[test]
    fn fails_naming_only_the_unknown_failure() {
        let t = evaluate(&v(&["TestA"]), false, &v(&["TestA", "TestLogin"]));
        assert!(!t.passes);
        assert_eq!(t.unknown, v(&["TestLogin"]));
        assert_eq!(t.tolerated, v(&["TestA"]));
        assert!(t.known_passing.is_empty());
    }

    /// AC-03: no failing names were read at all — never a pass, known list
    /// or not.
    #[test]
    fn unreadable_output_never_passes() {
        let t = evaluate(&v(&["TestA"]), false, &[]);
        assert!(!t.passes);
        assert!(t.unknown.is_empty());
    }

    /// AC-04: a known test absent from the failures is recorded as
    /// known-passing.
    #[test]
    fn a_recovered_known_test_is_flagged() {
        let t = evaluate(&v(&["TestA", "TestB"]), false, &v(&["TestA"]));
        assert_eq!(t.known_passing, v(&["TestB"]));
        assert_eq!(t.tolerated, v(&["TestA"]));
    }

    /// AC-05: an empty known list never tolerates a failure, and a clean
    /// pass reports nothing — exactly as before.
    #[test]
    fn empty_known_list_changes_nothing() {
        let t = evaluate(&[], false, &v(&["TestA"]));
        assert!(!t.passes);
        assert_eq!(t.unknown, v(&["TestA"]));

        let clean = evaluate(&[], true, &[]);
        assert!(clean.passes);
        assert!(clean.tolerated.is_empty());
        assert!(clean.known_passing.is_empty());
    }

    /// A clean exit tolerates nothing, and every known test counts as
    /// passing this time (rule 8: it either passed, or is gone from the
    /// output — both are reasons to consider dropping it).
    #[test]
    fn a_clean_pass_reports_every_known_test_as_passing() {
        let t = evaluate(&v(&["TestA", "TestB"]), true, &[]);
        assert!(t.passes);
        assert!(t.tolerated.is_empty());
        assert_eq!(t.known_passing, v(&["TestA", "TestB"]));
    }
}
