use anyhow::Result;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub passed: bool,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub failed_names: Vec<String>,
    pub coverage: Option<f64>,
    pub duration: Duration,
    pub raw_output: String,
    pub error: Option<String>,
}

pub fn run(command: &str, work_dir: &Path, timeout_secs: u64) -> Result<TestResult> {
    let start = Instant::now();
    let parts = shlex::split(command)
        .ok_or_else(|| anyhow::anyhow!("invalid shell quoting in command: {command:?}"))?;
    if parts.is_empty() {
        anyhow::bail!("empty command");
    }

    let mut child = Command::new(&parts[0])
        .args(&parts[1..])
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Drain stdout/stderr in dedicated threads. OS pipe buffers cap at ~64 KB;
    // a chatty `cargo test` will fill them and block the child if we only read
    // after wait_timeout returns. On timeout, killing the child closes the pipes,
    // which lets these reader threads hit EOF and join cleanly.
    let mut stdout_pipe = child.stdout.take().expect("stdout piped by Command builder");
    let mut stderr_pipe = child.stderr.take().expect("stderr piped by Command builder");
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
    let status_opt = child.wait_timeout(timeout)?;
    let timed_out = status_opt.is_none();
    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
    }

    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
    let duration = start.elapsed();
    let raw_output = format!("{}{}", stdout, stderr);

    if timed_out {
        return Ok(TestResult {
            passed: false,
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            failed_names: Vec::new(),
            coverage: None,
            duration,
            raw_output,
            error: Some(format!("test command timed out after {timeout_secs}s")),
        });
    }

    let status = status_opt.expect("status present when not timed out");
    let mut result = parse_cargo_test_output(&raw_output);
    result.duration = duration;
    result.passed = status.success();

    if !status.success() && result.total_tests == 0 {
        result.error = Some(stderr);
    }

    Ok(result)
}

pub fn parse_cargo_test_output(raw: &str) -> TestResult {
    let mut passed_tests = 0usize;
    let mut failed_tests = 0usize;
    let mut failed_names: Vec<String> = Vec::new();
    let mut coverage: Option<f64> = None;
    let mut passed = true;

    for line in raw.lines() {
        let trimmed = line.trim();

        // test module::name ... ok
        if trimmed.starts_with("test ") && trimmed.ends_with("... ok") {
            passed_tests += 1;
        }

        // test module::name ... FAILED
        if trimmed.starts_with("test ") && trimmed.ends_with("... FAILED") {
            failed_tests += 1;
            let name = trimmed
                .trim_start_matches("test ")
                .trim_end_matches("... FAILED")
                .trim()
                .to_string();
            failed_names.push(name);
        }

        // test result: FAILED. X passed; Y failed;
        if trimmed.starts_with("test result:") {
            if trimmed.contains("FAILED") {
                passed = false;
            }
            // Parse "X passed; Y failed" if not already counted
            if let Some(p) = parse_count(trimmed, "passed") {
                passed_tests = p;
            }
            if let Some(f) = parse_count(trimmed, "failed") {
                failed_tests = f;
            }
        }

        // Coverage: 78.30%
        if trimmed.starts_with("Coverage:") {
            if let Some(pct) = trimmed
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
            {
                coverage = Some(pct);
            }
        }
    }

    let total_tests = passed_tests + failed_tests;

    TestResult {
        passed,
        total_tests,
        passed_tests,
        failed_tests,
        failed_names,
        coverage,
        duration: Duration::default(),
        raw_output: raw.to_string(),
        error: None,
    }
}

fn parse_count(line: &str, label: &str) -> Option<usize> {
    // "39 passed" or "3 failed"
    for part in line.split(';') {
        let part = part.trim();
        if part.ends_with(label) {
            let num = part.trim_end_matches(label).trim();
            if let Ok(n) = num.parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn run_kills_command_that_exceeds_timeout() {
        let cwd = env::current_dir().unwrap();
        let result = run("sleep 10", &cwd, 1).unwrap();
        assert!(!result.passed);
        assert!(result.error.as_deref().unwrap_or("").contains("timed out"));
        assert!(result.duration.as_secs() < 5);
    }

    #[test]
    fn run_accepts_quoted_args_with_spaces() {
        let cwd = env::current_dir().unwrap();
        // echoing a quoted arg must not split inside the quotes
        let result = run("echo \"hello world\"", &cwd, 5).unwrap();
        assert!(result.raw_output.contains("hello world"));
    }

    #[test]
    fn run_rejects_invalid_shell_quoting() {
        let cwd = env::current_dir().unwrap();
        let err = run("echo \"unclosed", &cwd, 5).unwrap_err();
        assert!(err.to_string().contains("invalid shell quoting"));
    }

    // Regression: child writing more than the OS pipe buffer (~64 KB on macOS/Linux)
    // must not deadlock. Old impl waited on the child before reading the pipes,
    // so the child blocked on write and `wait_timeout` fired a false timeout.
    #[test]
    fn run_drains_large_output_without_deadlock() {
        let cwd = env::current_dir().unwrap();
        let result = run("sh -c \"yes | head -c 200000\"", &cwd, 10).unwrap();
        assert!(result.passed, "unexpected error: {:?}", result.error);
        assert!(
            result.raw_output.len() >= 200_000,
            "drained only {} bytes",
            result.raw_output.len()
        );
        assert!(result.duration.as_secs() < 10);
    }
}
