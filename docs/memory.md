# Memory

zforge accumulates project knowledge across tasks in `.zforge/memory/`. Each
completed review automatically extracts reusable patterns and feeds them back into
future prompts. Over time, the AI gets better context about your codebase.

## Files

```
.zforge/memory/
├── patterns.md        # good patterns to repeat
├── anti-patterns.md   # mistakes the AI should avoid
└── domain-glossary.md # project-specific terminology and abbreviations
```

All three files are created empty (with a `#` header only) by `zforge init`.

## How patterns accumulate

At the end of every `full`-flow task, the review-agent reads all artifacts and the
diff, then produces `review-summary.md`. When you mark review done:

```bash
zforge review TASK-001 --done
```

zforge runs automatic pattern extraction. It scans `review-summary.md` for lines
matching `- key: description` and appends new ones to `patterns.md`. Duplicates
(matched by key) are skipped — patterns are not repeated.

**What the review-agent extracts:**
- Architectural patterns the AI used correctly that should be repeated
- Error handling conventions that worked
- Abstractions introduced that should be used consistently going forward

Edit `patterns.md` manually any time to remove stale entries or add patterns the
review missed.

## Injection rules

Memory files are only injected when they contain real content. A file with only a
`# Patterns` header is treated as empty and silently skipped. This means a fresh
project gets lean prompts; injection scales with actual content.

**Which files are injected per phase:**

| Phase | patterns.md | anti-patterns.md | domain-glossary.md |
|-------|------------|------------------|--------------------|
| `spec` | ✓ (if non-empty) | — | ✓ (if non-empty) |
| `testspec` | — | ✓ (if non-empty) | — |
| `plan` | ✓ (if non-empty) | — | — |
| `code` | ✓ (if non-empty) | ✓ (if non-empty) | — |
| `review` | — | — | — |

Memory is always injected as `/file` references — the file path is passed to the AI
agent, not inlined into the prompt text. This keeps prompts short and lets the AI
read exactly what it needs.

## Editing memory manually

All three files are plain markdown. Edit freely:

**`patterns.md`** — List patterns as `- key: description`:
```markdown
# Patterns

- repository-trait: Always define data access behind a trait; inject via constructor for testability
- error-context: Use `.with_context(|| format!("..."))` on every `?` in public functions
- newtype-ids: Wrap IDs in newtype structs (e.g. `UserId(u64)`) to prevent argument mix-ups
```

**`anti-patterns.md`** — Same format, things to avoid:
```markdown
# Anti-patterns

- unwrap-in-handlers: Never call `.unwrap()` in request handlers; use `?` and map to status codes
- inline-sql: Never format user input into SQL strings; always use parameterized queries
```

**`domain-glossary.md`** — Free-form terminology:
```markdown
# Domain Glossary

- **task** — a unit of work tracked by zforge, identified by a string ID like `FEAT-42`
- **flow** — the pipeline preset that determines which phases a task walks through
- **artifact** — a markdown file produced by an AI agent for a specific phase
```

## Why domain-glossary.md matters

The glossary is injected at the `spec` phase — the earliest point in the pipeline.
If your codebase uses domain-specific terms (e.g. "ledger entry", "settlement
batch", "trunk policy"), defining them here means the spec-agent uses your
vocabulary rather than generic terms. This reduces spec drift.

## Clearing memory

To reset memory for a fresh start:

```bash
# clear all content but keep the files
echo "# Patterns" > .zforge/memory/patterns.md
echo "# Anti-patterns" > .zforge/memory/anti-patterns.md
echo "# Domain Glossary" > .zforge/memory/domain-glossary.md
```

Files with only a header line are treated as empty by the injection check — they
won't be passed to the AI.

## Memory and short flows

Memory is injected into `fixbug`, `spike`, and `docs` flows the same way as `full`.
However, short flows don't end with a review phase, so they don't contribute new
patterns back to `patterns.md`. Only tasks that complete `zforge review --done`
trigger pattern extraction.
