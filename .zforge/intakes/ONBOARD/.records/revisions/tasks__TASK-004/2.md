---
id: TASK-004
parent: ONBOARD
requirements: [REQ-009]
depends_on: [TASK-002]
---

# TASK-004 — Stale items

## Goal

zforge tells which accepted knowledge items no longer match the code, and which citations only moved.

## Input

- The accepted 01-outcome, 02-behavior, 03-solution and 04-breakdown (as pinned by the handover).
- Output of TASK-002 (items, cites, `pinned`).

## Output

- `stale::check(root)` over the accepted revisions of the three files: for each cite, the text of its lines at `pinned` compared with the same lines at HEAD — equal → fresh; the same block elsewhere in the HEAD file → moved (its new range reported); otherwise, or the file absent at HEAD → stale (`changed` or `gone`).
- `onboard status` lists stale items and moved citations; `zforge status` (text and JSON) shows `knowledge: N items stale — IDs (file)`.

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
- A plain text comparison through git at the two commits; no model, no network.

## Autonomy

- Module layout, names, types and how the tests are written, within the
  shared interfaces of 04-breakdown.
- Diagnosing and fixing test and clippy failures within the task.

## Acceptance and verification

- AC-01: a commit changing the lines DOM-004 cites makes DOM-004 stale (`changed`), named in `zforge status`.
- AC-02: a commit inserting lines above them leaves DOM-004 fresh and reports its new range.
- AC-03: deleting a cited file makes the item stale (`gone`).
- AC-04: uncommitted edits do not change the result; only HEAD counts.

## Delivery

Local changes on the task's branch and a verification report; no push or merge.

## Amend the contract when

- Staleness needs more than text equality (renames followed across files, semantic changes).

## Open questions

None.
