//! `zforge run compare <RUN> <RUN>…` — the same facts for each run, side by
//! side, so a change to agents, skills or prompts can be judged on work
//! already done: what the run cost and how often it went back, what the
//! reviewer found, what its prompt carried from the project's knowledge,
//! what the agent ran and read, and what its output changed against the
//! commit the whole task started from. With `--check <cmd>` the command
//! (a linter, say) also runs on each output in a throwaway worktree.
//!
//! Everything is read from the run records, traces and git; nothing is
//! written except the throwaway worktrees, which are removed.

use super::git;
use super::record::{Run, RunEvent, RunMeta, RunState};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// How long `--check` may run on one output.
const CHECK_TIMEOUT: Duration = Duration::from_secs(1800);

/// What the agent's shell commands are taken to be, by substring.
const COMMAND_KINDS: &[(&str, &[&str])] = &[
    (
        "test",
        &[
            "go test",
            "cargo test",
            "pytest",
            "npm test",
            "npm run test",
            "yarn test",
            "make test",
            "jest",
            "vitest",
            "flutter test",
            "./test.sh",
            "xcodebuild test",
            "gradlew test",
        ],
    ),
    (
        "lint",
        &[
            "golangci-lint",
            "make lint",
            "clippy",
            "eslint",
            "ruff",
            "flake8",
            "go vet",
            "staticcheck",
            "swiftlint",
            "ktlint",
            "detekt",
            "flutter analyze",
        ],
    ),
    (
        "format",
        &[
            "gofmt",
            "go fmt",
            "goimports",
            "cargo fmt",
            "rustfmt",
            "prettier",
            "black ",
            "swiftformat",
            "dart format",
        ],
    ),
    (
        "generate",
        &[
            "go generate",
            "make gen",
            "make swag",
            "swag init",
            "migrate-and-gen",
            "buf generate",
            "protoc",
            "mockery",
            "mockgen",
            "build_runner",
        ],
    ),
    (
        "knowledge",
        &[
            "zforge knowledge get",
            "INDEX.md",
            "conventions.md",
            "rules.md",
            "domain.md",
        ],
    ),
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunFacts {
    pub id: String,
    pub intake: String,
    pub handover: String,
    pub task: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reused_from: Option<String>,
    pub cost_usd: f64,
    pub attempts: u32,
    pub verify_failed: u32,
    pub reviews: Vec<ReviewFacts>,
    pub agent: AgentFacts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffFacts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<CheckFacts>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewFacts {
    pub approved: bool,
    pub findings: Vec<String>,
}

/// From the code phase's traces, every attempt summed.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct AgentFacts {
    pub calls: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub skills: Vec<String>,
    /// Knowledge items the prompt carried, by prefix (`CONV`, `RULE`, `DOM`).
    pub knowledge: BTreeMap<String, usize>,
    pub tool_calls: BTreeMap<String, u32>,
    /// Shell commands by kind (`COMMAND_KINDS`); traces written before
    /// commands were recorded have none.
    pub commands: BTreeMap<String, u32>,
    pub files_read: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiffFacts {
    /// The commit the whole task started from.
    pub base: String,
    /// The run's output.
    pub commit: String,
    pub files: Vec<FileChange>,
    pub insertions: u64,
    pub deletions: u64,
    /// Changed files whose new content says it is generated.
    pub generated: Vec<String>,
    /// Test files added or changed.
    pub tests: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileChange {
    /// `A`, `M`, `D` or `R`.
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckFacts {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// The output's last lines.
    pub tail: Vec<String>,
}

/// The facts of each run, in the order given.
pub fn compare(project_root: &Path, ids: &[String], check: Option<&str>) -> Result<Vec<RunFacts>> {
    ids.iter()
        .map(|id| facts(project_root, id, check))
        .collect()
}

fn facts(project_root: &Path, id: &str, check: Option<&str>) -> Result<RunFacts> {
    let run = Run::open(project_root, id)?;
    let meta = run.meta()?;
    let state = run.state()?;
    let events = run.events()?;
    let (traces, _) = crate::trace::log::read(&super::runs_dir(project_root), &run.id);

    let reviews = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::Reviewed {
                approved, findings, ..
            } => Some(ReviewFacts {
                approved: *approved,
                findings: findings.clone(),
            }),
            _ => None,
        })
        .collect();
    let verify_failed = events
        .iter()
        .filter(|e| matches!(e, RunEvent::VerifyFailed { .. }))
        .count() as u32;

    let mut agent = AgentFacts::default();
    for t in traces.iter().filter(|t| t.phase == "code") {
        agent.calls += 1;
        agent.model = agent.model.or_else(|| t.expected.model.clone());
        if agent.skills.is_empty() {
            agent.skills = t.expected.skills.clone();
        }
        if agent.knowledge.is_empty() {
            for item in &t.expected.knowledge_items {
                let prefix = item.split('-').next().unwrap_or(item).to_string();
                *agent.knowledge.entry(prefix).or_default() += 1;
            }
        }
        if let Some(o) = &t.observed {
            for (tool, n) in &o.tool_calls {
                *agent.tool_calls.entry(tool.clone()).or_default() += n;
            }
            for c in &o.commands {
                for kind in command_kinds(c) {
                    *agent.commands.entry(kind.to_string()).or_default() += 1;
                }
            }
            agent.files_read += o.files_read.len();
        }
    }

    let diff = match output_and_base(project_root, &meta, &state)? {
        Some((base, commit)) => Some(diff_facts(project_root, &base, &commit)?),
        None => None,
    };
    let check = match (check, &diff) {
        (Some(cmd), Some(d)) => Some(run_check(project_root, &run.id, &d.commit, cmd)?),
        _ => None,
    };

    Ok(RunFacts {
        id: run.id.clone(),
        intake: meta.intake.clone(),
        handover: meta.handover.clone(),
        task: meta.task.clone(),
        status: state.status.as_str().to_string(),
        reason: state.reason.clone(),
        reused_from: state.reused_from.clone(),
        cost_usd: state.cost_usd,
        attempts: state.attempts,
        verify_failed,
        reviews,
        agent,
        diff,
        check,
    })
}

/// The kinds a shell command counts as.
pub fn command_kinds(command: &str) -> Vec<&'static str> {
    COMMAND_KINDS
        .iter()
        .filter(|(_, needles)| needles.iter().any(|n| command.contains(n)))
        .map(|(kind, _)| *kind)
        .collect()
}

/// `(base, output)` of a run that has an output: the base is where the
/// whole task's work starts — a run continuing an earlier output, or one
/// reusing another's, is followed back to the first run of the chain.
fn output_and_base(
    project_root: &Path,
    meta: &RunMeta,
    state: &RunState,
) -> Result<Option<(String, String)>> {
    let Some(commit) = state.output.clone() else {
        return Ok(None);
    };
    let mut meta = meta.clone();
    if let Some(from) = &state.reused_from {
        let run = from.rsplit('/').next().unwrap_or(from);
        meta = Run::open(project_root, run)?.meta()?;
    }
    while let Some(p) = meta.start.as_ref().and_then(|s| s.previous.clone()) {
        meta = Run::open(project_root, &p.run)?.meta()?;
    }
    Ok(Some((meta.start_commit().to_string(), commit)))
}

fn diff_facts(project_root: &Path, base: &str, commit: &str) -> Result<DiffFacts> {
    let names = git::ok(project_root, &["diff", "--name-status", "-M", base, commit])
        .with_context(|| format!("diff {base}..{commit}"))?;
    let files: Vec<FileChange> = names
        .lines()
        .filter_map(|l| {
            let status = l.chars().next()?.to_string();
            let path = l.rsplit('\t').next()?.to_string();
            Some(FileChange { status, path })
        })
        .collect();
    let (mut insertions, mut deletions) = (0, 0);
    for l in git::ok(project_root, &["diff", "--numstat", "-M", base, commit])?.lines() {
        let mut cols = l.split('\t');
        insertions += cols.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        deletions += cols.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
    }
    let generated = files
        .iter()
        .filter(|f| f.status != "D")
        .filter(|f| is_generated(project_root, commit, &f.path))
        .map(|f| f.path.clone())
        .collect();
    let tests = files
        .iter()
        .filter(|f| f.status != "D" && is_test_path(&f.path))
        .map(|f| f.path.clone())
        .collect();
    Ok(DiffFacts {
        base: base.to_string(),
        commit: commit.to_string(),
        files,
        insertions,
        deletions,
        generated,
        tests,
    })
}

/// The file's head, as of `commit`, carries the conventional generated
/// marker (`Code generated … DO NOT EDIT`, or `@generated`).
fn is_generated(project_root: &Path, commit: &str, path: &str) -> bool {
    let Ok(out) = git::run(project_root, &["show", &format!("{commit}:{path}")]) else {
        return false;
    };
    let head = String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(2048)]).into_owned();
    (head.contains("Code generated") && head.contains("DO NOT EDIT")) || head.contains("@generated")
}

/// A path that conventionally holds tests.
pub fn is_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || name.starts_with("test_")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("Tests.swift")
        || name.ends_with("Test.kt")
        || name.ends_with("Test.java")
        || name.ends_with("_test.dart")
        || path.starts_with("tests/")
        || path.starts_with("test/")
        || path.contains("/tests/")
        || path.contains("/test/")
}

/// Run `command` (`sh -c`) on `commit` in a throwaway detached worktree.
fn run_check(project_root: &Path, run: &str, commit: &str, command: &str) -> Result<CheckFacts> {
    let dir = super::worktree::path_for(project_root, &format!("compare-{run}"));
    let dir_s = dir.to_string_lossy().into_owned();
    if dir.exists() {
        let _ = git::run(project_root, &["worktree", "remove", "--force", &dir_s]);
    }
    git::ok(
        project_root,
        &["worktree", "add", "--detach", &dir_s, commit],
    )
    .with_context(|| format!("check out {commit} to run the check"))?;
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(&dir);
    let out = crate::process::run_bounded(cmd, None, CHECK_TIMEOUT);
    let _ = git::run(project_root, &["worktree", "remove", "--force", &dir_s]);
    let _ = git::run(project_root, &["worktree", "prune"]);
    let out = out.with_context(|| format!("run `{command}`"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(15)..]
        .iter()
        .map(|l| l.to_string())
        .collect();
    Ok(CheckFacts {
        command: command.to_string(),
        exit_code: out.status.and_then(|s| s.code()),
        timed_out: out.timed_out,
        tail,
    })
}

/// The facts side by side, one column per run, then what differs in the
/// files they touched and what the reviewers found.
pub fn render(runs: &[RunFacts]) -> String {
    let mut rows: Vec<(String, Vec<String>)> = Vec::new();
    let mut row = |label: &str, f: &dyn Fn(&RunFacts) -> String| {
        rows.push((label.to_string(), runs.iter().map(f).collect()));
    };
    row("run", &|r| r.id.clone());
    row("task", &|r| format!("{}/{}", r.handover, r.task));
    row("status", &|r| match &r.reused_from {
        Some(from) => format!("{} (reused {from})", r.status),
        None => r.status.clone(),
    });
    row("cost", &|r| format!("${:.2}", r.cost_usd));
    row("attempts", &|r| r.attempts.to_string());
    row("verify failed", &|r| r.verify_failed.to_string());
    row("reviews", &|r| {
        if r.reviews.is_empty() {
            return "—".into();
        }
        r.reviews
            .iter()
            .map(|v| match v.approved {
                true => "APPROVE".to_string(),
                false => format!("CHANGES({})", v.findings.len()),
            })
            .collect::<Vec<_>>()
            .join(" ")
    });
    row("model", &|r| {
        r.agent.model.clone().unwrap_or_else(|| "—".into())
    });
    row("knowledge in prompt", &|r| counts(&r.agent.knowledge));
    row("tool calls", &|r| counts(&r.agent.tool_calls));
    // Traces written before commands and reads were recorded have the
    // tool counts but neither list.
    let calls = |r: &RunFacts, tool: &str| r.agent.tool_calls.get(tool).copied().unwrap_or(0);
    row(
        "commands run",
        &|r| match r.agent.commands.is_empty() && calls(r, "Bash") > 0 {
            true => "not recorded (older trace)".into(),
            false => counts(&r.agent.commands),
        },
    );
    row(
        "files read",
        &|r| match r.agent.files_read == 0 && calls(r, "Read") > 0 {
            true => "not recorded (older trace)".into(),
            false => r.agent.files_read.to_string(),
        },
    );
    row("files changed", &|r| {
        r.diff
            .as_ref()
            .map(|d| format!("{} (+{} −{})", d.files.len(), d.insertions, d.deletions))
            .unwrap_or_else(|| "—".into())
    });
    row("tests changed", &|r| {
        r.diff
            .as_ref()
            .map(|d| d.tests.len().to_string())
            .unwrap_or_else(|| "—".into())
    });
    row("generated touched", &|r| {
        r.diff
            .as_ref()
            .map(|d| d.generated.len().to_string())
            .unwrap_or_else(|| "—".into())
    });
    row("check", &|r| match &r.check {
        Some(c) if c.timed_out => "timed out".into(),
        Some(c) => format!("exit {}", c.exit_code.map_or("?".into(), |n| n.to_string())),
        None => "—".into(),
    });

    let label_w = rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);
    let col_w: Vec<usize> = (0..runs.len())
        .map(|i| {
            rows.iter()
                .map(|(_, v)| v[i].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for (label, values) in &rows {
        let mut line = format!("{label:<label_w$}");
        for (v, w) in values.iter().zip(&col_w) {
            line.push_str(&format!("  {v:<w$}"));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }

    for r in runs {
        let Some(d) = &r.diff else { continue };
        let others: Vec<&DiffFacts> = runs
            .iter()
            .filter(|o| o.id != r.id)
            .filter_map(|o| o.diff.as_ref())
            .collect();
        let only: Vec<&FileChange> = d
            .files
            .iter()
            .filter(|f| {
                !others
                    .iter()
                    .all(|o| o.files.iter().any(|g| g.path == f.path))
            })
            .collect();
        out.push_str(&format!(
            "\n{} — {} file(s) from {} to {}{}\n",
            r.id,
            d.files.len(),
            short(&d.base),
            short(&d.commit),
            if others.is_empty() {
                String::new()
            } else {
                format!(", {} not changed by every other run", only.len())
            }
        ));
        let list = if others.is_empty() {
            d.files.iter().collect()
        } else {
            only
        };
        for f in list {
            let mut tags = Vec::new();
            if d.tests.contains(&f.path) {
                tags.push("test");
            }
            if d.generated.contains(&f.path) {
                tags.push("generated");
            }
            let tags = if tags.is_empty() {
                String::new()
            } else {
                format!("  [{}]", tags.join(", "))
            };
            out.push_str(&format!("  {} {}{tags}\n", f.status, f.path));
        }
    }

    for r in runs {
        let findings: Vec<&String> = r.reviews.iter().flat_map(|v| &v.findings).collect();
        if !findings.is_empty() {
            out.push_str(&format!("\n{} — review findings\n", r.id));
            for f in findings {
                out.push_str(&format!("  {f}\n"));
            }
        }
        if let Some(c) = &r.check {
            out.push_str(&format!("\n{} — `{}`\n", r.id, c.command));
            for l in &c.tail {
                out.push_str(&format!("  {l}\n"));
            }
        }
    }
    out
}

fn counts<V: std::fmt::Display>(m: &BTreeMap<String, V>) -> String {
    if m.is_empty() {
        return "—".into();
    }
    m.iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_sorted_into_kinds() {
        assert_eq!(command_kinds("go test ./internal/..."), ["test"]);
        assert_eq!(command_kinds("make lint"), ["lint"]);
        assert_eq!(command_kinds("gofmt -l ."), ["format"]);
        assert_eq!(command_kinds("make migrate-and-gen"), ["generate"]);
        assert_eq!(
            command_kinds("zforge knowledge get CONV-001"),
            ["knowledge"]
        );
        assert!(command_kinds("ls -la").is_empty());
    }

    #[test]
    fn test_paths_are_recognised() {
        for p in [
            "internal/module/user/service/otp_test.go",
            "tests/run_test.rs",
            "test/otp/helpers_test.go",
            "web/src/a.spec.ts",
            "app/test_models.py",
        ] {
            assert!(is_test_path(p), "{p}");
        }
        for p in [
            "internal/module/user/service/otp.go",
            "src/lib.rs",
            "testdata.go",
        ] {
            assert!(!is_test_path(p), "{p}");
        }
    }
}
