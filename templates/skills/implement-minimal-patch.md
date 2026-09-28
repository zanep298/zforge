# Skill: implement-minimal-patch

## Purpose

Make the smallest working change that satisfies what was agreed and passes the
tests. Resist the urge to clean up, refactor, or improve things that are not
directly required by the current task.

## When to Use

- During every coding step
- When reviewing whether a changeset is appropriately scoped
- When deciding whether to include a "while I'm here" fix

## Required Inputs

The boundary of acceptable change: the task contract — Output, Ràng buộc
(constraints), Tự chủ (what you may decide) and the acceptance criteria — plus
the binding decisions ("Quyết định bắt buộc") in the accepted solution.

## Expected Outputs

A changeset that:

- Passes the whole test suite, including the new tests
- Stays inside the contract's output and constraints, or says why it had to
  leave it (a change request, not a silent detour)
- Preserves all existing public interfaces unless what was agreed changes them

## Checklist

- [ ] Every changed file is needed for the agreed output
- [ ] No public function signatures changed without that being agreed
- [ ] No error types renamed or removed without that being agreed
- [ ] No unrelated files modified
- [ ] No speculative abstractions added ("we might need this later")
- [ ] No formatting-only changes mixed with behavior changes
- [ ] The final message says what changed, why, and any assumption made

## Constraints

- If a necessary change is outside what was agreed, say so — in a contract run,
  when it changes an output, interface, binding decision or criterion, write a
  change request instead of making it
- "It's just a cleanup" is not a reason to change code outside the scope
- Keep changes reviewable: smaller diffs are reviewed more carefully

## Do Not Do

- Do not refactor code that is not blocking the current task
- Do not add abstractions for hypothetical future requirements
- Do not rename things for style unless that was agreed
- Do not introduce new dependencies without flagging it
- Do not hide breaking changes in the middle of a large diff
