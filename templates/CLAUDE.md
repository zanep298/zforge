# {{project_name}}

**Language:** {{language}}
**Test command:** `{{test_command}}`
**Workflow manager:** zforge

---

## Workflow

This project uses a gated TDD workflow managed by `zforge`. Every task moves through
the phases defined by its **flow** (chosen at import time, recorded in `.state.yaml`).

```
task import → spec → testspec → [APPROVE testspec] → plan → [APPROVE plan] → code → verify → review
```

The pipeline above is the default `full` flow. Three shorter presets are available for
smaller work:

| Flow | Phases | Pick when |
|------|--------|-----------|
| `full` (default) | full pipeline above | Features, risky changes, anything cross-cutting |
| `fixbug` | spec → testspec → code → verify | Bug with a clear reproducer |
| `spike` | spec → code | Research, throwaway prototypes |
| `docs` | code | Docs / README / comments only |

```bash
zforge task import BUG-42 --flow fixbug --title "Login crash"
```

Always check the active flow + state before starting work:

```
zforge status <TASK-ID>
```

The `Next:` line in `zforge status` (and the output of every phase command) already
follows the active flow — trust it. Phase commands that are not part of the flow
(e.g. `zforge plan` on a `fixbug` task) fail with a clear error.

Never skip a phase that *is* in the flow. For the `full` flow, never write code before
testspec and plan are both approved.

---

## Before Implementing Any Task

Read whichever of these files exist for the task (short flows skip some):

```
/file .zforge/tasks/<TASK-ID>/task.md
/file .zforge/tasks/<TASK-ID>/spec.md          # missing on docs flow
/file .zforge/tasks/<TASK-ID>/testspec.md      # missing on spike/docs flows
/file .zforge/tasks/<TASK-ID>/plan.md          # missing on fixbug/spike/docs flows
/file .zforge/memory/patterns.md
/file .zforge/memory/anti-patterns.md
```

Follow the approved plan exactly when one exists. On shorter flows without a plan,
follow `task.md` + `spec.md` directly.

---

{{lang_skills_section}}
## Workflow Skills

Use these skill files as checklists during each phase:

| Phase | Skill file |
|-------|-----------|
| Writing spec | `{{skills_dir}}/clarify-spec.md` |
| Deriving tests | `{{skills_dir}}/derive-test-cases.md` |
| Planning | `{{skills_dir}}/implementation-planning.md` |
| Writing tests | `{{skills_dir}}/write-tests-first.md` |
| Implementing | `{{skills_dir}}/implement-minimal-patch.md` |
| Reviewing | `{{skills_dir}}/review-patch.md` |

### Supplementary Skills

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

Load a skill with `/file {{skills_dir}}/<name>.md` before starting that phase.

---

## MCP Tools

### zforge

zforge exposes the pipeline as tools. Register it with Claude Code by running
`zforge mcp register --agent claude`, which uses `claude mcp add` under the
hood. Available tools: `task_import`, `get_prompt`, `approve`, `status`,
`verify`, `ship`

`ship` combines the `Coded` state advance + `verify` into a single tool call — call it after you've finished writing code from `get_prompt(phase="code")` instead of advancing state and calling `verify` separately.

### codegraph
Semantic code search over the pre-built codebase index. `zforge init` registers
codegraph for this project (`claude mcp add --scope local`) when codegraph is
installed. Use it instead of
grep/find when exploring the codebase.

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

Workflow agents live in `.claude/agents/`. Claude Code can invoke them as subagents
during each pipeline phase:

| Phase | Agent |
|-------|-------|
| `zforge spec <ID>` | `spec-agent` |
| `zforge testspec <ID>` | `testspec-agent` |
| `zforge plan <ID>` | `plan-agent` |
| `zforge code <ID>` | `code-agent` |
| `zforge review <ID>` | `review-agent` |

---

## Code Constraints

- **Tests first** — write the failing test before any production code
- **Minimal patch** — only change what is required by the approved spec
- **No scope creep** — if you need to change something outside the plan, flag it first
- **No silent errors** — propagate errors explicitly; never swallow them
- **Preserve public interfaces** — do not change signatures or error types unless the spec says so

---

## Memory

After each completed review, patterns and anti-patterns are extracted into:

- `.zforge/memory/patterns.md` — approved patterns to follow
- `.zforge/memory/anti-patterns.md` — known failure patterns to avoid

Read both files before implementing any task.
