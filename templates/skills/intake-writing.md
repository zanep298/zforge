# Skill: intake-writing

## Purpose

How to write an intake's files and its brief for the person who decides. Read it with `zforge-intake` before drafting or revising any intake file.

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
