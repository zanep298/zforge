//! `zforge init --git-exclude`: keep what init writes out of git without
//! touching a tracked file.
//!
//! The paths go into the repository's `info/exclude` (shared by every
//! worktree, never committed) as one block between [`BEGIN`] and [`END`].
//! Once the block exists every later `init` refreshes it, so files a newer
//! zforge writes are covered too; deleting the block turns it off.
//!
//! Only zforge's own files are listed — `.claude/agents/code-agent.md`, not
//! `.claude/agents/` — so the user's agents, skills and commands beside them
//! stay visible to git. A `CLAUDE.md` or command the project wrote itself is
//! not listed. Excluding does not untrack: a listed path git already tracks
//! is reported, never removed from the index.
//!
//! Runs keep working: `.zforge/` ignored means a run adds nothing to
//! `.gitignore` (`run::worktree::ensure_ignored`), and the agent still gets
//! Claude's configuration, which is brought into its worktree because git
//! ignores it (`run::agent_env::bring_claude_config`).

use super::{claude_commands, instructions};
use crate::run::git;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub(crate) const BEGIN: &str = "# >>> zforge init --git-exclude (refreshed by zforge init) >>>";
pub(crate) const END: &str = "# <<< zforge init --git-exclude <<<";

const AGENT_DIRS: [&str; 3] = [".claude/agents", ".codex/agents", ".opencode/agents"];
const RULES: [&str; 3] = ["coding-style.md", "testing.md", "security.md"];
const COMMANDS: [&str; 3] = ["accept.md", "revise.md", "handover.md"];
/// Files init writes or edits whatever their content.
const PLAIN: [&str; 4] = [
    ".claude/settings.json",
    ".codex/README.md",
    ".codex/config.toml",
    "opencode.json",
];

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Not asked for, and no block from an earlier init.
    Off,
    /// The project is not in a git repository.
    NotRepo,
    Written {
        file: PathBuf,
        entries: Vec<String>,
        /// Entries git already tracks: excluding them changes nothing.
        tracked: Vec<String>,
    },
}

/// Write (`enable`) or refresh (a block already there) the exclude block
/// for the project at `root`.
pub(crate) fn apply(root: &Path, enable: bool) -> Result<Outcome> {
    let Some((file, prefix)) = locate(root)? else {
        return Ok(if enable {
            Outcome::NotRepo
        } else {
            Outcome::Off
        });
    };
    let existing = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("read {}", file.display())),
    };
    if !enable && !existing.lines().any(|l| l == BEGIN) {
        return Ok(Outcome::Off);
    }
    let entries = entries(root);
    let updated = replace_block(&existing, &block(&entries, &prefix));
    if updated != existing {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::fs::write_atomic(&file, updated.as_bytes())
            .with_context(|| format!("write {}", file.display()))?;
    }
    let tracked = entries
        .iter()
        .filter(|e| is_tracked(root, e))
        .cloned()
        .collect();
    Ok(Outcome::Written {
        file,
        entries,
        tracked,
    })
}

/// The repository's exclude file and the project's path inside the
/// repository (`""` at its top, else `sub/dir/`); `None` outside git.
fn locate(root: &Path) -> Result<Option<(PathBuf, String)>> {
    let out = git::run(
        root,
        &["rev-parse", "--show-prefix", "--git-path", "info/exclude"],
    )?;
    if !out.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let prefix = lines.next().unwrap_or("").trim().to_string();
    let Some(path) = lines.next().map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    // `--git-path` answers relative to the directory git ran in.
    let path = PathBuf::from(path);
    let file = match path.is_absolute() {
        true => path,
        false => root.join(path),
    };
    Ok(Some((file, prefix)))
}

/// What init wrote at `root`, as paths relative to it (`/` ends a
/// directory). Only what exists is listed.
pub(crate) fn entries(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for dir in [".zforge", ".codegraph"] {
        if root.join(dir).is_dir() {
            out.push(format!("{dir}/"));
        }
    }
    for name in ["CLAUDE.md", "AGENTS.md"] {
        if read(root, name).is_some_and(|t| instructions::generated(&t)) {
            out.push(name.to_string());
        }
    }
    for dir in AGENT_DIRS {
        for (name, _) in crate::embedded::AGENTS {
            let rel = format!("{dir}/{name}");
            if root.join(&rel).is_file() {
                out.push(rel);
            }
        }
    }
    let skills = root.join(".claude/skills");
    let has_skill = std::fs::read_dir(&skills).is_ok_and(|mut d| {
        d.any(|e| {
            e.is_ok_and(|e| {
                e.file_name().to_string_lossy().starts_with("zforge-") && e.path().is_dir()
            })
        })
    });
    if has_skill {
        out.push(".claude/skills/zforge-*/".to_string());
    }
    for name in COMMANDS {
        let rel = format!(".claude/commands/{name}");
        if read(root, &rel).is_some_and(|t| t.contains(claude_commands::MARKER)) {
            out.push(rel);
        }
    }
    for name in RULES {
        let rel = format!(".claude/rules/{name}");
        if root.join(&rel).is_file() {
            out.push(rel);
        }
    }
    for rel in PLAIN {
        if root.join(rel).is_file() {
            out.push(rel.to_string());
        }
    }
    out
}

fn read(root: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(root.join(rel)).ok()
}

/// The block for `entries`, each anchored to the project's place in the
/// repository.
pub(crate) fn block(entries: &[String], prefix: &str) -> String {
    let mut out = format!("{BEGIN}\n");
    for e in entries {
        out.push_str(&format!("/{prefix}{e}\n"));
    }
    out.push_str(END);
    out.push('\n');
    out
}

/// `existing` with its block replaced by `block`, or `block` appended.
/// Everything outside the block is kept as it was.
pub(crate) fn replace_block(existing: &str, block: &str) -> String {
    let lines: Vec<&str> = existing.lines().collect();
    let begin = lines.iter().position(|l| *l == BEGIN);
    let end = begin.and_then(|b| lines[b..].iter().position(|l| *l == END).map(|e| b + e));
    match (begin, end) {
        (Some(b), Some(e)) => {
            let mut out = String::new();
            for l in &lines[..b] {
                out.push_str(l);
                out.push('\n');
            }
            out.push_str(block);
            for l in &lines[e + 1..] {
                out.push_str(l);
                out.push('\n');
            }
            out
        }
        _ => {
            let mut out = existing.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(block);
            out
        }
    }
}

fn is_tracked(root: &Path, entry: &str) -> bool {
    let spec = entry.trim_end_matches('/');
    git::run(root, &["ls-files", "--", spec])
        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_is_replaced_and_the_rest_kept() {
        let first = replace_block("# mine\n*.log", &block(&["a/".into()], ""));
        assert_eq!(first, format!("# mine\n*.log\n{BEGIN}\n/a/\n{END}\n"));

        let again = replace_block(
            &format!("{first}tail\n"),
            &block(&["a/".into(), "b.md".into()], "svc/"),
        );
        assert_eq!(
            again,
            format!("# mine\n*.log\n{BEGIN}\n/svc/a/\n/svc/b.md\n{END}\ntail\n")
        );
    }

    #[test]
    fn only_zforges_own_files_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |rel: &str, text: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        write(".zforge/config.yaml", "");
        write("CLAUDE.md", &format!("# x\n{}\n", instructions::MARKER));
        write("AGENTS.md", "# the project's own\n");
        write(".claude/agents/code-agent.md", "");
        write(".claude/agents/mine.md", "");
        write(".claude/skills/zforge-debug/SKILL.md", "");
        write(".claude/skills/mine/SKILL.md", "");
        write(
            ".claude/commands/accept.md",
            &format!("{} -->\n", claude_commands::MARKER),
        );
        write(".claude/commands/revise.md", "the project's own\n");
        write(".claude/settings.json", "{}");

        assert_eq!(
            entries(root),
            [
                ".zforge/",
                "CLAUDE.md",
                ".claude/agents/code-agent.md",
                ".claude/skills/zforge-*/",
                ".claude/commands/accept.md",
                ".claude/settings.json",
            ]
        );
    }
}
