# Skill: write-tests-first

## Purpose

Enforce the TDD discipline of writing a failing test before writing the production
code that makes it pass. Tests are the specification in executable form — they exist
before implementation, not after.

## When to Use

- At the start of every implementation step in plan.md
- When adding any new behavior to existing code
- When fixing a bug (write a failing test that reproduces it first)

## Required Inputs

- `testspec.md` — approved test cases to implement
- `plan.md` — execution order for current step
- The relevant source file(s) being modified

## Expected Outputs

For each test case being implemented:

1. A new test function written and committed **before** any production code changes
2. Confirmation that the test **fails** when run against the current code
3. The production code change that makes the test pass
4. Confirmation that all tests pass after the change

## Checklist

- [ ] Test function written before touching production code
- [ ] Test name is descriptive: `fn rejects_expired_token()` not `fn test1()`
- [ ] Test runs and fails with the expected reason (not a compile error)
- [ ] Production code change is minimal — only what is needed to pass this test
- [ ] All existing tests still pass after the change
- [ ] Test aligns with the corresponding case in testspec.md
- [ ] No `#[ignore]` added without a documented reason

## Constraints

- One failing test at a time — do not write multiple tests before implementing any of them
- The failing test must fail for the right reason (wrong behavior, not missing code)
- Test must use the same test command configured for this project: `{{test_command}}`

## Do Not Do

- Do not write production code before the failing test exists
- Do not edit unrelated tests while implementing a feature
- Do not silently delete a test that is hard to pass — flag it instead
- Do not write tests after the fact that are trivially constructed to pass
- Do not skip this discipline because the change "seems obvious"
