# {{project_name}}

**Language:** {{language}}
**Test command:** `{{test_command}}`
**Workflow manager:** zforge

---

## Workflow

This project uses a gated TDD workflow managed by `zforge`. Every task moves through
these phases in order. Two phases require explicit human approval before proceeding.

```
task import → spec → testspec → [APPROVE] → plan → [APPROVE] → code → verify → review
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
/file tasks/<TASK-ID>/task.md
/file tasks/<TASK-ID>/spec.md
/file tasks/<TASK-ID>/testspec.md
/file tasks/<TASK-ID>/plan.md
/file .zforge/memory/patterns.md
/file .zforge/memory/anti-patterns.md
```

Then follow the approved plan exactly. Document every decision in
`tasks/<TASK-ID>/implementation-log.md`.

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

Load a skill with `/file .zforge/skills/<name>.md` before starting that phase.

---

## MCP Tools

zforge registers an MCP server (`zforge mcp`) that exposes the pipeline as tools.
Claude Code discovers it from the project-local `.mcp.json`.

Available tools: `task_import`, `get_prompt`, `approve`, `status`, `verify`

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
