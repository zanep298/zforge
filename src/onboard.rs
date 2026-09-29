//! Whether the project is onboarded (ONBOARD REQ-001, TASK-003, D6-shaped
//! but its own thing): the project's own knowledge — `domain.md`,
//! `conventions.md`, `rules.md` — all accepted, and the baseline test run
//! green or every one of its failures recorded as known (business rule 8).
//!
//! Nothing here is stored: it is derived from the knowledge decision log
//! and `baseline.md`, read fresh every time, the same way `crate::status`
//! derives an intake's state from its own log.

use crate::config::Config;
use crate::intake::status::{DocState, DocStatus};
use crate::knowledge::{self, known, probe, Knowledge};
use anyhow::Result;
use serde::Serialize;

/// The last probe's test run, and the known-failure list judged against
/// it. `result`/`commit` are `None` before the project is ever probed.
#[derive(Debug, Clone, Serialize)]
pub struct BaselineState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failing: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub known: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OnboardState {
    pub files: Vec<DocStatus>,
    pub baseline: BaselineState,
    /// The three files accepted, and the baseline green or every failing
    /// test known (Output).
    pub onboarded: bool,
}

/// Derive the project's onboarding state from the knowledge document set
/// and `baseline.md`. Never errors on a project that was never onboarded
/// — an absent `baseline.md` or knowledge directory just means nothing is
/// accepted and nothing was probed yet.
pub fn state(config: &Config) -> Result<OnboardState> {
    let k = Knowledge::open(config);
    let files = knowledge::statuses(&k)?;
    let files_accepted = files.iter().all(|f| f.state == DocState::Accepted);

    let baseline = probe::read_baseline(&k)?;
    let known_list = known::list(&k)?;
    let baseline_ok = match &baseline {
        Some(b) if b.result => true,
        Some(b) => !b.failing.is_empty() && b.failing.iter().all(|t| known_list.contains(t)),
        None => false,
    };

    let baseline_state = BaselineState {
        commit: baseline.as_ref().map(|b| b.commit.clone()),
        result: baseline.as_ref().map(|b| b.result),
        failing: baseline
            .as_ref()
            .map(|b| b.failing.clone())
            .unwrap_or_default(),
        known: known_list,
        reason: baseline.as_ref().and_then(|b| b.reason.clone()),
    };

    Ok(OnboardState {
        onboarded: files_accepted && baseline_ok,
        files,
        baseline: baseline_state,
    })
}

/// Gaps in the project's own readiness (ONBOARD REQ-010, TASK-006): every
/// knowledge file not accepted, a baseline never probed or red with an
/// unknown failure, and every stale citation, named by its item ID
/// (AC-05). Pure text — `intake::readiness::Readiness::with_project`
/// decides whether a gap warns or blocks a handover
/// (`onboarding.required`). Never errors: a project that never onboarded
/// just has more gaps to report ([`state`]'s own guarantee).
pub fn readiness_gaps(config: &Config) -> Vec<String> {
    let k = Knowledge::open(config);
    let mut gaps = Vec::new();
    match state(config) {
        Ok(s) => {
            for f in &s.files {
                if f.state != DocState::Accepted {
                    gaps.push(format!(
                        "{} is not accepted ({}); see `zforge onboard`",
                        f.file,
                        f.state.as_str()
                    ));
                }
            }
            match (s.baseline.result, &s.baseline.reason) {
                (None, _) => gaps.push(
                    "the project's baseline test run has never been probed (`zforge onboard`)"
                        .to_string(),
                ),
                (Some(true), _) => {}
                (Some(false), Some(reason)) => {
                    gaps.push(format!("the baseline test run failed to run: {reason}"))
                }
                (Some(false), None) => {
                    let unknown: Vec<&str> = s
                        .baseline
                        .failing
                        .iter()
                        .filter(|t| !s.baseline.known.contains(t))
                        .map(String::as_str)
                        .collect();
                    if !unknown.is_empty() {
                        gaps.push(format!(
                            "the baseline has unknown failing test(s): {}",
                            unknown.join(", ")
                        ));
                    }
                }
            }
        }
        Err(e) => gaps.push(format!("cannot read the project's knowledge: {e:#}")),
    }
    if let Ok(report) = knowledge::stale::check(&k) {
        for item in report.stale {
            let reason = match item.reason {
                knowledge::stale::Reason::Changed => "changed",
                knowledge::stale::Reason::Gone => "gone",
            };
            gaps.push(format!(
                "{}: {} is stale ({reason} at {}:{}-{})",
                item.file, item.id, item.cite.path, item.cite.start, item.cite.end
            ));
        }
    }
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &std::path::Path) {
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
    }

    fn config_at(root: &std::path::Path) -> Config {
        let yaml = "project:\n  name: p\n  language: rust\n  test_command: \"true\"\n";
        let mut cfg: Config = serde_yaml::from_str(yaml).unwrap();
        cfg.config_file = root.join(".zforge").join("config.yaml");
        cfg
    }

    #[test]
    fn never_probed_is_not_onboarded() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let cfg = config_at(tmp.path());
        let s = state(&cfg).unwrap();
        assert!(!s.onboarded);
        assert!(s.baseline.result.is_none());
    }

    fn commit_all(dir: &std::path::Path) {
        for args in [
            vec!["add", "-A"],
            vec!["commit", "-q", "--allow-empty", "-m", "c"],
        ] {
            std::process::Command::new("git")
                .args(&args)
                .current_dir(dir)
                .output()
                .unwrap();
        }
    }

    /// Content that passes the knowledge linter without any items — real
    /// items and their evidence are TASK-002/TASK-008 territory; this test
    /// only cares about the review lifecycle, not what the file says.
    fn acceptable(title: &str) -> String {
        format!("# {title}\n\nA thing happens.\n\n## Open questions\n")
    }

    fn accept_all_knowledge_files(k: &Knowledge) {
        for f in crate::knowledge::FILES {
            let title = f.trim_end_matches(".md");
            std::fs::write(k.dir.join(f), acceptable(title)).unwrap();
            crate::intake::review::review(k, f).unwrap();
            crate::intake::review::accept(k, f, None).unwrap();
        }
    }

    #[test]
    fn files_accepted_and_baseline_green_is_onboarded() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_all(tmp.path());
        let cfg = config_at(tmp.path());

        let report = probe::run(&cfg).unwrap();
        assert!(report.baseline.result); // "true" always passes

        let k = Knowledge::open(&cfg);
        accept_all_knowledge_files(&k);

        let s = state(&cfg).unwrap();
        assert!(s.onboarded, "all three accepted and baseline green: {s:?}");
    }

    #[test]
    fn files_accepted_but_an_unknown_failure_is_not_onboarded() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        std::fs::write(
            tmp.path().join("fake_go_test.sh"),
            "#!/bin/sh\nprintf -- '--- FAIL: TestA (0.00s)\\nFAIL\\n'\nexit 1\n",
        )
        .unwrap();
        commit_all(tmp.path());
        let cfg = Config {
            project: crate::config::ProjectConfig {
                name: "p".into(),
                language: "go".into(),
                test_command: format!("sh {}", tmp.path().join("fake_go_test.sh").display()),
            },
            paths: Default::default(),
            runner: Default::default(),
            knowledge: Default::default(),
            execution: Default::default(),
            onboarding: Default::default(),
            config_file: tmp.path().join(".zforge").join("config.yaml"),
        };

        let report = probe::run(&cfg).unwrap();
        assert!(!report.baseline.result);
        assert_eq!(report.baseline.failing, vec!["TestA".to_string()]);

        let k = Knowledge::open(&cfg);
        accept_all_knowledge_files(&k);

        let s = state(&cfg).unwrap();
        assert!(!s.onboarded, "TestA is not on the known list yet: {s:?}");

        known::record_known(&k, &["TestA".to_string()], None).unwrap();
        let s = state(&cfg).unwrap();
        assert!(s.onboarded, "TestA is now known: {s:?}");
    }

    /// AC-03: a project that was never probed or onboarded gets a gap per
    /// unaccepted knowledge file plus one for the baseline, not an error.
    #[test]
    fn readiness_gaps_lists_unaccepted_files_and_the_unprobed_baseline() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = config_at(tmp.path());

        let gaps = readiness_gaps(&cfg);
        for f in crate::knowledge::FILES {
            assert!(gaps.iter().any(|g| g.contains(f)), "{f}: {gaps:?}");
        }
        assert!(
            gaps.iter().any(|g| g.contains("never been probed")),
            "{gaps:?}"
        );
    }

    fn head(dir: &std::path::Path) -> String {
        String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(dir)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }

    /// AC-05: a stale item shows up in the gaps by its own ID, not just a
    /// generic "knowledge is stale" line.
    #[test]
    fn readiness_gaps_names_a_stale_item_by_its_id() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo(root);
        std::fs::write(root.join("x.rs"), "line1\nline2\nline3\n").unwrap();
        commit_all(root);
        let pinned = head(root);
        let cfg = config_at(root);
        let k = Knowledge::open(&cfg);
        std::fs::create_dir_all(&k.dir).unwrap();
        std::fs::write(
            k.dir.join("domain.md"),
            format!(
                "---\npinned: {pinned}\n---\n## m\n- DOM-001: a thing happens. (x.rs:2)\n\n## Open questions\n"
            ),
        )
        .unwrap();
        crate::intake::review::review(&k, "domain.md").unwrap();
        crate::intake::review::accept(&k, "domain.md", None).unwrap();

        std::fs::write(root.join("x.rs"), "line1\nchanged\nline3\n").unwrap();
        commit_all(root);

        let gaps = readiness_gaps(&cfg);
        assert!(
            gaps.iter()
                .any(|g| g.contains("DOM-001") && g.contains("domain.md")),
            "{gaps:?}"
        );
    }
}
