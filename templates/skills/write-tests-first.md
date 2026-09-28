# Skill: write-tests-first

## Purpose

Enforce the TDD discipline of writing a failing test before writing the production
code that makes it pass. Tests are the specification in executable form — they exist
before implementation, not after.

## When to Use

- At the start of every implementation step
- When adding any new behavior to existing code
- When fixing a bug (write a failing test that reproduces it first)

## Required Inputs

What the tests must prove, from whichever you were given:

- **Pipeline task:** `testspec.md` (approved test cases) and `plan.md` (step order)
- **Contract run (v1.5):** the task contract's acceptance criteria (`AC-01` …) and
  the behavior in the accepted stages
- The relevant source file(s) being modified

## Expected Outputs

For each case or acceptance criterion being implemented:

1. A new test written **before** the production code it exercises
2. Confirmation that the test **fails** against the current code, for the right reason
3. The production code change that makes it pass
4. Confirmation that the whole suite passes after the change

## Checklist

- [ ] Test written and seen failing before the production code changes
- [ ] Test name is descriptive: `fn rejects_expired_token()` not `fn test1()`
- [ ] Test fails with the expected reason (wrong behavior, not a compile error)
- [ ] Production code change is minimal — only what is needed to pass this test
- [ ] All existing tests still pass after the change
- [ ] Each test traces to a testspec case or an acceptance criterion
- [ ] No `#[ignore]` (or equivalent) added without a documented reason

## Constraints

- One failing test at a time — do not write a batch of tests before implementing any
- Run the tests with the project's test command (`project.test_command`; the prompt
  names it) — that is what verification runs, not a command of your own
- Existing tests are part of what was agreed: add tests, do not rewrite old ones to fit

## Do Not Do

- Do not write production code before the failing test exists
- Do not edit unrelated tests while implementing a feature
- Do not delete or weaken a test that is hard to pass — flag it instead
- Do not write tests after the fact that are trivially constructed to pass
- Do not skip this discipline because the change "seems obvious"
