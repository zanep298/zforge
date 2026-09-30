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

Where things stand, and the next command, for every intake:

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
decision at a terminal), intakes read and cite it (`zforge-intake` skill)
and runs get the parts that apply to their task. `zforge onboard status`
shows each file's state and any citation gone stale since it was accepted.

| Stage | Command | Who |
|-------|---------|-----|
| Probe and draft the knowledge | `zforge onboard`, `zforge onboard draft` | agent or user |
| Send a knowledge file for review | `zforge onboard review <file>` | agent or user |
| Accept or ask for changes | `zforge onboard accept\|revise <file>` | **user, in a terminal** |
| Redraft what changed since | `zforge onboard refresh` | agent or user |

## When You Are Asked To Build Something

- If there is no intake for it yet, help the user write one (the
  `zforge-intake` skill). Ask about anything the files leave open; list it as
  an open question (`- [ ]`) rather than guessing.
- If a handover exists, run it (`zforge run <HANDOVER>`) instead of editing the
  code yourself: the run works in a worktree, checks the protected tests, and
  records what passed against which code.
- If a run stopped, read `zforge run status <RUN>` before anything else. A
  task that cannot be done as agreed needs an amendment to the intake, handed
  over again — not a workaround in the code.

## Inside A Run

A run's agent gets its task contract as the prompt: goal, scope, acceptance
criteria, the test command. Follow the contract, not this file's workflow:
write the failing test first, make the smallest change that passes it, keep
to the scope, and never change a protected test to make it pass. If the
contract cannot be met, write the change request the prompt names and stop.

---

{{claude_skills_section}}
---

## MCP Tools

### zforge

`zforge mcp register --agent claude` registers zforge with Claude Code. The
tools prepare and observe; none of them accepts, revises or hands over.

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

### codegraph
Semantic code search over the pre-built codebase index. `zforge init` registers
codegraph for this project (`claude mcp add --scope local`) when codegraph is
installed. Use it instead of grep/find when exploring the codebase. It indexes
the main checkout: inside a run's worktree, read the files you change directly.

| Tool | Use when |
|------|----------|
| `mcp__codegraph__codegraph_context` | Get semantic context for a task area (start here) |
| `mcp__codegraph__codegraph_search` | Find functions, types, modules by name or pattern |
| `mcp__codegraph__codegraph_node` | Show one symbol's source, signature, docstring |
| `mcp__codegraph__codegraph_explore` | Survey several related symbols at once |
| `mcp__codegraph__codegraph_callers` | Find what calls a symbol |
| `mcp__codegraph__codegraph_callees` | Find what a symbol calls |
| `mcp__codegraph__codegraph_impact` | See what a change to a symbol would break |
| `mcp__codegraph__codegraph_trace` | Trace the call path from X to Y |
| `mcp__codegraph__codegraph_files` | Browse file structure |
| `mcp__codegraph__codegraph_status` | Check index freshness and stats |

Run `codegraph index` to refresh the index after large changes.

## Agents

Runs start Claude with a named agent from `.claude/agents/`:

| Agent | Runs when |
|-------|-----------|
| `code-agent` | Every attempt of a run: implement the task contract, tests first |
| `review-agent` | After the tests pass, when `execution.review: true`: check the work against the contract |

Each phase's model is a default tier (`sonnet` for code, `opus` for review);
change it with `zforge models set <phase> <model>`.

---

## Code Constraints

- **Tests first** — write the failing test before any production code
- **Minimal patch** — only change what the task contract requires
- **No scope creep** — work outside the contract's scope needs an amendment first
- **Never weaken a test** — protected tests stay as they were; add new ones instead
- **No silent errors** — propagate errors explicitly; never swallow them
- **Preserve public interfaces** — do not change signatures or error types unless the contract says so
