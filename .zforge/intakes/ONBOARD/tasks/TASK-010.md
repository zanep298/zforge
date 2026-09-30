---
id: TASK-010
parent: ONBOARD
requirements: [REQ-005, REQ-006]
depends_on: [TASK-001]
---

# TASK-010 — Fix TASK-001 review findings

## Goal

TASK-001's output (HANDOVER-003 RUN-003, commit 8b3473f) passes its tests
but a review against its contract found it short: an acceptance criterion
broken, one unproven, side effects on refusal, a race, a copied
confirmation and a `DocSet` that departs from the shared interface. Bring
it to what TASK-001's contract and 04-breakdown fixed, before other tasks
build on it.

## Input

- The accepted 01-outcome, 02-behavior, 03-solution and 04-breakdown (as pinned by the handover).
- TASK-001's contract and its output: this task starts from it.
- The review findings summarised under Output; the code they point to:
  `src/knowledge/mod.rs`, `src/knowledge/docs.rs`, `src/cli/onboard.rs`,
  `src/cli/intake.rs` (`decide`), `src/intake/review.rs` (`lock`),
  `src/intake/mod.rs`.

## Output

- One confirmation helper — terminal check, the prompt, the typed word —
  used by both `zforge intake accept|revise` and `zforge onboard
  accept|revise`; the intake's messages stay word for word.
- The knowledge set's lock lives under `<knowledge.dir>/.records/`; the
  intake lock stays where it is. `onboard review` writes nothing outside
  `knowledge.dir` except the `.gitattributes` line.
- `.gitattributes` is changed only after a review succeeds.
- The `.gitattributes` line is the knowledge records path relative to the
  repository root, computed from canonical paths on both sides; when it
  cannot be made relative, the review fails with an error instead of
  writing an absolute path.
- The open-question check runs inside the decision's lock, on the exact
  revision being accepted.
- `DocSet::open(root, kind, id)` with kind `intake` or `knowledge`, as the
  shared interface in 04-breakdown states; intake and knowledge are opened
  through it.
- A binary-level test file for onboarding decisions; the unit test whose
  name claims a terminal check it does not make is renamed.

## Constraints

- The whole existing suite passes, and no existing file under `tests/` changes.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- No new crate dependency without saying why in the report.
- Intakes behave exactly as before: CLI output, record format, lock path, MCP tools.
- Deciding on knowledge — accept, revise — needs stdin and stdout terminals
  and a typed confirmation; never add a flag, env var or MCP tool that
  bypasses it (D1).
- Every acceptance criterion of TASK-001 stays met.
- Only the findings above change; no other refactor.

## Autonomy

- Module layout, names, types and how the tests are written, within the
  shared interfaces of 04-breakdown.
- How the pre-accept check reaches the lock (a hook on the document set, a
  parameter, …), as long as intakes are unaffected.
- Diagnosing and fixing test and clippy failures within the task.

## Acceptance and verification

- AC-01: `zforge onboard accept domain.md` and `onboard revise domain.md --note x` with stdin not a terminal are refused with "needs an interactive terminal", and `decisions.jsonl` keeps only the `review` line — proven by a test that runs the binary.
- AC-02: intake and onboard decisions call the same confirmation code; the intake's refusal and prompt texts are unchanged (the existing intake tests pass).
- AC-03: with `knowledge.dir: kb`, `onboard review domain.md` creates nothing outside `kb/` and `.gitattributes`; with the default dir, no file other than under `docs/knowledge/.records/` and the reviewed file appears in `docs/knowledge/`.
- AC-04: `onboard review glossary.md` (refused) and a review refused by lint leave `.gitattributes` byte-identical.
- AC-05: when the temp directory is reached through a symlink (`/var` → `/private/var`), the line written is exactly `docs/knowledge/.records/decisions.jsonl merge=union`; the test asserts the whole line.
- AC-06: a revision under review is accepted only if that same revision has no unchecked open question, decided under the lock.
- AC-07: `DocSet::open(root, Kind::Intake, "F")` and `DocSet::open(root, Kind::Knowledge, …)` open the two kinds; a test opens each.

## Delivery

Local changes on the task's branch and a verification report; no push or merge.

## Amend the contract when

- A finding cannot be fixed without changing the intake record format or lock path.
- The shared `DocSet` interface in 04-breakdown cannot be met as written.

## Open questions

None.
