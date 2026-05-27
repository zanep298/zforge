use crate::config;
use crate::cost::{
    log::load_all,
    report::{filter_since, filter_task, format_table, rollup, GroupBy},
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum CostCmd {
    /// Aggregate cost-log.jsonl entries into a table.
    Report(ReportArgs),
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Grouping: task | phase | agent | model | step. Default: task.
    #[arg(long, default_value = "task")]
    pub by: String,

    /// Only include entries for this task.
    #[arg(long)]
    pub task: Option<String>,

    /// ISO 8601 cutoff — only include entries on/after this timestamp.
    /// Example: `--since 2026-05-01T00:00:00Z`.
    #[arg(long)]
    pub since: Option<String>,

    /// Emit raw rollup data as JSON instead of the formatted table.
    #[arg(long)]
    pub json: bool,
}

pub fn run(cmd: CostCmd) -> Result<()> {
    match cmd {
        CostCmd::Report(a) => report(a),
    }
}

fn report(args: ReportArgs) -> Result<()> {
    let config = config::load().map_err(|_| anyhow!("Config not found. Run: zf init"))?;
    let project_root = config.project_root();
    let mut entries = load_all(&project_root)?;

    if let Some(s) = &args.since {
        let cutoff: DateTime<Utc> = s
            .parse()
            .map_err(|e| anyhow!("invalid --since timestamp {s:?}: {e}"))?;
        entries = filter_since(&entries, cutoff);
    }

    if let Some(task) = &args.task {
        entries = filter_task(&entries, task).into_iter().cloned().collect();
    }

    let by = GroupBy::parse(&args.by)
        .ok_or_else(|| anyhow!("--by must be one of: task, phase, agent, model, step"))?;
    let rollups = rollup(&entries, by);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&rollups)?);
    } else if rollups.is_empty() {
        println!("No cost entries.");
    } else {
        print!("{}", format_table(&rollups, by));
    }
    Ok(())
}
