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

/// Runs the test command and parses its output as Rust/cargo output.
/// Kept for backwards compatibility; new callers should use [`run_with_language`].
#[cfg(test)]
pub fn run(command: &str, work_dir: &Path, timeout_secs: u64) -> Result<TestResult> {
    run_with_language(command, work_dir, timeout_secs, "rust")
}

pub fn run_with_language(
    command: &str,
    work_dir: &Path,
    timeout_secs: u64,
    language: &str,
) -> Result<TestResult> {
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
    let mut result = parse_test_output(&raw_output, language);
    result.duration = duration;
    result.passed = status.success();

    if !status.success() && result.total_tests == 0 {
        result.error = Some(stderr);
    }

    Ok(result)
}

/// Dispatch raw test output to a language-specific parser. Falls back to a
/// generic exit-code-only TestResult for unknown languages so callers still
/// get sensible defaults (total_tests = 0; pass/fail set later from the
/// process exit status).
pub fn parse_test_output(raw: &str, language: &str) -> TestResult {
    match language.to_ascii_lowercase().as_str() {
        "rust" => parse_cargo_test_output(raw),
        "go" | "golang" => parse_go_test_output(raw),
        "python" | "py" | "pytest" => parse_pytest_output(raw),
        "flutter" | "dart" => parse_flutter_test_output(raw),
        "typescript" | "javascript" | "ts" | "js" | "node" => parse_jest_output(raw),
        // ios / android / others have noisy, framework-specific output that is
        // unreliable to parse without per-tool heuristics. Stay quiet and let
        // the exit code drive `passed`.
        _ => parse_generic_output(raw),
    }
}

fn parse_generic_output(raw: &str) -> TestResult {
    TestResult {
        passed: true,
        total_tests: 0,
        passed_tests: 0,
        failed_tests: 0,
        failed_names: Vec::new(),
        coverage: None,
        duration: Duration::default(),
        raw_output: raw.to_string(),
        error: None,
    }
}

/// Parses `go test ./...` output. Counts lines beginning with `--- PASS:` and
/// `--- FAIL:` (one per test func). The trailing `PASS`/`FAIL` line drives the
/// `passed` flag; counts are zero when running with `-v` disabled.
pub fn parse_go_test_output(raw: &str) -> TestResult {
    let mut passed_tests = 0usize;
    let mut failed_tests = 0usize;
    let mut failed_names: Vec<String> = Vec::new();
    let mut passed = true;

    for line in raw.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("--- PASS: ") {
            passed_tests += 1;
            // `--- PASS: TestName (0.00s)` — drop the timing suffix
            let _ = rest;
        }
        if let Some(rest) = trimmed.strip_prefix("--- FAIL: ") {
            failed_tests += 1;
            passed = false;
            let name = rest.split_whitespace().next().unwrap_or(rest).to_string();
            failed_names.push(name);
        }
        if trimmed == "FAIL" || trimmed.starts_with("FAIL\t") || trimmed.starts_with("FAIL: ") {
            passed = false;
        }
    }

    let total_tests = passed_tests + failed_tests;

    TestResult {
        passed,
        total_tests,
        passed_tests,
        failed_tests,
        failed_names,
        coverage: None,
        duration: Duration::default(),
        raw_output: raw.to_string(),
        error: None,
    }
}

/// Parses pytest's terminal summary line, e.g.
/// `===== 3 passed, 1 failed, 2 skipped in 0.42s =====`.
/// Also captures `FAILED tests/test_x.py::test_y` lines for failed_names.
pub fn parse_pytest_output(raw: &str) -> TestResult {
    let mut passed_tests = 0usize;
    let mut failed_tests = 0usize;
    let mut failed_names: Vec<String> = Vec::new();
    let mut passed = true;

    for line in raw.lines() {
        let trimmed = line.trim();

        // `FAILED tests/test_foo.py::test_bar - AssertionError: ...`
        if let Some(rest) = trimmed.strip_prefix("FAILED ") {
            let name = rest.split(" - ").next().unwrap_or(rest).trim().to_string();
            if !name.is_empty() {
                failed_names.push(name);
            }
            passed = false;
        }

        // Final summary line is wrapped in `=` runs.
        if trimmed.starts_with("=") && trimmed.ends_with("=") && trimmed.contains(" in ") {
            for chunk in trimmed.trim_matches('=').trim().split(',') {
                let chunk = chunk.trim();
                if let Some(n) = parse_leading_count(chunk, "passed") {
                    passed_tests = n;
                }
                if let Some(n) = parse_leading_count(chunk, "failed") {
                    failed_tests = n;
                    passed = false;
                }
                if let Some(n) = parse_leading_count(chunk, "error") {
                    failed_tests += n;
                    passed = false;
                }
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
        coverage: None,
        duration: Duration::default(),
        raw_output: raw.to_string(),
        error: None,
    }
}

/// Parses `flutter test` output. Uses the running `+N -M` counter and the
/// final `All tests passed!` / `Some tests failed.` line.
pub fn parse_flutter_test_output(raw: &str) -> TestResult {
    let mut passed_tests = 0usize;
    let mut failed_tests = 0usize;
    let mut passed = true;

    for line in raw.lines() {
        let trimmed = line.trim();
        // Lines look like `00:01 +12 -1: ...` or `00:01 +12: All tests passed!`.
        // `+N` is the pass count, optional `-M` (or `~M`) is the fail count.
        // Anything past the first non-counter, non-time token is ignored.
        let mut local_pass: Option<usize> = None;
        let mut local_fail: Option<usize> = None;
        for tok in trimmed.split_whitespace() {
            let cleaned = tok.trim_end_matches(':');
            if let Some(rest) = cleaned.strip_prefix('+') {
                if let Ok(n) = rest.parse::<usize>() {
                    local_pass = Some(n);
                }
            } else if let Some(rest) = cleaned.strip_prefix('-') {
                if let Ok(n) = rest.parse::<usize>() {
                    local_fail = Some(n);
                }
            }
        }
        if let Some(p) = local_pass {
            passed_tests = passed_tests.max(p);
        }
        if let Some(f) = local_fail {
            failed_tests = failed_tests.max(f);
        }
        if trimmed.contains("Some tests failed.") {
            passed = false;
        }
    }

    if failed_tests > 0 {
        passed = false;
    }

    let total_tests = passed_tests + failed_tests;

    TestResult {
        passed,
        total_tests,
        passed_tests,
        failed_tests,
        failed_names: Vec::new(),
        coverage: None,
        duration: Duration::default(),
        raw_output: raw.to_string(),
        error: None,
    }
}

/// Parses Jest's `Tests:` summary line, e.g.
/// `Tests:       2 failed, 5 passed, 7 total`.
pub fn parse_jest_output(raw: &str) -> TestResult {
    let mut passed_tests = 0usize;
    let mut failed_tests = 0usize;
    let mut passed = true;

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Tests:") {
            for chunk in rest.split(',') {
                let chunk = chunk.trim();
                if let Some(n) = parse_leading_count(chunk, "passed") {
                    passed_tests = n;
                }
                if let Some(n) = parse_leading_count(chunk, "failed") {
                    failed_tests = n;
                    if n > 0 {
                        passed = false;
                    }
                }
            }
        }
    }

    let total_tests = passed_tests + failed_tests;

    TestResult {
        passed,
        total_tests,
        passed_tests,
        failed_tests,
        failed_names: Vec::new(),
        coverage: None,
        duration: Duration::default(),
        raw_output: raw.to_string(),
        error: None,
    }
}

/// `"3 passed"` → `Some(3)`. Matches the leading integer when the chunk ends
/// with `<label>` (with optional trailing words like "passed in", which pytest
/// emits in some summary forms — we accept any trailing whitespace+text).
fn parse_leading_count(chunk: &str, label: &str) -> Option<usize> {
    let trimmed = chunk.trim();
    let (head, tail) = trimmed.split_once(' ')?;
    if tail.starts_with(label) {
        head.parse::<usize>().ok()
    } else {
        None
    }
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

    #[test]
    fn go_parser_counts_pass_and_fail_lines() {
        let raw = "\
=== RUN   TestAdd
--- PASS: TestAdd (0.00s)
=== RUN   TestMul
--- FAIL: TestMul (0.01s)
    foo_test.go:12: expected 6 got 5
FAIL
exit status 1
FAIL\texample.com/foo\t0.123s
";
        let r = parse_go_test_output(raw);
        assert_eq!(r.passed_tests, 1);
        assert_eq!(r.failed_tests, 1);
        assert_eq!(r.failed_names, vec!["TestMul"]);
        assert!(!r.passed);
    }

    #[test]
    fn go_parser_all_pass() {
        let raw = "\
--- PASS: TestA (0.00s)
--- PASS: TestB (0.00s)
PASS
ok\texample.com/foo\t0.123s
";
        let r = parse_go_test_output(raw);
        assert_eq!(r.passed_tests, 2);
        assert_eq!(r.failed_tests, 0);
        assert!(r.passed);
    }

    #[test]
    fn pytest_parser_reads_summary_line() {
        let raw = "\
============================= test session starts ==============================
collected 6 items

tests/test_a.py ...                                                      [ 50%]
tests/test_b.py F.F                                                      [100%]

FAILED tests/test_b.py::test_x - AssertionError
FAILED tests/test_b.py::test_z - AssertionError
========================= 4 passed, 2 failed in 0.42s ==========================
";
        let r = parse_pytest_output(raw);
        assert_eq!(r.passed_tests, 4);
        assert_eq!(r.failed_tests, 2);
        assert_eq!(r.failed_names.len(), 2);
        assert!(r.failed_names.iter().any(|n| n.contains("test_x")));
        assert!(!r.passed);
    }

    #[test]
    fn pytest_parser_treats_errors_as_failures() {
        let raw = "============= 1 passed, 1 error in 0.01s =============\n";
        let r = parse_pytest_output(raw);
        assert_eq!(r.passed_tests, 1);
        assert_eq!(r.failed_tests, 1);
        assert!(!r.passed);
    }

    #[test]
    fn flutter_parser_tracks_max_counters() {
        let raw = "\
00:01 +0: loading
00:02 +5: tests/widget_test.dart: looks good
00:03 +7 -1: tests/other_test.dart: something broke
00:04 +9 -1: Some tests failed.
";
        let r = parse_flutter_test_output(raw);
        assert_eq!(r.passed_tests, 9);
        assert_eq!(r.failed_tests, 1);
        assert!(!r.passed);
    }

    #[test]
    fn flutter_parser_all_pass() {
        let raw = "\
00:01 +0: loading
00:02 +12: All tests passed!
";
        let r = parse_flutter_test_output(raw);
        assert_eq!(r.passed_tests, 12);
        assert_eq!(r.failed_tests, 0);
        assert!(r.passed);
    }

    #[test]
    fn jest_parser_reads_summary_line() {
        let raw = "\
PASS  src/a.test.ts
FAIL  src/b.test.ts

Tests:       2 failed, 5 passed, 7 total
Snapshots:   0 total
Time:        0.812s
";
        let r = parse_jest_output(raw);
        assert_eq!(r.passed_tests, 5);
        assert_eq!(r.failed_tests, 2);
        assert!(!r.passed);
    }

    #[test]
    fn generic_parser_reports_no_tests() {
        let r = parse_generic_output("anything\nor nothing\n");
        assert_eq!(r.total_tests, 0);
        assert_eq!(r.passed_tests, 0);
        assert_eq!(r.failed_tests, 0);
    }

    #[test]
    fn dispatch_routes_by_language() {
        let cargo_raw = "test foo::bar ... ok\ntest result: ok. 1 passed; 0 failed\n";
        let go_raw = "--- PASS: TestA (0.00s)\nPASS\n";
        let pytest_raw = "===== 1 passed in 0.01s =====\n";

        assert_eq!(parse_test_output(cargo_raw, "rust").passed_tests, 1);
        assert_eq!(parse_test_output(go_raw, "go").passed_tests, 1);
        assert_eq!(parse_test_output(go_raw, "GoLang").passed_tests, 1);
        assert_eq!(parse_test_output(pytest_raw, "python").passed_tests, 1);
        assert_eq!(parse_test_output("nothing", "ios").total_tests, 0);
        assert_eq!(parse_test_output("nothing", "android").total_tests, 0);
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
