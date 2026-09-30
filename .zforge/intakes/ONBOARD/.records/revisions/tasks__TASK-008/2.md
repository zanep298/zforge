---
id: TASK-008
parent: ONBOARD
requirements: [REQ-003, REQ-009]
depends_on: [TASK-002, TASK-003, TASK-004]
---

# TASK-008 — Drafting

## Goal

The knowledge is drafted from the code — interactively for the first onboarding, headless for a module or a refresh — and every draft goes to review, never straight to accepted.

## Input

- The accepted 01-outcome, 02-behavior, 03-solution and 04-breakdown (as pinned by the handover).
- Outputs of TASK-002 (items, lint), TASK-003 (probe, modules, templates) and TASK-004 (stale items).
- `src/cli/init/claude_skills.rs` (catalog), `src/embedded.rs`, `src/run/execute.rs` (`call_agent`), `src/run/worktree.rs`, `src/orchestrator/`.

## Output

- A `zforge-onboard` skill in the catalog (preloaded by nothing; triggered by onboarding requests): module by module from `baseline.md`, CodeGraph to read, items with evidence at `pinned`, what the code cannot show as open questions, docs that contradict the code cited both ways, `onboard review` at the end; it never accepts.
- `zforge onboard draft [--module M] [--budget USD]`: one Claude call per module in a worktree at HEAD, the agent unable to edit files, answering with the module's items; zforge writes them as the module's section of the file, sets `pinned` to HEAD and sends the file for review.
- `zforge onboard refresh [--budget USD]`: one call per file with stale items, given those items and the code they cite at HEAD, answering with replacements for those IDs only; zforge replaces them and sends the file for review.
- `knowledge.draft_budget_usd` (config, default 2.0) per call; each call traced and costed like a run's.

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
- A failed, interrupted or over-budget call leaves every knowledge file as it was.

## Autonomy

- Module layout, names, types and how the tests are written, within the
  shared interfaces of 04-breakdown.
- Diagnosing and fixing test and clippy failures within the task.
- The draft and refresh prompt wording and how the agent's answer is delimited.

## Acceptance and verification

- AC-01: the skill is in the catalog, installed as `.claude/skills/zforge-onboard/SKILL.md`, and states the evidence rule, the questions rule and that it never accepts.
- AC-02: with a stub agent, `onboard draft --module internal/token` writes that section, sets `pinned`, and records a review revision.
- AC-03: an answer with an uncited item leaves the file unreviewed and reports the lint error.
- AC-04: `onboard refresh` replaces only the stale IDs; every other line is byte-identical.
- AC-05: a stub agent that exits non-zero, or exceeds the budget, changes no file.
- AC-06: the agent is started without editing tools.

## Delivery

Local changes on the task's branch and a verification report; no push or merge.

## Amend the contract when

- The agent must write files itself, or drafting needs more than one call per module.

## Open questions

None.
