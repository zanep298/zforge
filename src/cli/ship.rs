use crate::cli;
use crate::cli::dispatch_helper::run_phase_for_task_locked;
use crate::cli::flow_guard;
use crate::cli::outcome::OperationOutcome;
use crate::config;
use crate::note;
use crate::prompt::{build_context_for_phase, PromptPhase};
use crate::state::{dispatch_command, with_task_lock, Flow, State, TaskLockGuard, TaskState};
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
///
/// Takes the task lock for the whole run and returns `Busy` if another
/// process holds it. Use [`run_locked`] when the caller already owns it.
pub fn run(
    task_id: &str,
    command: Option<String>,
    timeout: u64,
    max_iterations: u32,
) -> Result<OperationOutcome> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let guard = crate::state::lock_task(&config.tasks_dir(), task_id)?;
    run_locked(&guard, task_id, command, timeout, max_iterations)
}

/// [`run`] with the task lock already owned by the caller — the background
/// worker holds it for its whole lifetime and passes it in here. The guard
/// is the proof of ownership: it is threaded into verify and the
/// orchestrator so nested operations never try to re-acquire (which would
/// deadlock against ourselves). Before FIX-004 the worker/foreground split
/// was signalled by `ZFORGE_HEADLESS`, an unrelated env var, and every other
/// mutation (`retry`, `verify`, `review`) ignored the lock entirely.
pub fn run_locked(
    guard: &TaskLockGuard,
    task_id: &str,
    command: Option<String>,
    timeout: u64,
    max_iterations: u32,
) -> Result<OperationOutcome> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();
    // Validates the guard belongs to this task before anything runs.
    with_task_lock(&tasks_dir, task_id, Some(guard), |_| Ok(()))?;

    let ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    flow_guard::ensure_phase_in_flow(&ts, State::Coded, "code")?;

    // A refused gate is a Blocked outcome, not a success. Before FIX-001 this
    // printed the reason and returned `Ok(())`, so a scripted `ship` that
    // never ran a line of code still exited 0.
    if let Err(e) = check_ship_gate(&ts.flow, &ts.state, task_id) {
        eprintln!("{} {}", "⛔".red(), e);
        return Ok(OperationOutcome::blocked(e.to_string()));
    }

    if ts.state >= State::Coded {
        // Code already done in a prior session — just verify (idempotent path).
        note!(
            "{} State already {}, skipping code phase",
            "ℹ".blue(),
            ts.state.as_str()
        );
        return cli::verify::run_locked(task_id, command, timeout, Some(guard));
    }

    let max_iter = max_iterations.max(1);
    if max_iter == 1 {
        // Single-shot ship: code → verify, no self-fix loop. The verify
        // outcome is the ship outcome — a red suite means ship failed.
        run_code_phase(&config, &ts, task_id, None, 1, 1, guard)?;
        advance_to_coded_after_dispatch(&tasks_dir, task_id)?;
        return cli::verify::run_locked(task_id, command, timeout, Some(guard));
    }

    // Verifier-driven loop. Hand control to the orchestrator-level iterate()
    // helper so the same plumbing is unit-tested in isolation.
    let outcome = crate::orchestrator::iterate(
        max_iter,
        |attempt, prev_failure| {
            // Reload each attempt: the previous attempt's dispatch may have
            // swapped `active_agent`, and the prompt should use it.
            let current = TaskState::load(&tasks_dir, task_id)?;
            run_code_phase(
                &config,
                &current,
                task_id,
                prev_failure,
                attempt,
                max_iter,
                guard,
            )?;
            advance_to_coded_after_dispatch(&tasks_dir, task_id)?;
            Ok(())
        },
        || {
            crate::cli::verify::run_with_outcome_locked(
                task_id,
                command.clone(),
                timeout,
                Some(guard),
            )
        },
    )?;

    note!();
    if outcome.iterations == 1 {
        note!("{} Verifier passed on first attempt", "✓".green().bold());
    } else {
        note!(
            "{} Verifier passed after {} iterations",
            "✓".green().bold(),
            outcome.iterations
        );
    }
    Ok(OperationOutcome::Success)
}

/// Advance to `Coded` once the code agent has returned.
///
/// Reloads `.state.yaml` rather than advancing a copy loaded before the
/// dispatch. The orchestrator persists fallback swaps (`active_agent`,
/// `fallback_history`) *during* the dispatch; advancing the pre-dispatch
/// snapshot and saving it overwrote those records, so after a primary →
/// fallback swap the state claimed the primary agent had done the work
/// (FIX-007). The caller holds the task lock, so the reload cannot race.
fn advance_to_coded_after_dispatch(tasks_dir: &std::path::Path, task_id: &str) -> Result<()> {
    let mut fresh = TaskState::load(tasks_dir, task_id)?;
    let prev = fresh.state.as_str().to_string();
    if ensure_coded_state(&mut fresh, tasks_dir, "code phase complete (ship)")? {
        note!("{} State advanced: {} → Coded", "✓".green(), prev);
        note!();
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
    let (job, pid) = submit_job(&config, task_id, command, timeout, max_iterations)?;
    note!("{} job {} spawned (pid {})", "▶".cyan(), job.job_id, pid);
    note!("  poll with: zforge job status {}", job.job_id);
    note!("  follow:    zforge job log {} --follow", job.job_id);
    note!("  wait:      zforge job wait {}", job.job_id);
    Ok(())
}

/// Validate, create the job record and launch its worker. Shared by the CLI
/// (`ship --async`) and the MCP `ship_async` tool, which used to carry its
/// own copy without the lock probe or the spawn-failure handling.
pub(crate) fn submit_job(
    config: &crate::config::Config,
    task_id: &str,
    command: Option<String>,
    timeout: u64,
    max_iterations: u32,
) -> Result<(crate::job::schema::Job, u32)> {
    let tasks_dir = config.tasks_dir();

    // Fail fast if another process holds the task. Released at once — the
    // worker takes its own lock. If something grabs the lock in between,
    // the worker records the failure on the job instead of exiting silently.
    {
        let _probe = crate::state::lock_task(&tasks_dir, task_id)?;
    }

    // Pre-validate everything `ship` checks synchronously so the failure
    // surfaces NOW, not in a worker log the user has to dig out.
    let ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;
    flow_guard::ensure_phase_in_flow(&ts, State::Coded, "code")?;
    check_ship_gate(&ts.flow, &ts.state, task_id)?;

    let job = crate::job::store::create_job(
        config,
        task_id,
        crate::job::schema::JobKind::Ship,
        command,
        timeout,
        max_iterations,
    )?;

    // A job that never got a worker must not sit in `queued` (FIX-009).
    let pid = match crate::job::spawn::spawn_worker(config, &job.job_id) {
        Ok(pid) => pid,
        Err(e) => {
            let reason = format!("failed to start worker: {e:#}");
            crate::job::lifecycle::mark_failed(config, &job.job_id, &reason)?;
            return Err(e.context(format!("job {} marked failed", job.job_id)));
        }
    };
    // Lets `reconcile_dead_worker` tell a worker that died before starting
    // from one still starting up. Best-effort: without it a dead worker is
    // still caught, after `LAUNCH_GRACE`.
    if let Err(e) = crate::job::store::record_launch(config, &job.job_id, pid) {
        eprintln!(
            "warning: could not record worker pid for {}: {e:#}",
            job.job_id
        );
    }
    Ok((job, pid))
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
    guard: &TaskLockGuard,
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

        note!();
        note!(
            "{} Verifier retry {}/{} — {} test(s) still failing",
            "🔁".yellow(),
            attempt,
            max_iter,
            failure.failed_tests
        );
    } else {
        ctx.next_command = format!("(auto) zf verify {}", task_id);
    }

    run_phase_for_task_locked(config, ts, "code", &ctx, Some(guard))
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
