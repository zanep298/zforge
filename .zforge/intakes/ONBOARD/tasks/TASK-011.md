---
id: TASK-011
parent: ONBOARD
requirements: [REQ-010]
depends_on: [TASK-007]
---

# TASK-011 — Known failures in run verification

## Goal

A run on a known-red baseline passes when only known tests fail, records what it tolerated, and zforge tells the user when a known failure no longer fails.

## Input

- The accepted 01-outcome, 02-behavior, 03-solution and 04-breakdown (as pinned by the handover).
- Output of TASK-007, which carries TASK-002 to TASK-006 in one chain (the known-failure list is pinned by TASK-006).
- `src/run/execute.rs` (verification and feedback), `src/run/record.rs` (`RunEvent::Verified`), `src/runner/mod.rs`, `src/status.rs`, `src/cli/status.rs`.

## Output

- Verification: a non-zero exit passes only when the runner read at least one failing test name and every failing test is on the pinned known list; `Verified` gains `tolerated` and `known_passing` (known tests not among the failures), both omitted when empty; the agent's feedback names only failures that are not known.
- `zforge status` suggests removing a known test that a run recorded in `known_passing`.

## Constraints

- The whole existing suite passes, and no existing test file changes.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- No new crate dependency without saying why in the report.
- Intakes behave exactly as before: CLI output, record format, MCP tools.
- Deciding on knowledge — accept, revise, recording known failures — needs
  stdin and stdout terminals and a typed confirmation; never add a flag, env
  var or MCP tool that bypasses it (D1).
- Knowledge informs agents; it never changes a run's verdict (except known
  failures, this task), the protected-test guard or a task's reuse key.
- A handover without pinned knowledge produces the same prompts as today.

- `events.jsonl` written before stays readable: the new `Verified` fields are optional.

## Autonomy

- Module layout, names, types and how the tests are written, within the
  shared interfaces of 04-breakdown.
- Diagnosing and fixing test and clippy failures within the task.

## Acceptance and verification

- AC-01: a run failing only known tests is `verified` with `tolerated`.
- AC-02: a run also failing another test fails, naming it; the feedback does not name the known ones.
- AC-03: output with no readable test names fails as today, known list or not.
- AC-04: a known test absent from the failures is recorded in `known_passing`, and `zforge status` suggests removing it.
- AC-05: a handover with no known failures verifies exactly as before (exit code 0 passes, anything else fails).

## Delivery

Local changes on the task's branch and a verification report; no push or merge.

## Amend the contract when

- Telling known failures apart needs test names the runner cannot read for a supported language.

## Open questions

None.
