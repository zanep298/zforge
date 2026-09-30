# Skill: onboard

## Purpose

Draft the project's own knowledge from the code — `domain.md`
(entities, business rules, invariants, workflows, states), `conventions.md`
(structure, naming, error handling, logging, patterns, how tests are
written) and `rules.md` (what must or must not be done: security,
compatibility, migrations, data) — so intakes and runs work from what the
project actually does, not from guesses. You draft and send files for
review. **You never accept a revision** — accepting, revising and recording
known baseline failures need a human at a terminal (D1); this skill has no
path around that.

## When to Use

- The user asks to onboard the project, draft or refresh its knowledge, or
  answers "how is this project structured" / "what are the rules here"
  questions that belong in `docs/knowledge/`
- `zforge onboard status` shows files in draft or with stale items

## Required Inputs

- `zforge onboard` (the probe) — commit, language, modules, docs found,
  baseline test result. Refuses on an uncommitted tree: knowledge is pinned
  to a commit, so commit or stash first.
- The code itself — read it, and use CodeGraph when available; do not
  guess from the file names alone.
- Any existing knowledge files (`domain.md`, `conventions.md`, `rules.md`)
  and what `zforge onboard status` reports stale.

## The evidence rule

Every statement is an item with a stable ID (`DOM-`, `CONV-` or `RULE-`
plus a number, never reused once dropped) and ends with its evidence as
`path:line` or `path:start-end`, comma-separated when there is more than
one:

```
- DOM-004: A refresh token is single-use; using it twice revokes the whole
  session family. (internal/token/refresh.go:88, internal/token/refresh_test.go:141)
```

The evidence must exist at the file's pinned commit — `zforge onboard
review` checks this and refuses a statement without it, or one whose
citation is past the file's end or names a file that does not exist there.
Never write a statement you cannot point at in the code.

## The questions rule

What the code cannot show — intent, a rule that is only implied, a
docs-vs-code disagreement — is never asserted as a statement. It goes under
"## Open questions" instead, as a question the user can answer:

```
## Open questions
- [ ] Is the 15-minute access token lifetime a product rule or a default? (config/defaults.go:21)
```

When an existing doc (README, an ADR) disagrees with the code, state what
the code does, cite both the code and the doc, and ask under Open questions
which one is intended — never guess.

## Module by module

Work from the probe's module list (`baseline.md`'s "## Modules"), one at a
time: read the module's code (CodeGraph first, then the files themselves),
write its items with evidence, note what you could not confirm as open
questions, and record which modules you covered in `domain.md`'s `covers:`
frontmatter. A module you did not reach is named, not silently skipped.

Only what is specific to *this* project goes in — a naming convention this
codebase actually follows, a rule this domain actually enforces. General
advice the skills already give (testing discipline, security checklists,
API design) does not belong here.

## Headless drafting

`zforge onboard draft [--module M] [--budget USD]` and `zforge onboard
refresh [--budget USD]` do the same drafting non-interactively: one Claude
call per module (draft) or per file with stale items (refresh), started
with no editing tools, its answer written into the file and sent for
review. Use them for a module you already understand well enough to draft
without asking the user anything, or to refresh what `zforge onboard
status` reports stale. A failed, interrupted or over-budget call changes no
file.

## Workflow

1. Run `zforge onboard` (or check `zforge onboard status`) first — the
   probe's modules and baseline are what you draft from and against.
2. For each module: read the code, write `domain.md`'s items for it under
   `## <module>`, and any `conventions.md` / `rules.md` items its code
   shows that are still missing.
3. Run `zforge onboard review <file>` for each file you changed. Fix what
   it refuses (usually missing evidence) and review again.
4. Tell the user what is ready: "domain.md, conventions.md and rules.md are
   up for review — `zforge onboard accept <file>` or `zforge onboard
   revise <file> --note …` at your terminal." Do not run `accept`,
   `revise` or `baseline` yourself; you cannot — they refuse outside an
   interactive terminal.
5. When `zforge onboard status` reports stale items, prefer `zforge onboard
   refresh` (or redraft the affected module by hand) over leaving them —
   they keep showing up until refreshed and reviewed.
