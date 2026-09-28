//! `zforge migrate [--dry-run] [--yes]`: move the current project from the
//! removed task pipeline to v1.5. See `crate::migrate::project` for what
//! changes; this adds the confirmation and the `init --force` refresh of
//! the clients the project has, which also migrates the global store.

use crate::migrate::project::{self, Plan};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

pub fn run(dry_run: bool, yes: bool) -> Result<()> {
    let root = find_root()?;
    let plan = project::plan(&root)?;
    if plan.is_empty() {
        println!("{} already on v1.5 — nothing to migrate", root.display());
        return Ok(());
    }
    print_plan(&root, &plan);
    if dry_run {
        return Ok(());
    }
    if !yes && !confirm()? {
        println!("Aborted.");
        return Ok(());
    }
    std::env::set_current_dir(&root).with_context(|| format!("change to {}", root.display()))?;
    let local = is_local(&root);
    let default_runner = current_runner(&root);
    let archive = project::apply_files(&root, &plan)?;
    if !plan.clients.is_empty() {
        println!();
        crate::cli::init::run_for(
            &plan.clients,
            crate::cli::init::InitOptions {
                agent: crate::cli::mcp_register::Agent::All,
                force: true,
                local,
                no_register: true,
                name: None,
                switch: false,
                default_runner: default_runner.filter(|r| plan.clients.contains(&r.as_str())),
                install_missing: false,
            },
        )?;
    }
    println!();
    println!("{} migrated to v1.5", "✓".green());
    println!("  archive: {}", archive.display());
    if plan.clients.is_empty() {
        println!(
            "  no client files found; set one up with {}",
            "zforge init --agent <client>".cyan()
        );
    }
    println!("  next: {}", "zforge status".cyan());
    Ok(())
}

/// The project whose `.zforge/` is at or above the working directory —
/// found by its config, or by the directory alone for a task-pipeline
/// project that never had one.
fn find_root() -> Result<PathBuf> {
    if let Some(cfg) = crate::config::Config::find_config_file() {
        if let Some(root) = cfg.parent().and_then(Path::parent) {
            return Ok(root.to_path_buf());
        }
    }
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join(".zforge").is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("no .zforge/ here or above — not a zforge project");
        }
    }
}

fn is_local(root: &Path) -> bool {
    crate::config::load_from(&root.join(".zforge/config.yaml"))
        .is_ok_and(|c| c.agents_dir().starts_with(root))
}

fn current_runner(root: &Path) -> Option<String> {
    crate::config::load_from(&root.join(".zforge/config.yaml"))
        .ok()
        .and_then(|c| c.runner.default)
}

fn print_plan(root: &Path, plan: &Plan) {
    println!("Migrate {} from the task pipeline to v1.5:", root.display());
    if !plan.archive.is_empty() {
        println!("  move to .zforge/v1-archive/<time>/:");
        for a in &plan.archive {
            println!("    {a}");
        }
    }
    if !plan.config_keys.is_empty() {
        println!(
            "  .zforge/config.yaml: remove {}",
            plan.config_keys.join(", ")
        );
    }
    if plan.add_execution {
        println!(
            "  .zforge/config.yaml: add the execution block (set budget_usd before a handover)"
        );
    }
    if plan.models {
        println!("  .zforge/models.yaml: remove the spec, testspec and plan phases");
    }
    if !plan.clients.is_empty() {
        println!(
            "  regenerate for {}: agents, skills, instruction files, settings (as init --force)",
            plan.clients.join(", ")
        );
    }
    if !plan.backups.is_empty() {
        println!("  copied to the archive first: {}", plan.backups.join(", "));
    }
    println!("  the global store (~/.zforge) is migrated too, when this project uses it");
}

fn confirm() -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!("not an interactive terminal — review the plan with --dry-run, then pass --yes");
    }
    print!("Proceed? [y/N] ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}
