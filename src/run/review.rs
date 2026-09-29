//! Reviewing a run's passing work (workflow §7: implement → test → fix →
//! review), when `execution.review` is on.
//!
//! After the suite passes and no protected test changed, a second agent
//! call — the `review` phase, `review-agent` when the worktree has it —
//! reads the change against the contract and ends with `VERDICT: APPROVE`
//! or `VERDICT: CHANGES`. Only an approval lets the run be verified. Asking
//! for changes, giving no verdict, failing, or changing a file (a reviewer
//! may only read) all count as not approved: the findings go back to the
//! coding agent through a `verify_failed`, within the run's iterations and
//! budget. A missing review is missing evidence, never a pass.

use super::execute::{call_agent, Call};
use super::record::RunEvent;
use anyhow::Result;
use chrono::Utc;

/// At most this many findings are kept from one review.
const MAX_FINDINGS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub approved: bool,
    pub findings: Vec<String>,
}

impl Verdict {
    fn refused(finding: impl Into<String>) -> Self {
        Self {
            approved: false,
            findings: vec![finding.into()],
        }
    }
}

/// Review the work in `call.work_dir`, which the tests passed as
/// `candidate`, recording `review_started` and `reviewed`.
pub(super) fn review(call: &Call, candidate: &str) -> Result<Verdict> {
    call.run.append(&RunEvent::ReviewStarted {
        at: Utc::now(),
        allotted_usd: call.left_usd,
    })?;
    eprintln!(
        "{}: reviewing the passing work (up to ${:.2})",
        call.run.id, call.left_usd
    );
    let called = call_agent(call)?;
    let (out, cost) = (&called.out, called.cost_usd);
    let mut verdict = if out.timed_out {
        Verdict::refused(format!("the review timed out after {}s", call.timeout_secs))
    } else if out.exit_code != 0 {
        Verdict::refused(called.exit_reason("the review agent"))
    } else {
        parse(&answer(&out.stdout))
    };
    let now = crate::evidence::fingerprint(call.work_dir);
    if now.hash() != Some(candidate) {
        verdict = Verdict::refused(
            "the reviewer changed files in the worktree; its review does not count",
        );
    }
    call.run.append(&RunEvent::Reviewed {
        at: Utc::now(),
        approved: verdict.approved,
        allotted_usd: call.left_usd,
        cost_usd: cost,
        findings: verdict.findings.clone(),
    })?;
    Ok(verdict)
}

/// What the agent answered: the `result` of a `stream-json` run, or the
/// whole output when it is not one.
pub fn answer(stdout: &str) -> String {
    stdout
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .find(|v| v["type"] == "result")
        .and_then(|v| v["result"].as_str().map(String::from))
        .unwrap_or_else(|| stdout.to_string())
}

/// The verdict in an answer: its last `VERDICT:` line, with the `- `
/// findings before it. No verdict is not an approval.
pub fn parse(answer: &str) -> Verdict {
    let lines: Vec<&str> = answer.lines().map(str::trim).collect();
    let Some(at) = lines
        .iter()
        .rposition(|l| l.to_ascii_uppercase().starts_with("VERDICT:"))
    else {
        return Verdict::refused("the review gave no verdict");
    };
    let word = lines[at]["VERDICT:".len()..].trim().to_ascii_uppercase();
    let findings: Vec<String> = lines[..at]
        .iter()
        .filter_map(|l| l.strip_prefix("- "))
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .take(MAX_FINDINGS)
        .collect();
    match word.as_str() {
        "APPROVE" => Verdict {
            approved: true,
            findings,
        },
        "CHANGES" if !findings.is_empty() => Verdict {
            approved: false,
            findings,
        },
        "CHANGES" => Verdict::refused("the review asked for changes without naming any"),
        other => Verdict::refused(format!("the review gave an unknown verdict `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approval_keeps_its_notes() {
        let v = parse("Looks right.\n- src/a.rs: fine\n\nVERDICT: APPROVE\n");
        assert!(v.approved);
        assert_eq!(v.findings, ["src/a.rs: fine"]);
    }

    #[test]
    fn changes_carry_the_findings() {
        let v =
            parse("- src/a.rs: AC-02 has no test\n- src/b.rs: debug print left\nVERDICT: CHANGES");
        assert!(!v.approved);
        assert_eq!(
            v.findings,
            ["src/a.rs: AC-02 has no test", "src/b.rs: debug print left"]
        );
    }

    #[test]
    fn no_verdict_or_an_empty_one_is_not_an_approval() {
        assert_eq!(
            parse("All good!"),
            Verdict::refused("the review gave no verdict")
        );
        assert_eq!(
            parse("VERDICT: CHANGES"),
            Verdict::refused("the review asked for changes without naming any")
        );
        assert!(!parse("VERDICT: maybe").approved);
        // The last verdict line decides.
        assert!(parse("VERDICT: APPROVE\n- a: wrong\nVERDICT: CHANGES").findings == ["a: wrong"]);
    }

    #[test]
    fn the_answer_is_the_streams_result() {
        let stream = concat!(
            r#"{"type":"system","subtype":"init"}"#,
            "\n",
            r#"{"type":"result","subtype":"success","result":"- x: y\nVERDICT: CHANGES"}"#,
            "\n"
        );
        assert_eq!(answer(stream), "- x: y\nVERDICT: CHANGES");
        assert_eq!(
            answer("plain text\nVERDICT: APPROVE"),
            "plain text\nVERDICT: APPROVE"
        );
    }
}
