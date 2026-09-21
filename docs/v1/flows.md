# Flows

A flow is a pipeline preset chosen at import time. It determines which phases run,
which approval gates exist, and where the task terminates. Not every task needs the
full nine-phase pipeline.

## Presets

| Flow | Phases | Gates | Use when |
|------|--------|-------|----------|
| `full` (default) | spec → testspec → **approve** → plan → **approve** → code → verify → review | testspec, plan | Feature work, cross-cutting changes, risky tasks |
| `fixbug` | spec → testspec → code → verify | none | Bug with a clear reproducer; no plan or review overhead |
| `spike` | spec → code | none | Research, prototypes, throwaway code |
| `docs` | code | none | README, comments, doc edits — no spec or tests |

## Choosing a flow

**Use `full` when:**
- You're building something new
- The scope is unclear at import time
- The task touches multiple files or modules
- You want pattern extraction into memory after review

**Use `fixbug` when:**
- The bug is reproducible and localized
- You can write a failing test without a formal plan
- You want fast feedback without approval gates slowing you down

**Use `spike` when:**
- You're evaluating a library or approach
- Output will be thrown away or heavily rewritten
- No test contract is needed

**Use `docs` when:**
- Pure documentation: README, CHANGELOG, code comments
- No spec, no tests, just prose or annotation

## Setting a flow

```bash
zforge task import FEAT-7  --flow full    # explicit default
zforge task import BUG-42  --flow fixbug  --title "Login crash on empty email"
zforge task import SPIKE-1 --flow spike   --title "Evaluate GraphQL adapter"
zforge task import DOC-9   --flow docs    --title "Document MCP setup"

# aliases work too
zforge task import BUG-42 --flow bug      # same as fixbug
zforge task import BUG-42 --flow bugfix   # same as fixbug
zforge task import DOC-9  --flow doc      # same as docs
```

## How flow enforcement works

The flow is stored in `.zforge/tasks/<ID>/.state.yaml` at import and **cannot be
changed afterwards**. Every subsequent command reads it.

**Phase commands not in the flow fail immediately:**
```
$ zforge plan BUG-42
Error: plan is not part of the fixbug flow for task BUG-42
```

**`zforge status` and `Next:` hints are flow-aware:**
```
$ zforge status BUG-42
Task:  BUG-42
Flow:  fixbug
State: TestspecDone
Next:  zf code BUG-42
```
On a `fixbug` task at `TestspecDone`, the next step is `code` — not `approve testspec`.

**Terminal state per flow:**

| Flow | Done when |
|------|-----------|
| `full` | `Reviewed` |
| `fixbug` | `Verified` |
| `spike` | `Coded` |
| `docs` | `Coded` |

`zforge status` reports the task complete once the terminal state is reached.

## Approval gates

Only `full` has human gates. The others run straight through to terminal state
without requiring any `zforge approve` calls.

```
full:    ... testspec → [approve testspec] → plan → [approve plan] → code ...
fixbug:  ... testspec → code ...
spike:   ... spec → code
docs:    code
```

## What each flow receives as context

Memory files (`patterns.md`, `anti-patterns.md`, `domain-glossary.md`) are injected
when non-empty regardless of flow. But shorter flows naturally receive less artifact
context at the `code` phase because earlier artifacts don't exist:

| Phase | Full | Fixbug | Spike | Docs |
|-------|------|--------|-------|------|
| spec receives | task, patterns, glossary | task, patterns, glossary | task, patterns, glossary | — |
| testspec receives | spec, task, anti-patterns | spec, task, anti-patterns | — | — |
| code receives | plan, testspec, spec, task, patterns, anti-patterns | testspec, spec, task, patterns, anti-patterns | spec, task, patterns, anti-patterns | task, patterns, anti-patterns |

The `code` prompt adapts: it only references artifacts that actually exist for the
active flow. A `docs` task gets a leaner code prompt than a `full` task.

## Changing a flow after import

There is no `--flow` update command. If you imported with the wrong flow:

```bash
# Re-import with a new ID or overwrite the existing one
zforge task import BUG-42 --flow full --title "Login crash on empty email"
```

Re-importing resets state to `Imported` and rewrites `task.md` — any artifacts
already written are orphaned but not deleted.
