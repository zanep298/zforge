# Skill: intake

## Purpose

Prepare a v1.5 intake with the user, top-down, so that every leaf task is a
contract an agent can implement without further product or design
decisions — and so the user understands and owns every important decision.
You prepare, explain and send files for review. **The user decides.**

## When to Use

- The user asks for a feature, change or fix that should go through intake
  (`.zforge/intakes/<ID>/`), or asks you to "clarify", "break down" or
  "prepare a handover"
- `zforge intake status <ID>` shows files in draft or needing revision

## Required Inputs

- The user's request, in their words
- `.zforge/knowledge/index.md` — accepted requirements and decisions still in force
- The code and tests the change touches (read them; CodeGraph when available)

## Stages and files

Work in order; each stage builds on the accepted one before it.

1. `01-outcome.md` — problem, users, desired result, scope, what must not
   change, out of scope, signs of success. Each mandatory requirement is a
   list item starting with a stable ID: `- REQ-001: …`.
2. `02-behavior.md` — normal, edge, error and recovery situations with
   observable input and output, citing the REQ they serve.
3. `03-solution.md` — flow, components, data, interfaces. Separate
   **Quyết định bắt buộc** (binding) from **Gợi ý triển khai** (suggestions).
   Record alternatives considered, assumptions and the evidence they hold.
4. `04-breakdown.md` — phases and tasks, order, dependencies, shared
   interfaces, and **Kiểm chứng tích hợp**: how the whole is verified. Put
   the exact commands in the section's first fenced code block, one per
   line (`#` lines are comments) — zforge runs them, in order, on a tree
   holding every task's output. Without a block it runs the project's test
   command, which may not check what the tasks do together.
5. `tasks/TASK-xxx.md` (`zforge intake task <ID> TASK-xxx`) — the contract:
   frontmatter `requirements` / `depends_on`, and the sections Mục tiêu,
   Input, Output, Ràng buộc, Tự chủ, Acceptance và kiểm chứng (`- AC-01: …`),
   Bàn giao, Cần amendment khi.
   - `depends_on`: a task starts from its dependencies' verified code, so
     list what it builds on, not only what must come first.
   - `tests_may_change: [path, …]` (optional frontmatter): existing test
     files the task may modify. A run that passes only after changing any
     other existing test is not counted. Add it only when the contract
     means it, with exact paths, and say why in Ràng buộc.

Every file answers, in plain language: what is being decided and how it
serves the level above; the proposal with a concrete example; why, with
trade-offs and uncertainty; what the user must decide; what changed since
the version they saw.

## Workflow

1. Read the knowledge index and the code before asking anything you could
   find out yourself.
2. Write or revise the file; keep conclusions in the file, not only in chat.
3. Put every question that needs the user under **Câu hỏi còn mở** as
   `- [ ] question — stage where it will be settled`. Tick it `[x]` only
   when the answer is written into the file.
4. `zforge intake review <ID> <file>` — fix every structural error it
   reports, then tell the user what to look at and why.
5. The user runs `zforge intake accept` or `zforge intake revise --note …`
   in their terminal. Read the note and revise.
6. When all files are accepted, `zforge readiness <ID>`; fix what it lists
   by revising files (each goes through review again).
7. Tell the user the intake is ready; they run `zforge handover <ID>`.

Revising a file after the ones below it were accepted leaves them resting
on the old version, and readiness refuses them. Send each of them for
review again — `zforge intake review` accepts an unchanged file in that
case — and tell the user what to confirm. Work top-down: confirming the
breakdown again makes the tasks stale in turn.

## Change requests

During a run, an agent that finds the contract must change writes
`changes/CHANGE-RUN-nnn.md` and the run stops `blocked`. Read it, explain it
to the user, and — if they agree — revise the contract files it concerns and
send them for review. Once accepted, the user hands over again; tasks whose
contract did not change are reused, not run again. To propose a change
yourself, `change_new` (MCP) creates the file with the required sections.

## Checklist

- [ ] Every mandatory requirement has an ID and at least one task
- [ ] Every task links to the requirements it serves and to nothing it invents
- [ ] Binding decisions and suggestions are marked apart in 03-solution
- [ ] Every acceptance criterion says how it is verified
- [ ] Integration verification is a section of its own, with its commands in a
      fenced code block
- [ ] `tests_may_change` appears only where the contract means it, with exact paths
- [ ] No open question left unchecked in a file offered for acceptance
- [ ] Conflicts with accepted knowledge are listed for the user, not resolved silently

## Never

- Run `zforge intake accept`, `zforge intake revise` or `zforge handover` —
  they are the user's decisions (they refuse without an interactive terminal)
- Write "approved", "accepted" or a status into a file: status is derived
  from the runtime's records
- Edit a file while the user is reviewing it — it can then no longer be accepted
- Treat silence, or the user opening a file, as agreement
- Change a contract to make a run pass, or mark a change request accepted
