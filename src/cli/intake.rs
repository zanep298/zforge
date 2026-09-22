//! `zforge intake …` — create an intake, send its files for review and
//! record the user's decisions (v1.5 Mốc A, decisions D1/D2/D4).
//!
//! `accept` and `revise` are the user's decisions. They run only with a
//! terminal on stdin and stdout and a typed confirmation, and there is no
//! flag to skip that: an agent runs zforge through a tool without a TTY, so
//! it can prepare and review files but never accept them. The project's
//! `.claude/settings.json` also denies these commands to Claude.

use crate::config;
use crate::intake::{hash, lint, review, status::DocState, Intake};
use anyhow::{anyhow, bail, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use std::io::{BufRead, IsTerminal, Write};

#[derive(Debug, Subcommand)]
pub enum IntakeCmd {
    /// Create `.zforge/intakes/<ID>/` with templates for every stage.
    New { id: String },
    /// Add a leaf task contract `tasks/<TASK_ID>.md` from the template.
    Task { id: String, task_id: String },
    /// Every file's review state, open questions and structural issues.
    Status {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Send a file's current content for review as a new revision.
    Review(FileArgs),
    /// Accept the revision under review. Interactive terminal only.
    Accept(FileArgs),
    /// Ask for changes to the revision under review. Interactive terminal only.
    Revise {
        #[command(flatten)]
        file: FileArgs,
        /// What needs to change.
        #[arg(long)]
        note: String,
    },
}

#[derive(Debug, Args)]
pub struct FileArgs {
    pub id: String,
    /// Intake-relative file: `01-outcome.md` … `04-breakdown.md`,
    /// `tasks/TASK-001.md`.
    pub file: String,
}

pub fn run(cmd: IntakeCmd) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let root = config.project_root();
    match cmd {
        IntakeCmd::New { id } => {
            let i = review::create(&root, &id)?;
            println!("{} created {}", "✓".green(), i.dir.display());
            println!("Next: fill 01-outcome.md, then `zforge intake review {id} 01-outcome.md`");
            Ok(())
        }
        IntakeCmd::Task { id, task_id } => {
            let i = Intake::open(&root, &id)?;
            let path = review::create_task(&i, &task_id)?;
            println!("{} created {}", "✓".green(), path.display());
            Ok(())
        }
        IntakeCmd::Status { id, json } => status(&Intake::open(&root, &id)?, json),
        IntakeCmd::Review(a) => send_for_review(&Intake::open(&root, &a.id)?, &a.file),
        IntakeCmd::Accept(a) => decide(&Intake::open(&root, &a.id)?, &a.file, None),
        IntakeCmd::Revise { file, note } => {
            decide(&Intake::open(&root, &file.id)?, &file.file, Some(&note))
        }
    }
}

fn send_for_review(i: &Intake, rel: &str) -> Result<()> {
    let r = review::review(i, rel)?;
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
            None => println!("First revision of this file."),
        }
    }
    println!(
        "The user decides at a terminal: `zforge intake accept {} {rel}` or `zforge intake revise {} {rel} --note …`",
        i.id, i.id
    );
    Ok(())
}

/// Accept (`note = None`) or request changes, after a human confirms.
fn decide(i: &Intake, rel: &str, note: Option<&str>) -> Result<()> {
    let (verb, word) = match note {
        None => ("accept", "accept"),
        Some(_) => ("request changes to", "revise"),
    };
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!(
            "`zforge intake {word}` records the user's decision and needs an interactive terminal; \
             it cannot be run by an agent or from a script"
        );
    }
    let pending = review::pending_review(i, rel)?;
    println!(
        "{} {}/{rel} revision {} ({})",
        verb.to_uppercase().bold(),
        i.id,
        pending.revision,
        hash::short(&pending.sha256)
    );
    if let Some(n) = note {
        println!("note: {n}");
    }
    print!("Type `{word}` to confirm: ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if answer.trim() != word {
        bail!("not confirmed; nothing recorded");
    }
    let by = std::env::var("USER").ok().filter(|u| !u.is_empty());
    let rev = match note {
        None => review::accept(i, rel, by)?,
        Some(n) => review::revise(i, rel, by, n)?,
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

fn status(i: &Intake, json: bool) -> Result<()> {
    let known = review::known(i);
    let views: Vec<FileView> = review::statuses(i)?
        .into_iter()
        .map(|status| {
            let text = std::fs::read_to_string(i.dir.join(&status.file)).unwrap_or_default();
            FileView {
                open_questions: lint::open_questions(&text),
                issues: lint::lint(&status.file, &text, &i.id, &known),
                status,
            }
        })
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&views)?);
        return Ok(());
    }
    println!("{}  ({})", i.id.bold(), i.dir.display());
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
