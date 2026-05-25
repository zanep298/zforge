use crate::registry::{
    io, lock,
    schema::{ProjectEntry, Registry, RegisteredBy},
    validate,
};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum ProjectCmd {
    Add(AddArgs),
    List(ListArgs),
    Remove(RemoveArgs),
    Switch(SwitchArgs),
    Current,
}

#[derive(Debug, Args)]
pub struct AddArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub name: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    pub name: String,
    #[arg(long)]
    pub purge: bool,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct SwitchArgs {
    pub name: String,
}

pub fn run(cmd: ProjectCmd) -> Result<()> {
    match cmd {
        ProjectCmd::Add(a) => add(a),
        ProjectCmd::List(a) => list(a),
        ProjectCmd::Remove(a) => remove(a),
        ProjectCmd::Switch(a) => switch(a),
        ProjectCmd::Current => current(),
    }
}

pub fn list_data() -> Result<Registry> {
    io::load()
}

pub fn add_entry(name: &str, path: &std::path::Path) -> Result<PathBuf> {
    validate::validate_name(name)?;
    let canon = validate::canonicalize_path(path)?;
    validate::ensure_zforge_dir(&canon)?;

    lock::with_lock(|| {
        let mut r = io::load()?;
        validate::ensure_unique_name(&r, name)?;
        if let Some(existing) = validate::find_by_path(&r, &canon) {
            return Err(anyhow!(
                "path {canon:?} already registered as {existing:?}",
                existing = existing.name,
            ));
        }
        r.projects.push(ProjectEntry {
            name: name.to_string(),
            path: canon.clone(),
            registered_at: Utc::now(),
            registered_by: RegisteredBy::Manual,
            agent_overrides: Default::default(),
        });
        io::save_atomic(&r)?;
        Ok(canon.clone())
    })
}

fn add(a: AddArgs) -> Result<()> {
    let canon = add_entry(&a.name, &a.path)?;
    println!("registered {} -> {}", a.name, canon.display());
    Ok(())
}

pub fn render_list(r: &Registry) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(
        out,
        "current: {}",
        r.current_project.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(out);
    let name_col = "NAME";
    let source_col = "SOURCE";
    let path_col = "PATH";
    let _ = writeln!(out, "{:<24} {:<8} {}", name_col, source_col, path_col);
    for e in &r.projects {
        let source = match e.registered_by {
            RegisteredBy::Init => "init",
            RegisteredBy::Manual => "manual",
        };
        let _ = writeln!(out, "{:<24} {:<8} {}", e.name, source, e.path.display());
    }
    out
}

fn list(a: ListArgs) -> Result<()> {
    let r = list_data()?;
    if a.json {
        let json = serde_json::to_string_pretty(&r).context("serialize registry to JSON")?;
        println!("{json}");
        return Ok(());
    }
    print!("{}", render_list(&r));
    Ok(())
}

pub fn remove_entry(name: &str) -> Result<PathBuf> {
    lock::with_lock(|| {
        let mut r = io::load()?;
        let pos = r
            .projects
            .iter()
            .position(|e| e.name == name)
            .ok_or_else(|| anyhow!("no project named {:?}", name))?;
        let removed = r.projects.remove(pos);
        if r.current_project.as_deref() == Some(name) {
            r.current_project = None;
        }
        io::save_atomic(&r)?;
        Ok::<_, anyhow::Error>(removed.path)
    })
}

/// Delete `.zforge/` under `target_path`. Returns true when a directory was removed,
/// false when nothing was there. Used by both CLI `--purge` and MCP `project_remove`.
pub fn purge_zforge_dir(target_path: &std::path::Path) -> Result<bool> {
    let zf = target_path.join(".zforge");
    if !zf.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&zf).with_context(|| format!("remove {zf:?}"))?;
    Ok(true)
}

/// Resolve a registered project's path without removing it. Lets `remove`
/// prompt for purge confirmation before mutating the registry — typing `n`
/// must be a full no-op, not just "skip the purge".
fn resolve_registered_path(name: &str) -> Result<PathBuf> {
    let r = io::load()?;
    r.projects
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.path.clone())
        .ok_or_else(|| anyhow!("no project named {:?}", name))
}

fn remove(a: RemoveArgs) -> Result<()> {
    let target_path = resolve_registered_path(&a.name)?;

    if a.purge && !a.yes {
        eprint!("purge {}? [y/N] ", target_path.join(".zforge").display());
        use std::io::Write;
        std::io::stderr().flush().ok();
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
        if !matches!(buf.trim(), "y" | "Y" | "yes") {
            eprintln!("aborted");
            return Ok(());
        }
    }

    remove_entry(&a.name)?;

    if a.purge {
        let zf = target_path.join(".zforge");
        if purge_zforge_dir(&target_path)? {
            println!("purged {}", zf.display());
        } else {
            eprintln!("note: {} did not exist", zf.display());
        }
    }

    println!("removed {}", a.name);
    Ok(())
}

pub fn switch_to(name: &str) -> Result<()> {
    lock::with_lock(|| {
        let mut r = io::load()?;
        if validate::find_by_name(&r, name).is_none() {
            return Err(anyhow!("no project named {:?}", name));
        }
        r.current_project = Some(name.to_string());
        io::save_atomic(&r)?;
        Ok(())
    })
}

fn switch(a: SwitchArgs) -> Result<()> {
    switch_to(&a.name)?;
    println!("current_project: {}", a.name);
    Ok(())
}

fn current() -> Result<()> {
    let r = io::load()?;
    match r.current_project {
        Some(n) => println!("{n}"),
        None => {
            eprintln!("no current project");
            std::process::exit(1);
        }
    }
    Ok(())
}
