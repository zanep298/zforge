//! `zforge questions new <NAME>` and `zforge questions status [<NAME>]
//! [--json]`: shared stakeholder questions (`intake::questions`). The file
//! is not reviewed — anyone records the answers as they arrive — so there
//! is nothing here to accept.

use crate::config;
use crate::intake::questions;
use anyhow::{anyhow, Result};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum QuestionsCmd {
    /// Create `.zforge/questions/<NAME>.md` for questions several intakes
    /// share; cite one from an intake file as `Q:<NAME>/Q-<n>`.
    New {
        /// The file's name, e.g. REFERRAL-V1.
        name: String,
    },
    /// Each question open or answered, and which intake files cite it.
    Status {
        /// One questions file (default: all).
        name: Option<String>,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: QuestionsCmd) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zforge init"))?;
    let root = config.project_root();
    match cmd {
        QuestionsCmd::New { name } => {
            let path = questions::create(&root, &name)?;
            println!(
                "created {}; add `- [ ] Q-1: …` lines and cite them as Q:{name}/Q-1",
                path.display()
            );
        }
        QuestionsCmd::Status { name, json } => {
            let files = questions::status(&root, name.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&files)?);
            } else {
                print!("{}", questions::render(&files));
            }
        }
    }
    Ok(())
}
