//! Result types for `zforge doctor` and their rendering.

use colored::Colorize;
use serde::Serialize;

/// How far a capability was confirmed, lowest to highest (IMP-005). Each
/// level is only claimed when it was actually checked; a file existing is
/// never reported as the client using it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Not there at all.
    Missing,
    /// There, but wrong (invalid, pointed elsewhere, failing).
    Broken,
    /// Binary / file present.
    Present,
    /// Configuration written in the right place and parses.
    Configured,
    /// The client itself reports it.
    Recognized,
    /// A smoke test that calls no model passed.
    Working,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Broken => "broken",
            Self::Present => "present",
            Self::Configured => "configured",
            Self::Recognized => "recognized",
            Self::Working => "working",
        }
    }

    pub fn is_problem(self) -> bool {
        matches!(self, Self::Missing | Self::Broken)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub level: Level,
    /// A required check below `Present` fails the doctor run.
    pub required: bool,
    pub detail: String,
    /// What was deliberately not verified, and why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_checked: Option<String>,
    /// Command or step that fixes a problem.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

impl Check {
    pub fn new(
        name: &'static str,
        required: bool,
        level: Level,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name,
            level,
            required,
            detail: detail.into(),
            not_checked: None,
            fix: None,
        }
    }

    pub fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    pub fn not_checked(mut self, why: impl Into<String>) -> Self {
        self.not_checked = Some(why.into());
        self
    }

    pub fn fails(&self) -> bool {
        self.required && self.level.is_problem()
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub client: &'static str,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn failed(&self) -> Vec<&Check> {
        self.checks.iter().filter(|c| c.fails()).collect()
    }

    pub fn render(&self) -> String {
        let mut out = format!("zforge doctor — {}\n\n", self.client);
        for c in &self.checks {
            // `!` whenever there is something to do, even if the level is
            // not a problem in itself (e.g. rtk installed but not wired in).
            let mark = if c.fails() {
                "✗".red()
            } else if c.level.is_problem() || c.fix.is_some() {
                "!".yellow()
            } else {
                "✓".green()
            };
            let req = if c.required { "" } else { " (optional)" };
            out.push_str(&format!(
                "  {mark} {:<14} {:<11} {}{req}\n",
                c.name,
                c.level.label(),
                c.detail
            ));
            if let Some(n) = &c.not_checked {
                out.push_str(&format!("      {} not checked: {n}\n", "·".dimmed()));
            }
            if let Some(f) = &c.fix {
                out.push_str(&format!("      {} fix: {f}\n", "→".cyan()));
            }
        }
        let failed = self.failed().len();
        out.push('\n');
        out.push_str(&if failed == 0 {
            "All required checks passed.\n".to_string()
        } else {
            format!("{failed} required check(s) failed.\n")
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_required_problems_fail_the_run() {
        let r = Report {
            client: "Claude Code",
            checks: vec![
                Check::new("a", true, Level::Working, "ok"),
                Check::new("b", false, Level::Missing, "absent"),
                Check::new("c", true, Level::Broken, "bad").fix("do x"),
            ],
        };
        let failed: Vec<_> = r.failed().iter().map(|c| c.name).collect();
        assert_eq!(failed, vec!["c"]);
        let text = r.render();
        assert!(text.contains("fix: do x"));
        let with_todo = Report {
            client: "x",
            checks: vec![Check::new("rtk", false, Level::Present, "not wired").fix("rtk init -g")],
        };
        assert!(
            with_todo.render().contains("! rtk"),
            "a pending fix is flagged"
        );
        assert!(text.contains("(optional)"));
        assert!(text.contains("1 required check(s) failed"));
    }

    #[test]
    fn levels_are_ordered_by_confidence() {
        assert!(Level::Missing < Level::Broken);
        assert!(Level::Present < Level::Configured);
        assert!(Level::Recognized < Level::Working);
    }
}
