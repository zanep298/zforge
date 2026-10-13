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
- The project's accepted knowledge, read through its index — never whole
  files (`domain.md` alone can run to hundreds of KB). Read
  `docs/knowledge/INDEX.md` (one line per accepted item, by file and
  module) before drafting any stage, then fetch only the items for the
  modules or IDs the change touches: `zforge knowledge get DOM-004
  RULE-001`, `--module <section|path>`, `--grep <text>` (MCP
  `knowledge_get`). Rules are few and apply everywhere — fetch those that
  apply. A project that has not onboarded has no index; proceed without it.
- Reference docs (`knowledge.extra`, listed under "Reference docs" in
  `INDEX.md`), when the change crosses services: start from their README,
  search with `zforge knowledge get --grep <text> --extra`, read only the
  sections that apply. They are background, not accepted knowledge — don't
  cite them as `DOM-`/`RULE-` items.
- `docs/knowledge/commitments.md` — accepted requirements and decisions still in force
- The code and tests the change touches (read them; CodeGraph when available)

## Stages and files

Each stage builds on the one before it. Section titles are English; intakes written with the earlier Vietnamese titles
(Câu hỏi còn mở, Kiểm chứng tích hợp, …) are still read — keep one language
per intake.

1. `01-outcome.md` — problem, users, desired result, scope, what must not
   change, out of scope, signs of success. Each mandatory requirement is a
   list item starting with a stable ID, one sentence of at most 25 words;
   its conditions go in sub-items: `- REQ-001: …`.
2. `02-behavior.md` — normal, edge, error and recovery situations as a
   table, one row each: `# | Kind | When | Then | REQ`. An example below
   the table only where a row alone would be ambiguous.
3. `03-solution.md` — the flow as a diagram, then **Binding decisions** as
   a table (`D-1 | Decision | Why | Rejected alternative`), kept apart from
   **Implementation suggestions**. Components, interfaces, assumptions and
   evidence go under `## Detail`.
4. `04-breakdown.md` — a table of tasks (`Task | Serves | Depends on |
   Output`, one line of output each) and **Integration verification**: how
   the whole is verified. Put the exact commands in the section's first
   fenced code block, one per line (`#` lines are comments) — zforge runs
   them, in order, on a tree holding every task's output. Without a block
   it runs the project's test command, which may not check what the tasks
   do together. Do not draw the dependency graph: `zforge intake graph
   <ID>` generates it from the tasks.
5. `tasks/TASK-xxx.md` (`zforge intake task <ID> TASK-xxx`) — the contract:
   frontmatter `requirements` / `depends_on`, and the sections Goal, Input,
   Output, Constraints, Autonomy, Acceptance and verification (`- AC-01: …`),
   Delivery, Amend the contract when.
   - `depends_on`: a task starts from its dependencies' verified code, so
     list what it builds on, not only what must come first.
   - `tests_may_change: [path, …]` (optional frontmatter): existing test
     files the task may modify. A run that passes only after changing any
     other existing test is not counted. Add it only when the contract
     means it, with exact paths, and say why under Constraints.
   - When the task builds on or departs from the accepted knowledge, name
     the items under Constraints: `Knowledge: DOM-004, RULE-001`. `zforge
     intake review` warns, naming the ID, on a citation that is not in the
     accepted knowledge — fix the ID or note the departure, don't ignore it.

## Workflow

By default the user reviews the whole intake once.

1. Read `INDEX.md`, the items it points you to, and the code before asking
   anything you could find out yourself. Every stage you draft cites the
   knowledge item IDs it relies on (`Keeps RULE-001: …`); where it departs
   from an accepted item, say so with the ID and why — never silently.
2. Ask every question that would change the files **before** drafting. What
   the user must still settle goes under **Open questions** as
   `- [ ] question — stage where it will be settled`; tick it `[x]` only
   when the answer is written into the file.
3. Draft 01 → 04 and every task in one pass, each stage building on the one
   before it, following `zforge-intake-writing` (`intake-writing.md`
   beside this file in the skills store). Keep conclusions in the files,
   not only in chat.
4. `zforge intake review <ID> --all` (MCP `intake_review`, `all: true`)
   sends every file that needs the user, in dependency order — nothing
   while any file has a structural error: fix them all and send again.
   Fix every readability warning too.
5. Present `zforge intake pack <ID>` (MCP `intake_pack`) in chat, in the
   user's language: the summaries, the diagrams rendered (for tasks, the
   graph from `zforge intake graph <ID>`), the changes, the points to
   decide. Do not paste whole files. Then stop and wait.
6. The user decides — `/accept all` once (recorded in dependency order, so
   nothing in it goes stale), `/accept <file>…` or `/revise <file>: <what
   to change>`; zforge's prompt hook records it and tells you. At a
   terminal: `zforge intake accept <ID> --all` or `zforge intake
   accept|revise <ID> <file>`. On a revision note: revise, `review --all`
   again, show the new pack.
7. When all files are accepted, `zforge readiness <ID>`; fix what it lists
   by revising files (each goes through review again).
8. Show what the handover would pin (readiness lists it) and ask the user
   to type `/handover` (or run `zforge handover <ID>`). Once the hook
   reports it recorded, start it with `run_start` and follow it.

**Staged** instead — stop after a stage and wait for its acceptance — when
the user asks, or when 01-outcome has open questions whose answers would
change everything below it: stop after 01 and ask.

**Small profile** (`zforge intake new <ID> --small`, MCP `small: true`) for
at most two tasks in one module with no new interface: 02-behavior is one
table and 03-solution only its binding decisions, with no summary. The
files all still exist and are accepted and pinned like any others.

Revising a file after the ones below it were accepted leaves those that
rest on what changed out of date, and readiness refuses them; `review
--all` sends them again with it, so one `/accept all` settles the lot.
Changing an intake after a handover, or answering a change request: follow
`zforge-intake-change` (`intake-change.md` beside this file).

## Checklist

- [ ] Every mandatory requirement has an ID and at least one task
- [ ] Every task links to the requirements it serves and to nothing it invents
- [ ] Binding decisions and suggestions are marked apart in 03-solution
- [ ] Every acceptance criterion says how it is verified
- [ ] Integration verification is a section of its own, with its commands in a
      fenced code block
- [ ] `tests_may_change` appears only where the contract means it, with exact paths
- [ ] Each stage opens with a Summary; no readability warning left unexplained
- [ ] Diagrams are mermaid, one question each; no hand-drawn dependency graph
- [ ] `brief.md` describes the files as they are now, in the user's language
- [ ] No open question left unchecked in a file offered for acceptance
- [ ] Sent with `review --all` and shown as one pack, unless the user asked for stages
- [ ] Conflicts with accepted knowledge are listed for the user, not resolved silently

## Never

- Run `zforge intake accept`, `zforge intake revise`, `zforge handover` or
  `zforge hook` — they are the user's decisions, recorded only from a
  terminal or from the user's own `/accept`, `/revise`, `/handover` message
- Take "ok", "looks good" or any other reply as a decision: only the
  hook's report that it recorded one counts
- Let the brief say something the files do not: fix the file, then the brief
- Write "approved", "accepted" or a status into a file: status is derived
  from the runtime's records
- Edit a file while the user is reviewing it — it can then no longer be accepted
- Treat silence, or the user opening a file, as agreement
- Change a contract to make a run pass, or mark a change request accepted
