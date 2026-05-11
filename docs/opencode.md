# Using zforge with OpenCode

OpenCode is a terminal-based AI coding tool. zforge integrates with it by generating
structured prompts and scaffolding `.opencode/` with agents, skills, and rules that
OpenCode loads automatically at startup.

## Setup

### 1. Install zforge globally

```bash
cargo install --path .
```

### 2. Initialize for OpenCode

Run this in your project root:

```bash
zforge init opencode
```

This creates:

```
.opencode/
├── opencode.json        # OpenCode config: model, context files, MCP server
├── instructions.md      # project overview and workflow summary
├── agents/
│   ├── spec-agent.md
│   ├── testspec-agent.md
│   ├── plan-agent.md
│   ├── code-agent.md
│   └── review-agent.md
├── skills/
│   ├── rust-patterns.md          # language patterns (auto-detected)
│   ├── rust-testing.md
│   ├── tdd-workflow.md
│   ├── verification-loop.md
│   └── security-review.md
└── rules/
    ├── coding-style.md
    ├── testing.md
    └── security.md
```

> Skills and rules are pulled from your ECC install (`~/.claude/skills/`,
> `~/.claude/rules/`) when available, so your personal standards flow in
> automatically.

`opencode.json` registers `zforge mcp` as a local MCP server so OpenCode's AI
can call zforge tools directly from inside the TUI.

### 2. Initialize the zforge pipeline

```bash
zforge init
```

This creates `.zforge/` with prompt templates and the memory store.

---

## How it works

When `.opencode/agents/` exists in the project and the `opencode` binary is
available, `zforge` calls OpenCode directly — no copy-paste needed.

```
zforge spec TASK-001
     │
     ▼  auto-detects .opencode/agents/spec-agent.md
     │
     └─► opencode run "<prompt>" --agent spec-agent
               │
               ▼
         OpenCode loads .opencode/ context (skills, rules, instructions)
         and runs the spec-agent
               │
               ▼
         writes tasks/TASK-001/spec.md
               │
               ▼
         control returns to your terminal
               │
               ▼
     zforge approve TASK-001 spec   (you review, then approve)
```

**Fallback:** if `opencode` is not installed or `.opencode/` does not exist,
`zforge` prints the prompt to stdout so you can paste it manually.

---

## End-to-end example

### Task: implement logging metrics for OpenTelemetry

#### Step 1 — Import the task

```bash
zforge task import TASK-001 --title "Implement logging metrics for OpenTelemetry"
```

Open `tasks/TASK-001/task.md` and describe the task:

```markdown
## Task

Add structured logging and metrics using the OpenTelemetry SDK.
Emit spans for each request and export counters for request_count,
request_duration_ms, and error_count to an OTLP collector.

## Context

The service has no observability today. The collector endpoint will be
configured via the OTLP_ENDPOINT environment variable.

## Done When

- Spans emitted per HTTP request with trace context propagation
- Three metrics exported: request_count, request_duration_ms, error_count
- Missing OTLP_ENDPOINT at startup produces a clear error
- All existing tests still pass
```

---

#### Step 2 — Spec phase

```bash
zforge spec TASK-001
```

zforge detects `.opencode/agents/spec-agent.md` and calls:

```
opencode run "<prompt>" --agent spec-agent
```

OpenCode runs non-interactively, reads `tasks/TASK-001/task.md`, and writes
`tasks/TASK-001/spec.md`. Control returns to your terminal when done.

Review `spec.md`, then approve:

```bash
zforge approve TASK-001 spec
```

---

#### Step 3 — Test spec phase

```bash
zforge testspec TASK-001
```

Paste prompt into OpenCode → OpenCode writes `tasks/TASK-001/testspec.md`:

```markdown
- [ ] span is emitted when HTTP handler is called
- [ ] request_count increments by 1 per request
- [ ] request_duration_ms recorded in milliseconds
- [ ] error_count increments on 5xx responses only
- [ ] OTLP_ENDPOINT missing → startup fails with descriptive error
- [ ] existing tests unaffected (no regressions)
```

Approve:

```bash
zforge approve TASK-001 testspec
```

---

#### Step 4 — Plan phase

```bash
zforge plan TASK-001
```

Paste prompt → OpenCode writes `tasks/TASK-001/plan.md` with ordered
implementation steps, file paths, and function signatures. No code yet.

---

#### Step 5 — Code phase

```bash
zforge code TASK-001
```

Paste prompt → OpenCode:

1. Writes failing tests for every case in `testspec.md`
2. Implements the minimal code to make them pass
3. Appends a change log to `tasks/TASK-001/implementation-log.md`

---

#### Step 6 — Verify

```bash
zforge verify TASK-001
```

zforge runs `cargo test` automatically and shows results. If tests fail,
copy the failure output back into OpenCode and ask the code-agent to fix it,
then re-run `zforge verify`.

---

#### Step 7 — Review

```bash
zforge review TASK-001
```

Paste prompt → OpenCode checks for spec drift, missing coverage, and extracts
reusable patterns into `.zforge/memory/patterns.md` for future tasks.

---

---

## Running zforge inside OpenCode TUI (MCP)

`zforge mcp` starts a stdio MCP server. OpenCode connects to it automatically
when you open the project, giving its AI access to these tools:

| Tool | What it does |
|------|-------------|
| `task_import` | Create a task and set up `task.md` |
| `get_prompt` | Render the prompt for spec/testspec/plan/code/review |
| `approve` | Approve spec or testspec to unlock the next phase |
| `verify` | Run the test suite, return pass/fail |
| `status` | Show task phase progress |

### How the AI uses them

Once inside OpenCode TUI, just describe what you want in natural language:

```
You: "Run the full pipeline for TASK-001"

OpenCode AI:
  1. calls get_prompt(phase="spec", task_id="TASK-001")
  2. processes the prompt → writes tasks/TASK-001/spec.md
  3. calls approve(task_id="TASK-001", artifact="spec")
  4. calls get_prompt(phase="testspec", task_id="TASK-001")
  5. processes → writes testspec.md
  6. calls approve(task_id="TASK-001", artifact="testspec")
  7. calls get_prompt(phase="plan", ...) → writes plan.md
  8. calls get_prompt(phase="code", ...) → implements code
  9. calls verify(task_id="TASK-001") → runs cargo test
 10. calls get_prompt(phase="review", ...) → extracts patterns
```

You never leave the OpenCode TUI. The entire pipeline runs through conversation.

### The opencode.json MCP entry

`zforge init opencode` generates this automatically:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "model": "claude-sonnet-4-5",
  "instructions": [
    ".opencode/instructions.md",
    ".opencode/rules/coding-style.md",
    ".opencode/rules/testing.md",
    ".opencode/rules/security.md"
  ],
  "skills": {
    "paths": [".opencode/skills"]
  },
  "mcp": {
    "zforge": {
      "type": "local",
      "command": ["zforge", "mcp"],
      "enabled": true
    }
  }
}
```

OpenCode starts `zforge mcp` as a subprocess when the project opens and connects
via stdio. No separate server management required.

---

## Regenerating .opencode/

If you add ECC skills to `~/.claude/skills/` and want them reflected:

```bash
zforge init opencode --force
```

This overwrites all files in `.opencode/` with the latest content.

---

## Troubleshooting

### "Configuration is invalid — expected record, received array tools"

Agent frontmatter had `tools` as a YAML array. Fixed in current version.
Regenerate:

```bash
zforge init opencode --force
```

### "Configuration is invalid — Unrecognized key: context_files"

Old version of `zforge` generated `context_files` which is not a valid OpenCode key.
The correct keys are `instructions` (file list) and `skills.paths` (directory list).
Regenerate:

```bash
zforge init opencode --force
```

### OpenCode does not load instruction files

Verify `.opencode/opencode.json` uses `instructions` (not `context_files`):

```json
{
  "model": "claude-sonnet-4-5",
  "instructions": [
    ".opencode/instructions.md",
    ".opencode/rules/coding-style.md",
    ".opencode/rules/testing.md",
    ".opencode/rules/security.md"
  ],
  "skills": {
    "paths": [".opencode/skills"]
  }
}
```

### Skills are empty or minimal

Install ECC first:

```bash
# in ~/.claude/
git clone https://github.com/affaan-m/everything-claude-code /tmp/ecc
cp -r /tmp/ecc/skills/rust-patterns ~/.claude/skills/
cp -r /tmp/ecc/skills/rust-testing ~/.claude/skills/
```

Then regenerate:

```bash
zforge init opencode --force
```
