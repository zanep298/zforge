# zForge

Agree on the work, then let an agent build it.

- **Intake.** You agree on the work in an intake: outcome, behavior,
  solution, breakdown, and one contract per task. Only you accept it, one
  file at a time.
- **Runs.** A run builds a handed-over intake with **Claude Code**. Each task
  runs in its own git worktree and never touches your checkout.
- **Verification.** A result counts only when your test command passes and no
  protected test changed. It is then sealed as a commit on its own branch, for
  you to review and merge.

**Codex** and **OpenCode** can prepare intakes and follow runs through the MCP
server. The decisions stay yours.

## Install

### Homebrew (macOS & Linux), recommended

```bash
brew tap zanep298/tap && brew install zforge
zforge --version
```

### From a checkout (requires Rust)

```bash
cargo install --path . --locked --force
zforge --version
```

If `zforge --version` is not the version you just installed, another copy
comes earlier on your `PATH`. `which -a zforge` lists every copy.

### Prebuilt binaries

Download the archive for your platform from the
[releases page](https://github.com/zanep298/zforge/releases):
`zforge-<target>.tar.gz` for `x86_64-unknown-linux-gnu`,
`x86_64-apple-darwin` and `aarch64-apple-darwin`, or
`zforge-x86_64-pc-windows-msvc.zip`. Extract it and put the `zforge` binary
on your `PATH`, for example in `~/.local/bin`.

### Install script (macOS & Linux)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/zanep298/zforge/main/install.sh | sh
```

The script downloads the release archive for your platform (Linux x86_64,
macOS x86_64 or arm64) and installs the `zforge` binary to `~/.local/bin`.
`ZFORGE_INSTALL=<dir>` picks another directory. `ZFORGE_VERSION=<tag>` installs
a specific release instead of the latest.

### Updating

`zforge update` installs the latest GitHub release in place of the running
binary. If you installed with Homebrew, use `brew upgrade zforge` instead.
After any upgrade, `zforge install` — or any `zforge init` — refreshes the
global store `~/.zforge/`; see [Upgrading](#upgrading).

## Requirements

- A git repository with at least one commit. Runs branch off it, and
  readiness refuses a project without one.
- `claude` (Claude Code) on `PATH`. Runs call it headless.
- The project trusted in Claude Code. Without trust, `claude -p` ignores the
  project's permission rules. `zforge doctor` checks this.
- A test command, `project.test_command` in `.zforge/config.yaml`. `init`
  fills it in from the detected language. Check that it is right.
- A budget, `execution.budget_usd`. This is what each task may spend across
  its runs in a handover. `zforge readiness` refuses a handover until it is
  set.

## Quick start

```bash
# 1. Set up the project for Claude Code
zforge init
zforge mcp register --agent claude
zforge doctor
#    In .zforge/config.yaml, check project.test_command and set execution.budget_usd

# 2. Optional: teach zforge the project (domain, conventions, rules)
zforge onboard                  # probe the project, no model call
zforge onboard draft            # draft domain.md with Claude, then review it

# 3. Agree on the work. Claude can draft the files (skill zforge-intake).
zforge intake new FEAT-1
zforge intake task FEAT-1 TASK-001
zforge intake review FEAT-1 --all
zforge intake pack FEAT-1       # what you are about to decide
#    then accept: type /accept all in Claude Code, or run
zforge intake accept FEAT-1 --all

# 4. Hand over and build
zforge readiness FEAT-1
#    then hand over: type /handover in Claude Code, or run
zforge handover FEAT-1
zforge run HANDOVER-001         # every task, then the integration check; --async for background
zforge run status HANDOVER-001

# Where things stand, and what to run next
zforge status
```

## Concepts

### Your decisions

You make three decisions: accepting a file, asking for changes to it, and
handing an intake over. zforge records them only from you, through one of two
channels:

- **In Claude Code**, type `/accept`, `/revise` or `/handover`. zforge's
  prompt hook reads your own message before the model sees it, then records
  the decision.
- **In a terminal**, run `zforge intake accept|revise` or `zforge handover`.
  These commands need an interactive terminal and a typed confirmation.

No flag, environment variable or MCP tool bypasses either channel, and the
generated `.claude/settings.json` denies the model these commands. The same
rule covers the project's knowledge: `zforge onboard accept|revise|baseline`.

### How a run works

```
handover: pins the accepted snapshots of every file
  → per task, in dependency order:
      worktree from its dependencies' outputs (or the baseline commit)
      → code agent → your test command → protected tests unchanged?
      → [review agent, if execution.review: true or the task says review: true]
      → retry with the feedback, up to execution.max_iterations
      → output sealed as a commit on zforge/<task>/<run>
  → integration run: merges the leaf tasks' outputs and runs the
    "Integration verification" checks of the breakdown (else
    project.integration_command, else the test command)
  → you merge
  → zforge knowledge index then reports the requirements as integrated
```

Each run records everything in `.zforge/runs/RUN-nnn/events.jsonl`. That file
is the only source of truth: `zforge run status` and `zforge status` derive
from it. A run ends in one of these states:

- **verified**: its output is sealed on its branch.
- **failed**: verification did not pass within `max_iterations`, or the run
  could not go on (for example, a merge conflict or a failing integration
  check).
- **blocked**: the budget ran out, or the agent asked to amend the contract.
  You amend the intake and hand it over again.
- **cancelled**: the run was stopped.

Running `zforge run HANDOVER-…` again resumes. Verified tasks stay verified,
and cancelled or interrupted runs are retried. Failed and blocked runs stay as
they are; `zforge run retry <RUN>` runs a failed one again.

Some details of a run:

- **Protected tests.** The prompt lists, before the agent starts, the tests it
  must not change: the protected patterns, the files the test command names,
  the task's `tests_may_change`, and the protected tests near the code the
  task names. A protected test that has to change calls for a change request.
- **Review per task.** A task's frontmatter `review: true` or `review: false`
  overrides `execution.review` for that task.
- **Stuck.** A run stops early with `stuck: …` when two attempts in a row
  fail on the same tests, since the next attempt would get the same feedback.
  `execution.test_timeout_secs` (default 300) bounds one run of the test
  command.
- **Conflicting outputs.** When a task's output conflicts with a sibling's
  that it must be merged with, zforge runs that task again on top of the
  other's output, and the agent reconciles the two.
- **Integration checks.** With `project.integration_command` (one command per
  line) the breakdown can leave out its own check block. A check that fails on
  the environment (Docker, testcontainers, a refused connection… and the
  substrings in `execution.infra_patterns`) is run once more after 5 seconds;
  failing again, the run fails with an `infra:` reason, and the next
  `zforge run HANDOVER-…` retries it.

### Project knowledge

`zforge onboard` probes the project without calling a model and writes
`baseline.md`. The project's own knowledge lives in `domain.md`,
`conventions.md` and `rules.md` under `knowledge.dir` (default
`docs/knowledge/`). You review and accept those files the same way as intake
files. `zforge onboard draft` and `zforge onboard refresh` draft or redraft
them with Claude, and `zforge onboard status` shows citations that went
stale. Runs get the relevant parts in their prompt: conventions and rules the
task cites or whose paths it touches first, then the others within
`knowledge.general_limit` (default 8000 bytes; 0 lists them by ID only).
Agents can look up more with `zforge knowledge get <ID…> | --module | --grep`.

### Questions and fixes across intakes

- **Stakeholder questions.** `zforge questions new <NAME>` creates
  `.zforge/questions/<NAME>.md`, a shared list of questions
  (`- [ ] Q-1: …`, answered as `- [x] Q-1: … => answer`). Intake files cite
  one as `Q:<NAME>/Q-1`. The file is not reviewed. Readiness refuses a
  handover while a cited question is open, and the handover pins the file.
- **Superseding.** A requirement line in `01-outcome.md` may end with
  `(supersedes <INTAKE>/REQ-nnn)`. The commitments then show the old
  requirement as superseded.
- **Tiny intakes.** `zforge intake new <ID> --tiny` is for one task in one
  module with no new interface: a short outcome (summary, requirements, what
  must not change, open questions), `tasks/TASK-001.md`, and 02/03/04 as
  standard text pointing at that task — its acceptance criteria are the
  behavior, its Constraints the binding decisions. All five files are still
  reviewed and accepted; the review pack shows untouched standard text as one
  line. A second task is a lint error: switch `profile:` to `small`.
- **Fixups.** `zforge intake amend <NEW> --from <ID> --fixup` starts a small
  intake for a few contained fixes to integrated work: a short outcome with
  new requirement IDs, behavior and solution stating only what changes, and
  one task.
- **Review notes.** `zforge intake review … --note "…"` records why a file
  changed. `zforge intake pack` shows the notes, and a new revision sent
  without one gets a warning.

### Status

`zforge status` shows these lines:

- **init**: the zforge version that last ran `init` here. If it differs from
  this binary, the line tells you to run `zforge init --force`.
- **project**: whether the project is onboarded.
- **Per intake**: each file's state, stale files, each handover's tasks and
  integration, and the next command to run.

`--global` covers every registered project, and `--json` gives the same data
for tools.

## Init and clients

`zforge init` sets up the project for one client (`--agent claude`, the
default) or several (`codex`, `opencode`, `all`).

| Command | What it creates | MCP auto-register? |
|---------|----------------|--------------------|
| `zforge init` | `.zforge/`, `CLAUDE.md`, `.claude/settings.json` (permissions + prompt hook), `.claude/agents/`, `.claude/skills/zforge-*`, `.claude/commands/` (`/accept`, `/revise`, `/handover`), `.claude/rules/` | No: run `zforge mcp register --agent claude` |
| `zforge init --agent codex` | `.zforge/`, `AGENTS.md`, `.codex/agents/`, `.codex/README.md` | Yes: `~/.codex/config.toml` |
| `zforge init --agent opencode` | `.zforge/`, `AGENTS.md`, `.opencode/agents/` | Yes: `~/.config/opencode/opencode.json` |
| `zforge init --agent all` | All of the above | Yes: Codex and OpenCode |

Init also does the following:

- **Tools.** Installs and configures rtk, CodeGraph (pinned to this project)
  and caveman. `--no-install` configures the tools already present but
  installs nothing.
- **Registry.** Registers the project in `~/.zforge/registry.yaml`. Use
  `--no-register`, `--name` or `--switch` to change this.
- **Version.** Records itself as `zforge.version` in `.zforge/config.yaml`.
- **Onboarding.** Offers to onboard when run at a terminal (not with
  `--force`).

You can run init again at any time. `.zforge/config.yaml` is created once;
later inits change only the keys init owns (`zforge.version`,
`runner.default`, `paths.*`). `models.yaml` is never overwritten.

Init has these options:

- **Shared or local store.** By default, agents and skills come from the
  global store `~/.zforge/`, which `zforge install` and every `init` bring to
  the binary's version. `--local` copies them into the project instead.
- **`--force`.** Refreshes generated files: agents, skills, commands,
  `CLAUDE.md`/`AGENTS.md` and settings. A `CLAUDE.md` or `AGENTS.md` your team
  wrote is left alone, and `settings.json` is merged rather than replaced.
- **`--git-exclude`.** Keeps zforge's own files out of git through
  `.git/info/exclude`, which is never committed.

The agents are `code-agent` (every attempt of a run) and `review-agent`
(optional review of passing work).

### Register the MCP server

```bash
zforge mcp register                    # Claude Code, Codex and OpenCode (default: all)
zforge mcp register --agent claude     # Claude Code only (uses `claude mcp add`)
zforge mcp register --agent codex      # Codex (writes ~/.codex/config.toml)
zforge mcp register --agent opencode   # OpenCode (writes ~/.config/opencode/opencode.json)
zforge mcp register --force            # re-register, overwriting any existing entry
```

| Agent | Config location | Method |
|-------|----------------|--------|
| Claude Code | local MCP registry | `claude mcp add zforge -- zforge mcp` |
| Codex | `~/.codex/config.toml` | `[mcp_servers.zforge]` block |
| OpenCode | `~/.config/opencode/opencode.json` | `mcp.zforge` entry |

If `claude` is not on `PATH`, Claude Code is skipped, not failed.

## Models

zforge names no model, only tiers. Claude runs `sonnet` for code and `opus`
for review by default. `zforge models` shows which model each client uses
for each phase, and where that choice comes from. To change it:

```bash
zforge models set code opus                  # this project
zforge models set review sonnet --global     # every project
zforge models set all inherit                # the client's own default model
zforge models unset code
```

`inherit` passes no `--model`, so the client uses its own default.

## Commands

| Command | Description |
|---------|-------------|
| `zforge init [--agent …] [--force] [--local] [--git-exclude] [--no-install]` | Set up the project for a client |
| `zforge install [--force]` | Populate or refresh the global store `~/.zforge/` (your edits kept; `--force` replaces them, archiving copies) |
| `zforge doctor [--json]` | Check the Claude Code setup: CLI, agents, skills, MCP, hooks, trust, runs left behind |
| `zforge status [--global] [--json]` | Every intake, its handovers and runs, and the next step |
| `zforge onboard` | Probe the project, with no model call; writes `baseline.md` |
| `zforge onboard status\|review [--note …]\|draft\|refresh` | Follow, send for review, or draft the project's knowledge |
| `zforge onboard accept\|revise\|baseline` | Record your decision (interactive terminal only) |
| `zforge knowledge get <ID…> [--module] [--grep] [--extra]` | Print accepted knowledge items |
| `zforge knowledge index` | Rewrite `commitments.md` (each requirement: decided and built) and `INDEX.md` in `knowledge.dir` |
| `zforge intake new [--small\|--tiny]\|task\|status\|graph\|pack <ID> …` | Prepare an intake, see where it stands |
| `zforge intake review <ID> <file>\|--all [--note …]` | Send files for review, saying what changed |
| `zforge intake accept <ID> <file>\|--all`, `intake revise <ID> <file> --note …` | Record your decision (interactive terminal only) |
| `zforge intake amend <NEW> --from <ID> [--fixup]` | Start a new intake amending an integrated one (`--fixup`: a short one for a few fixes) |
| `zforge questions new <NAME>` | Create a shared stakeholder questions file |
| `zforge questions status [<NAME>] [--json]` | Each question, open or answered, and the intake files citing it |
| `zforge readiness <ID>` | Whether the accepted files can be handed over, and why not |
| `zforge handover <ID>` | Hand the accepted contract over (interactive terminal only) |
| `zforge run <HANDOVER> [--task T \| --integration] [--async]` | Build a handover, one task of it, or its integration check |
| `zforge run status\|list\|log\|wait\|cancel\|retry\|clean` | Follow and manage runs |
| `zforge run compare <RUN> <RUN>… [--check <cmd>]` | Compare runs side by side |
| `zforge run prune-branches [--dry-run]` | Delete merged `zforge/*` branches of finished runs |
| `zforge models [set\|unset]` | Which model runs each phase |
| `zforge migrate [--dry-run] [--yes]` | Move a project from the old task pipeline to v1.5 |
| `zforge project add\|list\|remove\|switch\|current` | The registry of zforge projects on this machine |
| `zforge mcp` / `zforge mcp register` | Run the MCP server (stdio), or register it with a client |
| `zforge update` | Install the latest release |

## Upgrading

### After a new zforge version

`zforge install` — or any `zforge init` — refreshes the global store
`~/.zforge/`: every agent and skill zforge wrote and you did not change is
replaced by the new version's. zforge records what it wrote in
`~/.zforge/.shipped.json`, so a file you edited is kept and reported
(`kept your edit: skills/intake.md …`); `zforge install --force` replaces
those too, keeping a copy of each in `~/.zforge/archive/<time>/`. A store
written before zforge kept that record is refreshed the same way, with a
copy of every file that differed archived first. `zforge doctor` shows
whether the store matches the binary.

`zforge init` refreshes the agents and native skills it rendered into the
project the same way (record in `.zforge/.shipped.json`, copies in
`.zforge/archive/<time>/`). Agents rendered before that record existed, and
the other generated files, follow the zforge version that wrote them: when
`zforge status` shows an `init:` line naming another version, run:

```bash
zforge init --force     # refresh what init generated in this project
```

### From the task pipeline (v1)

`zforge install` migrates the global store `~/.zforge` (so does the first
`init` after an upgrade):

- Files that only the old pipeline used move to `~/.zforge/v1-archive/<time>/`.
- Agents and skills are brought up to date, and the old copies are archived.
- `registry.yaml` and `models.yaml` drop the removed settings.

Then, in each project:

```bash
zforge migrate --dry-run   # see what would change
zforge migrate             # archive .zforge/tasks, memory, old agents; clean config; regenerate
```

Nothing is deleted: everything moved or rewritten is kept under
`.zforge/v1-archive/<time>/`. A CLAUDE.md or AGENTS.md your team wrote, rather
than one zforge generated, is left as it is. `zforge status` and `zforge init`
tell you when a project still needs migrating.

## Documentation

| Guide | Description |
|-------|-------------|
| [docs/v1.5/README.md](docs/v1.5/README.md) | v1.5: intake, handover, runs (the current workflow; in Vietnamese) |
| [docs/v1.5/usage.md](docs/v1.5/usage.md) | v1.5 step by step, from a request to verified code (in Vietnamese) |
| [docs/v1.5/workflow.md](docs/v1.5/workflow.md) | v1.5 design: states, records, runs (in Vietnamese) |
| [docs/v1.5/decisions.md](docs/v1.5/decisions.md) | v1.5 decisions (in Vietnamese) |
| [docs/v1/README.md](docs/v1/README.md) | v1 task pipeline: removed, kept for history |
| [docs/v2/README.md](docs/v2/README.md) | v2: a design target, not implemented. Index and reading order |
