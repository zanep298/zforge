# {{project_name}}

**Language:** {{language}}
**Test command:** `{{test_command}}`
**Workflow manager:** zforge
**AI agent:** OpenAI Codex CLI

---

## Workflow

This project uses a gated TDD workflow managed by `zforge`. Every task moves through
the phases of its **flow** (chosen at import time, recorded in `.state.yaml`).

```
task import → spec → testspec → [APPROVE testspec] → plan → [APPROVE plan] → code → verify → review
```

The default `full` flow shown above runs every phase. Shorter presets skip phases
that don't make sense for smaller work:

| Flow | Phases | Pick when |
|------|--------|-----------|
| `full` (default) | full pipeline above | Features, risky changes |
| `fixbug` | spec → testspec → code → verify | Bug with a clear reproducer |
| `spike` | spec → code | Research, throwaway prototypes |
| `docs` | code | Docs / README / comments only |

```bash
zforge task import BUG-42 --flow fixbug --title "Login crash"
```

Always check the active flow + state first:

```
zforge status <TASK-ID>
```

The `Next:` line in `zforge status` (and every phase command's output) already
follows the active flow — trust it. Phases that are not part of the flow fail with
a clear error.

Never skip a phase that is in the flow. For the `full` flow, never write code before
testspec and plan are both approved.

---

## Before Implementing Any Task

Read whichever of these files exist (short flows skip some):

```
.zforge/tasks/<TASK-ID>/task.md
.zforge/tasks/<TASK-ID>/spec.md          # missing on docs flow
.zforge/tasks/<TASK-ID>/testspec.md      # missing on spike/docs flows
.zforge/tasks/<TASK-ID>/plan.md          # missing on fixbug/spike/docs flows
.zforge/memory/patterns.md
.zforge/memory/anti-patterns.md
```

Follow the approved plan exactly when one exists. On shorter flows without a plan,
follow `task.md` + `spec.md` directly.

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

## Supplementary Skills

Load these only when the task touches that area:

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

---

## MCP Tools

zforge registers an MCP server (`zforge mcp`) globally at `~/.codex/config.toml`
under the `[mcp_servers.zforge]` table. Run `zforge mcp register --agent codex` to
add or refresh the entry.

Available tools: `task_import`, `get_prompt`, `approve`, `status`, `verify`, `ship`

`ship` combines the `Coded` state advance + `verify` into a single tool call — call it after you've finished writing code from `get_prompt(phase="code")` instead of advancing state and calling `verify` separately.

## Agents

Workflow agent prompts live in `.codex/agents/`. They are materialized from the
zforge agent templates with a Codex-specific `model:` frontmatter line.
Reference them when running each pipeline phase:

| Phase | Agent prompt |
|-------|--------------|
| `zforge spec <ID>` | `.codex/agents/spec-agent.md` |
| `zforge testspec <ID>` | `.codex/agents/testspec-agent.md` |
| `zforge plan <ID>` | `.codex/agents/plan-agent.md` |
| `zforge code <ID>` | `.codex/agents/code-agent.md` |
| `zforge review <ID>` | `.codex/agents/review-agent.md` |

## Codex Profiles

`zforge init` writes per-phase model profiles to `~/.codex/config.toml` from the
same resolved models used in `.codex/agents/`. Edit `.zforge/models.yaml` and
re-run `zforge init --agent codex --force` to change them:

| Profile | Phase | Default model |
|---------|-------|---------------|
| `zforge_spec` | spec | `gpt-5.4-mini` |
| `zforge_testspec` | testspec | `gpt-5.4-mini` |
| `zforge_plan` | plan | `gpt-5.4` |
| `zforge_code` | code | `gpt-5.3-codex` |
| `zforge_review` | review | `gpt-5.4` |

```bash
codex --profile zforge_code "implement the approved plan for TASK-123"
```

Override a model by editing `.zforge/models.yaml`, or by editing the
`[profiles.zforge_<phase>]` block in `~/.codex/config.toml` for a one-off local
change.

---

## Permissions

Codex CLI handles command approval through global sandbox and approval modes
configured in `~/.codex/config.toml` (e.g. `approval_policy`, `[shell]` block).
There is no project-scoped permissions file at this time — see
`.codex/README.md` and current Codex CLI docs for the supported keys.

When running `zforge` commands through Codex, allow at minimum:

- `zforge status`, `zforge get_prompt`, `zforge approve`, `zforge verify`, `zforge ship`
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
