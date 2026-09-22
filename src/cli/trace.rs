//! `zforge trace <ID> [--json]` — follow a task from requirement to the
//! code it verified (IMP-006).
//!
//! Joins the task's phase traces (`trace.jsonl`: what each agent run loaded
//! and did) with its verification history (`verify-history.jsonl`: which
//! candidate each run tested) and the current evidence status, so a reviewer
//! can answer "which runner, agent, skills and tools produced this, and was
//! the code that shipped the code that passed?".

use crate::config;
use crate::evidence::{history::VerifyRecord, EvidenceStatus};
use crate::trace::{log, FindingKind, PhaseTrace};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::fmt::Write as _;

#[derive(Debug, Serialize)]
pub struct TraceReport {
    pub task_id: String,
    pub title: Option<String>,
    pub flow: String,
    pub state: String,
    pub phases: Vec<PhaseTrace>,
    /// `trace.jsonl` lines that could not be read.
    #[serde(skip_serializing_if = "is_zero")]
    pub unreadable_lines: usize,
    pub verifications: Vec<VerifyRecord>,
    /// `current`, `unbound` or `invalid`, with the reason when not current.
    pub evidence: EvidenceView,
}

#[derive(Debug, Serialize)]
pub struct EvidenceView {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

pub fn run(task_id: &str, json: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();
    let state = crate::state::TaskState::load(&tasks_dir, task_id)?;
    let (phases, unreadable_lines) = log::read(&tasks_dir, task_id);
    let evidence = match crate::evidence::current_status(
        &tasks_dir.join(task_id).join("verify.md"),
        &config.project_root(),
        &config.project.test_command,
    ) {
        EvidenceStatus::Current => EvidenceView {
            status: "current",
            reason: None,
        },
        EvidenceStatus::Unbound { reason } => EvidenceView {
            status: "unbound",
            reason: Some(reason),
        },
        EvidenceStatus::Invalid { reason } => EvidenceView {
            status: "invalid",
            reason: Some(reason),
        },
    };
    let report = TraceReport {
        task_id: task_id.to_string(),
        title: title(&tasks_dir.join(task_id).join("task.md")),
        flow: format!("{:?}", state.flow),
        state: format!("{:?}", state.state),
        phases,
        unreadable_lines,
        verifications: crate::evidence::history::read(&tasks_dir, task_id),
        evidence,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render(&report));
    }
    Ok(())
}

/// `title` from `task.md`'s frontmatter (how `task import` writes it), or
/// its first Markdown heading.
fn title(task_md: &std::path::Path) -> Option<String> {
    let md = crate::fs::reader::MarkdownFile::read(task_md).ok()?;
    if let Some(t) = md.get_str("title").filter(|t| !t.trim().is_empty()) {
        return Some(t.trim().to_string());
    }
    md.body
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
}

pub fn render(r: &TraceReport) -> String {
    let mut out = String::new();
    let title = r.title.as_deref().unwrap_or("(no title)");
    let _ = writeln!(out, "{} — {title}", r.task_id);
    let _ = writeln!(out, "flow {}, state {}\n", r.flow, r.state);

    if r.phases.is_empty() {
        let _ = writeln!(out, "no agent runs traced yet");
    }
    for p in &r.phases {
        render_phase(&mut out, p);
    }
    if r.unreadable_lines > 0 {
        let _ = writeln!(
            out,
            "({} unreadable trace line(s) skipped)",
            r.unreadable_lines
        );
    }

    let _ = writeln!(out, "verification");
    if r.verifications.is_empty() {
        let _ = writeln!(out, "  not run");
    }
    for (i, v) in r.verifications.iter().enumerate() {
        let verdict = match (v.passed, v.timed_out) {
            (_, true) => "timed out".to_string(),
            (true, _) => format!("passed ({} tests)", v.total_tests),
            (false, _) => format!("failed ({}/{} tests)", v.failed_tests, v.total_tests),
        };
        let candidate = v
            .candidate
            .as_deref()
            .map_or("no fingerprint".to_string(), |c| {
                format!("candidate {}", &c[..c.len().min(12)])
            });
        let _ = writeln!(out, "  run {}  {verdict}  {candidate}  {}", i + 1, v.ran_at);
    }
    let _ = write!(out, "  evidence: {}", r.evidence.status);
    if let Some(reason) = &r.evidence.reason {
        let _ = write!(out, " — {reason}");
    }
    out.push('\n');
    out
}

/// One phase attempt as text, for other commands' reports.
pub fn render_phase_text(p: &PhaseTrace) -> String {
    let mut out = String::new();
    render_phase(&mut out, p);
    out
}

fn render_phase(out: &mut String, p: &PhaseTrace) {
    let _ = writeln!(out, "{} · attempt {} · {}", p.phase, p.attempt, p.runner);
    match &p.observed {
        None => {
            let why = p.unavailable.as_deref().unwrap_or("no trace");
            let _ = writeln!(out, "  no trace: {why}");
        }
        Some(o) => {
            let version = o.client_version.as_deref().unwrap_or("?");
            let model = o.model.as_deref().unwrap_or("?");
            let expected_model = p.expected.model.as_deref().unwrap_or("default");
            let _ = writeln!(
                out,
                "  client {version}, model {model} (configured {expected_model})"
            );
            if let Some(agent) = &p.expected.named_agent {
                let _ = writeln!(out, "  agent {agent}");
            }
            if !p.expected.skills.is_empty() {
                let _ = writeln!(out, "  preload skills: {}", p.expected.skills.join(", "));
            }
            if !o.mcp_servers.is_empty() {
                let servers: Vec<String> = o
                    .mcp_servers
                    .iter()
                    .map(|m| format!("{} {}", m.name, m.status))
                    .collect();
                let _ = writeln!(out, "  mcp: {}", servers.join(", "));
            }
            let tools: Vec<String> = o
                .tool_calls
                .iter()
                .map(|(name, n)| format!("{name}×{n}"))
                .collect();
            let tools = if tools.is_empty() {
                "none".to_string()
            } else {
                tools.join(" ")
            };
            let _ = writeln!(out, "  tools: {tools}");
            if !o.skill_calls.is_empty() {
                let _ = writeln!(out, "  skills called: {}", o.skill_calls.join(", "));
            }
            if !o.subagents.is_empty() {
                let _ = writeln!(out, "  subagents: {}", o.subagents.join(", "));
            }
            match &o.result {
                Some(res) => {
                    let _ = writeln!(
                        out,
                        "  result: {}, {} turn(s), {}, {}",
                        res.subtype,
                        res.num_turns.map_or("?".into(), |n| n.to_string()),
                        res.duration_ms
                            .map_or("?".into(), |ms| format!("{:.1}s", ms as f64 / 1000.0)),
                        res.cost_usd.map_or("cost ?".into(), |c| format!("${c:.2}")),
                    );
                }
                None => {
                    let _ = writeln!(out, "  result: none");
                }
            }
        }
    }
    for f in &p.findings {
        let mark = if f.kind == FindingKind::AgentChoice {
            "?"
        } else {
            "!"
        };
        let _ = writeln!(out, "  {mark} {}: {}", f.kind.label(), f.message);
    }
    for n in &p.not_observable {
        let _ = writeln!(out, "  · not observable: {n}");
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Expected, Invocation};

    const STREAM: &str = include_str!("../../tests/fixtures/claude/stream_code_agent.jsonl");

    fn phase() -> PhaseTrace {
        crate::trace::from_invocation(Invocation {
            task_id: "T1",
            phase: "code",
            attempt: 1,
            runner: "claude",
            command: vec!["claude".into()],
            expected: Expected {
                named_agent: Some("code-agent".into()),
                model: Some("haiku".into()),
                skills: vec!["zforge-write-tests-first".into()],
                mcp_servers: vec!["codegraph".into()],
            },
            stdout: STREAM,
            stderr: "",
            exit_code: 0,
            timed_out: false,
            duration_ms: 1,
        })
    }

    fn report(verifications: Vec<VerifyRecord>, evidence: EvidenceView) -> TraceReport {
        TraceReport {
            task_id: "T1".into(),
            title: Some("Fix add".into()),
            flow: "Fixbug".into(),
            state: "Verified".into(),
            phases: vec![
                phase(),
                crate::trace::uncaptured("T1", "spec", "claude", Expected::default()),
            ],
            unreadable_lines: 0,
            verifications,
            evidence,
        }
    }

    #[test]
    fn renders_the_chain_from_task_to_verified_candidate() {
        let text = render(&report(
            vec![VerifyRecord {
                ran_at: "2026-09-22T10:00:00Z".into(),
                passed: true,
                timed_out: false,
                total_tests: 3,
                failed_tests: 0,
                command: "sh test.sh".into(),
                candidate: Some("0123456789abcdef0123".into()),
            }],
            EvidenceView {
                status: "current",
                reason: None,
            },
        ));
        for want in [
            "T1 — Fix add",
            "code · attempt 1 · claude",
            "client 2.1.278, model claude-haiku-4-5-20251001 (configured haiku)",
            "agent code-agent",
            "preload skills: zforge-write-tests-first",
            "mcp: codegraph connected",
            "mcp__codegraph__codegraph_search×1",
            "result: success, 9 turn(s)",
            "! infrastructure: 2 tool call(s) refused",
            "· not observable:",
            "spec · attempt 1 · claude",
            "no trace: interactive run",
            "run 1  passed (3 tests)  candidate 0123456789ab",
            "evidence: current",
        ] {
            assert!(text.contains(want), "missing {want:?} in:\n{text}");
        }
    }

    #[test]
    fn title_comes_from_frontmatter_or_the_first_heading() {
        let tmp = tempfile::tempdir().unwrap();
        let fm = tmp.path().join("a.md");
        std::fs::write(&fm, "---\ntitle: Fix add\ndomain: calc\n---\n\n# Task\n").unwrap();
        assert_eq!(title(&fm).as_deref(), Some("Fix add"));
        let heading = tmp.path().join("b.md");
        std::fs::write(&heading, "# Add docs\n").unwrap();
        assert_eq!(title(&heading).as_deref(), Some("Add docs"));
    }

    #[test]
    fn says_when_nothing_ran_or_evidence_is_stale() {
        let mut r = report(
            vec![],
            EvidenceView {
                status: "invalid",
                reason: Some("the code changed after it was verified".into()),
            },
        );
        r.phases.clear();
        let text = render(&r);
        assert!(text.contains("no agent runs traced yet"));
        assert!(text.contains("not run"));
        assert!(text.contains("evidence: invalid — the code changed after it was verified"));
    }
}
