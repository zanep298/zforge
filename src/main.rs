use anyhow::Result;
use clap::{Parser, Subcommand};
use zforge::{cli, mcp};

#[derive(Parser)]
#[command(name = "zforge", version, about = "TDD-first AI development workflow")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold .zforge/ + agent-specific files. Default agent: claude.
    /// Uses the global ~/.zforge/ store by default; pass --local to copy
    /// every template, agent definition, and skill into the project.
    Init {
        /// Which AI coding agent to scaffold for: claude (default), codex, opencode, all.
        #[arg(long, default_value = "claude")]
        agent: String,
        #[arg(long)]
        force: bool,
        /// Copy templates, agents, and skills into the project instead of
        /// sharing the global ~/.zforge/ store. Useful for forks that need
        /// per-project customization of every file.
        #[arg(long)]
        local: bool,
        /// Skip auto-registration of this project in ~/.zforge/registry.yaml.
        #[arg(long)]
        no_register: bool,
        /// Override the registered project name (default: sanitized basename of cwd).
        #[arg(long)]
        name: Option<String>,
        /// Set this project as the current_project after registering.
        #[arg(long)]
        switch: bool,
    },
    /// Populate the global ~/.zforge/ store with embedded templates,
    /// agents, and skills. Run once per machine; rerun after upgrading
    /// zforge to refresh. Use --force to overwrite local edits.
    Install {
        #[arg(long)]
        force: bool,
    },
    /// Download and install the latest zforge release from GitHub.
    /// Replaces the current binary in-place. No brew update required.
    Update,
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
    /// Run code + verify in one go. Idempotent: skips code if state already Coded.
    ///
    /// When `--max-iterations` > 1, ship runs the SWE-bench-style verifier loop:
    /// on test failure, the failed test names + verify.md are fed back into the
    /// next code attempt as feedback. Loop bails when the budget is exhausted.
    Ship {
        task_id: String,
        #[arg(long)]
        command: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
        /// Maximum number of code → verify cycles. 1 = legacy single-shot
        /// (default). > 1 enables the verifier-driven retry loop.
        #[arg(long, default_value_t = 1)]
        max_iterations: u32,
        /// Detach: spawn a background worker, print the job ID, exit.
        /// Poll with `zforge job status|wait|log <ID>`.
        #[arg(long)]
        r#async: bool,
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
        /// Aggregate active tasks across every registered project in ~/.zforge/registry.yaml.
        #[arg(long)]
        global: bool,
        /// Per-project scan timeout when running --global.
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
    },
    Retry {
        task_id: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        yes: bool,
    },
    /// MCP server: stdio JSON-RPC (no subcommand) or manage Claude Code registration.
    Mcp {
        #[command(subcommand)]
        action: Option<McpAction>,
    },
    /// Manage the global project registry at ~/.zforge/registry.yaml.
    Project {
        #[command(subcommand)]
        cmd: crate::cli::project::ProjectCmd,
    },
    /// Manage background jobs (spawned by `zforge ship --async`).
    Job {
        #[command(subcommand)]
        cmd: crate::cli::job::JobCmd,
    },
    /// Inspect spawn-level cost telemetry from .zforge/cost-log.jsonl.
    Cost {
        #[command(subcommand)]
        cmd: crate::cli::cost::CostCmd,
    },
    /// Git primitives for the auto-approve branch workflow.
    /// Invoked by the `/zforge` slash command via Bash; safe to call manually.
    Git {
        #[command(subcommand)]
        cmd: crate::cli::git::GitCmd,
    },
    /// INTERNAL: background worker entry point — invoked by `ship --async`.
    /// Do not call directly.
    #[command(hide = true)]
    Worker {
        #[arg(long)]
        job_id: String,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Register zforge as an MCP server with one or more AI coding agents.
    Register {
        /// Target agent: all (default), claude, codex, opencode.
        #[arg(long, default_value = "all")]
        agent: String,
        /// Re-register if already present.
        #[arg(long)]
        force: bool,
    },
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
        /// Pipeline preset: full (default), fixbug, spike, docs
        #[arg(long)]
        flow: Option<String>,
        /// Primary agent name (must exist in ~/.zforge/registry.yaml agents{} map).
        #[arg(long)]
        agent: Option<String>,
        /// Fallback agent triggered on retryable failures. Must differ from --agent.
        #[arg(long)]
        fallback: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            agent,
            force,
            local,
            no_register,
            name,
            switch,
        } => {
            let parsed = cli::mcp_register::Agent::parse(&agent)?;
            cli::init::run(parsed, force, local, no_register, name, switch)
        }
        Commands::Install { force } => cli::install::run(force),
        Commands::Update => cli::update::run(),
        Commands::Task { action } => match action {
            TaskAction::Import {
                task_id,
                title,
                domain,
                description,
                jira,
                figma,
                figma_context,
                flow,
                agent,
                fallback,
            } => {
                cli::task::run_import(
                    task_id.as_deref(),
                    title,
                    domain,
                    description,
                    jira,
                    figma,
                    figma_context,
                    flow.as_deref(),
                    agent,
                    fallback,
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
        Commands::Ship {
            task_id,
            command,
            timeout,
            max_iterations,
            r#async,
        } => {
            if r#async {
                cli::ship::run_async(&task_id, command, timeout, max_iterations)
            } else {
                cli::ship::run(&task_id, command, timeout, max_iterations)
            }
        }
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
            global,
            timeout_ms,
        } => {
            if global {
                cli::status::run_global(timeout_ms, json)
            } else {
                cli::status::run(task_id, json, short)
            }
        }
        Commands::Retry { task_id, from, yes } => cli::retry::run(&task_id, &from, yes),
        Commands::Mcp { action } => match action {
            None => mcp::run(),
            Some(McpAction::Register { agent, force }) => {
                let target = cli::mcp_register::Agent::parse(&agent)?;
                cli::mcp_register::run(target, force)
            }
        },
        Commands::Project { cmd } => cli::project::run(cmd),
        Commands::Job { cmd } => cli::job::run(cmd),
        Commands::Cost { cmd } => cli::cost::run(cmd),
        Commands::Git { cmd } => cli::git::run(cmd),
        Commands::Worker { job_id } => zforge::job::worker::run(&job_id),
    }
}
