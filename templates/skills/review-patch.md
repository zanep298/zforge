# Skill: review-patch

## Purpose

Review a completed implementation against its approved spec, testspec, and plan.
Surface spec drift, missing test coverage, regression risk, and maintainability
concerns. Produce a written verdict the team can act on.

## When to Use

- After `zforge verify` passes and before marking a task Reviewed
- When generating review-summary.md
- When deciding whether to approve or request changes on a PR

## Required Inputs

- `spec.md` — what was approved
- `testspec.md` — what tests were required
- `plan.md` — what implementation was approved
- `verify.md` — actual test results
- The implementation diff or changed files

## Expected Outputs

A `review-summary.md` with a clear verdict on each of the following:

- **Spec coverage** — does the implementation satisfy every acceptance criterion?
- **Test coverage** — does the test suite cover every case in testspec.md?
- **Unplanned changes** — anything changed that was not in the plan
- **Regression risk** — what could break in adjacent code
- **Readability and maintainability** — is the code clear enough for the next engineer?

## Checklist

- [ ] Every acceptance criterion in spec.md is checked — passed or flagged
- [ ] Every test case in testspec.md is accounted for — present or missing
- [ ] Unplanned changes are documented — each one marked acceptable or concern
- [ ] Regression risk is assessed with specific files or behaviors called out
- [ ] Review summary includes a verdict: approved / approved-with-notes / changes-requested
- [ ] Patterns worth keeping are listed for memory/patterns.md
- [ ] Anti-patterns discovered are listed for memory/anti-patterns.md

## Constraints

- Base every finding on evidence — cite the file, function, or test case
- Calibrate severity honestly: minor style issues are not blockers
- A passing test run does not guarantee spec coverage — check both

## Do Not Do

- Do not implement fixes — only identify and describe them
- Do not approve with "looks good" — every section must have explicit content
- Do not ignore unplanned changes even if the result is acceptable
- Do not inflate minor issues into blockers
- Do not skip checking that testspec.md coverage is complete in the actual test code
