use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use zforge::cli::outcome::OperationOutcome;
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
        /// Refresh generated files (agent definitions, CLAUDE.md/AGENTS.md,
        /// settings). Never resets .zforge/config.yaml, models.yaml or memory.
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
        /// Runner for tasks imported without --agent: claude, codex or
        /// opencode. Default: the only client set up, or with --agent all
        /// the first of claude → codex → opencode found on PATH.
        #[arg(long)]
        default_runner: Option<String>,
        /// Do not install missing tools (rtk, codegraph, caveman). Tools
        /// already installed are still configured.
        #[arg(long)]
        no_install: bool,
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
    /// Run code + verify in one go.
    ///
    /// When `--max-iterations` > 1, ship runs the SWE-bench-style verifier loop:
    /// on test failure, the failed test names + verify.md are fed back into the
    /// next code attempt. The budget counts verifier runs. A task already at
    /// Coded resumes by verifying the existing code first, then uses the
    /// remaining iterations for fixes. Flows without a verify step (docs,
    /// spike) end at Coded.
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
    /// Check what the Claude Code setup actually does: CLI, runner, phase
    /// agents, skills, MCP servers (connected?), hooks (do they run?), and
    /// whether verified tasks' evidence still matches the code. Exits 1 when
    /// a required check fails.
    Doctor {
        /// Machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Follow a task from requirement to verified code: per phase, the
    /// runner, agent, model, skills and tools each run actually used (as the
    /// client reported them), problems found, and which candidate passed.
    Trace {
        task_id: String,
        /// Machine-readable report.
        #[arg(long)]
        json: bool,
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

/// Exit-code contract:
///
/// | code | meaning                                                     |
/// |------|-------------------------------------------------------------|
/// | 0    | the operation ran and succeeded                              |
/// | 1    | the operation ran and produced a negative result, OR zforge could not run it |
/// | 2    | a precondition gate refused the request; nothing ran         |
/// | 124  | the operation exceeded its time budget                       |
/// | 130  | the operation was cancelled                                  |
///
/// Commands that can only succeed or error return `()`, which converts to
/// [`OperationOutcome::Success`]. Commands with a real verdict — `verify`,
/// `ship`, `worker` — return the verdict so it survives to the exit code
/// instead of being flattened into 0.
fn main() -> ExitCode {
    match dispatch() {
        Ok(outcome) => {
            if let Some(reason) = outcome.reason() {
                eprintln!("{}: {reason}", outcome.label());
            }
            outcome.exit_code()
        }
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::from(zforge::cli::outcome::EXIT_FAILED)
        }
    }
}

fn dispatch() -> Result<OperationOutcome> {
    let cli = Cli::parse();

    // Commands whose result is a verdict, not just success-or-error, are
    // handled here. Everything else can only succeed or fail to run, so it
    // goes through `dispatch_unit` and maps to `Success`.
    match cli.command {
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
                // The controller only schedules the job; the verdict belongs
                // to the worker and is read back via `zforge job status`.
                cli::ship::run_async(&task_id, command, timeout, max_iterations).map(Into::into)
            } else {
                cli::ship::run(&task_id, command, timeout, max_iterations)
            }
        }
        Commands::Worker { job_id } => zforge::job::worker::run(&job_id),
        Commands::Doctor { json } => cli::doctor::run(json),
        other => dispatch_unit(other).map(Into::into),
    }
}

fn dispatch_unit(command: Commands) -> Result<()> {
    match command {
        Commands::Init {
            agent,
            force,
            local,
            no_register,
            name,
            switch,
            default_runner,
            no_install,
        } => cli::init::run(cli::init::InitOptions {
            agent: cli::mcp_register::Agent::parse(&agent)?,
            force,
            local,
            no_register,
            name,
            switch,
            default_runner,
            install_missing: !no_install,
        }),
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
        Commands::Trace { task_id, json } => cli::trace::run(&task_id, json),
        // Handled by `dispatch` because they carry a verdict.
        Commands::Verify { .. }
        | Commands::Ship { .. }
        | Commands::Worker { .. }
        | Commands::Doctor { .. } => {
            unreachable!("outcome-bearing commands are dispatched before dispatch_unit")
        }
    }
}
