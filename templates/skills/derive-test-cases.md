# Skill: derive-test-cases

## Purpose

Systematically derive a complete set of test cases from an approved spec before
any implementation starts. Every case must be traceable back to a spec requirement.

## When to Use

- After spec.md is approved and before writing any code
- When generating testspec.md for a task
- When reviewing whether test coverage is complete

## Required Inputs

- `spec.md` — approved specification with acceptance criteria
- Task description for additional context

## Expected Outputs

A `testspec.md` with:

- A primary `Test Cases` table using stable IDs such as `TC-01`
- Concrete `Given / When / Then` fields for each case
- An `Acceptance Criteria Coverage` table linking each criterion to one or more case IDs
- An `Ambiguities / Gaps` section for anything the spec does not define clearly

The test cases should collectively cover all of the following categories:

- **Happy path** — the primary success scenario for each acceptance criterion
- **Edge cases** — boundary values, empty inputs, maximum sizes, zero counts
- **Invalid input** — malformed data, missing required fields, wrong types
- **Error propagation** — correct error type returned, correct message included
- **Regression cases** — behaviors that must not break (from prior bugs if known)
- **Auth/permission cases** — unauthorized access rejected, correct role required (if applicable)

## Checklist

- [ ] Every acceptance criterion in spec.md has at least one test case
- [ ] Each test case has a concrete input and a concrete expected output
- [ ] At least one negative test exists for every operation that can fail
- [ ] Edge cases cover: empty, single item, maximum, and boundary values where relevant
- [ ] Each test case is independent — no test depends on another test's side effects
- [ ] Auth cases present if the spec mentions roles, tokens, or permissions
- [ ] Traceability table links each criterion to at least one case ID

## Constraints

- Derive cases only from spec.md — do not add tests for features not in the spec
- If a spec requirement is ambiguous, note it with `[AMBIGUOUS: ...]` and write the most conservative test
- Test names must be descriptive: `rejects_order_when_stock_is_zero`, not `test3`
- Do not organize the testspec by implementation taxonomy such as unit vs integration unless the spec explicitly requires that distinction

## Do Not Do

- Do not let implementation details appear in test descriptions (test behavior, not code)
- Do not write duplicate test cases — each case must cover something distinct
- Do not skip negative tests — happy-path-only specs miss the most common real failures
- Do not start implementation before testspec.md is reviewed and approved
