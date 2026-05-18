pub mod flow;
pub use flow::{dispatch_command, Flow};

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StateError {
    #[error("invalid transition from {from} to {to} — states must advance one at a time")]
    InvalidTransition { from: String, to: String },
    #[error("state is {state}, need {required}\nRun: {hint}")]
    Insufficient {
        state: String,
        required: String,
        hint: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Unknown,
    Imported,
    SpecDone,
    TestspecDone,
    TestspecReviewed,
    Planned,
    PlanReviewed,
    Coded,
    Verified,
    Reviewed,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Unknown => "Unknown",
            State::Imported => "Imported",
            State::SpecDone => "SpecDone",
            State::TestspecDone => "TestspecDone",
            State::TestspecReviewed => "TestspecReviewed",
            State::Planned => "Planned",
            State::PlanReviewed => "PlanReviewed",
            State::Coded => "Coded",
            State::Verified => "Verified",
            State::Reviewed => "Reviewed",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            State::Unknown => "run: zf task import <TASK-ID>",
            State::Imported => "run: zf spec <TASK-ID>",
            State::SpecDone => "run: zf testspec <TASK-ID>",
            State::TestspecDone => "run: zf approve <TASK-ID> testspec",
            State::TestspecReviewed => "run: zf plan <TASK-ID>",
            State::Planned => "run: zf approve <TASK-ID> plan",
            State::PlanReviewed => "run: zf code <TASK-ID>",
            State::Coded => "run: zf verify <TASK-ID>",
            State::Verified => "run: zf review <TASK-ID>",
            State::Reviewed => "task complete",
        }
    }

}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StateEntry {
    pub state: State,
    pub at: DateTime<Local>,
    pub note: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskState {
    pub task_id: String,
    #[serde(default)]
    pub flow: Flow,
    pub state: State,
    pub updated_at: DateTime<Local>,
    pub history: Vec<StateEntry>,
}

impl TaskState {
    #[allow(dead_code)]
    pub fn new(task_id: &str) -> Self {
        Self::new_with_flow(task_id, Flow::default())
    }

    pub fn new_with_flow(task_id: &str, flow: Flow) -> Self {
        let now = Local::now();
        Self {
            task_id: task_id.to_string(),
            flow,
            state: State::Imported,
            updated_at: now,
            history: vec![StateEntry {
                state: State::Imported,
                at: now,
                note: String::new(),
            }],
        }
    }

    /// Dispatch command (no `--done`) for the next phase in this task's
    /// flow, e.g. `"zf testspec TASK-1"`. Used by `status` and any CLI
    /// command that prints "Next: …".
    pub fn next_hint(&self) -> String {
        self.flow
            .next_dispatch_command(&self.state, &self.task_id)
    }

    pub fn load(tasks_dir: &Path, task_id: &str) -> Result<Self> {
        let path = tasks_dir.join(task_id).join(".state.yaml");
        let content = std::fs::read_to_string(&path)
            .map_err(|_| anyhow::anyhow!("state file not found for task {}", task_id))?;
        let state: TaskState = serde_yaml::from_str(&content)?;
        Ok(state)
    }

    pub fn save(&self, tasks_dir: &Path) -> Result<()> {
        let dir = tasks_dir.join(&self.task_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(".state.yaml");
        let content = serde_yaml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn advance(&mut self, next: State, note: &str) -> Result<()> {
        // Advance is flow-aware: only the state that immediately follows
        // `self.state` *in this task's flow* is a legal transition. This
        // lets short flows (e.g. fixbug) skip phases like Planned without
        // sequencing through them.
        let expected = self.flow.next_after(&self.state).cloned();
        if expected.as_ref() != Some(&next) {
            return Err(StateError::InvalidTransition {
                from: self.state.as_str().to_string(),
                to: next.as_str().to_string(),
            }
            .into());
        }
        let now = Local::now();
        self.state = next.clone();
        self.updated_at = now;
        self.history.push(StateEntry {
            state: next,
            at: now,
            note: note.to_string(),
        });
        Ok(())
    }

    pub fn require(&self, minimum: State) -> Result<()> {
        if self.state < minimum {
            return Err(StateError::Insufficient {
                state: self.state.as_str().to_string(),
                required: minimum.as_str().to_string(),
                hint: minimum.hint().to_string(),
            }
            .into());
        }
        Ok(())
    }

    /// Rewinds the FSM to `target` and records the rewind in history. Unlike
    /// `advance`, this is allowed to move backwards — it is the only sanctioned
    /// way for `retry` to do so. History entries past `target` are dropped, but
    /// every entry at or below `target` is preserved so the audit trail is not
    /// wiped (regression: prior implementation in `retry::run` replaced history
    /// with a single synthetic entry).
    pub fn reset_to(&mut self, target: State, note: &str) -> Result<()> {
        let now = Local::now();
        // Keep every history entry whose state is at or below the reset target.
        self.history.retain(|entry| entry.state <= target);
        self.state = target.clone();
        self.updated_at = now;
        self.history.push(StateEntry {
            state: target,
            at: now,
            note: note.to_string(),
        });
        Ok(())
    }

    #[allow(dead_code)]
    pub fn is(&self, s: &State) -> bool {
        &self.state == s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_advance_valid_transition() {
        let mut ts = TaskState::new("TASK-1");
        ts.advance(State::SpecDone, "spec done").unwrap();
        assert_eq!(ts.state, State::SpecDone);
    }

    #[test]
    fn test_advance_invalid_transition() {
        let mut ts = TaskState::new("TASK-1");
        let result = ts.advance(State::Coded, "skip");
        assert!(result.is_err());
    }

    #[test]
    fn test_advance_sequential() {
        let mut ts = TaskState::new("TASK-1");
        ts.advance(State::SpecDone, "").unwrap();
        ts.advance(State::TestspecDone, "").unwrap();
        ts.advance(State::TestspecReviewed, "").unwrap();
        ts.advance(State::Planned, "").unwrap();
        ts.advance(State::PlanReviewed, "").unwrap();
        ts.advance(State::Coded, "").unwrap();
        ts.advance(State::Verified, "").unwrap();
        ts.advance(State::Reviewed, "").unwrap();
        assert_eq!(ts.state, State::Reviewed);
        assert_eq!(ts.history.len(), 9);
    }

    #[test]
    fn test_require_satisfied() {
        let ts = TaskState::new("TASK-1");
        assert!(ts.require(State::Imported).is_ok());
    }

    #[test]
    fn test_require_not_satisfied() {
        let ts = TaskState::new("TASK-1");
        let result = ts.require(State::SpecDone);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Imported"));
        assert!(msg.contains("SpecDone"));
    }

    #[test]
    fn test_save_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut ts = TaskState::new("TASK-99");
        ts.advance(State::SpecDone, "note").unwrap();
        ts.save(tmp.path()).unwrap();
        let loaded = TaskState::load(tmp.path(), "TASK-99").unwrap();
        assert_eq!(loaded.state, State::SpecDone);
        assert_eq!(loaded.task_id, "TASK-99");
        assert_eq!(loaded.history.len(), 2);
    }

    #[test]
    fn test_state_ordering() {
        assert!(State::Imported < State::SpecDone);
        assert!(State::SpecDone < State::TestspecDone);
        assert!(State::Coded < State::Verified);
    }

    #[test]
    fn reset_to_truncates_future_history() {
        let mut ts = TaskState::new("TASK-1");
        for next in [
            State::SpecDone,
            State::TestspecDone,
            State::TestspecReviewed,
            State::Planned,
        ] {
            ts.advance(next, "").unwrap();
        }
        assert_eq!(ts.history.len(), 5);

        ts.reset_to(State::SpecDone, "retry from testspec").unwrap();

        assert_eq!(ts.state, State::SpecDone);
        // Imported + SpecDone + reset entry → 3 entries; nothing past SpecDone retained.
        assert_eq!(ts.history.len(), 3);
        assert!(ts.history.iter().all(|e| e.state <= State::SpecDone));
        assert_eq!(
            ts.history.last().map(|e| e.note.as_str()),
            Some("retry from testspec")
        );
    }

    #[test]
    fn reset_to_imported_keeps_initial_entry() {
        let mut ts = TaskState::new("TASK-1");
        ts.advance(State::SpecDone, "").unwrap();
        ts.advance(State::TestspecDone, "").unwrap();

        ts.reset_to(State::Imported, "retry from spec").unwrap();

        assert_eq!(ts.state, State::Imported);
        // Original Imported + reset Imported = 2 entries.
        assert_eq!(ts.history.len(), 2);
        assert!(ts.history.iter().all(|e| e.state == State::Imported));
    }

    #[test]
    fn reset_to_persists_across_load() {
        let tmp = TempDir::new().unwrap();
        let mut ts = TaskState::new("TASK-42");
        ts.advance(State::SpecDone, "spec").unwrap();
        ts.advance(State::TestspecDone, "testspec").unwrap();
        ts.reset_to(State::SpecDone, "retry").unwrap();
        ts.save(tmp.path()).unwrap();

        let loaded = TaskState::load(tmp.path(), "TASK-42").unwrap();
        assert_eq!(loaded.state, State::SpecDone);
        assert_eq!(loaded.history.len(), 3);
    }
}
