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

## One pass, one decision

By default the user reviews the whole intake once:

1. Ask every question that would change the files **before** drafting.
2. Draft 01 → 04 and every task in one pass — each stage still builds on
   the one before it, as drafted.
3. `zforge intake review <ID> --all` (MCP `intake_review` with `all: true`)
   sends every file that needs the user, in dependency order. It sends
   nothing while any file has a structural error and lists them all: fix
   every one, then send again.
4. Show `zforge intake pack <ID>` (MCP `intake_pack`) — brief, each file's
   summary and changes, warnings, how to decide — not each file apart.
5. The user types `/accept all` once; zforge records it in dependency order,
   so nothing accepted in it goes stale. A `/revise` note: revise, send
   `--all` again, show the new pack.

**Staged** instead — stop after a stage and wait for its acceptance — when
the user asks for it, or when 01-outcome has open questions whose answers
would change everything below it: stop after 01 and ask.

**Small profile** (`zforge intake new <ID> --small`, MCP `small: true`) for
at most two tasks in one module with no new interface: 02-behavior is one
table and 03-solution only its binding decisions, with no summary. The
files all still exist and are accepted and pinned like any others.

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

## Writing for the person who decides

The user accepts what they read, so every word above `## Detail` costs
them. `zforge intake review` warns when a file goes past these:

- **Summary first.** Each stage opens with `## Summary`, at most 150 words:
  what this file decides, the main points, what the user must decide, and
  — from the second revision — what changed.
- **Budgets** for what is above `## Detail`: 01 — 500 words, 02 — 700,
  03 — 800, 04 — 400, a task — 300. Code blocks do not count.
- **`## Detail` is for the implementing agent**: paths, signatures,
  formats, full example outputs, evidence. The user may skip it; nothing
  they must decide goes there.
- **Tables and lists, not paragraphs.** One situation, decision or task
  per row; one line per output and per acceptance criterion.
- **Refer, do not retell.** A lower stage names `REQ-003`, `situation 4`,
  `D-2`; it does not explain them again.
- **Diagrams are `mermaid`**, never drawn in text. One question per
  diagram — who calls whom (`sequenceDiagram`), or which states exist
  (`stateDiagram-v2`), or what contains what (`graph`) — about eight
  nodes, a verb on every arrow. A second question gets a second diagram.

## The brief

`brief.md` is one page for the person who decides — the whole intake in
their language and in plain words: no IDs, no paths, no code. It is a
reading view, never the contract: zforge does not review, pin or lint it,
and accepting is still done on the files. Keep to this shape, about 400
words:

- **Goal** in one sentence, then **one concrete example** from the user's
  world.
- **Behavior** as a mermaid flowchart of what the user and the system do —
  plain-language nodes, at most eight.
- **Solution** in two or three sentences.
- **Choices to understand before accepting**: a table `Choice |
  Consequence`.
- **Work split**: a mermaid flowchart of the phases, not every task.
- **Done when**: what will have been proven.
- **Still unclear**: open points, and any place where the files disagree
  with each other — writing the brief is when you notice them.
- A last line: "Reading copy, not the contract. Describes: 01-outcome rev
  n, …".

Write it as soon as 01-outcome exists, and rewrite it every time a stage or
a task is written, revised or accepted, so it always describes the files as
they are. The review pack opens with it.

## Workflow

1. Read `INDEX.md`, the items it points you to, and the code before asking anything you
   could find out yourself. Every stage you draft cites the knowledge item
   IDs it relies on (`Keeps RULE-001: …`); where the stage departs from an
   accepted item, say so in the stage, with the item's ID and why, for the
   user to accept or not — never depart silently.
2. Write or revise the file; keep conclusions in the file, not only in chat.
3. Put every question that needs the user under **Open questions** as
   `- [ ] question — stage where it will be settled`. Tick it `[x]` only
   when the answer is written into the file.
4. `zforge intake review <ID> --all` — fix every structural error and
   readability warning it reports, then present the pack
   (`zforge intake pack <ID>`) in chat, in the user's language: the
   summaries, the diagrams rendered (for tasks, the graph from
   `zforge intake graph <ID>`), the changes, and the points to decide. Do
   not paste whole files. Then stop and wait.
5. The user decides — in Claude Code, by typing `/accept all`,
   `/accept <file>…` or `/revise <file>: <what to change>` (zforge's prompt
   hook records it and tells you what it recorded); at a terminal, with
   `zforge intake accept <ID> --all` or `zforge intake accept|revise <ID>
   <file>`. Read a revision note and revise.
6. When all files are accepted, `zforge readiness <ID>`; fix what it lists
   by revising files (each goes through review again).
7. Show the user what the handover would pin (readiness lists it) and ask
   them to type `/handover` (or run `zforge handover <ID>`). Once the hook
   reports the handover recorded, start it with `run_start` and follow it.

Revising a file after the ones below it were accepted leaves them resting
on the old version, and readiness refuses them. `zforge intake review <ID>
--all` sends them again with it, unchanged, so the user confirms the lot
with one `/accept all`.

## Change requests

During a run, an agent that finds the contract must change writes
`changes/CHANGE-RUN-nnn.md` and the run stops `blocked`. Read it, explain it
to the user, and — if they agree — revise the contract files it concerns and
send them for review. Once accepted, the user hands over again; tasks whose
contract did not change are reused, not run again. To propose a change
yourself, `change_new` (MCP) creates the file with the required sections.

## Changing an accepted intake

Only what rests on a changed item is confirmed again and run again: a
task's requirements, their behavior rows, the decisions (`D-n`) it cites,
its breakdown row and the tasks it depends on; "Must not change" / "Out of
scope" bind everything; prose binds nothing. Keep stages in the template's
shape (REQ lines, the situations table, the decisions table, the tasks
table) — a stage that is not is compared whole.

- **Not merged yet** (`zforge status` does not say the handover is
  merged): edit the intake in place, send the changed files for review,
  confirm what readiness lists as resting on them, hand over again. Tasks
  whose items did not change are reused; a changed task continues from its
  earlier output, told what changed.
- **Already integrated or merged into the baseline**: `zforge intake amend
  <NEW> --from <ID>` (MCP `intake_amend`). Every file starts as a draft
  copied from the accepted revisions; change only what changes, rewrite
  `brief.md` around the change, `zforge intake review <NEW> --all`, show
  the pack, and ask the user to type `/accept <NEW> all`. Never accept for them.

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
