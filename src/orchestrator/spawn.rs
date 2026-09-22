use crate::process::run_bounded;
use crate::registry::schema::AgentSpec;
use anyhow::{Context, Result};
use std::process::Command;
use std::time::{Duration, Instant};

/// Result of one agent invocation.
///
/// - `exit_code = -1` → child killed by signal (no exit code).
/// - `exit_code = 124` → zforge-side wall-clock timeout fired. Convention
///   matches GNU `timeout`. `FallbackPolicy::default().retryable_exit_codes`
///   already lists 124, so the orchestrator's retry loop fires fallback
///   without extra wiring.
#[derive(Debug)]
pub struct SpawnOutcome {
    pub exit_code: i32,
    pub stderr: String,
    pub stdout: String,
    pub duration_ms: u128,
    pub timed_out: bool,
}

/// Spawn `spec.command` with `spec.args`, pipe `prompt` to stdin, wait up
/// to `timeout_secs` seconds, collect stdout + stderr.
///
/// Process handling (process group, bounded drain, signal forwarding) lives
/// in [`crate::process::run_bounded`], shared with the test runner. A
/// timeout kills the agent's whole process tree — agent CLIs spawn helpers
/// (MCP servers, tool subprocesses) that previously survived the kill and
/// kept the output pipes open, so the "timeout" waited as long as they did.
pub fn spawn_agent(spec: &AgentSpec, prompt: &str, timeout_secs: u64) -> Result<SpawnOutcome> {
    spawn(spec, prompt, timeout_secs, None)
}

/// [`spawn_agent`] with the agent's working directory set explicitly — a
/// Mốc B run's worktree — instead of inherited from zforge's own cwd.
pub fn spawn_agent_in(
    spec: &AgentSpec,
    prompt: &str,
    timeout_secs: u64,
    work_dir: &std::path::Path,
) -> Result<SpawnOutcome> {
    spawn(spec, prompt, timeout_secs, Some(work_dir))
}

fn spawn(
    spec: &AgentSpec,
    prompt: &str,
    timeout_secs: u64,
    work_dir: Option<&std::path::Path>,
) -> Result<SpawnOutcome> {
    let started = Instant::now();
    let mut cmd = Command::new(&spec.command);
    cmd.args(&spec.args);
    if let Some(dir) = work_dir {
        cmd.current_dir(dir);
    }

    let out = run_bounded(
        cmd,
        Some(prompt.as_bytes().to_vec()),
        Duration::from_secs(timeout_secs),
    )
    .with_context(|| format!("spawn agent {:?}", spec.command))?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let exit_code = if out.timed_out {
        // GNU timeout convention; retryable per default policy.
        124
    } else {
        out.exit_code()
    };
    if out.timed_out {
        // Inject a signature line so `retryable_stderr_patterns` operators
        // can write rules like `(?i)spawn timeout` if they want extra
        // matching beyond exit-code 124. Also helps log readers spot the
        // distinction between agent-emitted 124 and zforge-injected 124.
        stderr.push_str(&format!("\nzforge: spawn timeout after {timeout_secs}s\n"));
    }
    if out.output_incomplete {
        stderr.push_str(
            "\nzforge: output may be incomplete — a process outside the agent's \
             process group still held its output pipe\n",
        );
    }

    Ok(SpawnOutcome {
        exit_code,
        stderr,
        stdout,
        duration_ms: started.elapsed().as_millis(),
        timed_out: out.timed_out,
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    // Unix-gated: tests below shell out to `sh` and `sleep`, which are not
    // available on stock Windows runners. The production code is
    // cross-platform; verifying it on Windows needs a separate suite
    // using `cmd /c timeout` etc.
    use super::*;

    #[test]
    fn timeout_kills_long_running_child() {
        let spec = AgentSpec {
            command: "sh".into(),
            args: vec!["-c".into(), "sleep 30".into()],
        };
        let started = Instant::now();
        let outcome = spawn_agent(&spec, "", 1).expect("spawn");
        let elapsed = started.elapsed();
        assert!(outcome.timed_out, "expected timeout");
        assert_eq!(outcome.exit_code, 124);
        assert!(outcome.stderr.contains("spawn timeout"));
        assert!(
            elapsed < Duration::from_secs(5),
            "kill should be prompt; elapsed={elapsed:?}"
        );
    }

    // FIX-006 at the agent-runner level: the agent backgrounds a helper
    // that holds the output pipes. The timeout must still bound the wait.
    #[test]
    fn timeout_is_bounded_when_the_agent_leaves_a_grandchild_on_the_pipe() {
        let spec = AgentSpec {
            command: "sh".into(),
            args: vec!["-c".into(), "sleep 30 & wait".into()],
        };
        let started = Instant::now();
        let outcome = spawn_agent(&spec, "", 1).expect("spawn");
        assert!(outcome.timed_out);
        assert_eq!(outcome.exit_code, 124);
        assert!(
            started.elapsed() < Duration::from_secs(6),
            "waited {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn fast_child_returns_real_exit_code() {
        let spec = AgentSpec {
            command: "sh".into(),
            args: vec!["-c".into(), "exit 7".into()],
        };
        let outcome = spawn_agent(&spec, "", 10).expect("spawn");
        assert!(!outcome.timed_out);
        assert_eq!(outcome.exit_code, 7);
    }
}
