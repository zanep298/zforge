//! Select the phase's named agent definition when launching a client
//! (FIX-014).
//!
//! `zforge init` renders one definition per phase — `spec-agent`,
//! `code-agent`, … — into `.claude/agents/` and `.opencode/agents/`, each
//! with its own instructions and model. Dispatch launched `claude -p` and
//! `opencode run` without naming one, so the definitions were written but
//! never used; the legacy path even printed "agent: spec-agent" while not
//! passing it. Both CLIs take `--agent <name>` (claude 2.1, opencode 1.14).
//!
//! Codex has no equivalent; its per-phase configuration is the
//! `zforge_<phase>` profile (see `model_args::profile_args_for_agent`).
//!
//! Declared fallback: when the definition file is missing the client runs
//! without `--agent` and a warning names the missing file. Re-running
//! `zforge init --agent <client>` restores it.

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum NamedAgent {
    /// Pass `--agent <name>`.
    Use(String),
    /// The client supports named agents but the definition is missing.
    Missing(PathBuf),
    /// The client has no named-agent mechanism (codex, custom runners).
    NotApplicable,
}

impl NamedAgent {
    pub fn args(&self) -> Vec<String> {
        match self {
            Self::Use(name) => vec!["--agent".into(), name.clone()],
            _ => Vec::new(),
        }
    }
}

/// Definitions directory for clients that support `--agent`.
fn definitions_dir(agent_name: &str, project_root: &Path) -> Option<PathBuf> {
    match agent_name {
        "claude" => Some(project_root.join(".claude").join("agents")),
        "opencode" => Some(project_root.join(".opencode").join("agents")),
        _ => None,
    }
}

pub fn named_agent_for(agent_name: &str, phase: &str, project_root: &Path) -> NamedAgent {
    let Some(dir) = definitions_dir(agent_name, project_root) else {
        return NamedAgent::NotApplicable;
    };
    let name = format!("{phase}-agent");
    let file = dir.join(format!("{name}.md"));
    if file.exists() {
        NamedAgent::Use(name)
    } else {
        NamedAgent::Missing(file)
    }
}

/// Warning for [`NamedAgent::Missing`].
pub fn missing_warning(agent_name: &str, phase: &str, file: &Path) -> String {
    format!(
        "warning: {agent_name} has no definition for the {phase} phase at {} — running \
         without --agent (phase instructions and model from the definition are not applied). \
         Restore it with: zforge init --agent {agent_name} --force",
        file.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with(client: &str, phase: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(format!(".{client}")).join("agents");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{phase}-agent.md")), "---\nname: x\n---\n").unwrap();
        tmp
    }

    #[test]
    fn claude_and_opencode_use_the_phase_definition() {
        for client in ["claude", "opencode"] {
            let p = project_with(client, "spec");
            let na = named_agent_for(client, "spec", p.path());
            assert_eq!(na, NamedAgent::Use("spec-agent".into()));
            assert_eq!(na.args(), vec!["--agent", "spec-agent"]);
        }
    }

    #[test]
    fn missing_definition_is_reported_not_silently_dropped() {
        let p = project_with("claude", "spec");
        match named_agent_for("claude", "code", p.path()) {
            NamedAgent::Missing(f) => assert!(f.ends_with(".claude/agents/code-agent.md")),
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    #[test]
    fn codex_and_custom_runners_have_no_named_agent() {
        let p = project_with("claude", "spec");
        assert_eq!(
            named_agent_for("codex", "spec", p.path()),
            NamedAgent::NotApplicable
        );
        assert_eq!(
            named_agent_for("primary", "spec", p.path()),
            NamedAgent::NotApplicable
        );
        assert!(NamedAgent::NotApplicable.args().is_empty());
    }
}
