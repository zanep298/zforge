//! Files the removed task pipeline left in a project.
//!
//! zforge no longer reads them, but it never deletes them: agent
//! definitions and slash commands carry no marker saying zforge wrote them,
//! and task folders, memory and cost logs are the user's records. `init`
//! lists what it finds so the user can remove it.

use std::path::{Path, PathBuf};

/// Relative paths the task pipeline wrote, per project.
const RETIRED: &[&str] = &[
    ".claude/agents/spec-agent.md",
    ".claude/agents/testspec-agent.md",
    ".claude/agents/plan-agent.md",
    ".claude/commands/zforge.md",
    ".codex/agents/spec-agent.md",
    ".codex/agents/testspec-agent.md",
    ".codex/agents/plan-agent.md",
    ".opencode/agents/spec-agent.md",
    ".opencode/agents/testspec-agent.md",
    ".opencode/agents/plan-agent.md",
    ".zforge/agents/spec-agent.md",
    ".zforge/agents/testspec-agent.md",
    ".zforge/agents/plan-agent.md",
    ".zforge/agents/spec.tmpl",
    ".zforge/agents/testspec.tmpl",
    ".zforge/agents/plan.tmpl",
    ".zforge/agents/code.tmpl",
    ".zforge/agents/review.tmpl",
    ".zforge/agents/verify-analysis.tmpl",
    ".zforge/tasks",
    ".zforge/memory",
    ".zforge/jobs",
    ".zforge/cost-log.jsonl",
];

/// The retired paths present under `root`.
pub(crate) fn found(root: &Path) -> Vec<PathBuf> {
    RETIRED
        .iter()
        .map(|rel| root.join(rel))
        .filter(|p| p.exists())
        .collect()
}

/// Tell the user which retired files remain, if any.
pub(crate) fn report(root: &Path) {
    let left = found(root);
    if left.is_empty() {
        return;
    }
    println!();
    println!("No longer used by zforge (the task pipeline was removed) — delete when you no longer need them:");
    for p in left {
        let shown = p.strip_prefix(root).unwrap_or(&p);
        println!("  {}", shown.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_what_is_there() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".claude/agents")).unwrap();
        std::fs::write(root.join(".claude/agents/spec-agent.md"), "x").unwrap();
        std::fs::write(root.join(".claude/agents/code-agent.md"), "x").unwrap();
        std::fs::create_dir_all(root.join(".zforge/tasks/T1")).unwrap();

        let left: Vec<PathBuf> = found(root)
            .into_iter()
            .map(|p| p.strip_prefix(root).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            left,
            [
                PathBuf::from(".claude/agents/spec-agent.md"),
                PathBuf::from(".zforge/tasks")
            ]
        );
        assert!(root.join(".zforge/tasks/T1").is_dir(), "nothing is deleted");
    }
}
