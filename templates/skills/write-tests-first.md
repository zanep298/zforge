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

- The task contract's acceptance criteria (`AC-01` …) — what the tests must
  prove — and the behavior described in the accepted stages
- The relevant source file(s) being modified, and the tests next to them —
  write new tests the way those are written (framework, helpers, mocks,
  file names)

## Expected Outputs

For each case or acceptance criterion being implemented:

1. A new test written **before** the production code it exercises
2. Confirmation that the test **fails** against the current code, for the right reason
3. The production code change that makes it pass
4. Confirmation that the whole suite passes after the change

## Checklist

- [ ] Test written and seen failing before the production code changes
- [ ] Test runs under the project's test command — not skipped by an env var,
      build tag or skip marker the command does not set
- [ ] Test name is descriptive: `fn rejects_expired_token()` not `fn test1()`
- [ ] Test fails for the expected reason: wrong behavior, or — in a compiled
      language — the missing function or type it calls (not a typo or an
      unrelated break)
- [ ] Production code change is minimal — only what is needed to pass this test
- [ ] All existing tests still pass after the change
- [ ] Each test traces to an acceptance criterion
- [ ] No `#[ignore]` (or equivalent) added without a documented reason

## Constraints

- Work one acceptance criterion at a time: its failing test(s), then the code
  — do not write the tests for every criterion before implementing any
- Run the tests with the project's test command (`project.test_command`; the prompt
  names it) — that is what verification runs, not a command of your own
- Existing tests are part of what was agreed: add tests, do not rewrite old ones to fit

## Do Not Do

- Do not write production code before the failing test exists
- Do not edit unrelated tests while implementing a feature
- Do not delete or weaken a test that is hard to pass — flag it instead
- Do not write tests after the fact that are trivially constructed to pass
- Do not skip this discipline because the change "seems obvious"
