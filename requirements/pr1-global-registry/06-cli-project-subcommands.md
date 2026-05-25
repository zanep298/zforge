# PR1-06 — `zforge project` CLI subcommands

## Goal

Expose registry CRUD via `zforge project add | list | remove | switch | current`.

## Files touched

- `src/cli/project.rs` (new)
- `src/cli/mod.rs` — declare `pub mod project;`
- `src/main.rs` — add `Project { ... }` variant to top-level `Commands` enum and dispatch

## Implementation

### `src/cli/project.rs`

```rust
use crate::registry::{
    io, lock, schema::{AgentSpec, ProjectEntry, RegisteredBy},
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

fn add(a: AddArgs) -> Result<()> {
    validate::validate_name(&a.name)?;
    let canon = validate::canonicalize_path(&a.path)?;
    validate::ensure_zforge_dir(&canon)?;

    lock::with_lock(|| {
        let mut r = io::load()?;
        validate::ensure_unique_name(&r, &a.name)?;
        if let Some(existing) = validate::find_by_path(&r, &canon) {
            return Err(anyhow!(
                "path {canon:?} already registered as {existing:?}",
                existing = existing.name,
            ));
        }
        r.projects.push(ProjectEntry {
            name: a.name.clone(),
            path: canon.clone(),
            registered_at: Utc::now(),
            registered_by: RegisteredBy::Manual,
            agent_overrides: Default::default(),
        });
        io::save_atomic(&r)?;
        Ok(())
    })?;

    println!("registered {} -> {}", a.name, canon.display());
    Ok(())
}

fn list(a: ListArgs) -> Result<()> {
    let r = io::load()?;
    if a.json {
        let json = serde_json::to_string_pretty(&r)
            .context("serialize registry to JSON")?;
        println!("{json}");
        return Ok(());
    }
    println!("current: {}", r.current_project.as_deref().unwrap_or("(none)"));
    println!();
    println!("{:<24} {:<8} {}", "NAME", "SOURCE", "PATH");
    for e in &r.projects {
        let source = match e.registered_by {
            RegisteredBy::Init => "init",
            RegisteredBy::Manual => "manual",
        };
        println!("{:<24} {:<8} {}", e.name, source, e.path.display());
    }
    Ok(())
}

fn remove(a: RemoveArgs) -> Result<()> {
    let target_path = lock::with_lock(|| {
        let mut r = io::load()?;
        let pos = r.projects.iter().position(|e| e.name == a.name)
            .ok_or_else(|| anyhow!("no project named {:?}", a.name))?;
        let removed = r.projects.remove(pos);
        if r.current_project.as_deref() == Some(&a.name) {
            r.current_project = None;
        }
        io::save_atomic(&r)?;
        Ok::<_, anyhow::Error>(removed.path)
    })?;

    if a.purge {
        if !a.yes {
            eprint!("purge {}? [y/N] ", target_path.display());
            use std::io::Write;
            std::io::stderr().flush().ok();
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf)?;
            if !matches!(buf.trim(), "y" | "Y" | "yes") {
                eprintln!("aborted");
                return Ok(());
            }
        }
        let zf = target_path.join(".zforge");
        if zf.exists() {
            std::fs::remove_dir_all(&zf)
                .with_context(|| format!("remove {zf:?}"))?;
            println!("purged {}", zf.display());
        }
    }

    println!("removed {}", a.name);
    Ok(())
}

fn switch(a: SwitchArgs) -> Result<()> {
    lock::with_lock(|| {
        let mut r = io::load()?;
        if validate::find_by_name(&r, &a.name).is_none() {
            return Err(anyhow!("no project named {:?}", a.name));
        }
        r.current_project = Some(a.name.clone());
        io::save_atomic(&r)?;
        Ok(())
    })?;
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
```

### `src/main.rs` wiring

Add to the `Commands` enum:

```rust
Project {
    #[command(subcommand)]
    cmd: crate::cli::project::ProjectCmd,
},
```

Dispatch:

```rust
Commands::Project { cmd } => cli::project::run(cmd),
```

## Acceptance criteria

- [ ] `zforge project add <path> --name foo` succeeds, registry contains the entry with `registered_by: manual`.
- [ ] `zforge project add` rejects duplicate name with non-zero exit code.
- [ ] `zforge project add` rejects path without `.zforge/`.
- [ ] `zforge project add` rejects name violating regex.
- [ ] `zforge project list` prints a table with `NAME`, `SOURCE`, `PATH` columns.
- [ ] `zforge project list --json` outputs valid JSON parseable by `jq`.
- [ ] `zforge project remove foo` removes the entry; `.zforge/` on disk untouched.
- [ ] `zforge project remove foo --purge --yes` deletes the `.zforge/` directory.
- [ ] `zforge project switch foo` updates `current_project`.
- [ ] `zforge project current` prints the current project name; exit code 1 if none.

## Depends on

- PR1-03, PR1-04.

## Notes

- Interactive prompt only triggers on `--purge` without `--yes` — keep simple stdin/stdout, no TUI deps.
- `remove` clears `current_project` if it pointed at the removed project. CLI must not leave a dangling pointer.

---

```yaml
status: done
owner: ""
pr: ""
```
