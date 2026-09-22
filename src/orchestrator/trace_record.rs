//! Append a phase trace for every agent spawn (IMP-006).
//!
//! Best-effort like cost telemetry: a trace that cannot be written must not
//! fail the phase, but it is reported on stderr.

use super::agent_args::NamedAgent;
use crate::trace::{self, Expected, Invocation};
use std::path::Path;

/// MCP server every phase prompt points agents at, when the project has an
/// index for it.
const CODEGRAPH: &str = "codegraph";

/// What zforge set up for this spawn.
pub(crate) fn expected_for(
    agent_name: &str,
    phase: &str,
    project_root: &Path,
    named: &NamedAgent,
    model: Option<&str>,
    language: Option<&str>,
) -> Expected {
    let named_agent = match named {
        NamedAgent::Use(name) => Some(name.clone()),
        _ => None,
    };
    // Preloaded skills come from the named agent's `skills:` frontmatter,
    // which only the Claude definitions carry.
    let skills = match (&named_agent, agent_name) {
        (Some(_), "claude") => {
            crate::cli::init::claude_skills::required_for(phase, language.unwrap_or(""))
        }
        _ => Vec::new(),
    };
    let mcp_servers = if project_root.join(".codegraph").is_dir() {
        vec![CODEGRAPH.to_string()]
    } else {
        Vec::new()
    };
    Expected {
        named_agent,
        model: model.map(str::to_string),
        skills,
        mcp_servers,
    }
}

pub(crate) fn record(tasks_dir: &Path, invocation: Invocation<'_>) {
    let entry = trace::from_invocation(invocation);
    if let Err(e) = trace::log::append(tasks_dir, &entry) {
        eprintln!("warning: trace append failed: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_phase_expects_its_agent_skills_and_codegraph() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".codegraph")).unwrap();
        let e = expected_for(
            "claude",
            "spec",
            tmp.path(),
            &NamedAgent::Use("spec-agent".into()),
            Some("sonnet"),
            Some("rust"),
        );
        assert_eq!(e.named_agent.as_deref(), Some("spec-agent"));
        assert_eq!(e.model.as_deref(), Some("sonnet"));
        assert_eq!(e.skills, vec!["zforge-clarify-spec"]);
        assert_eq!(e.mcp_servers, vec!["codegraph"]);
    }

    #[test]
    fn nothing_is_expected_that_was_not_set_up() {
        let tmp = tempfile::tempdir().unwrap();
        let e = expected_for(
            "claude",
            "spec",
            tmp.path(),
            &NamedAgent::Missing(tmp.path().join("x.md")),
            None,
            Some("rust"),
        );
        assert_eq!(
            e,
            Expected::default(),
            "no agent → no preloaded skills; no index → no server"
        );
    }
}
