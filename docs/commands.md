# Command Reference

## zf init

Initialize zforge in the current project.

```bash
zf init                  # scaffold .zforge/ and CLAUDE.md
zf init opencode         # scaffold .opencode/ for OpenCode AI
zf init opencode --force # overwrite existing .opencode/ files
zf init --force          # overwrite existing .zforge/ files
```

Detects project language automatically from `Cargo.toml`, `go.mod`,
`package.json`, or `pyproject.toml`.

---

## zf task import

Create a new task from the template.

```bash
zf task import                                        # auto-generate ID (TASK-001, TASK-002, ...)
zf task import TASK-001                               # explicit ID
zf task import --title "Add health check endpoint"    # auto ID + title
zf task import TASK-001 --title "Add health check endpoint"
zf task import TASK-001 --title "..." --domain "api"

# Import from Jira (fetches summary + description via JIRA_EMAIL / JIRA_API_TOKEN)
zf task import --jira https://company.atlassian.net/browse/PROJ-123

# UI task: pass Figma design context (pre-fetched via Figma MCP)
zf task import TASK-001 --title "Login screen" \
  --figma "https://figma.com/design/FILE/Login?node-id=1" \
  --figma-context "$(cat figma_export.md)"
```

When no ID is given, zf scans the tasks directory and assigns the next available
`TASK-NNN` number. Explicit IDs must match `[A-Z]+-[0-9]+` (e.g. `AUTH-42`).

Creates `tasks/<TASK-ID>/task.md`. For UI tasks with Figma context, also creates
`tasks/<TASK-ID>/figma.md` — automatically injected into spec and code prompts.

### Figma integration (UI tasks)

zforge does not call the Figma API directly. Instead, use your AI tool's Figma MCP
to fetch design context, then pass it to `task import`:

**Via MCP tool (Claude Code / OpenCode):**

```
1. Ask the AI: "fetch Figma node https://figma.com/design/... using Figma MCP"
2. The AI calls task_import with figma_url + figma_context
3. zforge writes figma.md and injects it into all subsequent prompts
```

**Via CLI:**
```bash
# Fetch from Figma MCP yourself, save to a file, then:
zf task import TASK-001 \
  --figma "https://figma.com/design/FILE/Screen?node-id=42" \
  --figma-context "$(cat figma_node.md)"
```

**What gets injected:** `figma.md` content appears as a `## Design (Figma)` section
in the `code` prompt and as `## Design Reference (Figma)` in the `spec` prompt.
Empty when no Figma context was provided — no effect on non-UI tasks.

---

## zf spec

Generate a spec prompt and print it to stdout.

```bash
zf spec TASK-001          # print prompt (paste into AI)
zf spec TASK-001 --copy   # copy prompt to clipboard
zf spec TASK-001 --done   # mark spec phase complete
```

**AI writes:** `tasks/TASK-001/spec.md`
**Requires:** `tasks/TASK-001/task.md` to exist and be filled in

---

## zf approve

Human approval gate. Marks an artifact as reviewed.

```bash
zf approve TASK-001 spec
zf approve TASK-001 testspec
zf approve TASK-001 spec --note "AC looks good"
zf approve TASK-001 spec --yes    # skip confirmation prompt
```

Sets `reviewed: true` in the artifact's YAML frontmatter.
Downstream phases are blocked until required approvals are in place.

---

## zf testspec

Generate a test specification prompt.

```bash
zf testspec TASK-001
zf testspec TASK-001 --done
```

**Requires:** approved `spec.md`
**AI writes:** `tasks/TASK-001/testspec.md`

---

## zf plan

Generate an implementation plan prompt.

```bash
zf plan TASK-001
zf plan TASK-001 --done
```

**Requires:** approved `spec.md` and `testspec.md`
**AI writes:** `tasks/TASK-001/plan.md`

---

## zf code

Generate an implementation prompt. AI writes tests first, then production code.

```bash
zf code TASK-001
zf code TASK-001 --done
```

**Requires:** approved `spec.md`, `testspec.md`, and completed `plan.md`
**AI writes:** source code + tests + `tasks/TASK-001/implementation-log.md`

---

## zf verify

Run the project's test command and record results.

```bash
zf verify TASK-001
zf verify TASK-001 --command "cargo test --features otel"
zf verify TASK-001 --timeout 120
```

Runs the `test_command` from `.zforge/config.yaml` by default.
Records pass/fail to `tasks/TASK-001/verify.md`.

---

## zf review

Generate a review prompt. AI checks spec drift and extracts patterns.

```bash
zf review TASK-001
zf review TASK-001 --done
```

**AI writes:** `tasks/TASK-001/review.md`
**AI updates:** `.zforge/memory/patterns.md` with reusable patterns

---

## zf status

Show task progress.

```bash
zf status                 # all tasks with phase indicators
zf status TASK-001        # detailed status for one task
zf status TASK-001 --json # machine-readable output
zf status TASK-001 --short
```

Phase indicators:
```
○ pending   ◑ in-progress   ✓ done   ✗ blocked
```

---

## zf retry

Reset a task to an earlier phase and redo from there.

```bash
zf retry TASK-001 --from spec       # redo everything from spec onward
zf retry TASK-001 --from testspec   # redo testspec, plan, code, verify, review
zf retry TASK-001 --from code       # redo code and verify only
zf retry TASK-001 --from spec --yes # skip confirmation
```

Backs up current artifacts before resetting.

---

## Global flags

| Flag | Description |
|------|-------------|
| `--help` | Print help for any command |
| `--version` | Print zf version |

---

## Config reference (`.zforge/config.yaml`)

```yaml
project:
  name: "my-service"
  language: "rust"          # rust | go | typescript | python
  test_command: "cargo test"
  root_dir: "."

opencode:
  model: "claude-sonnet-4-5"
  context_files: []         # extra files to load as context

paths:
  tasks: "./tasks"
  agents: "./.zforge/agents"
  memory: "./.zforge/memory"
  skills: "./.zforge/skills"

review:
  auto_approve: false       # set true to skip approval prompts
```
