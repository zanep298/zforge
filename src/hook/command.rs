//! The decisions a user can type to Claude Code: `/accept`, `/revise`,
//! `/handover`. Parsing only — what a word names (an intake, a file) is
//! resolved against the project in [`super::decide`].

/// A decision the user typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `/accept [TARGET] [all | FILE…]` — no file means every file under
    /// review.
    Accept { words: Vec<String> },
    /// `/revise [TARGET] FILE: NOTE…` — `note` is what follows the `:`.
    /// Without one, `note` is `None` and `words` holds everything: the
    /// note starts after the file, which only the project can tell.
    Revise {
        words: Vec<String>,
        note: Option<String>,
    },
    /// `/handover [TARGET]`.
    Handover { words: Vec<String> },
}

impl Command {
    /// The command's name as typed, for messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Accept { .. } => "/accept",
            Self::Revise { .. } => "/revise",
            Self::Handover { .. } => "/handover",
        }
    }
}

/// `None` when `prompt` is not one of zforge's decisions — the prompt then
/// reaches the model untouched. `Some(Err)` when it is one, malformed.
pub fn parse(prompt: &str) -> Option<Result<Command, String>> {
    let prompt = prompt.trim();
    let (head, rest) = prompt
        .split_once(char::is_whitespace)
        .unwrap_or((prompt, ""));
    let words = || rest.split_whitespace().map(String::from).collect();
    match head {
        "/accept" => Some(Ok(Command::Accept { words: words() })),
        "/handover" => Some(Ok(Command::Handover { words: words() })),
        "/revise" => Some(revise(rest)),
        _ => None,
    }
}

/// `[TARGET] FILE: NOTE…`, or the same without `:`.
fn revise(rest: &str) -> Result<Command, String> {
    const USAGE: &str = "usage: /revise [INTAKE] <file>: <what needs to change>";
    let split = |t: &str| -> Vec<String> { t.split_whitespace().map(String::from).collect() };
    let (words, note) = match rest.split_once(':') {
        Some((head, note)) => {
            let note = note.trim();
            if note.is_empty() {
                return Err(format!("say what needs to change; {USAGE}"));
            }
            (split(head), Some(note.to_string()))
        }
        None => (split(rest), None),
    };
    match (words.len(), &note) {
        (0, _) => Err(USAGE.into()),
        (1, None) => Err(format!("say what needs to change in {}; {USAGE}", words[0])),
        _ => Ok(Command::Revise { words, note }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(prompt: &str) -> Command {
        parse(prompt).unwrap().unwrap()
    }

    #[test]
    fn other_prompts_are_not_decisions() {
        for p in [
            "accept all",
            "please /accept all",
            "/acceptance criteria?",
            "/zforge",
            "",
        ] {
            assert_eq!(parse(p), None, "{p}");
        }
    }

    #[test]
    fn accept_takes_optional_words() {
        assert_eq!(ok("/accept"), Command::Accept { words: vec![] });
        assert_eq!(
            ok("  /accept all\n"),
            Command::Accept {
                words: vec!["all".into()]
            }
        );
        assert_eq!(
            ok("/accept ONBOARD 01-outcome TASK-002"),
            Command::Accept {
                words: vec!["ONBOARD".into(), "01-outcome".into(), "TASK-002".into()]
            }
        );
    }

    #[test]
    fn revise_needs_a_file_and_a_note() {
        assert_eq!(
            ok("/revise TASK-003: tách AC-04 ra"),
            Command::Revise {
                words: vec!["TASK-003".into()],
                note: Some("tách AC-04 ra".into())
            }
        );
        assert_eq!(
            ok("/revise ONBOARD 02-behavior: thiếu case lỗi"),
            Command::Revise {
                words: vec!["ONBOARD".into(), "02-behavior".into()],
                note: Some("thiếu case lỗi".into())
            }
        );
        // Without `:`, where the note starts is left to the resolver.
        assert_eq!(
            ok("/revise TASK-003 split AC-04"),
            Command::Revise {
                words: vec!["TASK-003".into(), "split".into(), "AC-04".into()],
                note: None
            }
        );
        assert!(parse("/revise").unwrap().is_err());
        assert!(parse("/revise TASK-003").unwrap().is_err());
        assert!(parse("/revise TASK-003:").unwrap().is_err());
    }

    #[test]
    fn handover_takes_an_optional_target() {
        assert_eq!(ok("/handover"), Command::Handover { words: vec![] });
        assert_eq!(
            ok("/handover ONBOARD"),
            Command::Handover {
                words: vec!["ONBOARD".into()]
            }
        );
    }
}
