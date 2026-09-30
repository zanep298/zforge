//! Per-run trace of what an agent actually loaded and did (IMP-006).
//!
//! Configuration says what should happen; a trace records what the client
//! reported while it happened. Each agent call of a run appends one
//! [`PhaseTrace`] to `<run>/trace.jsonl`, which `zforge run status <RUN>`
//! shows next to the run's events.
//!
//! Only Claude's stream is parsed today; any other runner gets a record
//! whose `unavailable` says there is no trace, never an empty "all good".

pub mod claude;
pub mod log;
pub mod schema;
pub mod text;

pub use schema::{
    Expected, Finding, FindingKind, KnowledgeFile, McpServer, Observed, PhaseTrace, RunResult,
};

use chrono::Utc;

/// One captured invocation, as the orchestrator saw it.
pub struct Invocation<'a> {
    pub task_id: &'a str,
    pub phase: &'a str,
    pub attempt: u32,
    pub runner: &'a str,
    pub command: Vec<String>,
    pub expected: Expected,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub exit_code: i32,
    pub timed_out: bool,
    pub duration_ms: u128,
}

/// Build the trace record for a captured invocation.
pub fn from_invocation(inv: Invocation<'_>) -> PhaseTrace {
    let mut record = PhaseTrace {
        timestamp: Utc::now(),
        task_id: inv.task_id.to_string(),
        phase: inv.phase.to_string(),
        attempt: inv.attempt,
        runner: inv.runner.to_string(),
        command: inv.command,
        expected: inv.expected,
        observed: None,
        unavailable: None,
        exit_code: Some(inv.exit_code),
        timed_out: inv.timed_out,
        duration_ms: Some(inv.duration_ms),
        findings: Vec::new(),
        not_observable: Vec::new(),
    };
    if inv.runner != "claude" {
        record.unavailable = Some(format!(
            "runner `{}` exposes no trace zforge can read",
            inv.runner
        ));
        return record;
    }
    let analysis = claude::analyze(inv.stdout, inv.stderr, &record.expected);
    record.observed = analysis.observed;
    record.unavailable = analysis.unavailable;
    record.findings = analysis.findings;
    record.not_observable = claude::NOT_OBSERVABLE
        .iter()
        .map(|s| s.to_string())
        .collect();
    if inv.timed_out {
        record.findings.push(Finding::new(
            FindingKind::Run,
            "zforge killed the run at the spawn timeout",
        ));
    }
    record
}

/// A run zforge could not capture (streamed to the terminal).
pub fn uncaptured(task_id: &str, phase: &str, runner: &str, expected: Expected) -> PhaseTrace {
    PhaseTrace {
        timestamp: Utc::now(),
        task_id: task_id.to_string(),
        phase: phase.to_string(),
        attempt: 1,
        runner: runner.to_string(),
        command: Vec::new(),
        expected,
        observed: None,
        unavailable: Some(
            "interactive run: output went to the terminal and was not captured; \
             import the task with `--agent <runner>` or run it without a TTY to trace it"
                .into(),
        ),
        exit_code: None,
        timed_out: false,
        duration_ms: None,
        findings: Vec::new(),
        not_observable: Vec::new(),
    }
}
