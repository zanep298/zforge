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
//! [`Guard::brief`] tells the agent all of this before its first attempt,
//! naming the existing protected tests next to the code the task names: a
//! test edited by mistake costs a whole attempt.
//!
//! This is a guard against the plain case, not a proof: a test weakened
//! inside a file that also holds code (a Rust unit test module, say) or a
//! test runner reconfigured elsewhere is not seen.

use super::git;
use crate::intake::lint;
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

/// How many existing protected tests [`Guard::brief`] names.
pub const BRIEF_FILES: usize = 30;

/// Which paths are protected, and which of those the contract lets change.
pub struct Guard {
    protected: Vec<Regex>,
    allowed: Vec<Regex>,
    /// The globs and files behind `protected` and `allowed`, as written, for
    /// [`Guard::brief`].
    patterns: Vec<String>,
    command_files: Vec<String>,
    allowed_globs: Vec<String>,
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
        let patterns: Vec<String> = match patterns {
            Some(p) => p.to_vec(),
            None => DEFAULT_PROTECTED.iter().map(|g| g.to_string()).collect(),
        };
        let command_files: Vec<String> = test_command
            .split_whitespace()
            .map(|t| t.trim_start_matches("./"))
            .filter(|t| !t.starts_with('-') && work_dir.join(t).is_file())
            .map(String::from)
            .collect();
        Self {
            protected: patterns
                .iter()
                .chain(&command_files)
                .map(|g| glob(g))
                .collect(),
            allowed: allowed.iter().map(|g| glob(g)).collect(),
            patterns,
            command_files,
            allowed_globs: allowed.to_vec(),
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

impl Guard {
    /// The prompt section telling the code agent, before it starts, which
    /// tests it may not change: the rule, the task's exemptions, and the
    /// protected files among `tracked` (the worktree's, at the start) near
    /// the code `task_text` names — at most [`BRIEF_FILES`]. Empty when
    /// nothing is protected.
    pub fn brief(&self, tracked: &[String], task_text: &str) -> String {
        if self.protected.is_empty() {
            return String::new();
        }
        let code = |v: &[String]| {
            v.iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut out = String::from(
            "## Tests you must not change\n\nWhen the suite passes, an existing protected test \
             file that was modified, deleted or renamed makes the attempt a failure, and the \
             attempt is lost. Adding new test files is fine. If an existing protected test has \
             to change for this task, do not edit it: write a change request (see \"When the \
             contract itself must change\") instead.\n\n",
        );
        if !self.patterns.is_empty() {
            out.push_str(&format!(
                "Protected: files matching {}",
                code(&self.patterns)
            ));
        }
        if !self.command_files.is_empty() {
            out.push_str(if self.patterns.is_empty() {
                "Protected: "
            } else {
                "; and "
            });
            out.push_str(&format!(
                "{} (named by the test command)",
                code(&self.command_files)
            ));
        }
        out.push_str(".\n");
        if !self.allowed_globs.is_empty() {
            out.push_str(&format!(
                "This task may change (its `tests_may_change`): {}.\n",
                code(&self.allowed_globs)
            ));
        }
        let near = self.near(tracked, task_text);
        if !near.is_empty() {
            out.push_str("\nExisting protected tests near the code this task names:\n");
            for p in near.iter().take(BRIEF_FILES) {
                out.push_str(&format!("- `{p}`\n"));
            }
            if near.len() > BRIEF_FILES {
                out.push_str(&format!("- … and {} more\n", near.len() - BRIEF_FILES));
            }
        }
        out.push('\n');
        out
    }

    /// Protected files of `tracked` near the paths cited in `task_text`'s
    /// Input, Output and Constraints: under the directory of a cited path,
    /// or named after a cited file (`test_models.py` for `app/models.py`).
    /// Sorted.
    fn near(&self, tracked: &[String], task_text: &str) -> Vec<String> {
        let (_, body) = lint::split_frontmatter(task_text);
        let clean = lint::strip_comments(body);
        let secs = lint::sections(&clean);
        let mut cited: Vec<String> = Vec::new();
        for h in &lint::TASK_SECTIONS[1..=3] {
            for line in lint::section(&secs, *h).unwrap_or_default() {
                cited.extend(backticked_paths(line));
            }
        }
        let dirs: Vec<String> = cited
            .iter()
            .filter_map(|p| {
                let is_file =
                    !p.ends_with('/') && p.rsplit('/').next().is_some_and(|l| l.contains('.'));
                let dir = if is_file {
                    p.rsplit_once('/').map(|(d, _)| d)?
                } else {
                    p.trim_end_matches('/')
                };
                (!dir.is_empty()).then(|| format!("{dir}/"))
            })
            .collect();
        let stems: Vec<&str> = cited
            .iter()
            .filter_map(|p| p.rsplit('/').next()?.split('.').next())
            .filter(|s| s.len() >= 3)
            .collect();
        let mut hits: Vec<String> = tracked
            .iter()
            .filter(|t| self.guards(t))
            .filter(|t| {
                dirs.iter().any(|d| t.starts_with(d.as_str()))
                    || stems
                        .iter()
                        .any(|s| t.rsplit('/').next().is_some_and(|name| name.contains(s)))
            })
            .cloned()
            .collect();
        hits.sort();
        hits.dedup();
        hits
    }
}

/// Files git tracks in `work_dir`, relative to it.
pub fn tracked(work_dir: &Path) -> Result<Vec<String>> {
    Ok(git::ok(work_dir, &["ls-files", "-z", "--full-name"])?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect())
}

/// Backticked spans that look like a path (contain `/`, no whitespace),
/// without a `:line` suffix or a leading `./`.
fn backticked_paths(line: &str) -> Vec<String> {
    line.split('`')
        .skip(1)
        .step_by(2)
        .filter(|s| s.contains('/') && !s.contains(char::is_whitespace) && !s.contains("://"))
        .map(|s| {
            let s = match s.rsplit_once(':') {
                Some((p, n))
                    if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '-') =>
                {
                    p
                }
                _ => s,
            };
            s.trim_start_matches("./").to_string()
        })
        .collect()
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

    const TASK: &str = "---\nid: TASK-001\n---\n\n# TASK-001\n\n## Goal\nSee `docs/not/this.md`.\n\
        ## Input\n`internal/audit/writer.go:12`\n## Output\n- `app/models.py`\n\
        ## Constraints\nKeep `web/src/` as is.\n<!-- `hidden/x.go` -->\n## Autonomy\n`other/y.go`\n";

    fn tracked() -> Vec<String> {
        [
            "internal/audit/writer.go",
            "internal/audit/writer_test.go",
            "internal/audit/sub/deep_test.go",
            "internal/auditor/x_test.go",
            "tests/test_models.py",
            "tests/test_views.py",
            "web/src/__tests__/App.test.tsx",
            "docs/not/this_test.md",
            "hidden/x_test.go",
            "other/y_test.go",
        ]
        .map(String::from)
        .to_vec()
    }

    /// Protected tests near the paths the task's Input, Output and
    /// Constraints cite: under their directories or named after a cited
    /// file; other sections and comments do not count.
    #[test]
    fn near_lists_protected_tests_by_the_code_the_task_names() {
        let dir = tempfile::tempdir().unwrap();
        let g = defaults("", &[], dir.path());
        assert_eq!(
            g.near(&tracked(), TASK),
            [
                "internal/audit/sub/deep_test.go",
                "internal/audit/writer_test.go",
                "tests/test_models.py",
                "web/src/__tests__/App.test.tsx",
            ]
        );
        // An exempted file is not protected, so not listed.
        let g = defaults("", &["tests/test_models.py"], dir.path());
        assert!(!g
            .near(&tracked(), TASK)
            .contains(&"tests/test_models.py".to_string()));
    }

    /// The brief states the rule, the patterns, the command's files, the
    /// exemptions and the nearby files, bounded with a tail.
    #[test]
    fn the_brief_names_rules_exemptions_and_nearby_tests() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.sh"), "").unwrap();
        let g = defaults("sh test.sh", &["tests/test_views.py"], dir.path());
        let b = g.brief(&tracked(), TASK);
        assert!(b.starts_with("## Tests you must not change\n"), "{b}");
        assert!(b.contains("Adding new test files is fine"), "{b}");
        assert!(b.contains("write a change request"), "{b}");
        assert!(b.contains("files matching `tests/**`, `test/**`"), "{b}");
        assert!(
            b.contains("; and `test.sh` (named by the test command)."),
            "{b}"
        );
        assert!(
            b.contains("(its `tests_may_change`): `tests/test_views.py`."),
            "{b}"
        );
        assert!(b.contains("- `internal/audit/writer_test.go`\n"), "{b}");
        assert!(!b.contains("other/y_test.go"), "{b}");
        assert!(b.ends_with("\n\n"), "{b:?}");

        let many: Vec<String> = (0..45)
            .map(|n| format!("internal/audit/t{n:02}_test.go"))
            .collect();
        let b = g.brief(&many, TASK);
        let listed = b.lines().filter(|l| l.starts_with("- `")).count();
        assert_eq!(listed, BRIEF_FILES);
        assert!(b.contains("- … and 15 more\n"), "{b}");

        // Nothing protected, nothing said.
        let none = Guard::new(Some(&[]), "", &[], dir.path());
        assert_eq!(none.brief(&tracked(), TASK), "");
        // Nothing near: the rule alone.
        let b = defaults("", &[], dir.path()).brief(&[], TASK);
        assert!(!b.contains("near the code"), "{b}");
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
