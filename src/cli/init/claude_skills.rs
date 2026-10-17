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
        source: "write-tests-first.md",
        description: "Test-first discipline: one acceptance criterion (AC-nn) at a time — write its failing test, see it fail for the right reason under the project's test command, then implement. Use when implementing a task contract or adding behavior or a bug fix to production code.",
        phases: &["code"],
    },
    BaseSkill {
        source: "implement-minimal-patch.md",
        description: "Make the smallest change that satisfies the task contract and passes the tests — no unrelated refactors, conventions of the surrounding code, generated files regenerated not edited; when the contract's output, interface or binding decisions must change, write a change request instead. Use while implementing a task.",
        phases: &["code"],
    },
    BaseSkill {
        source: "review-patch.md",
        description: "Review a change against its zforge task contract: every AC-nn met and proven by a test the test command runs, nothing outside the scope, binding decisions kept, no weakened tests, regression risk named; findings as '- ' lines, then exactly one VERDICT: APPROVE or VERDICT: CHANGES. Use when reviewing a run's work or any change made for a zforge task.",
        phases: &["review"],
    },
    BaseSkill {
        source: "debug.md",
        description: "Diagnose the root cause of a bug or unexpected behavior — reproduce, isolate, explain — before writing any fix. Use when something fails and the cause is not yet known.",
        phases: &[],
    },
    BaseSkill {
        source: "intake.md",
        description: "Prepare a zforge intake with the user — 01-outcome, 02-behavior, 03-solution, 04-breakdown and leaf task contracts under .zforge/intakes/ — send it for review and present the pack; only the user accepts or hands over. Use when a feature, change or fix should be clarified and broken down before implementation, or when zforge intake status shows files to write or revise.",
        phases: &[],
    },
    BaseSkill {
        source: "intake-writing.md",
        description: "How to write a zforge intake's stage files, task contracts and brief.md for the person who decides — summaries, word budgets, Detail sections, the brief. Use with zforge-intake whenever drafting or revising intake files.",
        phases: &[],
    },
    BaseSkill {
        source: "intake-change.md",
        description: "Change an accepted zforge intake — answer a run's change request, edit in place before the handover is merged, or `zforge intake amend` after it is. Use when a requirement must change after acceptance or a run stopped on a change request.",
        phases: &[],
    },
    BaseSkill {
        source: "onboard.md",
        description: "Draft the project's own knowledge from the code — domain.md, conventions.md, rules.md under docs/knowledge/ — every statement cited at path:line, what the code cannot show asked as an open question, sent for review but never accepted. Use for onboarding requests, or when `zforge onboard status` shows files in draft or stale.",
        phases: &[],
    },
    BaseSkill {
        source: "security-review.md",
        description: "Find security vulnerabilities in a changeset before merge: injection, authn/authz, secrets, unsafe input handling, dependency risks. Use when a change touches user input, auth, data access or external calls. Findings are reported in the final message, never written to a file.",
        phases: &[],
    },
    BaseSkill {
        source: "performance-optimize.md",
        description: "Fix a measured performance problem with data: baseline, locate the bottleneck, change one thing, measure again. Use when something is measurably too slow or uses too much memory or CPU — not for writing new code efficiently, which the language checklists cover.",
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
        description: "Add logs, metrics and traces that make failures diagnosable without leaking sensitive data. Use when adding or changing logging, metrics, tracing or alerts, or when a change adds a failure path that must be diagnosable in production.",
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
        description: "Turn Figma designs or Figma MCP output into faithful, maintainable UI code using the project's existing components and tokens. Use when a task includes a Figma link, Figma MCP output or a visual design to implement.",
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

pub(crate) fn language_label(lang: &str) -> &'static str {
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
/// Core `<lang>-patterns` / `<lang>-testing` (and Go's `go-memory`) are
/// preloaded; UI and device-level variants stay model-invocable to keep the
/// agent's context lean.
pub(crate) fn language_skill(lang: &str, stem: &str) -> (String, bool) {
    let l = language_label(lang);
    if stem.ends_with("-ui-patterns") || stem.ends_with("-compose-ui") {
        (
            format!("{l} UI patterns for screens and reusable components. Use when building or changing {l} UI."),
            false,
        )
    } else if stem.ends_with("-instrumented-testing") {
        (
            format!("{l} instrumented (on-device) tests, only where JVM tests cannot cover the behavior: device APIs, navigation, permissions, Compose UI. Use when a change needs on-device test coverage."),
            false,
        )
    } else if stem.ends_with("-snapshot-accessibility") {
        (
            format!("{l} snapshot and accessibility checks for screens, reusable components and visual states. Use when a UI change needs visual-regression or accessibility coverage."),
            false,
        )
    } else if stem == "go-memory" {
        (
            "Keep memory bounded and allocations low in Go backend code: bounded input, goroutines that end, closed bodies and rows, hot-path allocations, retained slices. Use when writing or changing handlers, workers, consumers or other per-request code; for a measured memory problem use zforge-performance-optimize.".to_string(),
            true,
        )
    } else if stem.ends_with("-testing") {
        (
            format!("General {l} testing checklist — test-first, structure, naming, fixtures — to apply where the project's own tests show no pattern; run tests only with the project's test command. Use before adding or changing {l} tests."),
            true,
        )
    } else {
        (
            format!("General {l} idioms — error handling, types, module organization — to apply where the project's own conventions and the surrounding code are silent. Use before writing or changing {l} code."),
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

/// Store files (relative to the skills store) the `<phase>-agent` preloads.
/// The same checklists, named by path, reach clients that cannot preload.
pub(crate) fn sources_for(phase: &str, language: &str) -> Vec<String> {
    catalog(language)
        .into_iter()
        .filter(|s| s.phases.contains(&phase))
        .map(|s| s.source)
        .collect()
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
    // Brought to this version (`crate::shipped`): a skill zforge wrote and
    // nobody changed since is updated; an edited one is kept. One written
    // before the project's manifest existed is replaced (old copy archived)
    // when it carries zforge's marker, kept otherwise.
    let files: Vec<(String, String)> = skills
        .iter()
        .map(|skill| {
            (
                format!(".claude/skills/{}/SKILL.md", skill.name),
                render(skill, vars),
            )
        })
        .collect();
    let sync = super::sync_project(project_root, &files, force, &|rel| {
        let dir = Path::new(rel).parent().unwrap_or(Path::new(rel));
        if is_generated_by_zforge(&project_root.join(dir)) {
            crate::shipped::Unrecorded::Replace
        } else {
            crate::shipped::Unrecorded::Keep
        }
    })?;
    report.written = sync.written();
    report.kept = sync.kept.len() + sync.current;
    report.total = skills.len();
    report.sync = sync;
    Ok(report)
}

#[derive(Debug, Default)]
pub(crate) struct WriteReport {
    pub total: usize,
    pub written: usize,
    pub kept: usize,
    pub removed: Vec<String>,
    /// What the refresh did, file by file.
    pub sync: crate::shipped::Report,
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

    #[test]
    fn go_code_agent_preloads_the_memory_checklist() {
        let code = required_for("code", "go");
        assert!(code.contains(&"zforge-go-memory".to_string()));
        assert!(sources_for("code", "go").contains(&"go-memory.md".to_string()));
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
            agent_frontmatter("review", "rust"),
            "skills:\n  - zforge-review-patch\n"
        );
    }

    /// ONBOARD TASK-008 AC-01: `zforge-onboard` is in the catalog, preloaded
    /// by nothing (it triggers on onboarding requests, not a run phase), and
    /// its body states the evidence rule, the questions rule and that it
    /// never accepts.
    #[test]
    fn onboard_skill_is_catalogued_and_states_its_three_rules() {
        let skill = catalog("rust")
            .into_iter()
            .find(|s| s.name == "zforge-onboard")
            .expect("zforge-onboard in the catalog");
        assert!(skill.phases.is_empty(), "{:?}", skill.phases);
        assert!(skill.source == "onboard.md");
        let body = crate::embedded::SKILLS
            .iter()
            .find(|(n, _)| *n == "onboard.md")
            .unwrap()
            .1;
        assert!(body.contains("path:line"), "evidence rule");
        assert!(body.contains("Open questions"), "questions rule");
        assert!(
            body.to_lowercase().contains("never accept"),
            "never-accepts rule"
        );
    }
}
