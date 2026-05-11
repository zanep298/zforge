# Workflow

zforge enforces a gated TDD-first pipeline. Each phase produces a markdown artifact.
Approval gates prevent the AI from moving forward without human sign-off.

## Pipeline overview

```
task import
    │
    ▼
 spec          ← AI clarifies requirements, defines acceptance criteria
    │
 approve ──── human gate (zforge approve TASK-001 spec)
    │
    ▼
 testspec      ← AI derives test cases before any code is written
    │
 approve ──── human gate (zforge approve TASK-001 testspec)
    │
    ▼
 plan          ← AI writes step-by-step implementation plan
    │
    ▼
 code          ← AI writes failing tests first, then minimal implementation
    │
    ▼
 verify        ← zforge runs your test command and records results
    │
    ▼
 review        ← AI checks for spec drift, extracts patterns into memory
```

---

## Phase details

### task import

Creates `tasks/<TASK-ID>/task.md` from a template. The task ID is auto-generated
(`TASK-001`, `TASK-002`, …) unless you supply one explicitly.

```bash
zforge task import                        # auto ID
zforge task import AUTH-42                # explicit ID
zforge task import --title "My feature"   # auto ID with title pre-filled

# Jira import
zforge task import --jira https://company.atlassian.net/browse/PROJ-123

# UI task with Figma context
zforge task import TASK-001 --title "Login screen" \
  --figma "https://figma.com/design/FILE/Login?node-id=1" \
  --figma-context "<output from Figma MCP>"
```

Fill in the task description, context, and "done when" criteria before running any
other phase. The richer the task description, the sharper the spec.

**UI tasks:** If `--figma-context` is provided, zforge writes `tasks/<ID>/figma.md`
and injects it into the `spec` and `code` prompts automatically. Fetch the context
using your AI tool's Figma MCP before calling `task import`.

### spec

The spec-agent reads `task.md` (and `figma.md` if present) and produces
`tasks/<TASK-ID>/spec.md` with:

- **Problem** — what is broken or missing
- **Goal** — measurable success definition
- **In Scope / Out of Scope** — explicit boundaries
- **Assumptions** — non-obvious things taken as true
- **Impacted Areas** — files, modules, APIs that will change
- **Acceptance Criteria** — observable, testable conditions

> **Approval required.** Read spec.md carefully. Reject vague acceptance criteria.
> The spec becomes the contract for every subsequent phase.

### testspec

The testspec-agent reads `spec.md` and produces `tasks/<TASK-ID>/testspec.md` —
a concrete list of test cases written **before** any implementation.

Each test case includes: scenario, input, expected output, edge cases.

> **Approval required.** Ensure coverage of all acceptance criteria and meaningful
> edge cases. Missing tests here means missing coverage later.

### plan

The plan-agent reads spec + testspec and produces `tasks/<TASK-ID>/plan.md` —
an ordered list of implementation steps with file paths and function signatures.
No code yet, just a precise map of what will change and why.

### code

The code-agent reads spec + testspec + plan (and `figma.md` if present) and:

1. Writes failing tests matching every case in `testspec.md`
2. Implements minimal production code to make them pass
3. Appends a change log to `tasks/<TASK-ID>/implementation-log.md`

The agent must not touch code outside the scope defined in the plan.

### verify

zforge runs your configured test command (e.g. `cargo test`) and records the
results in `tasks/<TASK-ID>/verify.md`. Pass/fail is shown in the terminal.

If tests fail, copy the failure output back to your AI tool and ask the code-agent
to fix them. Re-run `zforge verify` after each fix attempt.

### review

The review-agent reads all artifacts and the final diff and produces
`tasks/<TASK-ID>/review.md`. It checks for:

- Spec drift — did the implementation match what was agreed?
- Uncovered test cases — anything in testspec not tested?
- Pattern extraction — reusable patterns written to `.zforge/memory/patterns.md`

---

## Approval gates

```bash
zforge approve TASK-001 spec         # unlocks testspec phase
zforge approve TASK-001 testspec     # unlocks plan and code phases
```

Approval sets `reviewed: true` in the artifact's YAML frontmatter. zforge blocks
downstream phases until both spec and testspec are approved.

To add a note to an approval:

```bash
zforge approve TASK-001 spec --note "scope is correct, AC is testable"
```

---

## Retry a phase

If a phase produced bad output, reset and redo it:

```bash
zforge retry TASK-001 --from spec       # back to spec, clears everything after it
zforge retry TASK-001 --from testspec   # back to testspec only
```

---

## Memory accumulation

After each review, patterns extracted by the review-agent are appended to
`.zforge/memory/patterns.md`. Future spec and code phases automatically receive
this file as context, so the AI improves with every task completed.

```
.zforge/memory/
├── patterns.md          # good patterns to repeat
├── anti-patterns.md     # mistakes to avoid
└── domain-glossary.md   # project-specific terminology
```
