//! What zforge set up for an agent spawn, recorded next to what the client
//! reports it did (IMP-006).

use super::agent_args::NamedAgent;
use crate::trace::{Expected, KnowledgeFile};
use std::path::Path;

/// MCP server every phase prompt points agents at, when the project has an
/// index for it.
const CODEGRAPH: &str = "codegraph";

/// What zforge set up for this spawn. `knowledge` and `knowledge_items` are
/// this task's project knowledge, as given to the prompt (ONBOARD TASK-007,
/// AC-04): the pinned files and revisions it was drawn from, and the item
/// IDs the selection actually included — empty when the handover pins no
/// knowledge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn expected_for(
    agent_name: &str,
    phase: &str,
    project_root: &Path,
    named: &NamedAgent,
    model: Option<&str>,
    language: Option<&str>,
    knowledge: &[(String, u32)],
    knowledge_items: &[String],
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
        knowledge: knowledge
            .iter()
            .map(|(file, revision)| KnowledgeFile {
                file: file.clone(),
                revision: *revision,
            })
            .collect(),
        knowledge_items: knowledge_items.to_vec(),
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
            "review",
            tmp.path(),
            &NamedAgent::Use("review-agent".into()),
            Some("sonnet"),
            Some("rust"),
            &[("rules.md".to_string(), 3)],
            &["RULE-001".to_string()],
        );
        assert_eq!(e.named_agent.as_deref(), Some("review-agent"));
        assert_eq!(e.model.as_deref(), Some("sonnet"));
        assert_eq!(e.skills, vec!["zforge-review-patch"]);
        assert_eq!(e.mcp_servers, vec!["codegraph"]);
        assert_eq!(
            e.knowledge,
            vec![crate::trace::KnowledgeFile {
                file: "rules.md".into(),
                revision: 3
            }]
        );
        assert_eq!(e.knowledge_items, vec!["RULE-001".to_string()]);
    }

    #[test]
    fn nothing_is_expected_that_was_not_set_up() {
        let tmp = tempfile::tempdir().unwrap();
        let e = expected_for(
            "claude",
            "code",
            tmp.path(),
            &NamedAgent::Missing(tmp.path().join("x.md")),
            None,
            Some("rust"),
            &[],
            &[],
        );
        assert_eq!(
            e,
            Expected::default(),
            "no agent → no preloaded skills; no index → no server"
        );
    }
}
