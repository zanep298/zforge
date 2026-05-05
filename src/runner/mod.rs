use anyhow::Result;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

pub fn run(command: &str, work_dir: &Path) -> Result<TestResult> {
    let start = Instant::now();
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("empty command");
    }

    let child = Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output()?;
    let duration = start.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let raw_output = format!("{}{}", stdout, stderr);

    let mut result = parse_cargo_test_output(&raw_output);
    result.duration = duration;
    result.passed = output.status.success();

    if !output.status.success() && result.total_tests == 0 {
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
