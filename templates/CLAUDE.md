# {{project_name}}

**Language:** {{language}}
**Test command:** `{{test_command}}`
**Workflow manager:** zforge

---

## Workflow

This project uses a gated TDD workflow managed by `zforge`. Every task moves through
these phases in order. Two phases require explicit human approval before proceeding.

```
task import → spec → testspec → [APPROVE testspec] → plan → [APPROVE plan] → code → verify → review
```

When asked to implement a task, always check the current state first:

```
zforge status <TASK-ID>
```

Never skip a phase. Never write code before testspec and plan are both approved.

---

## Before Implementing Any Task

Read these files in order:

```
/file .zforge/tasks/<TASK-ID>/task.md
/file .zforge/tasks/<TASK-ID>/spec.md
/file .zforge/tasks/<TASK-ID>/testspec.md
/file .zforge/tasks/<TASK-ID>/plan.md
/file .zforge/memory/patterns.md
/file .zforge/memory/anti-patterns.md
```

Then follow the approved plan exactly.

---

{{lang_skills_section}}
## Workflow Skills

Use these skill files as checklists during each phase:

| Phase | Skill file |
|-------|-----------|
| Writing spec | `.zforge/skills/clarify-spec.md` |
| Deriving tests | `.zforge/skills/derive-test-cases.md` |
| Planning | `.zforge/skills/implementation-planning.md` |
| Writing tests | `.zforge/skills/write-tests-first.md` |
| Implementing | `.zforge/skills/implement-minimal-patch.md` |
| Reviewing | `.zforge/skills/review-patch.md` |

### Supplementary Skills

| When | Skill file |
|------|-----------|
| Diagnosing a bug | `.zforge/skills/debug.md` |
| Security check before merge | `.zforge/skills/security-review.md` |
| Fixing a performance problem | `.zforge/skills/performance-optimize.md` |
| Backend API contract changes | `.zforge/skills/backend/api-contracts.md` |
| Database schema or migration changes | `.zforge/skills/backend/database-migrations.md` |
| Logs, metrics, traces, jobs | `.zforge/skills/backend/observability.md` |
| Queue, cron, worker changes | `.zforge/skills/backend/background-jobs.md` |
| React or route UI changes | `.zforge/skills/frontend/react-patterns.md` |
| Frontend tests | `.zforge/skills/frontend/frontend-testing.md` |
| Accessibility-sensitive UI | `.zforge/skills/frontend/accessibility.md` |
| Figma-to-code UI work | `.zforge/skills/frontend/figma-to-ui.md` |
| Frontend state or data fetching | `.zforge/skills/frontend/state-data-fetching.md` |

Load a skill with `/file .zforge/skills/<name>.md` before starting that phase.

---

## MCP Tools

zforge registers an MCP server (`zforge mcp`) that exposes the pipeline as tools.
Claude Code discovers it from the project-local `.mcp.json`.

Available tools: `task_import`, `get_prompt`, `approve`, `status`, `verify`, `ship`

`ship` combines the `Coded` state advance + `verify` into a single tool call — call it after you've finished writing code from `get_prompt(phase="code")` instead of advancing state and calling `verify` separately.

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
