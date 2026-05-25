use crate::registry::schema::AgentSpec;
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

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
/// Implementation notes:
/// - stdin write runs in a dedicated thread so a chatty child can drain
///   stdout/stderr in parallel without back-pressure on the prompt pipe.
/// - stdout / stderr each drain in their own thread to avoid the 64KB
///   pipe-buffer deadlock that bit pre-PR10 callers of `wait_with_output`.
/// - `wait_timeout` from the `wait-timeout` crate provides a portable
///   blocking-wait-with-deadline. Same primitive `runner::run_with_language`
///   uses for the test command.
pub fn spawn_agent(spec: &AgentSpec, prompt: &str, timeout_secs: u64) -> Result<SpawnOutcome> {
    let started = Instant::now();
    let mut cmd = Command::new(&spec.command);
    cmd.args(&spec.args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn agent {:?}", spec.command))?;

    // Feed the prompt to stdin in a background thread. Drop closes the pipe
    // so the child sees EOF on read.
    if let Some(mut stdin) = child.stdin.take() {
        let bytes = prompt.as_bytes().to_vec();
        thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }

    // Drain both pipes concurrently. OS pipe buffers cap at ~64 KB; a
    // chatty child will fill them and block its own write if we only read
    // after the wait returns. Killing the child closes its end of the
    // pipes, which lets these readers hit EOF and join cleanly.
    let mut stdout_pipe = child
        .stdout
        .take()
        .expect("stdout piped by Command builder");
    let mut stderr_pipe = child
        .stderr
        .take()
        .expect("stderr piped by Command builder");
    let stdout_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let timeout = Duration::from_secs(timeout_secs);
    let status_opt = child.wait_timeout(timeout).context("wait for agent")?;
    let timed_out = status_opt.is_none();
    let exit_code = match status_opt {
        Some(status) => status.code().unwrap_or(-1),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            124 // GNU timeout convention; retryable per default policy.
        }
    };

    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    let mut stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
    if timed_out {
        // Inject a signature line so `retryable_stderr_patterns` operators
        // can write rules like `(?i)spawn timeout` if they want extra
        // matching beyond exit-code 124. Also helps log readers spot the
        // distinction between agent-emitted 124 and zforge-injected 124.
        stderr.push_str(&format!(
            "\nzforge: spawn timeout after {timeout_secs}s\n"
        ));
    }
    let duration_ms = started.elapsed().as_millis();

    Ok(SpawnOutcome {
        exit_code,
        stderr,
        stdout,
        duration_ms,
        timed_out,
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
