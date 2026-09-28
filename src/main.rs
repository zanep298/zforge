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

#[derive(Subcommand, Debug)]
enum KnowledgeCmd {
    /// Regenerate `.zforge/knowledge/index.md` and `index.json` from the
    /// accepted revisions of every intake.
    Index {
        #[arg(long)]
        json: bool,
    },
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
        /// Default runner: claude, codex or opencode. Default: the only client set up, or with --agent all
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
    /// Where each intake stands — files, handovers, runs — and the next
    /// step. `--global` covers every project in ~/.zforge/registry.yaml.
    Status {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        global: bool,
        /// With --global: how long to wait for all projects together.
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u64,
    },
    /// Move this project from the removed task pipeline to v1.5: its tasks,
    /// memory and old agents go to `.zforge/v1-archive/`, config and models
    /// lose the v1 keys, agents and instruction files are regenerated. Shows
    /// the plan and asks first.
    Migrate {
        /// Show what would change and stop.
        #[arg(long)]
        dry_run: bool,
        /// Do not ask for confirmation.
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
    /// Check what the Claude Code setup actually does: CLI, runner, agents,
    /// skills, MCP servers (connected?), hooks (do they run?) and runs left
    /// behind. Exits 1 when a required check fails.
    Doctor {
        /// Machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// v1.5 intake: create an intake, send its files for review, record the
    /// user's decisions. `accept` / `revise` need an interactive terminal.
    Intake {
        #[command(subcommand)]
        cmd: crate::cli::intake::IntakeCmd,
    },
    /// Build a handover — every task in its own worktree, then the
    /// integration check (`zforge run <HANDOVER> [--task <TASK>] [--async]`)
    /// — or operate runs (`status`, `list`, `log`, `wait`, `cancel`, `retry`,
    /// `clean`).
    Run(crate::cli::run::RunArgs),
    /// Which model runs each phase (`code`, `review`), per client: show,
    /// `set`, `unset`. Unset phases use zforge's default tier (Claude:
    /// sonnet for code, opus for review) or the client's own default.
    Models(crate::cli::models::ModelsArgs),
    /// INTERNAL: background run worker — started by `zforge run --async`,
    /// for one run or (`--handover`) a whole handover.
    #[command(hide = true)]
    RunWorker {
        #[arg(required_unless_present = "handover")]
        run_id: Option<String>,
        #[arg(long, conflicts_with = "run_id")]
        handover: Option<String>,
    },
    /// v1.5: check whether an intake's accepted files are ready to hand
    /// over (§6.2). Writes `readiness.md`; exits 1 when not ready.
    Readiness {
        id: String,
        /// Limit the handover to these tasks (default: all).
        #[arg(long = "task")]
        tasks: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// v1.5: hand the accepted contract over as a manifest (§6.3).
    /// Interactive terminal only.
    Handover {
        id: String,
        /// Limit the handover to these tasks (default: all).
        #[arg(long = "task")]
        tasks: Vec<String>,
    },
    /// v1.5 product knowledge built from accepted intakes.
    Knowledge {
        #[command(subcommand)]
        cmd: KnowledgeCmd,
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
/// [`OperationOutcome::Success`]. A command with a real verdict — `doctor` —
/// returns it so it survives to the exit code instead of being flattened
/// into 0; `run` maps its own verdicts to exit codes.
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
        Commands::Mcp { action } => match action {
            None => mcp::run(),
            Some(McpAction::Register { agent, force }) => {
                let target = cli::mcp_register::Agent::parse(&agent)?;
                cli::mcp_register::run(target, force)
            }
        },
        Commands::Project { cmd } => cli::project::run(cmd),
        Commands::Migrate { dry_run, yes } => cli::migrate::run(dry_run, yes),
        Commands::Status {
            json,
            global,
            timeout_ms,
        } => {
            if global {
                cli::status::run_global(timeout_ms, json)
            } else {
                cli::status::run(json)
            }
        }
        Commands::Intake { cmd } => cli::intake::run(cmd),
        Commands::Run(args) => cli::run::run(args),
        Commands::Models(args) => cli::models::run(args),
        Commands::RunWorker { run_id, handover } => match (run_id, handover) {
            (Some(run_id), _) => cli::run::worker(&run_id),
            (None, Some(handover)) => cli::run::feature_worker(&handover),
            (None, None) => unreachable!("clap requires one"),
        },
        Commands::Knowledge {
            cmd: KnowledgeCmd::Index { json },
        } => cli::intake::knowledge_index(json),
        Commands::Readiness { id, tasks, json } => cli::intake::readiness(&id, &tasks, json),
        Commands::Handover { id, tasks } => cli::intake::handover(&id, &tasks),
        // Handled by `dispatch` because it carries a verdict.
        Commands::Doctor { .. } => {
            unreachable!("outcome-bearing commands are dispatched before dispatch_unit")
        }
    }
}
