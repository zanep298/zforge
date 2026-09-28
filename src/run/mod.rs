//! v1.5 Mốc B: executing a handed-over leaf task (intake MOC-B, decision D3).
//!
//! Each run lives in `.zforge/runs/RUN-nnn/`: `run.yaml`, written once when
//! the run is created, and `events.jsonl`, the append-only log that is the
//! only source of truth for the run's state ([`record`]). Views such as
//! `progress.md` are generated from the log and carry no authority.

pub mod agent_env;
pub mod contract;
pub mod execute;
pub mod feature;
pub mod feature_ops;
pub mod git;
pub mod guard;
pub mod integrate;
pub mod ops;
pub mod output;
pub mod reconcile;
pub mod record;
pub mod reuse;
pub mod review;
pub mod start;
pub mod view;
pub mod worktree;

use std::path::{Path, PathBuf};

/// `HANDOVER-001` or `<INTAKE>/HANDOVER-001`, as opposed to a run id.
pub fn is_handover(id: &str) -> bool {
    id.rsplit('/')
        .next()
        .is_some_and(|last| last.starts_with("HANDOVER-"))
}

/// `<project>/.zforge/runs`.
pub fn runs_dir(project_root: &Path) -> PathBuf {
    project_root.join(".zforge").join("runs")
}
