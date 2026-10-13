# Skill: review-patch

## Purpose

Decide whether a change meets its task contract — the one the user accepted —
and say exactly what must change when it does not. Passing tests show the
suite is green, not that the contract is met; check both.

## When to Use

- Reviewing a run's work after its tests pass (`execution.review: true`
  starts `review-agent` with this checklist preloaded)
- Reviewing any change made for a zforge task before it is merged

## Required Inputs

- The task contract: Output, Constraints, Autonomy (what the
  implementer could decide), the acceptance criteria `AC-nn`, and
  `tests_may_change` if the user allowed a test to change
- The Binding decisions of the accepted solution
- The diff from the commit the task started from (`git diff <start>`) and
  `git status`; for a run that continues an earlier output, the whole
  task's diff from the base the prompt names
- The project's conventions and rules (a run's prompt shows the ones that
  apply) and the code around the change
- The test command and its result

## Expected Outputs

Findings, one per line starting with `- `, each naming the file and what must
change, then exactly one verdict line:

```
VERDICT: APPROVE
```

or

```
VERDICT: CHANGES
```

## Checklist

- [ ] Every `AC-nn` is met by the code — trace each one to where it happens
- [ ] Every `AC-nn` is proven by a test that would fail without the change
      and that the test command actually runs (not skipped by an env var,
      build tag or skip marker)
- [ ] Nothing changed outside the contract's Output and constraints
- [ ] Every binding decision is kept; choices under Autonomy are the implementer's
- [ ] No test was weakened, skipped or deleted (outside `tests_may_change`)
- [ ] No placeholder, dead code, debug output or silenced error in production paths
- [ ] Structure, file layout, naming and error/response handling follow the
      project's conventions and the nearest existing code in the module
      changed — a departure is a `VERDICT: CHANGES` finding
- [ ] Generated files regenerated with the project's command, not hand-edited
- [ ] Regression risk named with the specific behavior or file that could break

## Constraints

- Base every finding on evidence — cite the file, function, test or `AC-nn`
- Read, and run only commands that change no tracked file (the test
  command included); do not change any file
- Ask for changes only for what breaks the contract or a checklist item — not
  for taste where the conventions and surrounding code are silent
- When the contract itself looks wrong, say so as a finding; the fix is an
  amendment to the intake, not a change the implementer can make alone

## Do Not Do

- Do not implement fixes — describe them
- Do not approve with "looks good": each checklist item is checked or named
- Do not treat a green test run as proof that every criterion is covered
- Do not inflate minor issues into blockers
