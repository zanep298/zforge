# Skill: implementation-planning

## Purpose

Produce a concrete, ordered implementation plan that a developer can execute
step-by-step without ambiguity. The plan is the smallest path to passing the
approved test cases — nothing more.

## When to Use

- After testspec.md is approved and before writing any code
- When generating plan.md for a task
- When reviewing whether a plan is safe to execute

## Required Inputs

- `spec.md` — approved specification
- `testspec.md` — approved test cases
- Familiarity with the existing codebase structure

## Expected Outputs

A `plan.md` covering:

- **Impacted files** — every file that will be created, modified, or deleted
- **Implementation sequence** — numbered steps in execution order
- **Test-first execution order** — for each step: write test → watch it fail → implement → pass
- **Dependencies and risks** — external services, migrations, breaking changes
- **Rollback notes** — how to safely undo the change

## Checklist

- [ ] Every file in "impacted files" has a stated reason for the change
- [ ] Sequence is ordered so each step builds on a stable previous state
- [ ] Test-first order is explicit: failing test written before the implementation step
- [ ] No step says "implement X" without specifying which file and function
- [ ] Risks are identified — not just listed but with a mitigation
- [ ] Rollback path exists and is practical (not "revert the whole PR")
- [ ] Plan covers only what the spec and testspec require — no extras

## Constraints

- Favor the minimal patch: change the fewest files needed to pass the tests
- If a refactor is needed to make the change safe, list it as a separate step
- Execution order must be deterministic — another engineer follows it without asking questions

## Do Not Do

- Do not propose a broad rewrite to "clean things up" alongside the feature
- Do not leave steps vague ("update the service layer" — specify which file and what change)
- Do not skip the test-first execution order — it enforces TDD discipline during implementation
- Do not omit rollback notes — every production change must be reversible
- Do not add speculative steps for "possible future needs"
