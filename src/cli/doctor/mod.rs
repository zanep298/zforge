//! `zforge doctor` — what the Claude Code setup actually does, not what init
//! says it wrote (IMP-005).
//!
//! Init reports the files it created; that does not show Claude loads them,
//! that a registered MCP server connects, or that a hook runs. Each check
//! here confirms as far as it can without calling a model — see
//! [`report::Level`] — and says what it could not confirm. Required checks
//! (the `claude` CLI, the default runner, the phase agents) fail the run;
//! optional ones warn with the command that fixes them.
//!
//! Claude Code only for now: it is the primary client. Codex and OpenCode
//! checks can be added as their own sets of `checks` functions.

mod checks;
mod claude_config;
mod report;

use crate::cli::outcome::OperationOutcome;
use crate::config;
use anyhow::Result;
use checks::Ctx;
use report::Report;

pub fn run(json: bool) -> Result<OperationOutcome> {
    let config = config::load().map_err(|_| anyhow::anyhow!("Config not found. Run: zf init"))?;
    let ctx = Ctx {
        root: config.project_root(),
        config: &config,
    };

    let report = Report {
        client: "Claude Code",
        checks: vec![
            checks::claude_cli(&ctx),
            checks::runner(&ctx),
            checks::agents(&ctx),
            checks::skills(&ctx),
            checks::zforge_mcp(&ctx),
            checks::codegraph_mcp(&ctx),
            checks::rtk_hook(&ctx),
            checks::caveman_hook(&ctx),
            checks::workspace_trust(&ctx),
            checks::runs(&ctx),
        ],
    };

    // The report is the product of this command, so it goes to stdout
    // directly (it is not progress output).
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.render());
    }

    let failed: Vec<&str> = report.failed().iter().map(|c| c.name).collect();
    Ok(if failed.is_empty() {
        OperationOutcome::Success
    } else {
        OperationOutcome::failed(format!("required checks failed: {}", failed.join(", ")))
    })
}
