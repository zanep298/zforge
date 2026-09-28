//! Executing one run (MOC-B TASK-004; REQ-004, REQ-005, REQ-006).
//!
//! [`create`] records a run for a handed-over task — refusing when the
//! task's budget in that handover is used up — and [`execute`] carries it
//! out: worktree from where the task starts (the baseline, or its
//! dependencies' outputs — `start`), then the v1 verifier loop
//! (`orchestrator::verifier_loop::iterate`, flow `Contract`) with the agent
//! and the test suite running in the worktree, never in zforge's own cwd.
//!
//! Every step is an event in the run's log (`record`): each agent call with
//! its cost and trace, each verification with the candidate it tested, and
//! how the run ended. The agent is Claude (`claude` in the registry), run
//! headless in the run's worktree, with `--max-budget-usd` set to what is
//! left of the budget; an attempt whose cost is unknown counts its whole
//! allotment as spent.

use super::contract::{self, Contract};
use super::record::{self, Run, RunEvent, RunKind, RunMeta, RunState, RunStatus};
use super::worktree;
use crate::cli::verify::VerifyOutcome;
use crate::config::Config;
use crate::orchestrator::agent_args::{named_agent_for, NamedAgent};
use crate::orchestrator::{headless_args, model_args, spawn, trace_record};
use crate::registry::schema::AgentSpec;
use crate::state::{Flow, State, TaskState};
use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// The only runner for leaf tasks in Mốc B.
pub const RUNNER: &str = "claude";
/// Below this, no agent call is made: `--max-budget-usd` works in cents.
const MIN_ALLOTMENT_USD: f64 = 0.01;
/// Lines of test output handed back to the agent after a failed verification.
const FEEDBACK_LINES: usize = 60;
/// Test suite time limit, as `zforge verify`'s default.
const TEST_TIMEOUT_SECS: u64 = 300;

/// Why a run stopped before the verifier loop finished.
#[derive(Debug)]
enum Halt {
    Blocked(String),
    Failed(String),
    Cancelled(String),
}

impl std::fmt::Display for Halt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Halt::Blocked(r) | Halt::Failed(r) | Halt::Cancelled(r) => f.write_str(r),
        }
    }
}

impl std::error::Error for Halt {}

fn load_config(project_root: &Path) -> Result<Config> {
    crate::config::load_from(&project_root.join(".zforge").join("config.yaml"))
}

/// What earlier runs of `task` in handover `handover` of `intake` spent.
pub fn spent_before(project_root: &Path, intake: &str, handover: &str, task: &str) -> Result<f64> {
    let mut spent = 0.0;
    for run in record::list(project_root)? {
        let meta = run.meta()?;
        if meta.kind == RunKind::Task
            && meta.intake == intake
            && meta.handover == handover
            && meta.task == task
        {
            spent += run.state()?.cost_usd;
        }
    }
    Ok(spent)
}

/// Record a new run of `task` from handover `handover_id`. The contract is
/// loaded and verified first, so a tampered snapshot or a dependency
/// without a verified output creates nothing; so does a used-up budget.
pub fn create(
    project_root: &Path,
    handover_id: &str,
    task: &str,
    retry_of: Option<&str>,
) -> Result<Run> {
    let c = contract::load(project_root, handover_id, task)?;
    // A dependency without an output refuses the task before anything is
    // created (MOC-C REQ-003).
    let start = super::start::plan(project_root, &c.intake, &c.manifest.id, task, &c.depends_on)?;
    let spent = spent_before(project_root, &c.intake, &c.manifest.id, task)?;
    let left = c.manifest.policy.budget_usd - spent;
    if left < MIN_ALLOTMENT_USD {
        bail!(
            "budget of {task} in {} is used up (${spent:.2} of ${:.2}); hand over again to continue",
            c.manifest.id,
            c.manifest.policy.budget_usd
        );
    }
    let baseline = c.manifest.baseline.commit.clone();
    // One run of a task at a time in a handover (MOC-C REQ-008).
    let _claim =
        super::feature_ops::claim(project_root, &c.intake, &c.manifest.id, task, RunKind::Task)?;
    record::create(project_root, |id| RunMeta {
        id: id.to_string(),
        created_at: Utc::now(),
        intake: c.intake.clone(),
        handover: c.manifest.id.clone(),
        task: task.to_string(),
        manifest_sha256: c.manifest_sha256.clone(),
        worktree: worktree::path_for(project_root, id),
        branch: worktree::branch_name(task, id),
        baseline_commit: baseline.clone(),
        budget_usd: left,
        max_iterations: c.manifest.policy.max_iterations.max(1),
        retry_of: retry_of.map(str::to_string),
        start: start.clone(),
        kind: Default::default(),
        checks: None,
    })
}

/// Carry out a created run to its end. Returns the final state; a run that
/// could not start (worktree, contract changed) ends `failed` like any other.
pub fn execute(project_root: &Path, run: &Run) -> Result<RunState> {
    let meta = run.meta()?;
    if meta.kind == RunKind::Integration {
        return super::integrate::execute(project_root, run);
    }
    run.append(&RunEvent::Started {
        at: Utc::now(),
        pid: std::process::id(),
    })?;
    match prepare(project_root, &meta) {
        Ok((contract, config, spec, timeout)) => {
            let outcome = work(project_root, run, &meta, &contract, &config, &spec, timeout);
            finish(run, outcome)
        }
        Err(e) => run.append(&RunEvent::Failed {
            at: Utc::now(),
            reason: format!("{e:#}"),
        }),
    }
}

/// Everything checked before the first agent call; any error fails the run.
fn prepare(project_root: &Path, meta: &RunMeta) -> Result<(Contract, Config, AgentSpec, u64)> {
    let contract = contract::load(
        project_root,
        &format!("{}/{}", meta.intake, meta.handover),
        &meta.task,
    )?;
    if contract.manifest_sha256 != meta.manifest_sha256 {
        bail!("{} changed after the run was created", meta.handover);
    }
    let config = load_config(project_root)?;
    if config.project.test_command.trim().is_empty() {
        bail!("no test command configured (`project.test_command`); a run cannot verify");
    }
    let registry = crate::registry::io::load()?;
    let spec = registry
        .resolved_agent(RUNNER, project_root)
        .ok_or_else(|| {
            anyhow!("runner `{RUNNER}` is not in the registry; leaf tasks run on Claude")
        })?;
    let timeout = registry.fallback_policy.spawn_timeout_secs;
    let wt = worktree::create(project_root, &meta.id, &meta.task, meta.start_commit())?;
    if let Some(start) = &meta.start {
        super::start::apply(&wt.path, &meta.id, start)?;
    }
    report_claude_config(
        &meta.id,
        &super::agent_env::bring_claude_config(project_root, &wt.path)?,
    );
    Ok((contract, config, spec, timeout))
}

/// Say what the agent will find of Claude's project configuration.
fn report_claude_config(run: &str, brought: &[super::agent_env::Brought]) {
    use super::agent_env::Brought;
    for b in brought {
        match b {
            Brought::Copied(dir) => eprintln!("{run}: brought {dir} from the main checkout"),
            Brought::Present(_) => {}
            Brought::NotIgnored(dir) => eprintln!(
                "warning: {run}: {dir} is neither committed nor ignored, so the run's agent \
                 does not get it; commit it, or ignore it to let runs bring it"
            ),
        }
    }
}

/// The verifier loop over the worktree.
fn work(
    project_root: &Path,
    run: &Run,
    meta: &RunMeta,
    contract: &Contract,
    config: &Config,
    base: &AgentSpec,
    timeout_secs: u64,
) -> Result<crate::orchestrator::LoopOutcome> {
    let work_dir = meta.worktree.clone();
    let tasks_dir = run.dir.join("task");
    let _task_lock = crate::state::lock_task(&tasks_dir, &run.id)?;
    TaskState::new_with_flow(&run.id, Flow::Contract).save(&tasks_dir)?;
    // Absolute and in the main checkout: the worktree's `.zforge/intakes/`
    // is a committed copy the runtime never reads.
    let change_request = project_root
        .join(".zforge/intakes")
        .join(&meta.intake)
        .join("changes")
        .join(format!("CHANGE-{}.md", run.id))
        .display()
        .to_string();
    let last_output = RefCell::new(String::new());
    let checklists = super::agent_env::checklists(config);
    // Where the agent starts: protected tests must still be as they are here
    // when the suite passes (`guard`).
    let start = super::git::head(&work_dir)?;
    let guard = super::guard::Guard::new(
        config.execution.protected_tests.as_deref(),
        &config.project.test_command,
        &contract.tests_may_change,
        &work_dir,
    );

    let code = |n: u32, prev: Option<&VerifyOutcome>| -> Result<()> {
        interrupted()?;
        let state = run.state()?;
        let left = meta.budget_usd - state.cost_usd;
        if left < MIN_ALLOTMENT_USD {
            return Err(Halt::Blocked(format!(
                "budget: ${:.2} of ${:.2} used",
                state.cost_usd, meta.budget_usd
            ))
            .into());
        }
        let feedback = prev.map(|p| feedback_text(p, &last_output.borrow()));
        let prompt = contract.prompt(
            feedback.as_deref(),
            &change_request,
            &checklists,
            &config.project.test_command,
        );
        let (spec, named) = agent_spec(base, project_root, &work_dir, left);
        eprintln!("{}: attempt {n} (up to ${left:.2})", run.id);
        run.append(&RunEvent::AttemptStarted {
            at: Utc::now(),
            n,
            allotted_usd: left,
        })?;
        let out = spawn::spawn_agent_in(&spec, &prompt, timeout_secs, &work_dir)?;

        let trace = crate::trace::from_invocation(crate::trace::Invocation {
            task_id: &run.id,
            phase: "code",
            attempt: n,
            runner: RUNNER,
            command: std::iter::once(spec.command.clone())
                .chain(spec.args.iter().cloned())
                .collect(),
            expected: trace_record::expected_for(
                RUNNER,
                "code",
                &work_dir,
                &named,
                model_for(project_root).as_deref(),
                Some(&config.project.language),
            ),
            stdout: &out.stdout,
            stderr: &out.stderr,
            exit_code: out.exit_code,
            timed_out: out.timed_out,
            duration_ms: out.duration_ms,
        });
        let cost = trace
            .observed
            .as_ref()
            .and_then(|o| o.result.as_ref())
            .and_then(|r| r.cost_usd);
        if let Err(e) = crate::trace::log::append(&super::runs_dir(project_root), &trace) {
            eprintln!("warning: trace not recorded: {e:#}");
        }
        run.append(&RunEvent::Attempt {
            at: Utc::now(),
            n,
            exit_code: out.exit_code,
            allotted_usd: left,
            cost_usd: cost,
        })?;
        interrupted()?;
        // §8: the agent may only ask for the contract to change, never
        // change it. A change request ends the run; the user decides.
        if let Some(reason) = amendment_requested(&change_request, &meta.intake) {
            return Err(Halt::Blocked(reason).into());
        }
        if out.timed_out {
            return Err(Halt::Failed(format!("agent timed out after {timeout_secs}s")).into());
        }
        if out.exit_code != 0 {
            return Err(Halt::Failed(format!("agent exited with {}", out.exit_code)).into());
        }
        advance(&tasks_dir, &run.id, State::Coded);
        Ok(())
    };

    let verify = || -> Result<VerifyOutcome> {
        interrupted()?;
        run.append(&RunEvent::VerifyStarted { at: Utc::now() })?;
        eprintln!("{}: verifying — {}", run.id, config.project.test_command);
        let result = crate::runner::run_with_language(
            &config.project.test_command,
            &work_dir,
            TEST_TIMEOUT_SECS,
            &config.project.language,
        );
        interrupted()?;
        let result = result?;
        let candidate = crate::evidence::fingerprint(&work_dir)
            .hash()
            .map(String::from);
        *last_output.borrow_mut() = result.raw_output.clone();
        let outcome = VerifyOutcome {
            passed: result.passed && !result.timed_out,
            total_tests: result.total_tests,
            passed_tests: result.passed_tests,
            failed_tests: result.failed_tests,
            failed_names: result.failed_names.clone(),
            timed_out: result.timed_out,
        };
        match (outcome.passed, candidate) {
            (true, Some(candidate)) => {
                let changed = guard
                    .changed(&work_dir, &start)
                    .map_err(|e| Halt::Failed(format!("could not check the tests: {e:#}")))?;
                if !changed.is_empty() {
                    return protected_tests_changed(
                        run,
                        &last_output,
                        outcome,
                        candidate,
                        &changed,
                    );
                }
                // The output a dependent task starts from: the tested tree,
                // sealed before anything else can touch the worktree.
                let message = format!("zforge: output of {} ({})", run.id, meta.task);
                let commit = super::output::seal(&work_dir, &message, &candidate).map_err(|e| {
                    Halt::Failed(format!("tests passed but the output was not sealed: {e:#}"))
                })?;
                run.append(&RunEvent::Verified {
                    at: Utc::now(),
                    candidate,
                    commit: Some(commit),
                })?;
                advance(&tasks_dir, &run.id, State::Verified);
            }
            (true, None) => {
                return Err(Halt::Failed(
                    "tests passed but the worktree could not be fingerprinted; not verified".into(),
                )
                .into())
            }
            (false, candidate) => {
                let mut failed_tests = result.failed_names.clone();
                if result.timed_out {
                    failed_tests.push(format!("(test suite timed out after {TEST_TIMEOUT_SECS}s)"));
                }
                run.append(&RunEvent::VerifyFailed {
                    at: Utc::now(),
                    candidate,
                    failed_tests,
                })?;
            }
        }
        Ok(outcome)
    };

    crate::orchestrator::iterate(meta.max_iterations, code, verify)
}

/// Record how the loop ended. A caught signal ends the run `cancelled` and
/// the process with the conventional 128 + signal exit.
fn finish(run: &Run, outcome: Result<crate::orchestrator::LoopOutcome>) -> Result<RunState> {
    let state = match outcome {
        Ok(_) => run.state()?,
        Err(e) => {
            let event = match e.downcast_ref::<Halt>() {
                Some(Halt::Blocked(r)) => RunEvent::Blocked {
                    at: Utc::now(),
                    reason: r.clone(),
                },
                Some(Halt::Cancelled(r)) => RunEvent::Cancelled {
                    at: Utc::now(),
                    reason: r.clone(),
                },
                Some(Halt::Failed(r)) => RunEvent::Failed {
                    at: Utc::now(),
                    reason: r.clone(),
                },
                None => RunEvent::Failed {
                    at: Utc::now(),
                    reason: format!("{e:#}"),
                },
            };
            run.append(&event)?
        }
    };
    Ok(state)
}

/// The reason to block when the agent wrote a change request, with what the
/// linter makes of it (workflow §8): an incomplete request is still a stop,
/// but says what is missing.
fn amendment_requested(path: &str, intake: &str) -> Option<String> {
    let path = Path::new(path);
    let text = std::fs::read_to_string(path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let id = name.trim_end_matches(".md");
    let issues = crate::intake::lint::lint(
        &format!("{}{name}", crate::intake::lint::CHANGES_PREFIX),
        &text,
        intake,
        &crate::intake::lint::Known::default(),
    );
    let missing: Vec<&str> = issues
        .iter()
        .filter(|i| i.severity == crate::intake::lint::Severity::Error)
        .map(|i| i.message.as_str())
        .collect();
    Some(match missing.is_empty() {
        true => format!("amendment: {id}"),
        false => format!("amendment: {id} (incomplete — {})", missing.join("; ")),
    })
}

/// The suite passed, but only after protected tests changed: not a pass.
/// The agent is told which, and may restore them within its iterations.
fn protected_tests_changed(
    run: &Run,
    last_output: &RefCell<String>,
    outcome: VerifyOutcome,
    candidate: String,
    changed: &[String],
) -> Result<VerifyOutcome> {
    let failed_tests: Vec<String> = changed
        .iter()
        .map(|p| format!("protected test changed: {p}"))
        .collect();
    run.append(&RunEvent::VerifyFailed {
        at: Utc::now(),
        candidate: Some(candidate),
        failed_tests: failed_tests.clone(),
    })?;
    *last_output.borrow_mut() = format!(
        "The tests passed, but these test files differ from where this task started:\n{}\n\n\
         Restore them. A task may not change existing tests unless its contract lists \
         them under `tests_may_change`; if the contract has to change, write a change \
         request instead.",
        changed
            .iter()
            .map(|p| format!("- {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(VerifyOutcome {
        passed: false,
        failed_tests: failed_tests.len(),
        failed_names: failed_tests,
        ..outcome
    })
}

/// A caught SIGINT/SIGTERM/SIGHUP stops the run (see `process::catch_interrupts`).
fn interrupted() -> Result<()> {
    match crate::process::take_interrupt() {
        Some(sig) => Err(Halt::Cancelled(format!("interrupted by signal {sig}")).into()),
        None => Ok(()),
    }
}

/// Registered Claude spec, run headless in the worktree: permission bypass,
/// the phase's named agent when the worktree has one, the configured code
/// model, and the budget left.
fn agent_spec(
    base: &AgentSpec,
    project_root: &Path,
    work_dir: &Path,
    left_usd: f64,
) -> (AgentSpec, NamedAgent) {
    let mut spec = base.clone();
    let mut args = headless_args::headless_args_for_agent(RUNNER);
    args.append(&mut spec.args);
    let named = named_agent_for(RUNNER, "code", work_dir);
    args.extend(named.args());
    if let Some(model) = model_for(project_root) {
        args.extend(model_args::model_args_for_agent(RUNNER, &model));
    }
    args.push("--max-budget-usd".into());
    args.push(format!("{left_usd:.2}"));
    spec.args = args;
    (spec, named)
}

fn model_for(project_root: &Path) -> Option<String> {
    crate::config::load_models_from_root(project_root)?
        .for_assistant(RUNNER, "code")
        .map(str::to_string)
}

fn feedback_text(prev: &VerifyOutcome, output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let tail = lines[lines.len().saturating_sub(FEEDBACK_LINES)..].join("\n");
    let names = if prev.failed_names.is_empty() {
        "(no test names parsed)".to_string()
    } else {
        prev.failed_names.join(", ")
    };
    format!(
        "Failing: {names}{}\n\nLast {FEEDBACK_LINES} lines of the test output:\n\n```\n{tail}\n```",
        if prev.timed_out {
            " (the suite timed out)"
        } else {
            ""
        }
    )
}

/// Move the run's v1 task state along its `Contract` flow (best-effort: it
/// mirrors the run log, which stays the source of truth).
fn advance(tasks_dir: &Path, run_id: &str, target: State) {
    let Ok(mut ts) = TaskState::load(tasks_dir, run_id) else {
        return;
    };
    if ts.state < target && ts.advance(target, "zforge run").is_ok() {
        let _ = ts.save(tasks_dir);
    }
}

/// Create and execute in one go (foreground `zforge run`).
pub fn run(project_root: &Path, handover_id: &str, task: &str) -> Result<(Run, RunState)> {
    let run = create(project_root, handover_id, task, None)?;
    let state = execute(project_root, &run)?;
    Ok((run, state))
}

/// Worktree path of a run, for display.
pub fn worktree_of(run: &Run) -> Result<PathBuf> {
    Ok(run.meta()?.worktree)
}

impl RunStatus {
    /// Process exit code for a finished foreground run.
    pub fn exit_code(self) -> i32 {
        match self {
            RunStatus::Verified => 0,
            _ => 1,
        }
    }
}
