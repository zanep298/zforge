# ONBOARD — Solution

## Flow

```
zforge onboard            probe (no model) → docs/knowledge/baseline.md
   │                      prints the next step
   ▼
draft                     interactive: the zforge-onboard skill in a Claude session
   │                      headless:    zforge onboard draft [--budget]
   ▼                      → domain.md, conventions.md, rules.md (items + evidence)
zforge onboard review <file>          lint: IDs, evidence exists at the pinned commit
   ▼
zforge onboard accept|revise <file>   the user, at a terminal
zforge onboard baseline --known …     the user, at a terminal (red baseline only)
   ▼
onboarded ── zforge handover pins the accepted knowledge and known failures
   │         → the run prompt gets the relevant items; verify tolerates known failures
   ▼
code changes → items whose cited text changed are stale (status, readiness)
             → zforge onboard refresh redrafts them → review → accept
```

## Components and interfaces

**Where things live.** `knowledge.dir` (default `docs/knowledge/`) holds, all
committed with the code:

| File | Written by | Reviewed |
|---|---|---|
| `domain.md`, `conventions.md`, `rules.md` | the drafting agent, then the user | yes |
| `baseline.md` | the probe | no — generated |
| `commitments.md` | `zforge knowledge index` | no — generated |
| `.records/revisions/<file>/<n>.md`, `.records/decisions.jsonl` | review, accept, revise, baseline | — |

`.records/decisions.jsonl` gets `merge=union` in `.gitattributes`, so two
branches that each record decisions merge line by line. `commitments.json`
stays in `.zforge/knowledge/` (it names local runs; tools read it there).

**Knowledge file format.**
```
---
pinned: <commit the file was drafted from>
covers: [internal/token, internal/audit, api]   # modules, domain.md only
---
## internal/token
- DOM-004: A refresh token is single-use; reuse revokes the session family. (internal/token/refresh.go:88-95, internal/token/refresh_test.go:141)
## Open questions
(as in an intake file)
```
An item is a list item starting with an ID (`DOM-`, `CONV-`, `RULE-` and a
number); its evidence is the last parenthesised list of `path:line` or
`path:start-end`, comma-separated.

**Modules** (`src/knowledge/`, new; the commitments code moves here from
`intake/knowledge.rs`):

| Module | Does |
|---|---|
| `docs.rs` | A reviewed document set: a directory plus its `.records/`. `Intake` becomes one kind of set and knowledge another, so review, accept, revise, snapshots and derived states are the same code (`intake/review.rs`, `record.rs`, `status.rs`) |
| `items.rs` | Parse items, IDs and evidence; IDs are never reused: an ID in an earlier accepted revision may not name a different item |
| `lint.rs` | Every statement has evidence; each `path:line` exists at `pinned` (`git show <pinned>:<path>`); IDs unique; `covers` lists every module the probe found, or the missing ones are named as a warning |
| `probe.rs` | Language, size, docs, CodeGraph, modules (top-level source directories with their line counts); one run of the test command through `runner::run_with_language` at a clean commit → `baseline.md` |
| `stale.rs` | For each cited range: the text at `pinned`; at HEAD the same lines → fresh; the same block elsewhere in the file → moved (fresh, new line shown); otherwise, or the file gone → stale. Plain text comparison, no model |
| `select.rs` | What a task gets (below) |
| `commitments.rs` | Today's `knowledge index`, moved; writes `commitments.md` |

**CLI** (`cli/onboard.rs`): `zforge onboard` (probe, then state and next
step), `onboard status [--json]`, `onboard review|accept|revise <file>`,
`onboard baseline --known <tests>` / `--clear`, `onboard draft [--budget]`,
`onboard refresh [--budget]`. `accept`, `revise` and `baseline` need a
terminal and a typed confirmation, like intake decisions.

**MCP**: `onboard_status`, `onboard_probe`, `onboard_review`. No accept,
revise or baseline tool; they join `v15::FORBIDDEN`.

**Config**: `knowledge.dir`, `knowledge.prompt_limit` (bytes of knowledge in
a run prompt, default 24000), `knowledge.draft_budget_usd` (default 2.0 per
headless call), `onboarding.required` (default false).

**Choosing what a task gets** (`select.rs`), from the pinned snapshots only:
1. every accepted `rules.md` item, then every `conventions.md` item;
2. `domain.md` items the contract or its stages cite by ID;
3. `domain.md` sections whose module appears in a path the contract or
   stages name (backticked paths and `covers` modules), in the file's order;
4. stop at `knowledge.prompt_limit`; name what did not fit by ID and give
   the absolute path of the accepted snapshot, so the agent can read the
   rest.

**Intake side.** The intake skill reads the accepted knowledge first; task
contracts cite the items they rely on under Constraints (`Knowledge:
DOM-004, RULE-001`). Intake lint warns on a knowledge ID that does not
exist.

**Handover.** The manifest pins, besides the intake files, the accepted
revision (hash) of each knowledge file and the known-failure list at that
moment. Runs read both from the pins, as they read the contract.

**Run.** `contract.tmpl` and `review_contract.tmpl` get a
`{{project_knowledge}}` section; the trace records the knowledge revisions
and item IDs given. Verification: exit 0 passes as today; a non-zero exit
passes only when the parser read at least one failing test and every failing
test is on the pinned known list; `verified` gains `tolerated: [..]`. No
parser for the language, or no test names read → fails as today. A known
test that passes in a run, or no longer appears in the output, is recorded
(`verified.known_passing: [..]`); `zforge status` and readiness then suggest
removing it from the list — the list itself changes only when the user
records it.

**Readiness.** `Readiness::with` adds project checks: onboarded (three files
accepted), no stale item, baseline green or its failures known. Warnings by
default; errors with `onboarding.required: true`.

**Drafting.** Interactive (first onboarding): the `zforge-onboard` skill
works module by module from the probe's list, reads code through CodeGraph,
writes items with evidence, asks the user about what the code cannot show,
and runs `onboard review`. Headless (`draft`, `refresh`): one Claude call per
module (draft) or per file with stale items (refresh), in a worktree at HEAD,
budget per call; the result is sent for review, never accepted.

## Binding decisions

- Onboarding is a separate command; init, install and migrate never call a model.
- Knowledge files, generated files and their review records live in `knowledge.dir` (default `docs/knowledge/`) and are committed; review records of knowledge are not under `.zforge/`.
- Knowledge review reuses the intake review machinery (revisions, hashes, derived states, TTY-only accept and revise), generalised to a document set; there is no second implementation.
- Every knowledge item has a stable ID and `path:line` evidence checked against the file's pinned commit; an ID is never reused for another item.
- Staleness is a text comparison of the cited lines between the pinned commit and HEAD; moved but unchanged text is not stale.
- A handover pins the accepted knowledge revisions and the known-failure list; runs read knowledge and known failures only from those pins.
- A run with a non-zero test exit passes only when failing test names were read and all are on the pinned known list; unreadable output fails.
- The known-failure list has no expiry: it changes only when the user records it at a terminal; zforge reports known tests that now pass or no longer exist, and suggests removing them.
- Accepting knowledge, revising it and recording known failures are terminal-only decisions and never MCP tools.
- The knowledge informs agents; it never changes a run's verdict, the protected-test guard or the reuse key of a task.

## Implementation suggestions

- `onboard draft` can reuse `run::execute::call_agent` for the spawn, trace
  and cost, with a knowledge prompt template instead of the contract.
- `stale.rs` can find a moved block with a plain substring search of the
  cited lines joined by newlines; no diff library needed.
- The item parser can share `lint::strip_comments` and `lint::sections`.

## Alternatives considered

- **Knowledge in `.zforge/knowledge/`**: not shared where `.zforge/` is
  ignored (most projects here), so a teammate's runs would get none.
- **One `knowledge.md`**: one change forces the whole file through review,
  and the domain dwarfs conventions and rules.
- **An embedding/vector index**: not reviewable, and CodeGraph already
  answers "where is X".
- **Drafting during `init`**: rejected by REQ-001 — init must stay fast and
  model-free.
- **A model deciding staleness**: not reproducible; a text comparison is
  cheap and can be explained.
- **Knowledge changing the task reuse key**: a reused output was verified
  against the same contract; knowledge refines how, not what, so reusing it
  stays sound.

## Assumptions and evidence

- The runner reads failing test names for Rust, Go, Python, Flutter/Dart and
  JS/TS (`src/runner/mod.rs:87-97`); for other languages the strict rule
  applies — shown by `parse_test_output("nothing", "ios").total_tests == 0`
  (`src/runner/mod.rs:561`).
- Intake review is already file-set agnostic apart from the paths
  (`src/intake/review.rs`, `src/intake/mod.rs:39-75`), which is what makes
  the document-set generalisation small.
- Handovers already pin snapshots by hash and runs read only pins
  (`src/run/contract.rs`), so pinning knowledge follows the same path.
- `git show <commit>:<path>` gives the cited file at the pinned commit
  without a checkout (used by runs already through `run/git.rs`).

## Open questions

- [x] Does the known-failure list expire? — No: it changes only when the
  user records it; runs report known tests that now pass or are gone, and
  status and readiness suggest removing them (Run, binding decisions).
