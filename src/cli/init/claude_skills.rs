//! Native Claude Code skills (IMP-004, Claude first).
//!
//! zforge's skills were plain Markdown checklists under the skills store,
//! reachable only because CLAUDE.md told the agent to open them by path.
//! Claude Code has its own skill mechanism: `.claude/skills/<name>/SKILL.md`
//! whose `description` Claude reads to decide when to load the body, and a
//! subagent `skills:` field that injects named skills in full when the agent
//! starts. This module is the single catalog that drives all of it:
//!
//! - one `SKILL.md` per checklist, named `zforge-<stem>` so zforge-managed
//!   skills never collide with the user's own;
//! - the phase agents' `skills:` lists — the checklists each phase must
//!   follow are preloaded, not merely suggested;
//! - the skills section of CLAUDE.md.
//!
//! Verified against Claude Code 2.1: `claude plugin validate .claude/skills`
//! accepts the generated files without warnings. Whether the model then
//! invokes a skill on its own is not something a non-interactive check can
//! show; preloading through `skills:` does not depend on that.

use super::Vars;
use anyhow::{Context, Result};
use std::path::Path;

/// Prefix for every zforge-managed skill directory.
pub(crate) const PREFIX: &str = "zforge-";

/// A base (language-independent) skill: embedded source key, description,
/// and the phases whose agent preloads it (empty = model-invocable only).
struct BaseSkill {
    source: &'static str,
    description: &'static str,
    phases: &'static [&'static str],
}

const BASE: &[BaseSkill] = &[
    BaseSkill {
        source: "clarify-spec.md",
        description: "Turn a vague task description into an unambiguous, scoped specification: goals, non-goals, acceptance criteria, open questions. Use when writing or revising spec.md.",
        phases: &["spec"],
    },
    BaseSkill {
        source: "derive-test-cases.md",
        description: "Derive a complete set of test cases from an approved spec — happy paths, edge cases, errors, regressions — before any code is written. Use when writing testspec.md.",
        phases: &["testspec"],
    },
    BaseSkill {
        source: "implementation-planning.md",
        description: "Produce a concrete, ordered implementation plan (files, interfaces, steps, risks) that can be executed without further design decisions. Use when writing plan.md.",
        phases: &["plan"],
    },
    BaseSkill {
        source: "write-tests-first.md",
        description: "TDD discipline: write the failing test first, confirm it fails for the right reason, then implement. Use before writing any production code.",
        phases: &["code"],
    },
    BaseSkill {
        source: "implement-minimal-patch.md",
        description: "Make the smallest working change that satisfies the approved spec and passes the tests, without unrelated refactors. Use while implementing a task.",
        phases: &["code"],
    },
    BaseSkill {
        source: "review-patch.md",
        description: "Review a completed implementation against its approved spec, testspec and plan: correctness, coverage, scope creep, risks. Use when reviewing a change or writing review-summary.md.",
        phases: &["review"],
    },
    BaseSkill {
        source: "debug.md",
        description: "Diagnose the root cause of a bug or unexpected behavior — reproduce, isolate, explain — before writing any fix. Use when something fails and the cause is not yet known.",
        phases: &[],
    },
    BaseSkill {
        source: "intake.md",
        description: "Prepare a zforge v1.5 intake with the user — outcome, behavior, solution, breakdown and leaf task contracts under .zforge/intakes/ — and send each file for review. Use when a request should be clarified and broken down before implementation, or when `zforge intake status` shows files to write or revise.",
        phases: &[],
    },
    BaseSkill {
        source: "security-review.md",
        description: "Find security vulnerabilities in a changeset before merge: injection, authn/authz, secrets, unsafe input handling, dependency risks. Use when a change touches user input, auth, data access or external calls.",
        phases: &[],
    },
    BaseSkill {
        source: "performance-optimize.md",
        description: "Fix a measurable performance problem using data: measure, locate the bottleneck, change one thing, measure again. Use when a task is about speed, memory or resource use.",
        phases: &[],
    },
    BaseSkill {
        source: "backend/api-contracts.md",
        description: "Keep API contracts explicit and backward compatible: schemas, versioning, error shapes, compatibility. Use before adding or changing routes, RPC methods or public interfaces.",
        phases: &[],
    },
    BaseSkill {
        source: "backend/database-migrations.md",
        description: "Safe schema and data migrations: reversibility, locking, indexes, backfills, rollout order. Use when a change touches database schema, indexes or persisted data.",
        phases: &[],
    },
    BaseSkill {
        source: "backend/observability.md",
        description: "Add logs, metrics and traces that make failures diagnosable. Use when a change affects request handling, background work, external calls or failure paths.",
        phases: &[],
    },
    BaseSkill {
        source: "backend/background-jobs.md",
        description: "Reliable queues, cron jobs and workers: idempotency, retries, backoff, poison messages. Use when touching retry loops, scheduled jobs or message consumers.",
        phases: &[],
    },
    BaseSkill {
        source: "frontend/react-patterns.md",
        description: "React component, state and effect patterns with accessible interactions. Use for React, Next.js, Remix or similar UI and route changes.",
        phases: &[],
    },
    BaseSkill {
        source: "frontend/frontend-testing.md",
        description: "Test UI behavior the way users see it rather than implementation details. Use before adding or changing frontend tests.",
        phases: &[],
    },
    BaseSkill {
        source: "frontend/accessibility.md",
        description: "Accessibility for UI work: semantics, keyboard navigation, focus, labels, contrast, custom controls and tables. Use for any user-facing UI change.",
        phases: &[],
    },
    BaseSkill {
        source: "frontend/figma-to-ui.md",
        description: "Turn Figma designs or Figma MCP output into faithful, maintainable UI code. Use when implementing a design.",
        phases: &[],
    },
    BaseSkill {
        source: "frontend/state-data-fetching.md",
        description: "Frontend state and data fetching: forms, caches, mutations, optimistic UI, route data loading. Use when a UI change reads or writes server data.",
        phases: &[],
    },
];

/// One skill as written to `.claude/skills/<name>/SKILL.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeSkill {
    pub name: String,
    pub description: String,
    /// Phases whose agent preloads this skill.
    pub phases: Vec<&'static str>,
    /// Source file name, relative to the skills store.
    pub source: String,
    body: &'static str,
}

/// `zforge-<stem>` for a skills-store file such as `backend/api-contracts.md`.
fn native_name(source: &str) -> String {
    let stem = Path::new(source)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{PREFIX}{stem}")
}

fn language_label(lang: &str) -> &'static str {
    match lang {
        "rust" => "Rust",
        "go" => "Go",
        "typescript" => "TypeScript",
        "python" => "Python",
        "ios" => "iOS (Swift)",
        "android" => "Android (Kotlin)",
        "flutter" => "Flutter (Dart)",
        _ => "project",
    }
}

/// Description and whether the code agent preloads it, by file-name shape.
/// Core `<lang>-patterns` / `<lang>-testing` are preloaded; UI and
/// device-level variants stay model-invocable to keep the agent's context
/// lean.
fn language_skill(lang: &str, stem: &str) -> (String, bool) {
    let l = language_label(lang);
    if stem.ends_with("-ui-patterns") || stem.ends_with("-compose-ui") {
        (
            format!("{l} UI patterns for screens and reusable components. Use when building or changing {l} UI."),
            false,
        )
    } else if stem.ends_with("-instrumented-testing") || stem.ends_with("-snapshot-accessibility") {
        (
            format!("{l} device-level, snapshot and accessibility testing. Use when UI or platform integration needs test coverage beyond unit tests."),
            false,
        )
    } else if stem.ends_with("-testing") {
        (
            format!("How this project writes and runs {l} tests: structure, naming, fixtures, the test command. Use before adding or changing {l} tests."),
            true,
        )
    } else {
        (
            format!("{l} conventions for this project: idioms, error handling, module organization. Use before writing or changing {l} code."),
            true,
        )
    }
}

/// Every native skill for a project in `language`.
pub(crate) fn catalog(language: &str) -> Vec<NativeSkill> {
    let embedded: std::collections::HashMap<&str, &'static str> =
        crate::embedded::SKILLS.iter().copied().collect();
    let mut out: Vec<NativeSkill> = BASE
        .iter()
        .map(|b| NativeSkill {
            name: native_name(b.source),
            description: b.description.to_string(),
            phases: b.phases.to_vec(),
            source: b.source.to_string(),
            body: embedded
                .get(b.source)
                .copied()
                .unwrap_or_else(|| panic!("catalog names unknown skill {}", b.source)),
        })
        .collect();
    for (file, body) in crate::embedded::lang_skill_templates(language) {
        let stem = Path::new(&file)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (description, preload) = language_skill(language, &stem);
        out.push(NativeSkill {
            name: native_name(&file),
            description,
            phases: if preload { vec!["code"] } else { vec![] },
            source: file,
            body,
        });
    }
    out
}

/// Skills the `<phase>-agent` preloads.
pub(crate) fn required_for(phase: &str, language: &str) -> Vec<String> {
    catalog(language)
        .into_iter()
        .filter(|s| s.phases.contains(&phase))
        .map(|s| s.name)
        .collect()
}

/// `skills:` frontmatter block for a phase agent, or empty.
pub(crate) fn agent_frontmatter(phase: &str, language: &str) -> String {
    let names = required_for(phase, language);
    if names.is_empty() {
        return String::new();
    }
    let mut s = String::from("skills:\n");
    for n in names {
        s.push_str(&format!("  - {n}\n"));
    }
    s
}

/// `metadata.generated-by` of every skill zforge writes.
const GENERATED_BY: &str = "zforge";

/// Whether `dir/SKILL.md` carries zforge's `metadata.generated-by` marker.
fn is_generated_by_zforge(dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join("SKILL.md")) else {
        return false;
    };
    let Some(frontmatter) = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---").map(|(fm, _)| fm))
    else {
        return false;
    };
    serde_yaml::from_str::<serde_yaml::Value>(frontmatter)
        .ok()
        .and_then(|fm| {
            fm.get("metadata")?
                .get("generated-by")?
                .as_str()
                .map(|v| v == GENERATED_BY)
        })
        .unwrap_or(false)
}

fn render(skill: &NativeSkill, vars: &Vars) -> String {
    let fm = serde_yaml::to_string(&serde_yaml::Mapping::from_iter([
        ("name".into(), skill.name.clone().into()),
        ("description".into(), skill.description.clone().into()),
        (
            "metadata".into(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::from_iter([
                ("generated-by".into(), GENERATED_BY.into()),
                ("source".into(), skill.source.clone().into()),
                ("version".into(), env!("CARGO_PKG_VERSION").into()),
            ])),
        ),
    ]))
    .expect("static frontmatter serializes");
    format!("---\n{fm}---\n\n{}", super::apply_vars(skill.body, vars))
}

/// Write `.claude/skills/zforge-*/SKILL.md` and remove zforge-managed
/// skills that are no longer in the catalog (e.g. after a language change).
/// A directory is removed only if zforge generated it — `zforge-` prefix
/// *and* [`GENERATED_BY`] in its frontmatter — so a user skill that merely
/// shares the prefix is never touched.
pub(crate) fn write(project_root: &Path, vars: &Vars, force: bool) -> Result<WriteReport> {
    let dir = project_root.join(".claude").join("skills");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let skills = catalog(&vars.language);
    let wanted: std::collections::HashSet<&str> = skills.iter().map(|s| s.name.as_str()).collect();

    let mut report = WriteReport::default();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(PREFIX)
            && !wanted.contains(name.as_str())
            && is_generated_by_zforge(&entry.path())
        {
            std::fs::remove_dir_all(entry.path())
                .with_context(|| format!("remove stale {}", entry.path().display()))?;
            report.removed.push(name);
        }
    }
    for skill in &skills {
        let path = dir.join(&skill.name).join("SKILL.md");
        if path.exists() && !force {
            report.kept += 1;
            continue;
        }
        crate::fs::writer::write_file(&path, &render(skill, vars))?;
        report.written += 1;
    }
    report.total = skills.len();
    Ok(report)
}

#[derive(Debug, Default)]
pub(crate) struct WriteReport {
    pub total: usize,
    pub written: usize,
    pub kept: usize,
    pub removed: Vec<String>,
}

/// Skills section of CLAUDE.md, generated from the catalog so the document
/// and the files can never disagree.
pub(crate) fn claude_md_section(language: &str) -> String {
    let skills = catalog(language);
    let mut s = String::from(
        "## Skills\n\nzforge installs these as Claude Code skills in `.claude/skills/`. \
         Each phase agent preloads the skills marked for its phase (`skills:` in \
         `.claude/agents/<phase>-agent.md`); the rest load when their description \
         matches the work, or on request with `/<name>`.\n\n\
         | Skill | Preloaded by | Use |\n|-------|--------------|-----|\n",
    );
    for sk in &skills {
        let phases = if sk.phases.is_empty() {
            "—".to_string()
        } else {
            sk.phases
                .iter()
                .map(|p| format!("{p}-agent"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        s.push_str(&format!(
            "| `{}` | {} | {} |\n",
            sk.name, phases, sk.description
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGS: [&str; 8] = [
        "rust",
        "go",
        "typescript",
        "python",
        "ios",
        "android",
        "flutter",
        "unknown",
    ];

    // Adding a checklist to the store without a catalog entry would ship a
    // skill nobody can discover; fail here instead.
    #[test]
    fn every_embedded_skill_is_in_the_catalog() {
        let names: Vec<String> = catalog("rust").into_iter().map(|s| s.source).collect();
        for (src, _) in crate::embedded::SKILLS {
            assert!(names.iter().any(|n| n == src), "{src} missing from BASE");
        }
    }

    #[test]
    fn names_and_descriptions_are_valid_for_every_language() {
        for lang in LANGS {
            let skills = catalog(lang);
            let mut seen = std::collections::HashSet::new();
            for s in &skills {
                assert!(s.name.starts_with(PREFIX), "{}", s.name);
                assert!(
                    s.name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "{} must be lowercase-hyphenated",
                    s.name
                );
                assert!(seen.insert(s.name.clone()), "duplicate {}", s.name);
                assert!(
                    !s.description.is_empty() && s.description.len() <= 1024,
                    "{}",
                    s.name
                );
            }
        }
    }

    #[test]
    fn each_phase_preloads_its_checklists() {
        assert_eq!(required_for("spec", "rust"), vec!["zforge-clarify-spec"]);
        assert_eq!(
            required_for("testspec", "rust"),
            vec!["zforge-derive-test-cases"]
        );
        assert_eq!(
            required_for("plan", "rust"),
            vec!["zforge-implementation-planning"]
        );
        assert_eq!(required_for("review", "rust"), vec!["zforge-review-patch"]);
        assert_eq!(
            required_for("code", "rust"),
            vec![
                "zforge-write-tests-first",
                "zforge-implement-minimal-patch",
                "zforge-rust-patterns",
                "zforge-rust-testing",
            ]
        );
    }

    // UI/device variants stay invocable, not preloaded.
    #[test]
    fn only_core_language_skills_are_preloaded() {
        let code = required_for("code", "ios");
        assert!(code.contains(&"zforge-ios-patterns".to_string()));
        assert!(code.contains(&"zforge-ios-testing".to_string()));
        assert!(!code
            .iter()
            .any(|n| n.contains("ui-patterns") || n.contains("snapshot")));
    }

    #[test]
    fn agent_frontmatter_lists_skills() {
        assert_eq!(
            agent_frontmatter("spec", "rust"),
            "skills:\n  - zforge-clarify-spec\n"
        );
    }
}
