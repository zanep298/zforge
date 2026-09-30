//! MCP tools for v1.5 intake and runs (decision D4: CLI first, MCP after).
//!
//! The agent prepares and observes; the user decides. There is deliberately
//! **no** `intake_accept`, `intake_revise`, `handover` or change-request
//! acceptance here — those record the user's decision and exist only as CLI
//! commands that require a terminal (D1). `mcp::tool_definitions` is checked
//! against [`FORBIDDEN`] so one cannot be added by accident.
//!
//! Every handler works through the library rather than `cli::*`, whose
//! functions print progress to stdout — the MCP transport's own channel
//! (FIX-003).

use crate::config;
use crate::intake::{self, readiness, review, Intake};
use crate::knowledge::{self, commitments, probe, Knowledge};
use crate::run::{execute, feature, feature_ops, is_handover, ops, record::Run};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Tool names that must never exist: they would let an agent record a
/// decision that is the user's (D1).
pub const FORBIDDEN: [&str; 9] = [
    "intake_accept",
    "intake_revise",
    "handover",
    "change_accept",
    "accept",
    "revise",
    "onboard_accept",
    "onboard_revise",
    "onboard_baseline",
];

fn root() -> Result<PathBuf> {
    let config = config::load().map_err(|_| anyhow!("config not found — run: zf init"))?;
    Ok(config.project_root())
}

fn str_arg(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("missing required argument: {key}"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn string_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// `None` when `name` is not a v1.5 tool.
pub fn dispatch(name: &str, args: &Value) -> Option<Result<String>> {
    let handler = match name {
        "intake_new" => intake_new,
        "intake_task" => intake_task,
        "intake_status" => intake_status,
        "intake_review" => intake_review,
        "intake_diff" => intake_diff,
        "change_new" => change_new,
        "readiness" => readiness_report,
        "knowledge_index" => knowledge_index,
        "onboard_probe" => onboard_probe,
        "onboard_status" => onboard_status,
        "onboard_review" => onboard_review,
        "run_start" => run_start,
        "run_status" => run_status,
        "run_list" => run_list,
        "run_log" => run_log,
        "run_cancel" => run_cancel,
        _ => return None,
    };
    Some(handler(args))
}

fn intake_new(args: &Value) -> Result<String> {
    let id = str_arg(args, "intake_id")?;
    let i = review::create(&root()?, &id)?;
    Ok(format!(
        "created {}; fill 01-outcome.md, then call intake_review",
        i.dir.display()
    ))
}

fn intake_task(args: &Value) -> Result<String> {
    let i = Intake::open(&root()?, &str_arg(args, "intake_id")?)?;
    let path = review::create_task(&i, &str_arg(args, "task_id")?)?;
    Ok(format!("created {}", path.display()))
}

/// Every file with its derived state, open questions and lint issues.
fn intake_status(args: &Value) -> Result<String> {
    let i = Intake::open(&root()?, &str_arg(args, "intake_id")?)?;
    let known = review::known(&i);
    let files: Vec<Value> = review::statuses(&i)?
        .into_iter()
        .map(|status| {
            let text = std::fs::read_to_string(i.dir.join(&status.file)).unwrap_or_default();
            json!({
                "status": status,
                "open_questions": intake::lint::open_questions(&text),
                "issues": intake::lint::lint(&status.file, &text, &i.id, &known),
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&json!({
        "intake": i.id,
        "dir": i.dir,
        "files": files,
        "decides": "the user accepts at a terminal: zforge intake accept <INTAKE> <FILE>",
    }))?)
}

/// Send a file for the user's review (this is not acceptance).
fn intake_review(args: &Value) -> Result<String> {
    let i = Intake::open(&root()?, &str_arg(args, "intake_id")?)?;
    let file = str_arg(args, "file")?;
    let r = review::review(&i, &file)?;
    Ok(serde_json::to_string_pretty(&json!({
        "file": file,
        "revision": r.revision,
        "sha256": r.sha256,
        "already_under_review": r.unchanged,
        "warnings": r.warnings,
        "diff": r.diff,
        "next": format!(
            "tell the user what to look at, then they run: zforge intake accept {} {file}",
            i.id
        ),
    }))?)
}

fn intake_diff(args: &Value) -> Result<String> {
    let i = Intake::open(&root()?, &str_arg(args, "intake_id")?)?;
    let file = str_arg(args, "file")?;
    let st = review::file_status(&i, &file)?;
    let from = args
        .get("from_revision")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .or_else(|| st.accepted.as_ref().map(|a| a.revision));
    let to = args
        .get("to_revision")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .or(Some(st.last_revision))
        .filter(|n| *n > 0);
    match (from, to) {
        (Some(from), Some(to)) if from != to => Ok(review::diff_against(&i, &file, from, to)),
        _ => Ok(format!(
            "no two recorded revisions of {file} to compare (accepted: {:?}, last: {})",
            st.accepted.map(|a| a.revision),
            st.last_revision
        )),
    }
}

/// Scaffold a change request (workflow §8). Only the user acts on it.
fn change_new(args: &Value) -> Result<String> {
    let i = Intake::open(&root()?, &str_arg(args, "intake_id")?)?;
    let id = str_arg(args, "change_id")?;
    intake::validate_id(&id)?;
    let path = i.dir.join(intake::CHANGES_DIR).join(format!("{id}.md"));
    if path.exists() {
        return Err(anyhow!("{} already exists", path.display()));
    }
    std::fs::create_dir_all(i.dir.join(intake::CHANGES_DIR))?;
    crate::fs::write_atomic(&path, intake::templates::change(&i.id, &id).as_bytes())?;
    Ok(format!(
        "created {}; fill every section, then the user decides whether to change the contract",
        path.display()
    ))
}

fn readiness_report(args: &Value) -> Result<String> {
    let root = root()?;
    let i = Intake::open(&root, &str_arg(args, "intake_id")?)?;
    let config = config::load().map_err(|_| anyhow!("config not found — run: zf init"))?;
    let r = readiness::check(&i, &root, &string_list(args, "tasks"), &config)?
        .with(readiness::runtime(&root))
        .with_project(&config);
    Ok(serde_json::to_string_pretty(&r)?)
}

fn knowledge_index(_args: &Value) -> Result<String> {
    Ok(serde_json::to_string_pretty(
        &commitments::build(&root()?)?,
    )?)
}

fn load_config() -> Result<config::Config> {
    config::load().map_err(|_| anyhow!("config not found — run: zf init"))
}

/// Probe the project (no model call): language, docs, baseline test run.
/// Refuses on an uncommitted working tree — the knowledge is pinned to a
/// commit — a real refusal, not a hidden decision (ONBOARD TASK-003).
fn onboard_probe(_args: &Value) -> Result<String> {
    let config = load_config()?;
    Ok(serde_json::to_string_pretty(&probe::run(&config)?)?)
}

/// Every knowledge file's review state, open questions, lint issues (a
/// citation of a knowledge item ID not in the accepted knowledge is one),
/// and every stale or moved citation — plus whether the project counts as
/// onboarded (ONBOARD TASK-003, TASK-004). Read-only: deciding is the
/// user's, at a terminal (`zforge onboard accept|revise|baseline`).
fn onboard_status(_args: &Value) -> Result<String> {
    let config = load_config()?;
    let k = Knowledge::open(&config);
    let state = crate::onboard::state(&config)?;
    let files: Vec<Value> = state
        .files
        .into_iter()
        .map(|status| {
            let text = std::fs::read_to_string(k.dir.join(&status.file)).unwrap_or_default();
            json!({
                "status": status,
                "open_questions": intake::lint::open_questions(&text),
                "issues": knowledge::lint_issues(&status.file, &text),
            })
        })
        .collect();
    let stale = knowledge::stale::check(&k)?;
    Ok(serde_json::to_string_pretty(&json!({
        "onboarded": state.onboarded,
        "baseline": state.baseline,
        "files": files,
        "stale": stale.stale,
        "moved": stale.moved,
        "decides": "the user accepts, revises or records known baseline failures at a terminal: zforge onboard accept|revise|baseline",
    }))?)
}

/// Send a knowledge file's current content for the user's review (this is
/// not acceptance).
fn onboard_review(args: &Value) -> Result<String> {
    let root = root()?;
    let config = load_config()?;
    let k = Knowledge::open(&config);
    let file = str_arg(args, "file")?;
    let r = knowledge::review(&root, &k, &file)?;
    Ok(serde_json::to_string_pretty(&json!({
        "file": file,
        "revision": r.revision,
        "sha256": r.sha256,
        "already_under_review": r.unchanged,
        "warnings": r.warnings,
        "diff": r.diff,
        "next": format!(
            "tell the user what to look at, then they run: zforge onboard accept {file}"
        ),
    }))?)
}

/// Start a run of a handed-over task in the background — or, without a
/// task, the whole handover. The budget the user set in the handover is the
/// cap; a run cannot raise it.
fn run_start(args: &Value) -> Result<String> {
    let root = root()?;
    let handover = str_arg(args, "handover")?;
    let Some(task) = opt_str(args, "task") else {
        let (qualified, pid) = feature_ops::spawn_async(&root, &handover)?;
        return Ok(serde_json::to_string_pretty(&json!({
            "handover": qualified,
            "worker_pid": pid,
            "next": format!("poll run_status with run={qualified}"),
        }))?);
    };
    let run = execute::create(&root, &handover, &task, None)?;
    let meta = run.meta()?;
    let pid = ops::spawn_async(&root, &run)?;
    Ok(serde_json::to_string_pretty(&json!({
        "run": run.id,
        "worker_pid": pid,
        "branch": meta.branch,
        "worktree": meta.worktree,
        "budget_usd": meta.budget_usd,
        "max_iterations": meta.max_iterations,
        "next": format!("poll run_status with run={}", run.id),
    }))?)
}

fn run_status(args: &Value) -> Result<String> {
    let root = root()?;
    let id = str_arg(args, "run")?;
    if is_handover(&id) {
        return Ok(serde_json::to_string_pretty(&feature::load(&root, &id)?)?);
    }
    let run = Run::open(&root, &id)?;
    let state = ops::refresh(&run)?;
    let (traces, _) = crate::trace::log::read(&crate::run::runs_dir(&root), &run.id);
    Ok(serde_json::to_string_pretty(&json!({
        "meta": run.meta()?,
        "state": state,
        "events": run.events()?,
        "traces": traces,
    }))?)
}

fn run_list(args: &Value) -> Result<String> {
    let runs = ops::list(&root()?, opt_str(args, "handover").as_deref())?;
    let rows: Vec<Value> = runs
        .iter()
        .map(|(_, meta, state)| json!({ "meta": meta, "state": state }))
        .collect();
    Ok(serde_json::to_string_pretty(&rows)?)
}

fn run_log(args: &Value) -> Result<String> {
    let root = root()?;
    let id = str_arg(args, "run")?;
    let path = if is_handover(&id) {
        let h = crate::run::contract::load_handover(&root, &id)?;
        feature_ops::dir(&root, &h.intake, &h.manifest.id).join(feature_ops::LOG)
    } else {
        Run::open(&root, &id)?.dir.join(ops::LOG)
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|_| anyhow!("{id} has no log; only a background run writes one"))?;
    let tail = args.get("tail").and_then(Value::as_u64).unwrap_or(200) as usize;
    let lines: Vec<&str> = text.lines().collect();
    Ok(lines[lines.len().saturating_sub(tail)..].join("\n"))
}

fn run_cancel(args: &Value) -> Result<String> {
    let root = root()?;
    let id = str_arg(args, "run")?;
    if is_handover(&id) {
        return Ok(serde_json::to_string_pretty(&feature_ops::cancel(
            &root, &id,
        )?)?);
    }
    let run = Run::open(&root, &id)?;
    let state = ops::cancel(&root, &run)?;
    Ok(format!(
        "{} {}{}",
        run.id,
        state.status.as_str(),
        state
            .reason
            .as_deref()
            .map(|r| format!(" — {r}"))
            .unwrap_or_default()
    ))
}

/// Schemas appended to `tool_definitions`.
pub fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "intake_new",
            "description": "v1.5: create an intake — outcome, behavior, solution, breakdown and a tasks/ folder — for a request that should be clarified before implementation. You fill the files; the user accepts them at a terminal.",
            "inputSchema": { "type": "object", "properties": {
                "intake_id": { "type": "string", "description": "e.g. FEATURE-001" }
            }, "required": ["intake_id"] }
        }),
        json!({
            "name": "intake_task",
            "description": "v1.5: add a leaf task contract (tasks/<TASK_ID>.md) from the template.",
            "inputSchema": { "type": "object", "properties": {
                "intake_id": { "type": "string" },
                "task_id": { "type": "string", "description": "e.g. TASK-001" }
            }, "required": ["intake_id", "task_id"] }
        }),
        json!({
            "name": "intake_status",
            "description": "v1.5: every intake file with its review state (draft, in_review, changed_since_review, needs_revision, accepted), pending drafts, the user's revision notes, unanswered questions and structural issues. Read this before writing anything.",
            "inputSchema": { "type": "object", "properties": {
                "intake_id": { "type": "string" }
            }, "required": ["intake_id"] }
        }),
        json!({
            "name": "intake_review",
            "description": "v1.5: send a file's current content for the user's review as a new revision. Refuses while structural errors remain. This is not acceptance: only the user accepts, at a terminal. Editing the file afterwards voids the review.",
            "inputSchema": { "type": "object", "properties": {
                "intake_id": { "type": "string" },
                "file": { "type": "string", "description": "01-outcome.md … 04-breakdown.md, tasks/TASK-001.md, changes/CHANGE-x.md" }
            }, "required": ["intake_id", "file"] }
        }),
        json!({
            "name": "intake_diff",
            "description": "v1.5: diff two recorded revisions of an intake file (default: the accepted one against the latest).",
            "inputSchema": { "type": "object", "properties": {
                "intake_id": { "type": "string" },
                "file": { "type": "string" },
                "from_revision": { "type": "number" },
                "to_revision": { "type": "number" }
            }, "required": ["intake_id", "file"] }
        }),
        json!({
            "name": "change_new",
            "description": "v1.5: scaffold a change request (workflow §8) when a contract cannot be met as accepted. Fill every section; the user decides. Never edit an accepted contract file to make a run pass.",
            "inputSchema": { "type": "object", "properties": {
                "intake_id": { "type": "string" },
                "change_id": { "type": "string", "description": "e.g. CHANGE-001 or CHANGE-RUN-003" }
            }, "required": ["intake_id", "change_id"] }
        }),
        json!({
            "name": "readiness",
            "description": "v1.5: check whether an intake's accepted revisions are ready to hand over — everything accepted with no pending draft, structure and IDs valid, no open question, every requirement covered, dependencies acyclic and inside the scope, integration verification present, git and a budget. The handover itself is the user's, at a terminal.",
            "inputSchema": { "type": "object", "properties": {
                "intake_id": { "type": "string" },
                "tasks": { "type": "array", "items": { "type": "string" }, "description": "Limit to these tasks (default: all)" }
            }, "required": ["intake_id"] }
        }),
        json!({
            "name": "knowledge_index",
            "description": "v1.5: accepted requirements and binding decisions across all intakes, each with its source revision, whether the decision is still active, and whether it is implemented (handed over, or verified by a run with the candidate it tested).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "onboard_probe",
            "description": "ONBOARD: probe the project without calling a model — language and toolchain, repository size, existing docs, CodeGraph index state, and one run of the test command (pass/fail, failing tests, duration). Refuses on an uncommitted working tree; the knowledge is pinned to a commit.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "onboard_status",
            "description": "ONBOARD: each knowledge file's review state (draft, in_review, changed_since_review, needs_revision, accepted), open questions, structural issues (including a citation of a knowledge item ID not in the accepted knowledge), stale and moved citations, and whether the project counts as onboarded. Read this before drafting or refreshing.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "onboard_review",
            "description": "ONBOARD: send a knowledge file's (domain.md, conventions.md, rules.md) current content for the user's review as a new revision. Refuses while structural errors remain (missing evidence, an ID reused for a different item). This is not acceptance: only the user accepts, revises or records known baseline failures, at a terminal.",
            "inputSchema": { "type": "object", "properties": {
                "file": { "type": "string", "description": "domain.md, conventions.md or rules.md" }
            }, "required": ["file"] }
        }),
        json!({
            "name": "run_start",
            "description": "v1.5: in the background, execute a handed-over leaf task in its own git worktree on its own branch — or, without `task`, every task of the handover in dependency order and then its integration check, continuing where an earlier run stopped. Spends only the budget the user set in the handover, per task. Poll run_status; the work lands on run branches, never on the user's checkout.",
            "inputSchema": { "type": "object", "properties": {
                "handover": { "type": "string", "description": "HANDOVER-001, or <INTAKE>/HANDOVER-001 when ambiguous" },
                "task": { "type": "string", "description": "One task; omit to run the whole handover" }
            }, "required": ["handover"] }
        }),
        json!({
            "name": "run_status",
            "description": "v1.5: a run's state, events (agent calls with cost, verifications with the candidate tested), and the trace of each agent call — or, given a handover, where each of its tasks and its integration check stand. Records a dead worker as interrupted.",
            "inputSchema": { "type": "object", "properties": {
                "run": { "type": "string", "description": "e.g. RUN-001, or HANDOVER-001" }
            }, "required": ["run"] }
        }),
        json!({
            "name": "run_list",
            "description": "v1.5: every run with its task, handover, state and cost.",
            "inputSchema": { "type": "object", "properties": {
                "handover": { "type": "string", "description": "Only runs of this handover" }
            } }
        }),
        json!({
            "name": "run_log",
            "description": "v1.5: the tail of a background run's worker log, or of a background handover's (run=HANDOVER-001).",
            "inputSchema": { "type": "object", "properties": {
                "run": { "type": "string" },
                "tail": { "type": "number", "description": "Lines from the end (default 200)" }
            }, "required": ["run"] }
        }),
        json!({
            "name": "run_cancel",
            "description": "v1.5: stop a run's agent and tests and record it cancelled — or, given a handover, stop its loop and whatever of it is running. Worktrees and branches are kept.",
            "inputSchema": { "type": "object", "properties": {
                "run": { "type": "string" }
            }, "required": ["run"] }
        }),
    ]
}
