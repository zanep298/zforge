---
id: TASK-001
parent: ONBOARD
requirements: [REQ-005, REQ-006]
depends_on: []
---

# TASK-001 — Document sets and knowledge review

## Goal

Knowledge files are reviewed, accepted and revised with the same machinery as intake files — one implementation — and live with their records in the repository.

## Input

- The accepted 01-outcome, 02-behavior, 03-solution and 04-breakdown (as pinned by the handover).
- `src/intake/mod.rs` (`Intake`, `files`, `file`, `records_dir`), `src/intake/review.rs`, `src/intake/record.rs`, `src/intake/status.rs`.
- `src/cli/intake.rs` (`decide`: terminal check and typed confirmation), `src/config/mod.rs`, `src/main.rs`.

## Output

- A `DocSet` (kind intake or knowledge, id, dir, records dir) that `review`, `accept`, `revise`, `statuses`, `file_status`, `pending_review` and snapshot reads accept; an intake is a `DocSet` at `.zforge/intakes/<ID>/` with records in `.records/`, as today.
- The knowledge set: directory `knowledge.dir` (config, default `docs/knowledge`), records in `<dir>/.records/`, reviewable files `domain.md`, `conventions.md`, `rules.md`.
- `zforge onboard status [--json]`, `onboard review <file>`, `onboard accept <file>`, `onboard revise <file> --note <text>`; accept and revise share the intake's terminal check and typed confirmation.
- `onboard accept` refuses a revision whose *Open questions* still has an unchecked `- [ ]` item, naming it.
- On the first knowledge review, `.gitattributes` at the repository root gains `<dir>/.records/decisions.jsonl merge=union` unless an equal line exists; nothing else in it changes.

## Constraints

- The whole existing suite passes, and no existing test file changes.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- No new crate dependency without saying why in the report.
- Intakes behave exactly as before: CLI output, record format, MCP tools.
- Deciding on knowledge — accept, revise, recording known failures — needs
  stdin and stdout terminals and a typed confirmation; never add a flag, env
  var or MCP tool that bypasses it (D1).
- Knowledge informs agents; it never changes a run's verdict (except known
  failures, TASK-007), the protected-test guard or a task's reuse key.
- Intake records keep their paths and format; a `decisions.jsonl` written before reads the same.

## Autonomy

- Module layout, names, types and how the tests are written, within the
  shared interfaces of 04-breakdown.
- Diagnosing and fixing test and clippy failures within the task.

## Acceptance and verification

- AC-01: `onboard review domain.md` writes revision 1 to `docs/knowledge/.records/revisions/domain.md/1.md`, appends a `review` decision, and `onboard status` shows `in review`.
- AC-02: `onboard accept domain.md` with a typed `accept` in a terminal records `accepted`; without a terminal it is refused and records nothing.
- AC-03: `onboard accept` is refused while the revision under review has an unchecked open question.
- AC-04: with `knowledge.dir: kb` in the config, files and records live under `kb/`.
- AC-05: `.gitattributes` gains the union line once and keeps its other lines.
- AC-06: a file other than the three is refused by `onboard review`.
- AC-07: every intake test passes unchanged.

## Delivery

Local changes on the task's branch and a verification report; no push or merge.

## Amend the contract when

- The review machinery cannot be shared without changing the intake record format.
- Knowledge needs files beyond the three.

## Open questions

None.
