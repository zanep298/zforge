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

    pub fn valid_next(&self) -> Vec<State> {
        match self {
            State::Unknown => vec![State::Imported],
            State::Imported => vec![State::SpecDone],
            State::SpecDone => vec![State::TestspecDone],
            State::TestspecDone => vec![State::TestspecReviewed],
            State::TestspecReviewed => vec![State::Planned],
            State::Planned => vec![State::PlanReviewed],
            State::PlanReviewed => vec![State::Coded],
            State::Coded => vec![State::Verified],
            State::Verified => vec![State::Reviewed],
            State::Reviewed => vec![],
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
    pub state: State,
    pub updated_at: DateTime<Local>,
    pub history: Vec<StateEntry>,
}

impl TaskState {
    pub fn new(task_id: &str) -> Self {
        let now = Local::now();
        Self {
            task_id: task_id.to_string(),
            state: State::Imported,
            updated_at: now,
            history: vec![StateEntry {
                state: State::Imported,
                at: now,
                note: String::new(),
            }],
        }
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
        let valid = self.state.valid_next();
        if !valid.contains(&next) {
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
}
