//! What a run's agent needs besides the contract.
//!
//! A run works in a worktree, which holds only what the project committed.
//! The checklists and agent definitions `zforge init` wrote are often not
//! committed — the skills store lives outside the project, and `.claude/` is
//! commonly ignored — so without help the agent would start without them.
//! Two layers make sure it does not:
//!
//! 1. [`checklists`]: the code phase's checklists, named in the prompt by
//!    their absolute path in the skills store. Any client can read a path
//!    outside the worktree; whether it does is the agent's choice.
//! 2. [`bring_claude_config`]: Claude's `.claude/agents`, `.claude/skills`
//!    and `.claude/settings.json` from the main checkout, so `--agent
//!    code-agent` resolves, its skills are preloaded, and the project's
//!    permission rules and hooks apply. Brought only where git ignores them:
//!    then sealing the output cannot commit them. One that is neither
//!    committed nor ignored is left out, with a warning, since the output
//!    would carry it.

use super::git;
use crate::config::Config;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Claude's project configuration a run's agent depends on: directories
/// and files, relative to the project root.
pub const CLAUDE_CONFIG: [&str; 3] = [".claude/agents", ".claude/skills", ".claude/settings.json"];

/// Absolute paths of the code phase's checklists that exist in the store.
pub fn checklists(config: &Config) -> Vec<PathBuf> {
    // `paths.skills` is often `./.zforge/skills`; drop the `.` so the path
    // the agent reads is plain.
    let store: PathBuf = config
        .skills_dir()
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();
    crate::cli::init::claude_skills::sources_for("code", &config.project.language)
        .into_iter()
        .map(|s| store.join(s))
        .filter(|p| p.is_file())
        .collect()
}

/// What [`bring_claude_config`] did for each path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Brought {
    /// Copied from the main checkout.
    Copied(String),
    /// Committed, so the worktree already has it.
    Present(String),
    /// Neither committed nor ignored: not copied.
    NotIgnored(String),
}

/// Bring [`CLAUDE_CONFIG`] into `work_dir` from `project_root` where the
/// worktree lacks it and git ignores it. What the main checkout does not
/// have is skipped.
pub fn bring_claude_config(project_root: &Path, work_dir: &Path) -> Result<Vec<Brought>> {
    let mut out = Vec::new();
    for rel in CLAUDE_CONFIG {
        let src = project_root.join(rel);
        let is_dir = src.is_dir();
        if !is_dir && !src.is_file() {
            continue;
        }
        let dst = work_dir.join(rel);
        if dst.exists() {
            out.push(Brought::Present(rel.to_string()));
            continue;
        }
        // A directory is ignored when what would go in it is.
        let probe = match is_dir {
            true => format!("{rel}/zforge-probe"),
            false => rel.to_string(),
        };
        let ignored = git::run(work_dir, &["check-ignore", "-q", &probe])?
            .status
            .success();
        if !ignored {
            out.push(Brought::NotIgnored(rel.to_string()));
            continue;
        }
        let copied = match is_dir {
            true => copy_dir(&src, &dst),
            false => copy_file(&src, &dst),
        };
        copied.with_context(|| format!("bring {rel} into the worktree"))?;
        out.push(Brought::Copied(rel.to_string()));
    }
    Ok(out)
}

fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)?;
    Ok(())
}

/// Copy a directory tree, following symlinks to what they point at.
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else if from.is_file() {
            copy_file(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(ignore: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("main");
        let work = dir.path().canonicalize().unwrap().join("work");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".gitignore"), ignore).unwrap();
        for args in [
            &["init", "-q", "-b", "main", "."][..],
            &["add", "-A"],
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "base",
            ],
        ] {
            git::ok(&root, args).unwrap();
        }
        git::ok(
            &root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "run",
                &work.display().to_string(),
            ],
        )
        .unwrap();
        let agents = root.join(".claude/agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(agents.join("code-agent.md"), "---\nname: code-agent\n---\n").unwrap();
        let skill = root.join(".claude/skills/zforge-debug");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "---\nname: zforge-debug\n---\n").unwrap();
        std::fs::write(
            root.join(".claude/settings.json"),
            r#"{"permissions":{"deny":["Bash(zforge intake accept:*)"]}}"#,
        )
        .unwrap();
        (dir, root, work)
    }

    #[test]
    fn ignored_claude_config_is_brought_and_never_committed() {
        let (_d, root, work) = repo(".claude/\n");

        let brought = bring_claude_config(&root, &work).unwrap();

        assert_eq!(
            brought,
            [
                Brought::Copied(".claude/agents".into()),
                Brought::Copied(".claude/skills".into()),
                Brought::Copied(".claude/settings.json".into())
            ]
        );
        assert!(work.join(".claude/agents/code-agent.md").is_file());
        assert!(work.join(".claude/skills/zforge-debug/SKILL.md").is_file());
        assert_eq!(
            std::fs::read_to_string(work.join(".claude/settings.json")).unwrap(),
            std::fs::read_to_string(root.join(".claude/settings.json")).unwrap()
        );
        assert!(git::is_clean(&work).unwrap(), "ignored, so nothing to seal");
    }

    #[test]
    fn config_that_is_neither_committed_nor_ignored_is_left_out() {
        let (_d, root, work) = repo("");

        let brought = bring_claude_config(&root, &work).unwrap();

        assert_eq!(
            brought,
            [
                Brought::NotIgnored(".claude/agents".into()),
                Brought::NotIgnored(".claude/skills".into()),
                Brought::NotIgnored(".claude/settings.json".into())
            ]
        );
        assert!(!work.join(".claude").exists());
    }

    #[test]
    fn committed_config_is_already_there() {
        let (_d, root, work) = repo("");
        git::ok(&root, &["add", "-A"]).unwrap();
        git::ok(
            &root,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "claude config",
            ],
        )
        .unwrap();
        git::ok(&work, &["merge", "-q", "--ff-only", "main"]).unwrap();

        let brought = bring_claude_config(&root, &work).unwrap();

        assert_eq!(
            brought,
            [
                Brought::Present(".claude/agents".into()),
                Brought::Present(".claude/skills".into()),
                Brought::Present(".claude/settings.json".into())
            ]
        );
    }

    #[test]
    fn checklists_are_the_code_phase_files_in_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let store = root.join("store");
        std::fs::create_dir_all(&store).unwrap();
        for f in ["write-tests-first.md", "rust-patterns.md", "debug.md"] {
            std::fs::write(store.join(f), "x").unwrap();
        }
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        let cfg = root.join(".zforge/config.yaml");
        std::fs::write(
            &cfg,
            format!(
                "project:\n  name: t\n  language: rust\npaths:\n  skills: {}\n",
                store.display()
            ),
        )
        .unwrap();
        let config = crate::config::load_from(&cfg).unwrap();

        let found = checklists(&config);

        // Only code-phase checklists, only those present: `debug` is not a
        // code-phase preload, `implement-minimal-patch` and `rust-testing`
        // are missing from this store.
        assert_eq!(
            found,
            [
                store.join("write-tests-first.md"),
                store.join("rust-patterns.md")
            ]
        );
    }
}
