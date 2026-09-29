//! The probe (ONBOARD REQ-001, REQ-002, TASK-003): what `zforge onboard`
//! records about the project without calling a model. Refuses on an
//! uncommitted working tree — the knowledge is pinned to a commit — and
//! otherwise never fails on what it finds: a missing, failing or timed-out
//! test command is recorded as red with its reason (Output, AC-04), not
//! propagated as an error.
//!
//! `baseline.md` is generated, not reviewed (03-solution "Where things
//! live"): every probe rewrites it whole. What must survive a probe — the
//! known-failure list — lives in the decision log instead (`super::known`).

use super::Knowledge;
use crate::config::Config;
use crate::run::git;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

/// Chosen for this task (Autonomy: "the probe's timeout for the test run,
/// stated in the report"): generous enough for a real suite, short enough
/// that a hung command does not leave `zforge onboard` looking stuck.
pub const TEST_TIMEOUT_SECS: u64 = 600;

/// `baseline.md`'s frontmatter: everything [`super::onboard`]-equivalent
/// callers (`crate::onboard::state`) read back, plus the descriptive
/// fields that only ever feed the human-readable body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Baseline {
    pub commit: String,
    pub branch: String,
    pub clean: bool,
    pub language: String,
    pub toolchain: String,
    pub tracked_files: usize,
    pub tracked_lines: usize,
    #[serde(default)]
    pub docs: Vec<String>,
    pub codegraph: bool,
    pub test_command: String,
    /// The one field callers outside this module actually branch on:
    /// whether the run was green.
    pub result: bool,
    pub duration_secs: f64,
    /// Failing test names, when the runner could read them (Output).
    #[serde(default)]
    pub failing: Vec<String>,
    /// Set when `result` is red and `failing` could not be read: the
    /// command was missing, failed to start, or timed out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A top-level source directory the probe found tracked files under.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub files: usize,
    pub lines: usize,
}

/// Everything one probe produced.
#[derive(Debug, Clone)]
pub struct Report {
    pub baseline: Baseline,
    pub modules: Vec<Module>,
    /// The three knowledge files created from a stub because they did not
    /// exist yet (Output).
    pub created_files: Vec<String>,
}

/// Probe the project at HEAD and write `<knowledge.dir>/baseline.md`
/// (Output). Refuses — writing nothing at all — while the working tree
/// has uncommitted changes (AC-03).
pub fn run(config: &Config) -> Result<Report> {
    let root = config.project_root();
    if !git::is_clean(&root)? {
        bail!(
            "the working tree has uncommitted changes; commit or stash them first — \
             onboarding pins the project's knowledge to a commit"
        );
    }

    let k = Knowledge::open(config);
    let created_files = super::ensure_files(&k)?;

    let commit = git::head(&root)?;
    let branch = git::ok(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let tracked = tracked_files(&root)?;
    let tracked_files_count = tracked.len();
    let (modules, tracked_lines) = modules_and_lines(&root, &tracked);
    let docs = detect_docs(&root);
    let codegraph = root.join(".codegraph").is_dir();
    let language = config.project.language.clone();
    let toolchain = toolchain_version(&language);
    let test_command = config.project.test_command.clone();
    let (result, duration_secs, failing, reason) = run_baseline(&root, &test_command, &language);

    let baseline = Baseline {
        commit,
        branch,
        clean: true,
        language,
        toolchain,
        tracked_files: tracked_files_count,
        tracked_lines,
        docs,
        codegraph,
        test_command,
        result,
        duration_secs,
        failing,
        reason,
    };

    std::fs::create_dir_all(&k.dir)?;
    crate::fs::write_atomic(
        &k.dir.join(super::BASELINE_FILE),
        render(&baseline, &modules).as_bytes(),
    )?;

    Ok(Report {
        baseline,
        modules,
        created_files,
    })
}

/// `baseline.md`'s frontmatter, read back — `None` when the project was
/// never probed. Used by `crate::onboard::state`, readiness (ONBOARD
/// TASK-006) and a run's verification (ONBOARD TASK-007).
pub fn read_baseline(k: &Knowledge) -> Result<Option<Baseline>> {
    let path = k.dir.join(super::BASELINE_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let (fm, _) = crate::intake::lint::split_frontmatter(&text);
    let Some(fm) = fm else {
        bail!(
            "{} has no frontmatter — it was not written by the probe",
            path.display()
        );
    };
    Ok(Some(serde_yaml::from_str(fm)?))
}

fn tracked_files(root: &Path) -> Result<Vec<String>> {
    let out = git::ok(root, &["ls-files"])?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// Group tracked files by their top-level directory (Output: "the modules
/// — top-level source directories with their line counts"). A file
/// tracked directly at the project root belongs to no module but still
/// counts toward the total. Binary or otherwise undecodable files count as
/// zero lines rather than failing the whole probe.
fn modules_and_lines(root: &Path, files: &[String]) -> (Vec<Module>, usize) {
    let mut by_top: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut total_lines = 0usize;
    for f in files {
        let lines = std::fs::read_to_string(root.join(f))
            .map(|t| t.lines().count())
            .unwrap_or(0);
        total_lines += lines;
        if let Some((top, _)) = f.split_once('/') {
            let entry = by_top.entry(top.to_string()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += lines;
        }
    }
    let modules = by_top
        .into_iter()
        .map(|(name, (files, lines))| Module { name, files, lines })
        .collect();
    (modules, total_lines)
}

/// README (any casing/extension), CLAUDE.md/AGENTS.md (noting whether
/// zforge generated them), an ADR directory, and a plain `docs/` directory
/// otherwise unaccounted for.
fn detect_docs(root: &Path) -> Vec<String> {
    let mut docs = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.to_ascii_uppercase().starts_with("README") {
            docs.push(name);
        }
    }
    docs.sort();
    for f in ["CLAUDE.md", "AGENTS.md"] {
        let path = root.join(f);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let own = if crate::cli::init::instructions::generated(&text) {
            ""
        } else {
            " (project's own)"
        };
        docs.push(format!("{f}{own}"));
    }
    let mut adr_found = false;
    for cand in ["docs/adr", "docs/ADR", "adr", "ADR"] {
        let path = root.join(cand);
        if path.is_dir() {
            let count = std::fs::read_dir(&path).map(|d| d.count()).unwrap_or(0);
            docs.push(format!("{cand}/ ({count})"));
            adr_found = true;
            break;
        }
    }
    if !adr_found && root.join("docs").is_dir() {
        docs.push("docs/".to_string());
    }
    docs
}

/// Best-effort version string; "unknown" for a language with no obvious
/// single tool, or when the tool is not on PATH.
fn toolchain_version(language: &str) -> String {
    let bin = match language.to_ascii_lowercase().as_str() {
        "rust" => "rustc",
        "go" | "golang" => "go",
        "python" | "py" | "pytest" => "python3",
        "flutter" | "dart" => "flutter",
        "typescript" | "javascript" | "ts" | "js" | "node" => "node",
        "android" => "java",
        "ios" => "swift",
        _ => return "unknown".to_string(),
    };
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("--version");
    match crate::process::run_bounded(cmd, None, Duration::from_secs(5)) {
        Ok(out) if !out.timed_out => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let text = if stdout.trim().is_empty() {
                String::from_utf8_lossy(&out.stderr).into_owned()
            } else {
                stdout.into_owned()
            };
            text.lines()
                .next()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .unwrap_or("unknown")
                .to_string()
        }
        _ => "unknown".to_string(),
    }
}

/// One run of `command` (Output). Never returns an error: a command that
/// is missing, fails to start or times out is reported through `reason`
/// instead (AC-04), so `zforge onboard` always exits 0.
fn run_baseline(
    root: &Path,
    command: &str,
    language: &str,
) -> (bool, f64, Vec<String>, Option<String>) {
    match crate::runner::run_with_language(command, root, TEST_TIMEOUT_SECS, language) {
        Ok(tr) if tr.timed_out => (
            false,
            tr.duration.as_secs_f64(),
            Vec::new(),
            Some(format!("timed out after {TEST_TIMEOUT_SECS}s")),
        ),
        Ok(tr) if tr.passed => (true, tr.duration.as_secs_f64(), Vec::new(), None),
        Ok(tr) if !tr.failed_names.is_empty() => {
            (false, tr.duration.as_secs_f64(), tr.failed_names, None)
        }
        Ok(tr) => {
            let reason = tr
                .error
                .filter(|e| !e.trim().is_empty())
                .unwrap_or_else(|| "test command failed".to_string());
            (false, tr.duration.as_secs_f64(), Vec::new(), Some(reason))
        }
        Err(e) => (false, 0.0, Vec::new(), Some(e.to_string())),
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn baseline_line(b: &Baseline) -> String {
    if b.result {
        return format!("{} → PASS in {:.1}s", b.test_command, b.duration_secs);
    }
    match (&b.reason, b.failing.is_empty()) {
        (Some(reason), true) => format!("{} → RED: {reason}", b.test_command),
        (None, false) => format!(
            "{} → FAIL in {:.1}s: {}",
            b.test_command,
            b.duration_secs,
            b.failing.join(", ")
        ),
        (Some(reason), false) => format!(
            "{} → FAIL in {:.1}s: {} ({reason})",
            b.test_command,
            b.duration_secs,
            b.failing.join(", ")
        ),
        (None, true) => format!(
            "{} → RED in {:.1}s (no failing test names could be read)",
            b.test_command, b.duration_secs
        ),
    }
}

/// `baseline.md`'s full text: YAML frontmatter (everything above), then a
/// human-readable body ending in the `## Modules` section
/// `knowledge::lint::covers_warnings` reads (TASK-002).
fn render(b: &Baseline, modules: &[Module]) -> String {
    let fm = serde_yaml::to_string(b).expect("Baseline serializes");
    let mut body = String::new();
    body.push_str("# Baseline\n\n");
    body.push_str(
        "Generated by `zforge onboard` from a probe that calls no model; \
         rewritten whole by every probe. Not reviewed.\n\n",
    );
    body.push_str(&format!(
        "commit     {} ({}{})\n",
        short(&b.commit),
        b.branch,
        if b.clean { ", clean" } else { "" }
    ));
    body.push_str(&format!("language   {}; {}\n", b.language, b.toolchain));
    body.push_str(&format!(
        "tracked    {} files, {} lines\n",
        b.tracked_files, b.tracked_lines
    ));
    body.push_str(&format!(
        "docs       {}\n",
        if b.docs.is_empty() {
            "none found".to_string()
        } else {
            b.docs.join(", ")
        }
    ));
    body.push_str(&format!(
        "codegraph  {}\n",
        if b.codegraph {
            "indexed"
        } else {
            "not indexed"
        }
    ));
    body.push_str(&format!("baseline   {}\n\n", baseline_line(b)));
    body.push_str("## Modules\n\n");
    if modules.is_empty() {
        body.push_str("(none found)\n");
    } else {
        for m in modules {
            body.push_str(&format!(
                "- {} ({} files, {} lines)\n",
                m.name, m.files, m.lines
            ));
        }
    }
    format!("---\n{fm}---\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) {
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

    fn commit_all(dir: &Path) {
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
        };
        run(&["add", "-A"]);
        run(&["commit", "-q", "--allow-empty", "-m", "c"]);
    }

    /// Built directly (not parsed from a YAML string) so a test command
    /// with quotes or newlines never has to survive a round trip through
    /// YAML string escaping.
    fn config_with(root: &Path, language: &str, test_command: &str) -> Config {
        Config {
            project: crate::config::ProjectConfig {
                name: "p".into(),
                language: language.into(),
                test_command: test_command.into(),
            },
            paths: Default::default(),
            runner: Default::default(),
            knowledge: Default::default(),
            execution: Default::default(),
            onboarding: Default::default(),
            config_file: root.join(".zforge").join("config.yaml"),
        }
    }

    fn config_at(root: &Path, test_command: &str) -> Config {
        config_with(root, "rust", test_command)
    }

    /// AC-01: a clean project with a passing suite records the commit,
    /// the language, the modules and a green result.
    #[test]
    fn a_clean_passing_project_probes_green() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "fn x() {}\n").unwrap();
        commit_all(tmp.path());

        let cfg = config_at(tmp.path(), "true");
        let report = run(&cfg).unwrap();
        assert!(report.baseline.result);
        assert_eq!(report.baseline.language, "rust");
        assert!(report.modules.iter().any(|m| m.name == "src"));
        assert_eq!(report.created_files.len(), 3);

        let text = std::fs::read_to_string(tmp.path().join("docs/knowledge/baseline.md")).unwrap();
        assert!(text.contains("## Modules"));
        assert!(text.contains("- src ("));
        let (fm, _) = crate::intake::lint::split_frontmatter(&text);
        let back: Baseline = serde_yaml::from_str(fm.unwrap()).unwrap();
        assert_eq!(back.commit, report.baseline.commit);
        assert!(back.result);
    }

    /// AC-02: a failing suite lists its failing tests.
    #[test]
    fn a_failing_suite_lists_failing_tests() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        std::fs::write(tmp.path().join("go.mod"), "module x\n").unwrap();
        std::fs::write(
            tmp.path().join("fake_go_test.sh"),
            "#!/bin/sh\nprintf -- '--- FAIL: TestA (0.00s)\\nFAIL\\n'\nexit 1\n",
        )
        .unwrap();
        commit_all(tmp.path());

        let cfg = config_with(
            tmp.path(),
            "go",
            &format!("sh {}", tmp.path().join("fake_go_test.sh").display()),
        );

        let report = run(&cfg).unwrap();
        assert!(!report.baseline.result);
        assert_eq!(report.baseline.failing, vec!["TestA".to_string()]);
        assert!(report.baseline.reason.is_none());
    }

    /// AC-03: uncommitted changes refuse the probe and nothing is written.
    #[test]
    fn uncommitted_changes_refuse_and_write_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        std::fs::write(tmp.path().join("a.txt"), "x").unwrap();
        commit_all(tmp.path());
        std::fs::write(tmp.path().join("a.txt"), "y").unwrap(); // dirty now

        let cfg = config_at(tmp.path(), "true");
        let err = run(&cfg).unwrap_err().to_string();
        assert!(err.contains("uncommitted changes"), "{err}");
        assert!(!tmp.path().join("docs/knowledge").exists());
    }

    /// AC-04: a test command that does not exist is recorded as red with
    /// its reason — `run` itself still returns `Ok`.
    #[test]
    fn a_missing_test_command_is_recorded_red_with_a_reason() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_all(tmp.path());

        let cfg = config_at(tmp.path(), "zforge-onboard-test-nonexistent-command-xyz");
        let report = run(&cfg).unwrap();
        assert!(!report.baseline.result);
        assert!(report.baseline.failing.is_empty());
        assert!(report.baseline.reason.is_some(), "{:?}", report.baseline);
    }

    #[test]
    fn round_trips_through_read_baseline() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_all(tmp.path());
        let cfg = config_at(tmp.path(), "true");
        let report = run(&cfg).unwrap();

        let k = Knowledge::open(&cfg);
        let back = read_baseline(&k).unwrap().unwrap();
        assert_eq!(back, report.baseline);
    }

    #[test]
    fn no_baseline_file_reads_as_none() {
        let tmp = tempfile::tempdir().unwrap();
        let k = Knowledge {
            dir: tmp.path().join("docs/knowledge"),
        };
        assert!(read_baseline(&k).unwrap().is_none());
    }

    #[test]
    fn creates_the_three_files_from_stubs_when_absent_but_not_twice() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        commit_all(tmp.path());
        let cfg = config_at(tmp.path(), "true");

        let report = run(&cfg).unwrap();
        assert_eq!(report.created_files.len(), 3);
        for f in crate::knowledge::FILES {
            assert!(tmp.path().join("docs/knowledge").join(f).is_file());
        }

        // Edit one by hand, commit, probe again: it is not recreated.
        std::fs::write(
            tmp.path().join("docs/knowledge/domain.md"),
            "# Domain\n\nhand-written.\n",
        )
        .unwrap();
        commit_all(tmp.path());
        let report2 = run(&cfg).unwrap();
        assert!(report2.created_files.is_empty());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("docs/knowledge/domain.md")).unwrap(),
            "# Domain\n\nhand-written.\n"
        );
    }
}
