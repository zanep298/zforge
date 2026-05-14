# Skill: implement-minimal-patch

## Purpose

Make the smallest working change that satisfies the approved spec and passes the
approved tests. Resist the urge to clean up, refactor, or improve things that are
not directly required by the current task.

## When to Use

- During every coding step in plan.md
- When reviewing whether a changeset is appropriately scoped
- When deciding whether to include a "while I'm here" fix

## Required Inputs

- `spec.md` — defines the boundary of acceptable change
- `testspec.md` — defines what must pass
- `plan.md` — defines which files to touch and in what order

## Expected Outputs

A changeset that:

- Passes all tests in testspec.md
- Touches only the files listed in plan.md (or documents why an unlisted file was needed)
- Preserves all existing public interfaces unless the spec explicitly changes them

## Checklist

- [ ] Changed files match the impacted files list in plan.md
- [ ] No public function signatures changed without spec approval
- [ ] No error types renamed or removed without spec approval
- [ ] No unrelated files modified
- [ ] No speculative abstractions added ("we might need this later")
- [ ] No formatting-only changes mixed with behavior changes
- [ ] Implementation log updated with: files changed, behavior changed, assumptions used

## Constraints

- If a necessary change is not in the plan, document it and flag it — do not silently expand scope
- "It's just a cleanup" is not a reason to change code outside the plan
- Keep changes reviewable: smaller diffs are reviewed more carefully

## Do Not Do

- Do not refactor code that is not blocking the current task
- Do not add abstractions for hypothetical future requirements
- Do not rename things for style unless the plan explicitly calls for it
- Do not introduce new dependencies without flagging it
- Do not hide breaking changes in the middle of a large diff
