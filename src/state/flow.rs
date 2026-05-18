use super::State;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Pipeline preset that determines which `State`s a task walks through.
///
/// `Full` is the default and matches the legacy 9-state pipeline. The
/// short flows skip phases that don't make sense for their task class:
/// `Docs` has no spec/test/plan, `Spike` has no tests, `Fixbug` has no
/// plan or final review.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flow {
    #[default]
    Full,
    Fixbug,
    Spike,
    Docs,
}

impl Flow {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "full" | "default" => Ok(Flow::Full),
            "fixbug" | "bug" | "bugfix" => Ok(Flow::Fixbug),
            "spike" => Ok(Flow::Spike),
            "docs" | "doc" => Ok(Flow::Docs),
            _ => anyhow::bail!("unknown flow '{s}'. Valid: full, fixbug, spike, docs"),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Flow::Full => "full",
            Flow::Fixbug => "fixbug",
            Flow::Spike => "spike",
            Flow::Docs => "docs",
        }
    }

    pub fn states(&self) -> &'static [State] {
        use State::*;
        match self {
            Flow::Full => &[
                Imported,
                SpecDone,
                TestspecDone,
                TestspecReviewed,
                Planned,
                PlanReviewed,
                Coded,
                Verified,
                Reviewed,
            ],
            Flow::Fixbug => &[Imported, SpecDone, TestspecDone, Coded, Verified],
            Flow::Spike => &[Imported, SpecDone, Coded],
            Flow::Docs => &[Imported, Coded],
        }
    }

    pub fn contains(&self, state: &State) -> bool {
        self.states().iter().any(|s| s == state)
    }

    pub fn next_after(&self, state: &State) -> Option<&'static State> {
        let s = self.states();
        let idx = s.iter().position(|x| x == state)?;
        s.get(idx + 1)
    }

    pub fn previous_of(&self, state: &State) -> Option<&'static State> {
        let s = self.states();
        let idx = s.iter().position(|x| x == state)?;
        if idx == 0 {
            None
        } else {
            Some(&s[idx - 1])
        }
    }

    /// Dispatch-form command (no `--done`) for the state *after* `current`
    /// in this flow. Used by status/print-next lines.
    pub fn next_dispatch_command(&self, current: &State, task_id: &str) -> String {
        match self.next_after(current) {
            None => "task complete".to_string(),
            Some(next) => dispatch_command(next, task_id),
        }
    }
}

/// Maps a target state to the CLI command that produces it (dispatch form,
/// no `--done`). Used when a CLI prints "Next: …" so a user can copy/paste.
pub fn dispatch_command(target: &State, task_id: &str) -> String {
    match target {
        State::SpecDone => format!("zf spec {}", task_id),
        State::TestspecDone => format!("zf testspec {}", task_id),
        State::TestspecReviewed => format!("zf approve {} testspec", task_id),
        State::Planned => format!("zf plan {}", task_id),
        State::PlanReviewed => format!("zf approve {} plan", task_id),
        State::Coded => format!("zf code {}", task_id),
        State::Verified => format!("zf verify {}", task_id),
        State::Reviewed => format!("zf review {}", task_id),
        State::Imported | State::Unknown => "task complete".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_recognises_aliases() {
        assert_eq!(Flow::parse("full").unwrap(), Flow::Full);
        assert_eq!(Flow::parse("default").unwrap(), Flow::Full);
        assert_eq!(Flow::parse("FIXBUG").unwrap(), Flow::Fixbug);
        assert_eq!(Flow::parse("bug").unwrap(), Flow::Fixbug);
        assert_eq!(Flow::parse("bugfix").unwrap(), Flow::Fixbug);
        assert_eq!(Flow::parse("spike").unwrap(), Flow::Spike);
        assert_eq!(Flow::parse("docs").unwrap(), Flow::Docs);
        assert_eq!(Flow::parse("doc").unwrap(), Flow::Docs);
    }

    #[test]
    fn parse_rejects_unknown() {
        assert!(Flow::parse("frobnicate").is_err());
    }

    #[test]
    fn full_flow_matches_legacy_order() {
        let s = Flow::Full.states();
        assert_eq!(s.first(), Some(&State::Imported));
        assert_eq!(s.last(), Some(&State::Reviewed));
        assert_eq!(s.len(), 9);
    }

    #[test]
    fn fixbug_skips_plan_and_review() {
        let s = Flow::Fixbug.states();
        assert!(!s.contains(&State::Planned));
        assert!(!s.contains(&State::PlanReviewed));
        assert!(!s.contains(&State::TestspecReviewed));
        assert!(!s.contains(&State::Reviewed));
        assert!(s.contains(&State::Verified));
    }

    #[test]
    fn spike_has_no_tests() {
        let s = Flow::Spike.states();
        assert_eq!(
            s,
            &[State::Imported, State::SpecDone, State::Coded][..]
        );
    }

    #[test]
    fn docs_has_no_spec() {
        assert_eq!(Flow::Docs.states(), &[State::Imported, State::Coded][..]);
    }

    #[test]
    fn next_after_chains_through_flow() {
        let f = Flow::Fixbug;
        assert_eq!(f.next_after(&State::Imported), Some(&State::SpecDone));
        assert_eq!(f.next_after(&State::SpecDone), Some(&State::TestspecDone));
        assert_eq!(f.next_after(&State::TestspecDone), Some(&State::Coded));
        assert_eq!(f.next_after(&State::Coded), Some(&State::Verified));
        assert_eq!(f.next_after(&State::Verified), None);
    }

    #[test]
    fn next_after_skips_phases_not_in_flow() {
        // Docs: Imported → Coded directly.
        assert_eq!(Flow::Docs.next_after(&State::Imported), Some(&State::Coded));
    }

    #[test]
    fn next_after_returns_none_for_state_not_in_flow() {
        // Docs doesn't include SpecDone — should return None rather than
        // silently advancing through unrelated phases.
        assert_eq!(Flow::Docs.next_after(&State::SpecDone), None);
    }

    #[test]
    fn previous_of_returns_predecessor_within_flow() {
        let f = Flow::Fixbug;
        assert_eq!(f.previous_of(&State::Coded), Some(&State::TestspecDone));
        assert_eq!(f.previous_of(&State::Imported), None);
    }

    #[test]
    fn contains_only_returns_true_for_flow_states() {
        assert!(Flow::Spike.contains(&State::SpecDone));
        assert!(!Flow::Spike.contains(&State::TestspecDone));
        assert!(!Flow::Docs.contains(&State::SpecDone));
    }

    #[test]
    fn next_dispatch_command_uses_flow_specific_successor() {
        let cmd = Flow::Fixbug.next_dispatch_command(&State::TestspecDone, "TASK-1");
        assert_eq!(cmd, "zf code TASK-1");
    }

    #[test]
    fn dispatch_command_covers_every_pipeline_state() {
        for s in Flow::Full.states() {
            let c = dispatch_command(s, "T-1");
            assert!(!c.is_empty());
        }
    }
}
