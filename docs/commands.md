# Command Reference

## zforge init

Initialize zforge in the current project.

```bash
zforge init          # scaffold .zforge/, .mcp.json, CLAUDE.md, and .claude/
zforge init --force  # overwrite existing zforge/Claude Code files
```

Detects project language automatically from `Cargo.toml`, `go.mod`,
`package.json`, `pyproject.toml`, Android Gradle files, or iOS project files.

---

## zforge task import

Create a new task from the template.

```bash
zforge task import                                        # auto-generate ID (TASK-001, TASK-002, ...)
zforge task import TASK-001                               # explicit ID
zforge task import --title "Add health check endpoint"    # auto ID + title
zforge task import TASK-001 --title "Add health check endpoint"
zforge task import TASK-001 --title "..." --domain "api"

# Import from Jira (fetches summary + description via JIRA_EMAIL / JIRA_API_TOKEN)
zforge task import --jira https://company.atlassian.net/browse/PROJ-123

# UI task: pass Figma design context (pre-fetched via Figma MCP)
zforge task import TASK-001 --title "Login screen" \
  --figma "https://figma.com/design/FILE/Login?node-id=1" \
  --figma-context "$(cat figma_export.md)"
```

When no ID is given, zforge scans the tasks directory and assigns the next available
`TASK-NNN` number. Explicit IDs must match `[A-Z]+-[0-9]+` (e.g. `AUTH-42`).

Creates `.zforge/tasks/<TASK-ID>/task.md`. For UI tasks with Figma context, also
creates `.zforge/tasks/<TASK-ID>/figma.md` — automatically injected into spec and
code prompts.

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
zforge task import TASK-001 \
  --figma "https://figma.com/design/FILE/Screen?node-id=42" \
  --figma-context "$(cat figma_node.md)"
```

**What gets injected:** `figma.md` content appears as a `## Design (Figma)` section
in the `code` prompt and as `## Design Reference (Figma)` in the `spec` prompt.
Empty when no Figma context was provided — no effect on non-UI tasks.

---

## zforge spec

Generate a spec prompt and print it to stdout.

```bash
zforge spec TASK-001          # print prompt (paste into AI)
zforge spec TASK-001 --copy   # copy prompt to clipboard
zforge spec TASK-001 --done   # mark spec phase complete
```

**AI writes:** `.zforge/tasks/TASK-001/spec.md`
**Requires:** `.zforge/tasks/TASK-001/task.md` to exist and be filled in

---

## zforge approve

Human approval gate. Marks an artifact as reviewed.

```bash
zforge approve TASK-001 testspec
zforge approve TASK-001 plan
zforge approve TASK-001 verify
zforge approve TASK-001 testspec --note "coverage is complete"
zforge approve TASK-001 testspec --yes    # skip confirmation prompt
```

Sets `reviewed: true` in the artifact's YAML frontmatter.
Downstream phases are blocked until required approvals are in place.

---

## zforge testspec

Generate a test specification prompt.

```bash
zforge testspec TASK-001
zforge testspec TASK-001 --done
```

**Requires:** completed `spec.md`
**AI writes:** `.zforge/tasks/TASK-001/testspec.md`

---

## zforge plan

Generate an implementation plan prompt.

```bash
zforge plan TASK-001
zforge plan TASK-001 --done
```

**Requires:** approved `testspec.md`
**AI writes:** `.zforge/tasks/TASK-001/plan.md`

---

## zforge code

Generate an implementation prompt. AI writes tests first, then production code.

```bash
zforge code TASK-001
zforge code TASK-001 --done
```

**Requires:** approved `testspec.md` and approved `plan.md`
**AI writes:** source code + tests + `.zforge/tasks/TASK-001/implementation-log.md`

---

## zforge verify

Run the project's test command and record results.

```bash
zforge verify TASK-001
zforge verify TASK-001 --command "cargo test --features otel"
zforge verify TASK-001 --timeout 120
```

Runs the `test_command` from `.zforge/config.yaml` by default.
Records pass/fail to `.zforge/tasks/TASK-001/verify.md`.

---

## zforge review

Generate a review prompt. AI checks spec drift and extracts patterns.

```bash
zforge review TASK-001
zforge review TASK-001 --done
```

**AI writes:** `.zforge/tasks/TASK-001/review-summary.md`
**AI updates:** `.zforge/memory/patterns.md` with reusable patterns

---

## zforge status

Show task progress.

```bash
zforge status                 # all tasks with phase indicators
zforge status TASK-001        # detailed status for one task
zforge status TASK-001 --json # machine-readable output
zforge status TASK-001 --short
```

Phase indicators:
```
○ pending   ◑ in-progress   ✓ done   ✗ blocked
```

---

## zforge retry

Reset a task to an earlier phase and redo from there.

```bash
zforge retry TASK-001 --from spec       # redo everything from spec onward
zforge retry TASK-001 --from testspec   # redo testspec, plan, code, verify, review
zforge retry TASK-001 --from code       # redo code and verify only
zforge retry TASK-001 --from spec --yes # skip confirmation
```

Backs up current artifacts before resetting.

---

## Global flags

| Flag | Description |
|------|-------------|
| `--help` | Print help for any command |
| `--version` | Print zforge version |

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
  tasks: "./.zforge/tasks"
  agents: "./.zforge/agents"
  memory: "./.zforge/memory"
  skills: "./.zforge/skills"

review:
  auto_approve: false       # set true to skip approval prompts
```
