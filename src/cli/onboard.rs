//! `zforge onboard …` — review and decide on the project's own knowledge
//! files (ONBOARD TASK-001). `accept` and `revise` need an interactive
//! terminal and a typed confirmation, exactly like `zforge intake accept`
//! / `revise` (D1): an agent can prepare and review, never decide.

use crate::cli::confirm;
use crate::config;
use crate::intake::hash;
use crate::intake::lint;
use crate::intake::status::DocState;
use crate::knowledge::{self, Knowledge};
use anyhow::{anyhow, Result};
use clap::Subcommand;
use colored::Colorize;

#[derive(Debug, Subcommand)]
pub enum OnboardCmd {
    /// Every knowledge file's review state and open questions.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Send a knowledge file's current content for review as a new revision.
    Review { file: String },
    /// Accept the revision under review. Interactive terminal only.
    Accept { file: String },
    /// Ask for changes to the revision under review. Interactive terminal only.
    Revise {
        file: String,
        /// What needs to change.
        #[arg(long)]
        note: String,
    },
}

pub fn run(cmd: OnboardCmd) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let root = config.project_root();
    let k = Knowledge::open(&config);
    match cmd {
        OnboardCmd::Status { json } => status(&k, json),
        OnboardCmd::Review { file } => send_for_review(&root, &k, &file),
        OnboardCmd::Accept { file } => decide(&k, &file, None),
        OnboardCmd::Revise { file, note } => decide(&k, &file, Some(&note)),
    }
}

fn send_for_review(root: &std::path::Path, k: &Knowledge, rel: &str) -> Result<()> {
    let r = knowledge::review(root, k, rel)?;
    for w in &r.warnings {
        eprintln!("{} {}: {}", "⚠".yellow(), w.file, w.message);
    }
    if r.unchanged {
        println!(
            "{rel} revision {} is already under review ({})",
            r.revision,
            hash::short(&r.sha256)
        );
    } else {
        println!(
            "{} {rel} sent for review as revision {} ({})",
            "✓".green(),
            r.revision,
            hash::short(&r.sha256)
        );
        match &r.diff {
            Some(d) if !d.trim().is_empty() => {
                println!("\nChanges since the accepted revision:\n{d}")
            }
            Some(_) => {}
            None => println!("No accepted revision yet to compare with."),
        }
    }
    println!(
        "The user decides at a terminal: `zforge onboard accept {rel}` or `zforge onboard revise {rel} --note …`"
    );
    Ok(())
}

/// Accept (`note = None`) or request changes, after a human confirms.
fn decide(k: &Knowledge, rel: &str, note: Option<&str>) -> Result<()> {
    let (verb, word) = match note {
        None => ("accept", "accept"),
        Some(_) => ("request changes to", "revise"),
    };
    confirm::require_terminal(&format!("zforge onboard {word}"))?;
    let pending = knowledge::pending_review(k, rel)?;
    println!(
        "{} knowledge/{rel} revision {} ({})",
        verb.to_uppercase().bold(),
        pending.revision,
        hash::short(&pending.sha256)
    );
    if let Some(n) = note {
        println!("note: {n}");
    }
    confirm::type_to_confirm(word)?;
    let by = std::env::var("USER").ok().filter(|u| !u.is_empty());
    let rev = match note {
        None => knowledge::accept(k, rel, by)?,
        Some(n) => knowledge::revise(k, rel, by, n)?,
    };
    println!(
        "{} recorded for {rel} revision {}",
        "✓".green(),
        rev.revision
    );
    Ok(())
}

#[derive(serde::Serialize)]
struct FileView {
    #[serde(flatten)]
    status: crate::intake::status::DocStatus,
    open_questions: Vec<String>,
    issues: Vec<lint::Issue>,
}

fn status(k: &Knowledge, json: bool) -> Result<()> {
    let views: Vec<FileView> = knowledge::statuses(k)?
        .into_iter()
        .map(|status| {
            let text = std::fs::read_to_string(k.dir.join(&status.file)).unwrap_or_default();
            FileView {
                open_questions: lint::open_questions(&text),
                issues: knowledge::lint_issues(&status.file, &text),
                status,
            }
        })
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&views)?);
        return Ok(());
    }
    println!("{}  ({})", "knowledge".bold(), k.dir.display());
    for v in &views {
        let s = &v.status;
        let state = match s.state {
            DocState::Accepted if s.has_draft => "accepted, draft pending".yellow(),
            DocState::Accepted => "accepted".green(),
            DocState::InReview => "in review".cyan(),
            DocState::ChangedSinceReview => "changed since review".red(),
            DocState::NeedsRevision => "needs revision".red(),
            DocState::Draft => "draft".normal(),
        };
        let rev = s
            .accepted
            .as_ref()
            .map(|a| format!(" rev {}", a.revision))
            .unwrap_or_default();
        println!("  {:<24} {state}{rev}", s.file);
        if let Some(n) = &s.revision_note {
            println!("      requested: {n}");
        }
        for q in &v.open_questions {
            println!("      ? {q}");
        }
        for issue in &v.issues {
            let mark = match issue.severity {
                lint::Severity::Error => "✗".red(),
                lint::Severity::Warning => "⚠".yellow(),
            };
            println!("      {mark} {}", issue.message);
        }
    }
    Ok(())
}
