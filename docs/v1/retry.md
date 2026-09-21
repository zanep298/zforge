# Retry

`zforge retry` rewinds a task to a phase and lets you redo from there. Use it when
an AI-generated artifact is wrong and you want to regenerate it without starting the
task from scratch.

## When to use retry

- Spec is too vague or missed key requirements
- Testspec coverage is incomplete after approval
- Plan prescribed wrong files or incorrect execution order
- Code generated doesn't match the spec
- Verify command was misconfigured
- Review summary missed important patterns

## Syntax

```bash
zforge retry <TASK-ID> --from <phase>
zforge retry <TASK-ID> --from <phase> --yes   # skip confirmation prompt
```

Valid phases: `spec`, `testspec`, `plan`, `code`, `verify`, `review`

## What gets reset

Each phase clears its own artifact and everything that depends on it. Earlier
artifacts are preserved.

| `--from` | State reset to | Artifacts cleared |
|----------|---------------|-------------------|
| `spec` | `Imported` | spec.md, testspec.md, plan.md, verify.md, review-summary.md |
| `testspec` | `SpecDone` | testspec.md, plan.md, verify.md, review-summary.md |
| `plan` | `TestspecReviewed` | plan.md, verify.md, review-summary.md |
| `code` | `PlanReviewed` | verify.md, review-summary.md |
| `verify` | `Coded` | verify.md, review-summary.md |
| `review` | `Verified` | review-summary.md |

**Spec and task.md are never automatically deleted.** `task.md` is the source of
truth you filled in — edit it directly if the task description was wrong.

## Backup

Before resetting, zforge backs up all affected artifacts to:

```
.zforge/tasks/<ID>/.history/<timestamp>/
```

The backup is automatic — nothing is permanently lost. You can copy a backed-up file
back to the task directory if you want to restore it.

## Example: bad spec

```bash
# Spec was generated but the problem statement is wrong
$ zforge retry TASK-001 --from spec

? Retry TASK-001 from [spec] phase?
  Will backup and reset:
  • spec.md
  • testspec.md
  • plan.md
  • verify.md
  • review-summary.md
  Backup to: tasks/TASK-001/.history/2026-05-19T10-30-00/
  State reset to: imported

  Continue? [y/N] y

✓ Backed up 3 artifacts
✓ Reset state: SpecDone → imported
✓ Cleared: spec.md, testspec.md, plan.md, verify.md, review-summary.md

Next: zf spec TASK-001
```

Then edit `task.md` to fix the problem description, re-run `zforge spec TASK-001`,
and continue from there.

## Example: failing verify, bad code

```bash
# Tests are failing and the fix requires redoing code
$ zforge retry TASK-001 --from code --yes

✓ Backed up 2 artifacts
✓ Reset state: Verified → PlanReviewed
✓ Cleared: verify.md, review-summary.md

Next: zf code TASK-001
```

## Flows and retry

Retry respects the task's flow. Resetting to `TestspecReviewed` (the `--from plan`
target state) on a `fixbug` task will fail because `TestspecReviewed` is not part of
the `fixbug` flow. Use `--from testspec` instead (resets to `SpecDone`).

```bash
# fixbug task — plan phase doesn't exist in this flow
$ zforge retry BUG-42 --from plan
Error: state TestspecReviewed is not part of the fixbug flow

# correct: go back to testspec
$ zforge retry BUG-42 --from testspec
```

## Viewing backup history

```bash
ls .zforge/tasks/TASK-001/.history/
# 2026-05-19T10-30-00/
# 2026-05-19T14-15-22/

ls .zforge/tasks/TASK-001/.history/2026-05-19T10-30-00/
# spec.md  testspec.md
```
