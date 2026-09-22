//! The contract a run works from (MOC-B TASK-003; REQ-001, REQ-004, REQ-009).
//!
//! A run never reads the working intake files. It loads the handover
//! manifest, reads every snapshot the manifest pins from the intake's
//! `.records/revisions/`, and checks each against the pinned SHA-256; one
//! mismatch and nothing is run. The prompt is built from those snapshots
//! only, so editing a contract file while a run is going — or ever after —
//! does not change what that run was given.

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

/// Load and verify the contract of `task` in handover `handover_id`
/// (`HANDOVER-001` or `<INTAKE>/HANDOVER-001`).
pub fn load(project_root: &Path, handover_id: &str, task: &str) -> Result<Contract> {
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
    if !manifest.tasks.iter().any(|t| t == task) {
        bail!(
            "{task} is not in {id} (handed over: {})",
            manifest.tasks.join(", ")
        );
    }
    let task_file = format!("tasks/{task}.md");
    if !manifest.files.iter().any(|f| f.file == task_file) {
        bail!("{id} does not pin {task_file}");
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

    let task_text = &files
        .iter()
        .find(|f| f.file == task_file)
        .expect("checked above")
        .text;
    let meta = lint::parse_task_meta(task_text).map_err(|e| anyhow::anyhow!("{task_file}: {e}"))?;
    if !meta.depends_on.is_empty() {
        bail!(
            "{task} depends on {}; running dependent tasks comes with Mốc C",
            meta.depends_on.join(", ")
        );
    }

    Ok(Contract {
        intake: intake.id,
        manifest,
        manifest_sha256: hash::sha256(&raw),
        task: task.to_string(),
        files,
    })
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
    /// writes a change request when the contract must change.
    pub fn prompt(&self, feedback: Option<&str>, change_request_path: &str) -> String {
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
        TEMPLATE
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
        let r = readiness::check(&intake, &root, &[], &config.execution).unwrap();
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
        let before = load(&f.root, "HANDOVER-001", "TASK-001")
            .unwrap()
            .prompt(None, ".zforge/intakes/F/changes/CHANGE-001.md");
        std::fs::write(
            f.intake.dir.join("tasks/TASK-001.md"),
            task("TASK-001", "").replace("Lọc theo", "SỬA"),
        )
        .unwrap();
        std::fs::write(f.intake.dir.join("01-outcome.md"), "edited").unwrap();
        let after = load(&f.root, "HANDOVER-001", "TASK-001")
            .unwrap()
            .prompt(None, ".zforge/intakes/F/changes/CHANGE-001.md");
        assert_eq!(before, after);
        assert!(!after.contains("SỬA"));
    }

    /// AC-03.
    #[test]
    fn a_task_with_dependencies_is_refused() {
        let f = handed_over();
        let err = load(&f.root, "HANDOVER-001", "TASK-002")
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "TASK-002 depends on TASK-001; running dependent tasks comes with Mốc C"
        );
        let err = load(&f.root, "HANDOVER-001", "TASK-009")
            .unwrap_err()
            .to_string();
        assert!(err.contains("is not in HANDOVER-001"), "{err}");
        assert!(load(&f.root, "HANDOVER-404", "TASK-001")
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }

    #[test]
    fn prompt_carries_the_contract_context_feedback_and_change_path() {
        let f = handed_over();
        let c = load(&f.root, "HANDOVER-001", "TASK-001").unwrap();
        let p = c.prompt(None, "CR.md");
        assert!(p.starts_with("# Leaf task TASK-001 — F / HANDOVER-001"));
        assert!(p.contains("## The task contract\n\n---\nid: TASK-001"));
        assert!(
            p.contains("### 01-outcome.md (revision 1)")
                && p.contains("### 04-breakdown.md (revision 1)")
        );
        assert!(!p.contains("### tasks/"), "other tasks are not context");
        assert!(p.contains("to `CR.md`"));
        assert!(!p.contains("Verifier feedback") && !p.contains("{{"));

        let p = c.prompt(Some("FAIL add_small"), "CR.md");
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
