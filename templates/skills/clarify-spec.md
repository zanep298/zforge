# Skill: clarify-spec

## Purpose

Systematically turn a vague task description into an unambiguous specification
by surfacing scope, assumptions, and acceptance criteria before any design or
coding begins.

## When to Use

- Before writing spec.md for any task
- When a task description has missing fields, vague goals, or no acceptance criteria
- When you are unsure what "done" looks like

## Required Inputs

- Task description (task.md or raw text)
- Any related context: existing code, prior decisions, linked tickets

## Expected Outputs

A completed spec.md with all of the following filled in:

- **Problem** — what is broken or missing, in one paragraph
- **Goal** — concrete, measurable definition of success
- **In Scope** — explicit list of what this task covers
- **Out of Scope** — explicit list of related things this task does NOT cover
- **Assumptions** — everything taken as true without confirmation
- **Impacted Areas** — files, modules, services, or APIs that will change
- **Draft Acceptance Criteria** — testable conditions that define done

## Checklist

- [ ] Problem statement written without solution language
- [ ] Goal is specific and measurable (not "improve performance", but "p99 < 100ms")
- [ ] Every in-scope item is concrete (not "handle errors better")
- [ ] Out-of-scope list exists and is non-empty
- [ ] Every assumption is written down — nothing implicit
- [ ] Impacted areas include file paths where known
- [ ] Each acceptance criterion starts with an observable action ("returns", "rejects", "emits")
- [ ] No acceptance criterion is vague ("works correctly", "handles edge cases")

## Constraints

- Derive scope from the task description only — do not invent requirements
- If information is missing, mark it as a gap with `[UNCLEAR: ...]` rather than guessing
- Keep acceptance criteria independent of implementation approach

## Do Not Do

- Do not write acceptance criteria that reference internal function names or data structures
- Do not skip the out-of-scope section — ambiguous boundaries cause scope creep
- Do not assume what "fast" or "correct" means — make it numeric or observable
- Do not fold multiple distinct changes into one acceptance criterion
