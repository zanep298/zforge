use crate::config;
use crate::fs::writer;
use crate::prompt::{build_context, Engine};
use crate::runner;
use crate::state::{State, TaskState};
use anyhow::Result;
use chrono::Local;
use colored::Colorize;
use std::env;

pub fn run(task_id: &str, command: Option<String>, _timeout: u64) -> Result<()> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let tasks_dir = config.tasks_dir();

    let mut ts = TaskState::load(&tasks_dir, task_id)
        .map_err(|_| anyhow::anyhow!("Task {} not found.", task_id))?;

    ts.require(State::Coded)?;

    let cmd = command.unwrap_or_else(|| config.project.test_command.clone());
    let work_dir = env::current_dir()?;

    println!("{} Running: {}", "🧪".bold(), cmd);

    let result = runner::run(&cmd, &work_dir)?;

    let duration_secs = result.duration.as_secs_f64();

    if result.passed {
        println!(
            "{} All tests passed ({}/{}) — {:.2}s",
            "✓".green(),
            result.passed_tests,
            result.total_tests,
            duration_secs
        );
    } else {
        println!(
            "{} Tests failed: {}/{} failed",
            "✗".red(),
            result.failed_tests,
            result.total_tests
        );
        println!();
        println!("Failed tests:");
        for name in &result.failed_names {
            println!("  • {}", name);
        }
    }

    // Write verify.md
    let coverage_str = result
        .coverage
        .map(|c| format!("{:.1}", c))
        .unwrap_or_else(|| "null".to_string());
    let verify_content = format!(
        r#"---
id: "{}"
type: verify
passed: {}
total_tests: {}
passed_tests: {}
failed_tests: {}
coverage: {}
duration_seconds: {:.2}
ran_at: "{}"
command: "{}"
---

## Test Results

### Summary
{} — {}/{} passed — {:.2}s

### Failed Tests
{}

### Raw Output
```
{}
```
"#,
        task_id,
        result.passed,
        result.total_tests,
        result.passed_tests,
        result.failed_tests,
        coverage_str,
        duration_secs,
        Local::now().to_rfc3339(),
        cmd,
        if result.passed { "PASS" } else { "FAIL" },
        result.passed_tests,
        result.total_tests,
        duration_secs,
        result
            .failed_names
            .iter()
            .map(|n| format!("- {}", n))
            .collect::<Vec<_>>()
            .join("\n"),
        result.raw_output
    );

    let verify_path = tasks_dir.join(task_id).join("verify.md");
    writer::write_file(&verify_path, &verify_content)?;
    println!();
    println!("Verify report: tasks/{}/verify.md", task_id);

    if result.passed {
        ts.advance(State::Verified, "tests passed")?;
        ts.save(&tasks_dir)?;
        println!();
        println!("{} State advanced: Coded → Verified", "✓".green());
        println!("Next: zf review {}", task_id);
    } else {
        // Generate analysis prompt
        let mut ctx = build_context(&config, task_id)?;
        ctx.failed_tests = result.failed_names.join("\n");
        ctx.verify_file = verify_content;

        let engine = Engine::new(&config.agents_dir());
        println!();
        println!("{}", "─".repeat(40));
        println!("{} AI Analysis Prompt (paste into Claude):", "🔍".bold());
        if let Ok(rendered) = engine.render("verify-analysis", &ctx) {
            println!("{}", rendered);
        }
        println!("{}", "─".repeat(40));
    }

    Ok(())
}
