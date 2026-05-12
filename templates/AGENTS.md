# {{project_name}}

**Language:** {{language}}
**Test command:** `{{test_command}}`
**Workflow manager:** zforge
**AI agent:** OpenAI Codex CLI

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
tasks/<TASK-ID>/task.md
tasks/<TASK-ID>/spec.md
tasks/<TASK-ID>/testspec.md
tasks/<TASK-ID>/plan.md
.zforge/memory/patterns.md
.zforge/memory/anti-patterns.md
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

Load the file directly before starting that phase.

---

## MCP Tools

zforge registers an MCP server (`zforge mcp`) globally at `~/.codex/config.toml`
under the `[mcp_servers.zforge]` table. Run `zforge mcp register --agent codex` to
add or refresh the entry.

Available tools: `task_import`, `get_prompt`, `approve`, `status`, `verify`

## Agents

Workflow agent prompts live in `.codex/agents/` (symlinks to `.zforge/agents/`).
Reference them when running each pipeline phase:

| Phase | Agent prompt |
|-------|--------------|
| `zforge spec <ID>` | `.codex/agents/spec-agent.md` |
| `zforge testspec <ID>` | `.codex/agents/testspec-agent.md` |
| `zforge plan <ID>` | `.codex/agents/plan-agent.md` |
| `zforge code <ID>` | `.codex/agents/code-agent.md` |
| `zforge review <ID>` | `.codex/agents/review-agent.md` |

## Codex Profiles

`zforge init` writes per-phase model profiles to `~/.codex/config.toml`. Use them
to automatically select the right model for each pipeline phase:

| Profile | Phase | Default model |
|---------|-------|---------------|
| `zforge_spec` | spec | `gpt-4o-mini` |
| `zforge_testspec` | testspec | `gpt-4o-mini` |
| `zforge_plan` | plan | `gpt-4o` |
| `zforge_code` | code | `codex-mini-latest` |
| `zforge_review` | review | `gpt-4o` |

```bash
codex --profile zforge_code "implement the approved plan for TASK-123"
```

Override a model by editing the `[profiles.zforge_<phase>]` block in
`~/.codex/config.toml`, or re-run `zforge init --force` to regenerate from
updated agent frontmatter.

---

## Permissions

Codex CLI handles command approval through global sandbox and approval modes
configured in `~/.codex/config.toml` (e.g. `approval_policy`, `[shell]` block).
There is no project-scoped permissions file at this time — see
`.codex/README.md` and current Codex CLI docs for the supported keys.

When running `zforge` commands through Codex, allow at minimum:

- `zforge status`, `zforge get_prompt`, `zforge approve`, `zforge verify`
- The configured test command (`{{test_command}}`)

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
