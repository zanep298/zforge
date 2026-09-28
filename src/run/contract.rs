//! The contract a run works from (MOC-B TASK-003; REQ-001, REQ-004, REQ-009).
//!
//! A run never reads the working intake files. It loads the handover
//! manifest, reads every snapshot the manifest pins from the intake's
//! `.records/revisions/`, and checks each against the pinned SHA-256; one
//! mismatch and nothing is run. The prompt is built from those snapshots
//! only, so editing a contract file while a run is going — or ever after —
//! does not change what that run was given.
//!
//! [`load_handover`] gives the same verified view of a whole handover, for
//! what spans its tasks: their dependency graph and the integration check
//! (MOC-C TASK-003).

use super::record::{Checks, ChecksFrom};
use crate::intake::handover::Manifest;
use crate::intake::{hash, lint, record, Intake, STAGES};
use anyhow::{bail, Context, Result};
use std::path::Path;

const TEMPLATE: &str = include_str!("../../templates/contract.tmpl");

/// One pinned file, as accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractFile {
    pub file: String,
    pub revision: u32,
    pub sha256: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Contract {
    pub intake: String,
    pub manifest: Manifest,
    /// SHA-256 of the manifest file, recorded in the run's `run.yaml`.
    pub manifest_sha256: String,
    pub task: String,
    /// The task's dependencies, from its pinned snapshot, all in the
    /// handover.
    pub depends_on: Vec<String>,
    /// Protected test files the contract lets it change (`run::guard`).
    pub tests_may_change: Vec<String>,
    /// Every file the manifest pins, verified.
    pub files: Vec<ContractFile>,
}

/// The intake that holds `handover_id`. Handover ids are numbered per
/// intake, so an id found in more than one intake must be qualified.
pub fn find_handover(project_root: &Path, handover_id: &str) -> Result<Intake> {
    if let Some((intake, id)) = handover_id.split_once('/') {
        let i = Intake::open(project_root, intake)?;
        if !manifest_path(&i, id).is_file() {
            bail!("{id} not found in intake {intake}");
        }
        return Ok(i);
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(crate::intake::intakes_dir(project_root))
        .into_iter()
        .flatten()
        .flatten()
    {
        let id = entry.file_name().to_string_lossy().into_owned();
        if let Ok(i) = Intake::open(project_root, &id) {
            if manifest_path(&i, handover_id).is_file() {
                found.push(i);
            }
        }
    }
    match found.len() {
        0 => bail!("handover {handover_id} not found in any intake"),
        1 => Ok(found.remove(0)),
        _ => {
            let names: Vec<&str> = found.iter().map(|i| i.id.as_str()).collect();
            bail!(
                "{handover_id} exists in intakes {}; name it as <INTAKE>/{handover_id}",
                names.join(", ")
            )
        }
    }
}

fn manifest_path(intake: &Intake, id: &str) -> std::path::PathBuf {
    intake
        .records_dir()
        .join("handovers")
        .join(format!("{id}.json"))
}

/// A handover as its manifest pins it: every snapshot read and verified.
#[derive(Debug, Clone)]
pub struct Handover {
    pub intake: String,
    pub manifest: Manifest,
    /// SHA-256 of the manifest file, recorded in each run's `run.yaml`.
    pub manifest_sha256: String,
    pub files: Vec<ContractFile>,
}

/// Load and verify the contract of `task` in handover `handover_id`
/// (`HANDOVER-001` or `<INTAKE>/HANDOVER-001`).
pub fn load(project_root: &Path, handover_id: &str, task: &str) -> Result<Contract> {
    let h = load_handover(project_root, handover_id)?;
    let id = &h.manifest.id;
    if !h.manifest.tasks.iter().any(|t| t == task) {
        bail!(
            "{task} is not in {id} (handed over: {})",
            h.manifest.tasks.join(", ")
        );
    }
    let meta = h.task_meta(task)?;
    let depends_on = meta.depends_on;
    // Readiness keeps dependencies inside the scope; the pinned snapshot is
    // checked again rather than trusting the manifest was made that way.
    if let Some(outside) = depends_on.iter().find(|d| !h.manifest.tasks.contains(d)) {
        bail!("{task} depends on {outside}, which is not in {id}");
    }
    Ok(Contract {
        intake: h.intake,
        manifest: h.manifest,
        manifest_sha256: h.manifest_sha256,
        task: task.to_string(),
        depends_on,
        tests_may_change: meta.tests_may_change,
        files: h.files,
    })
}

/// Load handover `handover_id` and verify every snapshot it pins.
pub fn load_handover(project_root: &Path, handover_id: &str) -> Result<Handover> {
    let intake = find_handover(project_root, handover_id)?;
    let id = handover_id.rsplit('/').next().unwrap_or(handover_id);
    let path = manifest_path(&intake, id);
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let manifest: Manifest =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    if manifest.id != id || manifest.intake != intake.id {
        bail!(
            "{} does not describe {id} of intake {}",
            path.display(),
            intake.id
        );
    }
    let mut files = Vec::with_capacity(manifest.files.len());
    for pin in &manifest.files {
        crate::intake::validate_file(&pin.file)
            .with_context(|| format!("{} pins an invalid file", path.display()))?;
        let text = record::read_snapshot(&intake, &pin.file, pin.revision).with_context(|| {
            format!(
                "contract snapshot {} rev {} of {id} is missing; nothing was run",
                pin.file, pin.revision
            )
        })?;
        if hash::sha256(&text) != pin.sha256 {
            bail!(
                "contract snapshot {} rev {} does not match {id}; nothing was run",
                pin.file,
                pin.revision
            );
        }
        files.push(ContractFile {
            file: pin.file.clone(),
            revision: pin.revision,
            sha256: pin.sha256.clone(),
            text,
        });
    }

    Ok(Handover {
        intake: intake.id,
        manifest,
        manifest_sha256: hash::sha256(&raw),
        files,
    })
}

impl Handover {
    fn file(&self, name: &str) -> Result<&ContractFile> {
        self.files
            .iter()
            .find(|f| f.file == name)
            .ok_or_else(|| anyhow::anyhow!("{} does not pin {name}", self.manifest.id))
    }

    /// `task`'s frontmatter, from its pinned snapshot.
    pub fn task_meta(&self, task: &str) -> Result<lint::TaskMeta> {
        let name = format!("tasks/{task}.md");
        lint::parse_task_meta(&self.file(&name)?.text).map_err(|e| anyhow::anyhow!("{name}: {e}"))
    }

    /// `task`'s dependencies, from its pinned snapshot.
    pub fn depends_on(&self, task: &str) -> Result<Vec<String>> {
        Ok(self.task_meta(task)?.depends_on)
    }

    /// Tasks no other handed-over task depends on, in manifest order. Their
    /// outputs hold every other task's (MOC-C 02-behavior, rule 3).
    pub fn leaves(&self) -> Result<Vec<String>> {
        let mut needed = std::collections::BTreeSet::new();
        for t in &self.manifest.tasks {
            needed.extend(self.depends_on(t)?);
        }
        Ok(self
            .manifest
            .tasks
            .iter()
            .filter(|t| !needed.contains(*t))
            .cloned()
            .collect())
    }

    /// The integration check: the first code block under "Kiểm chứng tích
    /// hợp" in the pinned breakdown, one command per non-empty, non-comment
    /// line; without one, `test_command`.
    pub fn checks(&self, test_command: &str) -> Result<Checks> {
        let breakdown = self.file(STAGES[3])?;
        let commands = integration_block(&breakdown.text);
        if !commands.is_empty() {
            return Ok(Checks {
                from: ChecksFrom::Breakdown,
                commands,
            });
        }
        let test_command = test_command.trim();
        if test_command.is_empty() {
            bail!(
                "no integration check: {} has no code block under \"{}\" and no `project.test_command` is configured",
                STAGES[3],
                lint::INTEGRATION
            );
        }
        Ok(Checks {
            from: ChecksFrom::Config,
            commands: vec![test_command.to_string()],
        })
    }
}

/// Commands of the first fenced code block in the integration section.
fn integration_block(breakdown: &str) -> Vec<String> {
    let heading = format!("## {}", lint::INTEGRATION);
    let fence = |l: &str| l.trim_start().starts_with("```");
    let mut lines = breakdown
        .lines()
        .skip_while(|l| l.trim() != heading)
        .skip(1);
    // Outside a block, the next heading ends the section; inside one, a
    // line starting with `#` is a shell comment.
    loop {
        match lines.next() {
            None => return Vec::new(),
            Some(l) if fence(l) => break,
            Some(l) if l.starts_with("## ") || l.starts_with("# ") => return Vec::new(),
            Some(_) => {}
        }
    }
    lines
        .take_while(|l| !fence(l))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

impl Contract {
    pub fn task_file(&self) -> &ContractFile {
        let name = format!("tasks/{}.md", self.task);
        self.files
            .iter()
            .find(|f| f.file == name)
            .expect("load checks the task is pinned")
    }

    /// The code prompt for this run. `feedback` is the verifier's report on
    /// the previous attempt, if any. `change_request_path` is where the agent
    /// writes a change request when the contract must change. `checklists`
    /// are the project's checklists for writing code and tests, by absolute
    /// path (`agent_env::checklists`); `test_command` is what verification
    /// runs.
    pub fn prompt(
        &self,
        feedback: Option<&str>,
        change_request_path: &str,
        checklists: &[std::path::PathBuf],
        test_command: &str,
    ) -> String {
        let stages: Vec<String> = STAGES
            .iter()
            .filter_map(|s| self.files.iter().find(|f| f.file == *s))
            .map(|f| {
                format!(
                    "### {} (revision {})\n\n{}",
                    f.file,
                    f.revision,
                    f.text.trim_end()
                )
            })
            .collect();
        let feedback = match feedback {
            Some(text) if !text.trim().is_empty() => format!(
                "## Verifier feedback\n\nThe previous attempt did not pass verification. Fix the \
                 cause; do not weaken the tests.\n\n{}\n\n",
                text.trim_end()
            ),
            _ => String::new(),
        };
        let checklists = match checklists {
            [] => String::new(),
            files => format!(
                "## Checklists\n\nRead these before writing code or tests; they are how this project \
                 does both:\n\n{}\n\n",
                files
                    .iter()
                    .map(|f| format!("- `{}`", f.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        };
        TEMPLATE
            .replace("{{checklists}}\n", &checklists)
            .replace("{{test_command}}", test_command)
            .replace("{{task_id}}", &self.task)
            .replace("{{intake_id}}", &self.intake)
            .replace("{{handover_id}}", &self.manifest.id)
            .replace("{{change_request_path}}", change_request_path)
            .replace("{{verifier_feedback}}\n", &feedback)
            .replace("{{task_contract}}", self.task_file().text.trim_end())
            .replace("{{stage_context}}", &stages.join("\n\n"))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::intake::{handover, readiness, review};
    use std::path::PathBuf;

    const OUTCOME: &str = "# F — Outcome\n\n## Yêu cầu\n\n- REQ-001: lọc\n\n## Câu hỏi còn mở\n";
    const BREAKDOWN: &str = "# F — Breakdown\n\n## Task\nTASK-001, TASK-002\n\n## Kiểm chứng tích hợp\nChạy toàn bộ test.\n\n## Câu hỏi còn mở\n";

    fn task(id: &str, deps: &str) -> String {
        format!(
            "---\nid: {id}\nparent: F\nrequirements: [REQ-001]\ndepends_on: [{deps}]\n---\n\n# {id}\n\n\
             ## Mục tiêu\nLọc theo {id}.\n## Input\nAPI.\n## Output\nDanh sách.\n## Ràng buộc\nGiữ shape.\n\
             ## Tự chủ\nTự chọn hàm.\n## Acceptance và kiểm chứng\n- AC-01: lọc đúng\n## Bàn giao\nLocal.\n\
             ## Cần amendment khi\nĐổi shape.\n## Câu hỏi còn mở\n"
        )
    }

    pub(crate) struct Fixture {
        _dir: tempfile::TempDir,
        pub root: PathBuf,
        pub intake: Intake,
    }

    /// A git project with intake F accepted and handed over as HANDOVER-001:
    /// TASK-001 (no dependency) and TASK-002 (depends on TASK-001).
    pub(crate) fn handed_over() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".zforge")).unwrap();
        std::fs::write(
            root.join(".zforge/config.yaml"),
            "project:\n  name: t\n  test_command: \"true\"\nexecution:\n  budget_usd: 2.0\n",
        )
        .unwrap();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success())
        };
        git(&["init", "-q", "."]);
        git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "base",
        ]);

        let intake = review::create(&root, "F").unwrap();
        let d = &intake.dir;
        std::fs::write(d.join("01-outcome.md"), OUTCOME).unwrap();
        std::fs::write(
            d.join("02-behavior.md"),
            "# B\n\n## Tình huống\nREQ-001 lọc.\n\n## Câu hỏi còn mở\n",
        )
        .unwrap();
        std::fs::write(
            d.join("03-solution.md"),
            "# S\n\n## Luồng\nLọc.\n\n## Câu hỏi còn mở\n",
        )
        .unwrap();
        std::fs::write(d.join("04-breakdown.md"), BREAKDOWN).unwrap();
        std::fs::write(d.join("tasks/TASK-001.md"), task("TASK-001", "")).unwrap();
        std::fs::write(d.join("tasks/TASK-002.md"), task("TASK-002", "TASK-001")).unwrap();
        for f in intake.files() {
            review::review(&intake, &f).unwrap();
            review::accept(&intake, &f, None).unwrap();
        }
        let config = crate::config::load_from(&root.join(".zforge/config.yaml")).unwrap();
        let r = readiness::check(&intake, &root, &[], &config).unwrap();
        assert!(r.ready, "{:?}", r.checks);
        handover::create(&intake, &root, &[], &config, &r.files, None).unwrap();
        Fixture {
            _dir: dir,
            root,
            intake,
        }
    }

    /// AC-01.
    #[test]
    fn loads_verified_snapshots_and_refuses_a_tampered_one() {
        let f = handed_over();
        let c = load(&f.root, "HANDOVER-001", "TASK-001").unwrap();
        assert_eq!(c.intake, "F");
        assert_eq!(c.files.len(), 6);
        assert_eq!(c.manifest_sha256.len(), 64);
        assert!(c.task_file().text.contains("Lọc theo TASK-001"));
        assert_eq!(
            load(&f.root, "F/HANDOVER-001", "TASK-001").unwrap().files,
            c.files
        );

        let snap = record::snapshot_path(&f.intake, "03-solution.md", 1);
        let text = std::fs::read_to_string(&snap).unwrap();
        std::fs::write(&snap, text.replacen('L', "l", 1)).unwrap();
        let err = load(&f.root, "HANDOVER-001", "TASK-001")
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "contract snapshot 03-solution.md rev 1 does not match HANDOVER-001; nothing was run"
        );

        std::fs::remove_file(&snap).unwrap();
        let err = load(&f.root, "HANDOVER-001", "TASK-001")
            .unwrap_err()
            .to_string();
        assert!(err.contains("is missing"), "{err}");
    }

    /// AC-02.
    #[test]
    fn editing_the_working_files_does_not_change_the_prompt() {
        let f = handed_over();
        let before = load(&f.root, "HANDOVER-001", "TASK-001").unwrap().prompt(
            None,
            ".zforge/intakes/F/changes/CHANGE-001.md",
            &[],
            "cargo test",
        );
        std::fs::write(
            f.intake.dir.join("tasks/TASK-001.md"),
            task("TASK-001", "").replace("Lọc theo", "SỬA"),
        )
        .unwrap();
        std::fs::write(f.intake.dir.join("01-outcome.md"), "edited").unwrap();
        let after = load(&f.root, "HANDOVER-001", "TASK-001").unwrap().prompt(
            None,
            ".zforge/intakes/F/changes/CHANGE-001.md",
            &[],
            "cargo test",
        );
        assert_eq!(before, after);
        assert!(!after.contains("SỬA"));
    }

    /// MOC-C TASK-003: the leaves hold every other task's output; a
    /// breakdown without a code block falls back to the test command.
    #[test]
    fn a_handover_knows_its_leaves_and_its_integration_check() {
        let f = handed_over();
        let h = load_handover(&f.root, "HANDOVER-001").unwrap();
        assert_eq!(h.leaves().unwrap(), ["TASK-002"]);
        assert_eq!(
            h.checks("cargo test").unwrap(),
            Checks {
                from: ChecksFrom::Config,
                commands: vec!["cargo test".into()]
            }
        );
        let err = h.checks("  ").unwrap_err().to_string();
        assert!(err.contains("no integration check"), "{err}");
    }

    /// AC-01/AC-04: the first block of the integration section, line by
    /// line, comments and blank lines dropped; blocks elsewhere ignored.
    #[test]
    fn the_integration_block_is_read_from_its_section_only() {
        let breakdown = "# K\n\n## Task\n\n```bash\nnot this\n```\n\n\
            ## Kiểm chứng tích hợp\n\nChạy ở gốc repo:\n\n```bash\n# all of it\ncargo test\n\n  cargo fmt --check  \n```\n\n\
            ```bash\nnor this\n```\n\n## Câu hỏi còn mở\n\n```\nnor this either\n```\n";
        assert_eq!(
            integration_block(breakdown),
            ["cargo test", "cargo fmt --check"]
        );
        assert!(integration_block(
            "# K\n\n## Kiểm chứng tích hợp\nprose only\n\n## Câu hỏi còn mở\n```\nx\n```\n"
        )
        .is_empty());
    }

    /// MOC-C TASK-002: a dependent task loads, with its dependencies.
    #[test]
    fn a_task_carries_its_dependencies() {
        let f = handed_over();
        assert_eq!(
            load(&f.root, "HANDOVER-001", "TASK-002")
                .unwrap()
                .depends_on,
            ["TASK-001"]
        );
        assert!(load(&f.root, "HANDOVER-001", "TASK-001")
            .unwrap()
            .depends_on
            .is_empty());
        let err = load(&f.root, "HANDOVER-001", "TASK-009")
            .unwrap_err()
            .to_string();
        assert!(err.contains("is not in HANDOVER-001"), "{err}");
        assert!(load(&f.root, "HANDOVER-404", "TASK-001")
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }

    /// Layer 1 of `agent_env`: checklists are named by absolute path, and
    /// the section is absent when there are none.
    #[test]
    fn prompt_names_the_checklists_by_path() {
        let f = handed_over();
        let c = load(&f.root, "HANDOVER-001", "TASK-001").unwrap();
        let list = [std::path::PathBuf::from("/store/write-tests-first.md")];
        let p = c.prompt(None, "CR.md", &list, "cargo test");
        assert!(p.contains("## Checklists"), "{p}");
        assert!(p.contains("- `/store/write-tests-first.md`"), "{p}");
        assert!(!c
            .prompt(None, "CR.md", &[], "cargo test")
            .contains("## Checklists"));
        assert!(!p.contains("{{checklists}}"));
    }

    #[test]
    fn prompt_carries_the_contract_context_feedback_and_change_path() {
        let f = handed_over();
        let c = load(&f.root, "HANDOVER-001", "TASK-001").unwrap();
        let p = c.prompt(None, "CR.md", &[], "cargo test");
        assert!(p.starts_with("# Leaf task TASK-001 — F / HANDOVER-001"));
        assert!(p.contains("## The task contract\n\n---\nid: TASK-001"));
        assert!(
            p.contains("### 01-outcome.md (revision 1)")
                && p.contains("### 04-breakdown.md (revision 1)")
        );
        assert!(!p.contains("### tasks/"), "other tasks are not context");
        assert!(p.contains("to `CR.md`"));
        assert!(p.contains("Verification runs `cargo test` in this directory"));
        assert!(!p.contains("Verifier feedback") && !p.contains("{{"));

        let p = c.prompt(Some("FAIL add_small"), "CR.md", &[], "cargo test");
        assert!(p.contains("## Verifier feedback") && p.contains("FAIL add_small"));
        assert!(p.find("Verifier feedback").unwrap() < p.find("The task contract").unwrap());
    }

    #[test]
    fn an_ambiguous_handover_id_must_be_qualified() {
        let f = handed_over();
        let other = f.root.join(".zforge/intakes/G/.records/handovers");
        std::fs::create_dir_all(&other).unwrap();
        for sub in ["tasks", "changes"] {
            std::fs::create_dir_all(f.root.join(".zforge/intakes/G").join(sub)).unwrap();
        }
        std::fs::write(other.join("HANDOVER-001.json"), "{}").unwrap();
        let err = find_handover(&f.root, "HANDOVER-001")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("exists in intakes F, G") || err.contains("exists in intakes G, F"),
            "{err}"
        );
        assert_eq!(find_handover(&f.root, "F/HANDOVER-001").unwrap().id, "F");
    }
}
