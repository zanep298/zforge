//! `zforge status [--global] [--json]`: every intake of this project (or of
//! every registered project) — its files, handovers, runs — and the next
//! step. The data comes from `crate::status`.

use crate::status::{self, GlobalResult, IntakeStatus, OnboardingStatus};
use anyhow::Result;
use colored::Colorize;
use std::time::Duration;

const MIGRATE_HINT: &str =
    "task-pipeline files left — run `zforge migrate` in the project to move to v1.5";

/// `<summary> — next: zforge onboard` while not onboarded, else just the
/// summary (Output, AC-06).
fn onboarding_line(o: &OnboardingStatus) -> String {
    let summary = if o.onboarded {
        o.summary.green()
    } else {
        o.summary.yellow()
    };
    match &o.next {
        Some(next) => format!("{summary} — next: {}", next.cyan()),
        None => summary.to_string(),
    }
}

pub fn run(json: bool) -> Result<()> {
    let config = crate::config::load()?;
    let s = status::project(&config.project_root())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&s)?);
        return Ok(());
    }
    if s.needs_migration {
        println!("{}", MIGRATE_HINT.yellow());
    }
    println!("{} {}", "project:".bold(), onboarding_line(&s.onboarding));
    if s.intakes.is_empty() {
        println!("No intake yet. Start one: `zforge intake new <ID>`");
        return Ok(());
    }
    for i in &s.intakes {
        print!("{}", intake_text(i));
    }
    Ok(())
}

pub fn run_global(timeout_ms: u64, json: bool) -> Result<()> {
    let rows = status::global(Duration::from_millis(timeout_ms))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("No project registered. `zforge init` registers the current one.");
        return Ok(());
    }
    for r in &rows {
        println!("{}  ({})", r.project.bold(), r.path.display());
        match &r.result {
            GlobalResult::Skipped { reason } => println!("  {} {reason}", "skipped:".yellow()),
            GlobalResult::Ok {
                needs_migration,
                onboarding,
                intakes,
            } => {
                if *needs_migration {
                    println!("  {}", MIGRATE_HINT.yellow());
                }
                println!("  {}", onboarding_line(onboarding));
                if intakes.is_empty() {
                    println!("  no intake");
                }
                for i in intakes {
                    println!("  {:<16} {}", i.id, one_line(i));
                }
            }
        }
    }
    Ok(())
}

/// Files by state, then each handover's tasks, then the next step.
pub fn intake_text(i: &IntakeStatus) -> String {
    let mut out = format!("{}\n", i.id.bold());
    for (file, state) in &i.files {
        let state = match state.as_str() {
            "accepted" => state.green(),
            "in_review" => state.cyan(),
            "draft" => state.normal(),
            _ => state.yellow(),
        };
        let stale = if i.stale.contains(file) {
            format!(" {}", "(confirm again)".yellow())
        } else {
            String::new()
        };
        out.push_str(&format!("  {file:<28} {state}{stale}\n"));
    }
    for h in &i.handovers {
        let tasks: Vec<String> = h.tasks.iter().map(|(t, p)| format!("{t} {p}")).collect();
        out.push_str(&format!(
            "  {}: {} · integration {}\n",
            h.id,
            tasks.join(", "),
            h.integration
        ));
    }
    out.push_str(&format!("  {} {}\n", "next:".bold(), i.next));
    out
}

/// `3/4 accepted · HANDOVER-002 2/3 verified` for the global table.
fn one_line(i: &IntakeStatus) -> String {
    let accepted = i.files.values().filter(|s| *s == "accepted").count();
    let mut parts = vec![format!("{accepted}/{} accepted", i.files.len())];
    if let Some(h) = i.handovers.last() {
        let done = h
            .tasks
            .values()
            .filter(|p| *p == "verified" || *p == "reused")
            .count();
        parts.push(format!(
            "{} {done}/{} verified, integration {}",
            h.id,
            h.tasks.len(),
            h.integration
        ));
    }
    parts.join(" · ")
}
