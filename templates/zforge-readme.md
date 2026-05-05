# .zforge — {{project_name}}

This directory contains the agents, skills, and memory for your zforge workflow.
OpenCode reads agent files as instructions. Skills are reference checklists.

## Workflow

```
zf task import TASK-123     →  create task stub
zf spec TASK-123            →  generate spec prompt  →  fill spec.md in OpenCode
zf spec TASK-123 --done     →  mark spec complete
zf testspec TASK-123        →  generate testspec prompt  →  fill testspec.md
zf testspec TASK-123 --done →  mark testspec complete
zf approve TASK-123 testspec   ← human gate
zf plan TASK-123            →  generate plan prompt  →  fill plan.md
zf plan TASK-123 --done     →  mark plan complete
zf approve TASK-123 plan       ← human gate
zf code TASK-123            →  generate code prompt  →  implement in OpenCode
zf code TASK-123 --done     →  mark coding complete
zf verify TASK-123          →  run {{test_command}}
zf review TASK-123          →  generate review prompt  →  fill review-summary.md
zf review TASK-123 --done   →  extract patterns, mark complete
```

Human gates are mandatory. `zf plan` is blocked until testspec is approved.
`zf code` is blocked until plan is approved.

## Agents

| File | Purpose |
|------|---------|
| `agents/spec-agent.md` | Clarifies requirements, produces scoped spec |
| `agents/testspec-agent.md` | Derives test cases from approved spec |
| `agents/plan-agent.md` | Creates concrete implementation plan |
| `agents/code-agent.md` | Implements approved plan, tests first |
| `agents/review-agent.md` | Reviews completed implementation |

Load an agent in OpenCode with `/agent .zforge/agents/spec-agent.md`.

## Skills

| File | Used by |
|------|---------|
| `skills/clarify-spec.md` | spec-agent |
| `skills/derive-test-cases.md` | testspec-agent |
| `skills/implementation-planning.md` | plan-agent |
| `skills/write-tests-first.md` | code-agent |
| `skills/implement-minimal-patch.md` | code-agent |
| `skills/review-patch.md` | review-agent |

Skills are checklists and constraints. Reference them in your agent prompts with
`/file .zforge/skills/<name>.md`.

## Memory

| File | Purpose |
|------|---------|
| `memory/patterns.md` | Approved patterns extracted from reviews |
| `memory/anti-patterns.md` | Lessons learned from failed verifications |
| `memory/domain-glossary.md` | Project terminology and ubiquitous language |

Memory files grow automatically when you run `zf review --done`.
