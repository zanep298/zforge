use crate::cli;
use crate::cli::dispatch_helper::run_phase_for_task;
use crate::cli::flow_guard;
use crate::config;
use crate::prompt::{build_context_for_phase, PromptPhase};
use crate::state::{dispatch_command, Flow, State, TaskState};
use anyhow::Result;
use colored::Colorize;

/// FSM gate for the `ship` command. Returns `Err` when the task has not yet
/// reached the state immediately preceding `Coded` in its flow (e.g. for
/// the full flow that is `PlanReviewed`; for fixbug it is `TestspecDone`).
pub(crate) fn check_ship_gate(flow: &Flow, state: &State, task_id: &str) -> Result<()> {
    if !flow.contains(&State::Coded) {
        anyhow::bail!(
            "flow '{}' has no code phase — ship is not applicable",
            flow.as_str()
        );
    }
    let Some(prev) = flow.previous_of(&State::Coded) else {
        return Ok(());
    };
    if state < prev {
        anyhow::bail!(
            "BLOCKED: {} required before ship (flow: {}). Run: {}",
            prev.as_str(),
            flow.as_str(),
            dispatch_command(prev, task_id)
        );
    }
    Ok(())
}

/// Idempotent advance to `Coded`. Both the CLI `ship` (after dispatching the
/// code agent) and the MCP `ship` tool (the LLM has already written code)
/// call this before running verify. Returns `Ok(true)` when the FSM was
/// advanced this call, `Ok(false)` when it was already past `Coded`.
pub(crate) fn ensure_coded_state(
    ts: &mut TaskState,
    tasks_dir: &std::path::Path,
    note: &str,
) -> Result<bool> {
    if ts.state < State::Coded {
        ts.advance(State::Coded, note)?;
        ts.save(tasks_dir)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Run code + verify back-to-back. Idempotent: skips code if state already Coded.
///
/// When `max_iterations > 1` the verifier-driven loop kicks in: on a verify
/// failure the loop feeds the failed test names + `verify.md` back into the
/// next code attempt as context, repeating up to `max_iterations` times. This
/// is the SWE-bench-winning pattern — let the test suite be ground truth.
pub fn run(
    task_id: &str,
    command: Option<String>,
    timeout: u64,
    max_iterations: u32,
) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    // Acquire per-task lock for the foreground ship lifetime — UNLESS the
    // worker has already acquired the outer lock and re-entered via
    // `ship::run`. `ZFORGE_HEADLESS=1` is the signal: only the worker sets
    // it (see `job::worker::run`), and the worker holds the lock for its
    // whole lifecycle. Without this skip, the inner re-acquire here would
    // deadlock the worker against itself.
    let _task_lock = if std::env::var("ZFORGE_HEADLESS").is_ok() {
        None
    } else {
        Some(
            crate::state::try_acquire_task_lock(&tasks_dir, task_id)
                .map_err(|e| anyhow::anyhow!("{e}"))?,
        )
    };

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    flow_guard::ensure_phase_in_flow(&ts, State::Coded, "code")?;

    if let Err(e) = check_ship_gate(&ts.flow, &ts.state, task_id) {
        eprintln!("{} {}", "⛔".red(), e);
        return Ok(());
    }

    if ts.state >= State::Coded {
        // Code already done in a prior session — just verify (idempotent path).
        println!(
            "{} State already {}, skipping code phase",
            "ℹ".blue(),
            ts.state.as_str()
        );
        return cli::verify::run(task_id, command, timeout);
    }

    let max_iter = max_iterations.max(1);
    if max_iter == 1 {
        // Legacy single-shot ship: code → verify. Verify failure propagates.
        run_code_phase(&config, &ts, task_id, None, 1, 1)?;
        let prev = ts.state.as_str().to_string();
        if ensure_coded_state(&mut ts, &tasks_dir, "code phase complete (ship)")? {
            println!("{} State advanced: {} → Coded", "✓".green(), prev);
            println!();
        }
        return cli::verify::run(task_id, command, timeout);
    }

    // Verifier-driven loop. Hand control to the orchestrator-level iterate()
    // helper so the same plumbing is unit-tested in isolation.
    let outcome = crate::orchestrator::iterate(
        max_iter,
        |attempt, prev_failure| {
            run_code_phase(&config, &ts, task_id, prev_failure, attempt, max_iter)?;
            ensure_coded_state(&mut ts, &tasks_dir, "code phase complete (ship)")?;
            Ok(())
        },
        || crate::cli::verify::run_with_outcome(task_id, command.clone(), timeout),
    )?;

    println!();
    if outcome.iterations == 1 {
        println!("{} Verifier passed on first attempt", "✓".green().bold());
    } else {
        println!(
            "{} Verifier passed after {} iterations",
            "✓".green().bold(),
            outcome.iterations
        );
    }
    Ok(())
}

/// Detached variant: validate ship gate, scaffold a job record, spawn a
/// worker subprocess of `zforge` that re-enters `ship::run` via the hidden
/// `worker` subcommand. Returns immediately after printing the job ID.
///
/// `ship --async --max-iterations N` is the recommended path for MCP
/// orchestrator agents — the tool call resolves in milliseconds, the agent
/// continues working, and `job_wait` / `job_status` retrieves the result
/// later.
pub fn run_async(
    task_id: &str,
    command: Option<String>,
    timeout: u64,
    max_iterations: u32,
) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    // Probe the task lock to fail fast if another worker / foreground process
    // is already running this task. Release immediately — the spawned worker
    // re-acquires for its own lifetime. Brief race window (microseconds) is
    // acceptable; worker's acquire is the real guard.
    {
        let _probe = crate::state::try_acquire_task_lock(&tasks_dir, task_id)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    // Pre-validate everything `ship` checks synchronously so the failure
    // surfaces NOW, not in a worker log the user has to dig out.
    let ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;
    flow_guard::ensure_phase_in_flow(&ts, State::Coded, "code")?;
    check_ship_gate(&ts.flow, &ts.state, task_id)?;

    let job = crate::job::store::create_job(
        &config,
        task_id,
        crate::job::schema::JobKind::Ship,
        command,
        timeout,
        max_iterations,
    )?;
    let pid = crate::job::spawn::spawn_worker(&config, &job.job_id)?;
    println!("{} job {} spawned (pid {})", "▶".cyan(), job.job_id, pid);
    println!("  poll with: zforge job status {}", job.job_id);
    println!("  follow:    zforge job log {} --follow", job.job_id);
    println!("  wait:      zforge job wait {}", job.job_id);
    Ok(())
}

/// Build the code-phase prompt context and dispatch the agent. On retry
/// (`prev_failure.is_some()`), inject the failed test names + `verify.md`
/// content so the template's `{{if failed_tests}}...{{end}}` block fires.
fn run_code_phase(
    config: &crate::config::Config,
    ts: &TaskState,
    task_id: &str,
    prev_failure: Option<&crate::cli::verify::VerifyOutcome>,
    attempt: u32,
    max_iter: u32,
) -> Result<()> {
    let mut ctx = build_context_for_phase(config, task_id, PromptPhase::Code)?;

    if let Some(failure) = prev_failure {
        let verify_path = config.tasks_dir().join(task_id).join("verify.md");
        let verify_md = std::fs::read_to_string(&verify_path).unwrap_or_default();

        ctx.failed_tests = failure.failed_names.join("\n");
        ctx.verify_file = verify_md;
        // Make verify.md reachable as a /file ref for agents that prefer that.
        let verify_ref = ctx.verify_ref.clone();
        if !ctx.context_files.contains(&verify_ref) {
            ctx.context_files.push(verify_ref);
        }
        ctx.next_command = format!("(retry {}/{}) zf verify {}", attempt, max_iter, task_id);

        println!();
        println!(
            "{} Verifier retry {}/{} — {} test(s) still failing",
            "🔁".yellow(),
            attempt,
            max_iter,
            failure.failed_tests
        );
    } else {
        ctx.next_command = format!("(auto) zf verify {}", task_id);
    }

    run_phase_for_task(config, ts, "code", &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_blocks_states_before_plan_reviewed() {
        for blocked in [
            State::Imported,
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
        ] {
            let err = check_ship_gate(&Flow::Full, &blocked, "TASK-1").unwrap_err();
            assert!(
                err.to_string().contains("BLOCKED"),
                "state {blocked:?} should block ship but message was: {err}"
            );
            assert!(err.to_string().contains("TASK-1"));
        }
    }

    #[test]
    fn fixbug_gate_passes_after_testspec() {
        check_ship_gate(&Flow::Fixbug, &State::TestspecDone, "TASK-1").unwrap();
    }

    #[test]
    fn fixbug_gate_blocks_before_testspec() {
        let err = check_ship_gate(&Flow::Fixbug, &State::SpecDone, "TASK-1").unwrap_err();
        assert!(err.to_string().contains("TestspecDone"));
        assert!(err.to_string().contains("fixbug"));
    }

    #[test]
    fn ensure_coded_advances_when_below() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut ts = TaskState::new("TASK-1");
        for next in [
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
            State::PlanReviewed,
        ] {
            ts.advance(next, "").unwrap();
        }
        ts.save(tmp.path()).unwrap();

        let advanced = ensure_coded_state(&mut ts, tmp.path(), "ship test").unwrap();
        assert!(advanced);
        assert_eq!(ts.state, State::Coded);
        let reloaded = TaskState::load(tmp.path(), "TASK-1").unwrap();
        assert_eq!(reloaded.state, State::Coded);
    }

    #[test]
    fn ensure_coded_is_idempotent_when_already_coded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut ts = TaskState::new("TASK-1");
        for next in [
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
            State::PlanReviewed,
            State::Coded,
        ] {
            ts.advance(next, "").unwrap();
        }
        ts.save(tmp.path()).unwrap();

        let advanced = ensure_coded_state(&mut ts, tmp.path(), "ship test").unwrap();
        assert!(!advanced);
        assert_eq!(ts.state, State::Coded);
    }

    #[test]
    fn gate_allows_states_at_or_past_plan_reviewed() {
        for ok in [
            State::PlanReviewed,
            State::Coded,
            State::Verified,
            State::Reviewed,
        ] {
            check_ship_gate(&Flow::Full, &ok, "TASK-1")
                .unwrap_or_else(|e| panic!("state {ok:?} should pass gate but errored: {e}"));
        }
    }
}
