//! The intake's task dependency graph as Mermaid, generated from each
//! task's `depends_on` — never drawn by hand, so it cannot drift from the
//! contracts (`zforge intake graph <ID>`).

use super::{lint, Intake, TASKS_DIR};
use anyhow::{Context, Result};

/// One task as the graph shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    pub title: String,
    pub depends_on: Vec<String>,
}

/// The intake's tasks, in file order, from their current files.
pub fn nodes(intake: &Intake) -> Result<Vec<Node>> {
    let prefix = format!("{TASKS_DIR}/");
    let mut out = Vec::new();
    for rel in intake.files() {
        let Some(id) = rel
            .strip_prefix(&prefix)
            .and_then(|f| f.strip_suffix(".md"))
        else {
            continue;
        };
        let path = intake.dir.join(&rel);
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let meta = lint::parse_task_meta(&text).map_err(|e| anyhow::anyhow!("{rel}: {e}"))?;
        out.push(Node {
            id: id.to_string(),
            title: title(&text, id),
            depends_on: meta.depends_on,
        });
    }
    Ok(out)
}

/// The task's `# TASK-001 — title` heading, without the id.
fn title(text: &str, id: &str) -> String {
    let (_, body) = lint::split_frontmatter(text);
    body.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|h| {
            h.trim()
                .trim_start_matches(id)
                .trim_start_matches([' ', '—', '-', ':'])
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

/// A `graph TD` block: one node per task, an arrow from each dependency to
/// the task that builds on it.
pub fn mermaid(nodes: &[Node]) -> String {
    let mut out = String::from("```mermaid\ngraph TD\n");
    for n in nodes {
        let label = match n.title.is_empty() {
            true => n.id.clone(),
            false => format!("{}<br/>{}", n.id, n.title.replace('"', "'")),
        };
        out.push_str(&format!("  {}[\"{label}\"]\n", n.id));
    }
    for n in nodes {
        for d in &n.depends_on {
            out.push_str(&format!("  {d} --> {}\n", n.id));
        }
    }
    out.push_str("```\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_and_their_dependencies_become_a_graph() {
        let tmp = tempfile::tempdir().unwrap();
        let i = super::super::review::create(tmp.path(), "F").unwrap();
        let task = |id: &str, deps: &str, title: &str| {
            std::fs::write(
                i.dir.join(format!("tasks/{id}.md")),
                format!("---\nid: {id}\nparent: F\nrequirements: []\ndepends_on: [{deps}]\n---\n\n# {id} — {title}\n"),
            )
            .unwrap();
        };
        task("TASK-001", "", "Parse \"items\"");
        task("TASK-002", "TASK-001", "");

        let got = mermaid(&nodes(&i).unwrap());
        assert_eq!(
            got,
            "```mermaid\ngraph TD\n  TASK-001[\"TASK-001<br/>Parse 'items'\"]\n  \
             TASK-002[\"TASK-002\"]\n  TASK-001 --> TASK-002\n```\n"
        );
    }
}
