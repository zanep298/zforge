//! `zforge intake …` — create an intake, send its files for review and
//! record the user's decisions (v1.5 Mốc A, decisions D1/D2/D4).
//!
//! `accept` and `revise` are the user's decisions. They run only with a
//! terminal on stdin and stdout and a typed confirmation, and there is no
//! flag to skip that: an agent runs zforge through a tool without a TTY, so
//! it can prepare and review files but never accept them. The project's
//! `.claude/settings.json` also denies these commands to Claude.

use crate::cli::confirm;
use crate::config;
use crate::intake::{batch, hash, lint, review, status::DocState, Intake};
use anyhow::{anyhow, bail, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use std::io::{BufRead, IsTerminal, Write};

#[derive(Debug, Subcommand)]
pub enum IntakeCmd {
    /// Create `.zforge/intakes/<ID>/` with templates for 01-outcome,
    /// 02-behavior, 03-solution, 04-breakdown and `brief.md`, and empty
    /// `tasks/` and `changes/`.
    New {
        /// The intake ID, e.g. FEAT-001.
        id: String,
        /// Small profile: compact 02-behavior and 03-solution (one table
        /// each), for a change of at most two tasks in one module with no
        /// new interface.
        #[arg(long)]
        small: bool,
        /// Tiny profile: one task, TASK-001, created with the intake; 02/03/04
        /// are standard text pointing at it (its acceptance criteria are the
        /// behavior, its Constraints the binding decisions). For one task in
        /// one module with no new interface.
        #[arg(long, conflicts_with = "small")]
        tiny: bool,
    },
    /// A new intake amending one already integrated: its stages and tasks
    /// start as drafts copied from the source's accepted revisions.
    Amend {
        /// The new intake's ID.
        id: String,
        /// The intake it amends.
        #[arg(long)]
        from: String,
        /// The source handover to record (default: its latest).
        #[arg(long)]
        handover: Option<String>,
        /// A few contained fixes: a short 01-outcome with new requirements,
        /// 02/03 saying only what changes from the source's accepted
        /// revisions, and one task — instead of copies of every file.
        #[arg(long)]
        fixup: bool,
    },
    /// Add a leaf task contract `tasks/<TASK_ID>.md` from the template.
    Task {
        /// The intake ID, e.g. FEAT-001.
        id: String,
        /// The task ID, e.g. TASK-001.
        task_id: String,
    },
    /// Every file's review state, open questions and structural issues.
    Status {
        /// The intake ID, e.g. FEAT-001.
        id: String,
        /// Machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// The task dependency graph as Mermaid, from each task's `depends_on`.
    Graph {
        /// The intake ID, e.g. FEAT-001.
        id: String,
    },
    /// Send a file's current content for review as a new revision — or,
    /// with `--all`, every file that needs the user, in dependency order.
    Review(ReviewArgs),
    /// Accept the revision under review — or, with `--all`, every file
    /// under review, in dependency order. Needs an interactive terminal (in
    /// Claude Code the user types `/accept`).
    Accept(BatchArgs),
    /// Print the review pack: the brief, then each file under review with
    /// its summary and changes, the warnings, and how to decide.
    Pack {
        /// The intake ID, e.g. FEAT-001.
        id: String,
    },
    /// Ask for changes to the revision under review. Needs an interactive
    /// terminal (in Claude Code the user types `/revise`).
    Revise {
        #[command(flatten)]
        file: FileArgs,
        /// What needs to change.
        #[arg(long)]
        note: String,
    },
}

#[derive(Debug, Args)]
pub struct BatchArgs {
    /// The intake ID, e.g. FEAT-001.
    pub id: String,
    /// Intake-relative file: `01-outcome.md` … `04-breakdown.md`,
    /// `tasks/TASK-001.md`.
    #[arg(required_unless_present = "all", conflicts_with = "all")]
    pub file: Option<String>,
    /// Every file, in dependency order (stages, then tasks).
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct ReviewArgs {
    #[command(flatten)]
    pub batch: BatchArgs,
    /// What changed since the last revision and why — e.g. what the user
    /// asked for in chat. Recorded on the review; with `--all`, on every
    /// new revision. Warned about when a later revision is sent without it.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct FileArgs {
    /// The intake ID, e.g. FEAT-001.
    pub id: String,
    /// Intake-relative file: `01-outcome.md` … `04-breakdown.md`,
    /// `tasks/TASK-001.md`.
    pub file: String,
}

pub fn run(cmd: IntakeCmd) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zforge init"))?;
    let root = config.project_root();
    match cmd {
        IntakeCmd::New { id, small, tiny } => {
            let profile = review::NewProfile::from_flags(small, tiny)?;
            let i = review::create_profile(&root, &id, profile)?;
            println!(
                "{} created {}{}",
                "✓".green(),
                i.dir.display(),
                profile.label()
            );
            if profile == review::NewProfile::Tiny {
                println!(
                    "Next: draft 01-outcome.md and tasks/{}.md (02/03/04 are standard text — \
                     leave them unless the task cannot hold something), then `zforge intake \
                     review {id} --all` and `zforge intake pack {id}`",
                    review::TINY_TASK
                );
            } else {
                println!(
                    "Next: draft 01-outcome.md … 04-breakdown.md and the tasks, then \
                     `zforge intake review {id} --all` and `zforge intake pack {id}`"
                );
            }
            Ok(())
        }
        IntakeCmd::Amend {
            id,
            from,
            handover,
            fixup: true,
        } => {
            let f = crate::intake::amend::fixup(&root, &id, &from, handover.as_deref())?;
            println!(
                "{} created {id}, a fixup of {}",
                "✓".green(),
                f.origin.intake
            );
            for file in &f.written {
                let rev = f.refers_to.iter().find(|(s, _)| s == file).map(|(_, r)| *r);
                match rev {
                    Some(r) => println!("  {file:<28} draft, refers to {from} rev {r}"),
                    None => println!("  {file:<28} draft"),
                }
            }
            println!(
                "Next: write 01-outcome's problem and requirements (from {}), what changes in \
                 02/03 if anything, TASK-001 and brief.md, then `zforge intake review {id} \
                 --all` and `zforge intake pack {id}`; the user accepts them — `/accept {id} \
                 all` in Claude Code.",
                f.first_requirement
            );
            Ok(())
        }
        IntakeCmd::Amend {
            id, from, handover, ..
        } => {
            let a = crate::intake::amend::amend(&root, &id, &from, handover.as_deref())?;
            println!("{} created {id}, amending {}", "✓".green(), a.origin.intake);
            for (file, rev) in &a.copied {
                println!("  {file:<28} draft, from {from} rev {rev}");
            }
            for file in &a.skipped {
                println!("  {file:<28} not copied: {from} has no accepted revision");
            }
            println!(
                "Next: change only what changes and update brief.md, then `zforge intake review \
                 {id} --all` and `zforge intake pack {id}`; the user accepts them — \
                 `/accept {id} all` in Claude Code."
            );
            Ok(())
        }
        IntakeCmd::Task { id, task_id } => {
            let i = Intake::open(&root, &id)?;
            let path = review::create_task(&i, &task_id)?;
            println!("{} created {}", "✓".green(), path.display());
            Ok(())
        }
        IntakeCmd::Status { id, json } => status(&Intake::open(&root, &id)?, json),
        IntakeCmd::Graph { id } => {
            let nodes = crate::intake::graph::nodes(&Intake::open(&root, &id)?)?;
            print!("{}", crate::intake::graph::mermaid(&nodes));
            Ok(())
        }
        IntakeCmd::Review(r) => {
            let i = Intake::open(&root, &r.batch.id)?;
            let note = r.note.as_deref().unwrap_or("");
            match r.batch.file {
                Some(f) => send_for_review(&i, &f, note),
                None => send_all_for_review(&i, note),
            }
        }
        IntakeCmd::Accept(a) => {
            let i = Intake::open(&root, &a.id)?;
            match a.file {
                Some(f) => decide(&i, &f, None),
                None => accept_all(&i),
            }
        }
        IntakeCmd::Pack { id } => {
            print!(
                "{}",
                crate::intake::pack::render(&Intake::open(&root, &id)?)?
            );
            Ok(())
        }
        IntakeCmd::Revise { file, note } => {
            decide(&Intake::open(&root, &file.id)?, &file.file, Some(&note))
        }
    }
}

fn send_for_review(i: &Intake, rel: &str, note: &str) -> Result<()> {
    let r = review::review_noted(i, rel, note)?;
    for w in &r.warnings {
        eprintln!("{} {}: {}", "⚠".yellow(), w.file, w.message);
    }
    if let Some(w) = r.note_warning(rel) {
        eprintln!("{} {w}", "⚠".yellow());
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
            Some(_) => {
                // Unchanged: sent again because something it rests on
                // changed (D6). Say what.
                if let Some(why) = crate::intake::readiness::stale_reasons(i)
                    .ok()
                    .and_then(|r| r.get(rel).cloned())
                {
                    println!("Unchanged; to confirm again because {}", why.join("; "));
                }
            }
            None => println!("No accepted revision yet to compare with."),
        }
    }
    println!(
        "The user decides: `/accept {} {rel}` or `/revise {} {rel}: <note>` (at a terminal: `zforge intake accept|revise {} {rel}`)",
        i.id, i.id, i.id
    );
    Ok(())
}

/// `zforge intake review <ID> --all`.
fn send_all_for_review(i: &Intake, note: &str) -> Result<()> {
    let r = batch::review_all_noted(i, note)?;
    for (f, rev) in &r.sent {
        for w in &rev.warnings {
            eprintln!("{} {}: {}", "⚠".yellow(), w.file, w.message);
        }
        if let Some(w) = rev.note_warning(f) {
            eprintln!("{} {w}", "⚠".yellow());
        }
        if rev.unchanged {
            println!(
                "  {f} revision {} already under review ({})",
                rev.revision,
                hash::short(&rev.sha256)
            );
        } else {
            println!(
                "{} {f} sent for review as revision {} ({})",
                "✓".green(),
                rev.revision,
                hash::short(&rev.sha256)
            );
        }
    }
    if !r.kept.is_empty() {
        println!("  accepted and unchanged: {}", r.kept.join(", "));
    }
    if r.sent.is_empty() {
        println!("Nothing needs the user's review.");
        return Ok(());
    }
    println!(
        "Show the user `zforge intake pack {}`; they decide with `/accept {} all` or \
         `/revise {} <file>: <note>` (at a terminal: `zforge intake accept {} --all`)",
        i.id, i.id, i.id, i.id
    );
    Ok(())
}

/// `zforge intake accept <ID> --all`: every file under review, in
/// dependency order, after one typed confirmation.
fn accept_all(i: &Intake) -> Result<()> {
    confirm::require_terminal("zforge intake accept")?;
    let files = batch::under_review(i)?;
    if files.is_empty() {
        bail!("nothing of {} is under review", i.id);
    }
    let statuses = review::statuses(i)?;
    println!("{} {}, in this order:", "ACCEPT".bold(), i.id);
    for f in &files {
        match statuses
            .iter()
            .find(|s| &s.file == f)
            .and_then(|s| s.in_review.as_ref())
        {
            Some(r) => println!("  {f} revision {} ({})", r.revision, hash::short(&r.sha256)),
            None => println!("  {f}"),
        }
    }
    confirm::type_to_confirm("accept")?;
    let by = std::env::var("USER").ok().filter(|u| !u.is_empty());
    let mut failed = false;
    for (f, result) in batch::accept_all(i, &files, &crate::intake::record::Decider::terminal(by))?
    {
        match result {
            Ok(rev) => println!(
                "{} accepted {f} revision {} ({})",
                "✓".green(),
                rev.revision,
                hash::short(&rev.sha256)
            ),
            Err(e) => {
                failed = true;
                eprintln!("{} {f}: {e:#}", "✗".red());
            }
        }
    }
    if let Some(root) = i
        .dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        refresh_knowledge(root);
    }
    if failed {
        bail!("some files were not accepted");
    }
    Ok(())
}

/// Accept (`note = None`) or request changes, after a human confirms.
fn decide(i: &Intake, rel: &str, note: Option<&str>) -> Result<()> {
    let (verb, word) = match note {
        None => ("accept", "accept"),
        Some(_) => ("request changes to", "revise"),
    };
    confirm::require_terminal(&format!("zforge intake {word}"))?;
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
    confirm::type_to_confirm(word)?;
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
    if let Some(root) = i
        .dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        refresh_knowledge(root);
    }
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
                issues: lint::lint(&status.file, &text, &i.id, &known)
                    .into_iter()
                    .chain(crate::intake::crossref::lint(i, &status.file, &text))
                    .collect(),
                status,
            }
        })
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&views)?);
        return Ok(());
    }
    println!("{}  ({})", i.id.bold(), i.dir.display());
    if let Some(hint) = review::profile_hint(i) {
        println!("  {} {hint}", "ℹ".cyan());
    }
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

/// `zforge readiness <ID>`: check what a handover would pin (§6.2), write
/// the `readiness.md` view, fail when not ready.
pub fn readiness(id: &str, tasks: &[String], json: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zforge init"))?;
    let root = config.project_root();
    let i = Intake::open(&root, id)?;
    let r = crate::intake::readiness::check(&i, &root, tasks, &config)?
        .with(crate::intake::readiness::runtime(&root))
        .with_project(&config);
    let view = crate::intake::readiness::render(&r);
    crate::fs::write_atomic(&i.dir.join("readiness.md"), view.as_bytes())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&r)?);
    } else {
        print!("{view}");
    }
    if !r.ready {
        bail!("{id} is not ready to hand over");
    }
    Ok(())
}

/// `zforge handover <ID>`: the user hands the accepted contract over (§6.3).
/// Interactive terminal only, like `intake accept`.
pub fn handover(id: &str, tasks: &[String]) -> Result<()> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!(
            "`zforge handover` records the user's decision and needs an interactive terminal; \
             it cannot be run by an agent or from a script"
        );
    }
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zforge init"))?;
    let root = config.project_root();
    let i = Intake::open(&root, id)?;
    let r = crate::intake::readiness::check(&i, &root, tasks, &config)?
        .with(crate::intake::readiness::runtime(&root))
        .with_project(&config);
    if !r.ready {
        print!("{}", crate::intake::readiness::render(&r));
        bail!("{id} is not ready to hand over");
    }
    println!("{} {id}: {}", "HANDOVER".bold(), r.tasks.join(" → "));
    for f in &r.files {
        println!(
            "  {} rev {} ({})",
            f.file,
            f.revision,
            hash::short(&f.sha256)
        );
    }
    println!(
        "  baseline {}, at most {} verifier iteration(s), budget ${:.2} per run",
        config.knowledge.baseline,
        config.execution.max_iterations,
        config.execution.budget_usd.unwrap_or_default()
    );
    println!("  delivery: {}", crate::intake::handover::DELIVERY);
    print!("Type `handover` to confirm: ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if answer.trim() != "handover" {
        bail!("not confirmed; nothing recorded");
    }
    let by = std::env::var("USER").ok().filter(|u| !u.is_empty());
    let m = crate::intake::handover::create(&i, &root, tasks, &config, &r.files, by)?;
    println!("{} {} recorded", "✓".green(), m.id);
    refresh_knowledge(&root);
    Ok(())
}

/// `zforge knowledge index`: regenerate `commitments.md` (in
/// `knowledge.dir`) and `.zforge/knowledge/commitments.json`, and the
/// project knowledge's `INDEX.md`.
pub fn knowledge_index(json: bool) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zforge init"))?;
    let entries = crate::knowledge::commitments::write(&config)?;
    let (index, _) = crate::knowledge::index::write(&config)?;
    if !json {
        eprintln!("{} {}", "✓".green(), index.display());
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        print!("{}", crate::knowledge::commitments::render(&entries));
    }
    Ok(())
}

/// Keep the commitments current after a decision; a failure is reported,
/// not fatal.
fn refresh_knowledge(root: &std::path::Path) {
    let config = match config::load_from(&root.join(".zforge").join("config.yaml")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: commitments not updated: {e:#}");
            return;
        }
    };
    if let Err(e) = crate::knowledge::commitments::write(&config) {
        eprintln!("warning: commitments not updated: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-02: refreshing after a decision writes the new locations —
    /// `commitments.md` in `knowledge.dir`, `commitments.json` under
    /// `.zforge/knowledge/`.
    #[test]
    fn refresh_knowledge_writes_commitments_at_the_new_locations() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge").join("config.yaml"),
            "project:\n  name: t\n  language: rust\n  test_command: \"true\"\n",
        )
        .unwrap();

        refresh_knowledge(root);

        assert!(root.join("docs/knowledge/commitments.md").is_file());
        assert!(root.join(".zforge/knowledge/commitments.json").is_file());
    }
}
