mod cli;
mod config;
mod error;
mod fs;
mod jira;
mod mcp;
mod prompt;
mod runner;
mod state;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "zf", version, about = "TDD-first AI development workflow")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold .zforge/ + .claude/ + CLAUDE.md for Claude Code workflow.
    Init {
        #[arg(long)]
        force: bool,
    },
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    Spec {
        task_id: String,
        #[arg(long)]
        done: bool,
        #[arg(long)]
        copy: bool,
    },
    Testspec {
        task_id: String,
        #[arg(long)]
        done: bool,
    },
    Plan {
        task_id: String,
        #[arg(long)]
        done: bool,
    },
    Code {
        task_id: String,
        #[arg(long)]
        done: bool,
    },
    Verify {
        task_id: String,
        #[arg(long)]
        command: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
    },
    Review {
        task_id: String,
        #[arg(long)]
        done: bool,
    },
    Approve {
        task_id: String,
        artifact: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    Status {
        task_id: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        short: bool,
    },
    Retry {
        task_id: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        yes: bool,
    },
    /// Start the zforge MCP server (stdio). Register locally as `zf mcp`.
    Mcp,
}

#[derive(Subcommand)]
enum TaskAction {
    Import {
        task_id: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Jira ticket URL (e.g. https://company.atlassian.net/browse/PROJ-123)
        #[arg(long)]
        jira: Option<String>,
        /// Figma node URL (e.g. https://figma.com/design/FILE/...?node-id=...)
        #[arg(long)]
        figma: Option<String>,
        /// Figma design context (pre-fetched via Figma MCP)
        #[arg(long)]
        figma_context: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => cli::init::run(force),
        Commands::Task { action } => match action {
            TaskAction::Import {
                task_id,
                title,
                domain,
                description,
                jira,
                figma,
                figma_context,
            } => {
                cli::task::run_import(
                    task_id.as_deref(),
                    title,
                    domain,
                    description,
                    jira,
                    figma,
                    figma_context,
                )?;
                Ok(())
            }
        },
        Commands::Spec {
            task_id,
            done,
            copy,
        } => cli::spec::run(&task_id, done, copy),
        Commands::Testspec { task_id, done } => cli::testspec::run(&task_id, done),
        Commands::Plan { task_id, done } => cli::plan::run(&task_id, done),
        Commands::Code { task_id, done } => cli::code::run(&task_id, done),
        Commands::Verify {
            task_id,
            command,
            timeout,
        } => cli::verify::run(&task_id, command, timeout),
        Commands::Review { task_id, done } => cli::review::run(&task_id, done),
        Commands::Approve {
            task_id,
            artifact,
            note,
            yes,
        } => cli::approve::run(&task_id, &artifact, note, yes),
        Commands::Status {
            task_id,
            json,
            short,
        } => cli::status::run(task_id, json, short),
        Commands::Retry { task_id, from, yes } => cli::retry::run(&task_id, &from, yes),
        Commands::Mcp => mcp::run(),
    }
}
