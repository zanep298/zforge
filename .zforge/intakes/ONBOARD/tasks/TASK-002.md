---
id: TASK-002
parent: ONBOARD
requirements: [REQ-004]
depends_on: [TASK-001]
---

# TASK-002 — Items, evidence and lint

## Goal

Every knowledge statement is an item with a stable ID and evidence in the code, checked against the commit the file was drafted from.

## Input

- The accepted 01-outcome, 02-behavior, 03-solution and 04-breakdown (as pinned by the handover).
- Output of TASK-001 (`DocSet`, `onboard review`).
- `src/intake/lint.rs` (`strip_comments`, `sections`, `section`, `Heading`), `src/run/git.rs`.

## Output

- A parser: frontmatter `pinned` (commit) and `covers` (modules, `domain.md`); items `- <ID>: <text> (<cite>, <cite>)` with IDs `DOM-`, `CONV-`, `RULE-` and a number for `domain.md`, `conventions.md`, `rules.md`; a cite is `path:line` or `path:start-end`, the item's last parenthesised list.
- Knowledge lint, run by `onboard review` (errors refuse the review): missing or unknown `pinned` commit; a list item outside *Open questions* that is not an item or has no evidence; an ID with the wrong prefix or used twice; a cited file absent at `pinned`, a line past its end, or start after end; an ID present in an earlier accepted revision, dropped from a later accepted one, and back again.
- A warning when `covers` misses a module `baseline.md` lists, if `baseline.md` exists.

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
- Evidence is read at `pinned` with `git show <pinned>:<path>`; the working tree is never read for it.

## Autonomy

- Module layout, names, types and how the tests are written, within the
  shared interfaces of 04-breakdown.
- Diagnosing and fixing test and clippy failures within the task.

## Acceptance and verification

- AC-01: a file whose items all cite existing lines at `pinned` passes review.
- AC-02: a statement without evidence is an error naming its line.
- AC-03: `DOM-007: internal/token/rotate.go:200` against a 143-line file is refused with `file has 143 lines`; a path absent at `pinned` is refused.
- AC-04: a duplicate ID, or `RULE-` in `domain.md`, is refused.
- AC-05: an ID dropped in accepted revision 2 and written again in revision 3 is refused.
- AC-06: a cited file changed after `pinned` in the working tree does not affect the check.

## Delivery

Local changes on the task's branch and a verification report; no push or merge.

## Amend the contract when

- Evidence needs a form other than `path:line` ranges (symbols, URLs).

## Open questions

None.
