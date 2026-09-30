# {{project_name}}

**Language:** {{language}}
**Test command:** `{{test_command}}`
**Workflow manager:** zforge

---

## Workflow

Work is agreed before it is built. zforge keeps the agreement in an **intake**
(`.zforge/intakes/<ID>/`) and builds it in **runs**, each in its own git
worktree, so the user's checkout is never touched.

```
intake new → write 01-outcome … 04-breakdown + tasks/ → review → [USER accepts]
          → readiness → [USER hands over] → run (code → verify, per task)
          → integration check → merge → knowledge index
```

| Stage | Command | Who |
|-------|---------|-----|
| Start an intake | `zforge intake new <ID>`, `zforge intake task <ID> <TASK>` | agent or user |
| Send a file for review | `zforge intake review <ID> <file>` | agent or user |
| Accept or ask for changes | `zforge intake accept\|revise <ID> <file>` | **user, in a terminal** |
| Check it can be handed over | `zforge readiness <ID>` | agent or user |
| Hand over | `zforge handover <ID>` | **user, in a terminal** |
| Build every task, then check integration | `zforge run <HANDOVER> [--async]` | agent or user |
| Follow a run or a handover | `zforge run status\|log\|cancel <ID>` | agent or user |
| Record what is built | `zforge knowledge index` | agent or user |

Accepting, asking for changes and handing over are the user's decisions. They
need an interactive terminal and a typed confirmation; there is no flag, env
var or MCP tool that makes them for you. Prepare the files, send them for
review, and tell the user what to decide.

Runs execute with Claude Code (`claude`), whichever client prepared the
intake. Where things stand, and the next command, for every intake:

```
zforge status
```

---

## Project Knowledge

`zforge onboard` drafts the project's own knowledge from the code —
`domain.md`, `conventions.md`, `rules.md` under `docs/knowledge/`, each
statement citing `path:line` — and probes the test suite's baseline. It is
separate from `init`: a project can skip it and intakes and runs still
work, with a warning at readiness. Once a file is accepted (same review as
an intake file: `zforge onboard review\|accept\|revise`, the user's
decision at a terminal), intakes read and cite it and runs get the parts
that apply to their task. `zforge onboard status` shows each file's state
and any citation gone stale since it was accepted; `zforge onboard refresh`
redrafts only what is stale.

---

## When You Are Asked To Build Something

- If there is no intake for it yet, help the user write one (the intake
  skill below). Ask about anything the files leave open; list it as an open
  question (`- [ ]`) rather than guessing.
- If a handover exists, run it (`zforge run <HANDOVER>`) instead of editing
  the code yourself.
- If a run stopped, read `zforge run status <RUN>` first. A task that cannot
  be done as agreed needs an amendment to the intake, handed over again.

---

{{lang_skills_section}}
## Workflow Skills

Load the file before starting that work:

| When | Skill file |
|------|-----------|
| Preparing an intake | `{{skills_dir}}/intake.md` |
| Writing tests | `{{skills_dir}}/write-tests-first.md` |
| Implementing a task | `{{skills_dir}}/implement-minimal-patch.md` |
| Reviewing a change against its contract | `{{skills_dir}}/review-patch.md` |

---

## Supplementary Skills

Load these only when the task touches that area:

| When | Skill file |
|------|-----------|
| Diagnosing a bug | `{{skills_dir}}/debug.md` |
| Security check before merge | `{{skills_dir}}/security-review.md` |
| Fixing a performance problem | `{{skills_dir}}/performance-optimize.md` |
| Backend API contract changes | `{{skills_dir}}/backend/api-contracts.md` |
| Database schema or migration changes | `{{skills_dir}}/backend/database-migrations.md` |
| Logs, metrics, traces, jobs | `{{skills_dir}}/backend/observability.md` |
| Queue, cron, worker changes | `{{skills_dir}}/backend/background-jobs.md` |
| React or route UI changes | `{{skills_dir}}/frontend/react-patterns.md` |
| Frontend tests | `{{skills_dir}}/frontend/frontend-testing.md` |
| Accessibility-sensitive UI | `{{skills_dir}}/frontend/accessibility.md` |
| Figma-to-code UI work | `{{skills_dir}}/frontend/figma-to-ui.md` |
| Frontend state or data fetching | `{{skills_dir}}/frontend/state-data-fetching.md` |

---

## MCP Tools

`zforge mcp register --agent codex` (or `--agent opencode`) registers zforge.
The tools prepare and observe; none of them accepts, revises or hands over.

| Tool | Use |
|------|-----|
| `status` | Every intake, its handovers and runs, and the next step (`global` for all projects) |
| `intake_new`, `intake_task` | Start an intake or a task contract |
| `intake_status`, `intake_diff` | Files, their review state, open questions, lint issues; what changed |
| `intake_review` | Send a file for the user's review |
| `change_new` | Start a change request for an intake |
| `readiness` | Can the accepted files be handed over, and why not |
| `run_start`, `run_status`, `run_log`, `run_list`, `run_cancel` | Build a handover or one task, and follow it |
| `knowledge_index` | Rebuild `.zforge/knowledge/` from accepted intakes and runs |
| `onboard_probe` | Probe the project (language, docs, baseline test run) — no model call |
| `onboard_status` | Each knowledge file's review state, open questions and stale citations |
| `onboard_review` | Send a knowledge file for the user's review |
| `project_list`, `project_add`, `project_remove`, `switch_project` | The registry of zforge projects |

Accepting a knowledge file, asking for changes, and recording known baseline
failures are the user's decisions too — no `onboard_accept`, `onboard_revise`
or `onboard_baseline` tool exists; they are terminal-only, like
`intake_accept` and `handover`.

---

## Permissions

Codex CLI handles command approval through global sandbox and approval modes
configured in `~/.codex/config.toml`. When running zforge through Codex, allow
at minimum `zforge status`, `zforge intake …`, `zforge readiness`,
`zforge run …` and the configured test command (`{{test_command}}`).

---

## Code Constraints

- **Tests first** — write the failing test before any production code
- **Minimal patch** — only change what the task contract requires
- **No scope creep** — work outside the contract's scope needs an amendment first
- **Never weaken a test** — protected tests stay as they were; add new ones instead
- **No silent errors** — propagate errors explicitly; never swallow them
- **Preserve public interfaces** — do not change signatures or error types unless the contract says so
