//! Tests a run may not change (workflow §13: "an agent that edits a test to
//! drop a requirement must not have its output confirmed").
//!
//! A run verifies with the tests in its own worktree, which the agent can
//! edit. So a passing verification is only accepted if no protected test
//! file differs from where the run started: modified, deleted or renamed
//! away. Adding tests is always fine. Protected are the files matching
//! `execution.protected_tests` (default [`DEFAULT_PROTECTED`]) and any file
//! the test command names (`sh test.sh` protects `test.sh`). A contract the
//! user accepted may allow specific changes with `tests_may_change` in the
//! task's frontmatter; nothing else can.
//!
//! This is a guard against the plain case, not a proof: a test weakened
//! inside a file that also holds code (a Rust unit test module, say) or a
//! test runner reconfigured elsewhere is not seen.

use super::git;
use anyhow::Result;
use regex::Regex;
use std::path::Path;

/// Where tests live by the common conventions of the languages zforge knows.
pub const DEFAULT_PROTECTED: &[&str] = &[
    "tests/**",
    "test/**",
    "**/__tests__/**",
    "**/*_test.*",
    "**/test_*.*",
    "**/*.test.*",
    "**/*.spec.*",
    "**/*_spec.*",
];

/// Which paths are protected, and which of those the contract lets change.
pub struct Guard {
    protected: Vec<Regex>,
    allowed: Vec<Regex>,
}

impl Guard {
    /// `patterns`: `execution.protected_tests`, or `None` for the defaults.
    /// Files named by `test_command` that exist in `work_dir` are added.
    pub fn new(
        patterns: Option<&[String]>,
        test_command: &str,
        allowed: &[String],
        work_dir: &Path,
    ) -> Self {
        let mut protected: Vec<Regex> = match patterns {
            Some(p) => p.iter().map(|g| glob(g)).collect(),
            None => DEFAULT_PROTECTED.iter().map(|g| glob(g)).collect(),
        };
        protected.extend(
            test_command
                .split_whitespace()
                .map(|t| t.trim_start_matches("./"))
                .filter(|t| !t.starts_with('-') && work_dir.join(t).is_file())
                .map(glob),
        );
        Self {
            protected,
            allowed: allowed.iter().map(|g| glob(g)).collect(),
        }
    }

    fn guards(&self, path: &str) -> bool {
        self.protected.iter().any(|r| r.is_match(path))
            && !self.allowed.iter().any(|r| r.is_match(path))
    }

    /// Protected files in `work_dir` that differ from `base`: modified,
    /// deleted, or renamed away. Sorted.
    pub fn changed(&self, work_dir: &Path, base: &str) -> Result<Vec<String>> {
        let out = git::ok(work_dir, &["diff", "--name-status", "-M", base])?;
        let mut hits: Vec<String> = changed_away(&out)
            .into_iter()
            .filter(|p| self.guards(p))
            .collect();
        hits.sort();
        hits.dedup();
        Ok(hits)
    }
}

/// Paths `git diff --name-status` shows as no longer what they were: every
/// status but an addition, and for a rename or copy, the source.
fn changed_away(name_status: &str) -> Vec<String> {
    name_status
        .lines()
        .filter_map(|l| {
            let mut cols = l.split('\t');
            let status = cols.next()?;
            let path = cols.next()?;
            match status.chars().next()? {
                'A' | 'C' => None,
                _ => Some(path.to_string()),
            }
        })
        .collect()
}

/// A glob as an anchored regex: `**` spans directories, `*` and `?` do not.
fn glob(pattern: &str) -> Regex {
    let mut re = String::from("^");
    let mut rest = pattern;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("**/") {
            re.push_str("(?:.*/)?");
            rest = r;
        } else if rest == "/**" {
            re.push_str("(?:/.*)?");
            rest = "";
        } else if let Some(r) = rest.strip_prefix("**") {
            re.push_str(".*");
            rest = r;
        } else {
            let c = rest.chars().next().expect("not empty");
            match c {
                '*' => re.push_str("[^/]*"),
                '?' => re.push_str("[^/]"),
                c => re.push_str(&regex::escape(&c.to_string())),
            }
            rest = &rest[c.len_utf8()..];
        }
    }
    re.push('$');
    Regex::new(&re).expect("a glob always makes a valid regex")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults(test_command: &str, allowed: &[&str], dir: &Path) -> Guard {
        let allowed: Vec<String> = allowed.iter().map(|s| s.to_string()).collect();
        Guard::new(None, test_command, &allowed, dir)
    }

    #[test]
    fn globs_follow_path_segments() {
        let g = glob("tests/**");
        assert!(g.is_match("tests/a.rs") && g.is_match("tests/x/y.rs"));
        assert!(!g.is_match("src/tests.rs") && !g.is_match("mytests/a.rs"));
        let g = glob("**/*_test.*");
        assert!(g.is_match("a_test.go") && g.is_match("pkg/a_test.go"));
        assert!(!g.is_match("pkg/a_test") && !g.is_match("a_test.go/x"));
        let g = glob("src/*.rs");
        assert!(g.is_match("src/a.rs") && !g.is_match("src/x/a.rs"));
        assert!(glob("a.b").is_match("a.b") && !glob("a.b").is_match("aXb"));
    }

    #[test]
    fn default_conventions_and_the_test_command_are_protected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.sh"), "").unwrap();
        let g = defaults("sh ./test.sh --quick", &[], dir.path());
        for p in [
            "tests/api.rs",
            "test/widget_test.dart",
            "pkg/handler_test.go",
            "app/test_models.py",
            "web/Button.test.tsx",
            "web/__tests__/x.js",
            "spec/user_spec.rb",
            "test.sh",
        ] {
            assert!(g.guards(p), "{p}");
        }
        for p in ["src/lib.rs", "lib.sh", "README.md", "src/testing.rs"] {
            assert!(!g.guards(p), "{p}");
        }
        // A word of the command that is no file protects nothing.
        assert!(!defaults("cargo test", &[], dir.path()).guards("cargo"));
    }

    #[test]
    fn the_contract_can_allow_a_change_and_config_can_replace_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let g = defaults("", &["tests/api.rs"], dir.path());
        assert!(!g.guards("tests/api.rs") && g.guards("tests/other.rs"));
        let own = vec!["checks/**".to_string()];
        let g = Guard::new(Some(&own), "", &[], dir.path());
        assert!(g.guards("checks/a.sh") && !g.guards("tests/a.rs"));
        let g = Guard::new(Some(&[]), "", &[], dir.path());
        assert!(!g.guards("tests/a.rs"), "an empty list protects nothing");
    }

    #[test]
    fn additions_are_not_changes() {
        let out = "M\ttests/a.rs\nA\ttests/new.rs\nD\ttests/gone.rs\nR100\ttests/old.rs\ttests/moved.rs\nC75\ttests/src.rs\ttests/copy.rs\n";
        assert_eq!(
            changed_away(out),
            ["tests/a.rs", "tests/gone.rs", "tests/old.rs"]
        );
    }
}
