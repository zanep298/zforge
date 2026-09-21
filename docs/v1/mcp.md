# MCP Integration

zforge exposes a Model Context Protocol (MCP) server that AI coding tools use to
drive the pipeline without the user manually running CLI commands. The same
`cli::*` functions back both the CLI and MCP server — there are no separate code
paths.

## Starting the server

```bash
zforge mcp
```

Runs a stdio JSON-RPC 2.0 server on stdin/stdout. AI tools launch this process
automatically when the project is opened.

## Registering with an AI tool

### Claude Code

`zforge init` writes `CLAUDE.md`, `.claude/settings.json`, and agent files. To
register the zforge MCP server with Claude Code, run:

```bash
zforge mcp register --agent claude
```

This runs `claude mcp add zforge -- zforge mcp` under the hood.

### Manual registration

```bash
zforge mcp register                      # register with all detected agents
zforge mcp register --agent claude       # Claude Code only
zforge mcp register --agent opencode     # OpenCode only
zforge mcp register --agent codex        # Codex only
zforge mcp register --agent all          # explicit all (default)
zforge mcp register --agent claude --force   # re-register if already present
```

`register` writes to the agent's config or registry (for example
`~/.config/opencode/opencode.json` for OpenCode and `~/.codex/config.toml` for
Codex). For Claude Code, it shells out to `claude mcp add`.

## MCP tools

Six tools are exposed. An AI orchestrator calls these instead of running shell
commands.

---

### `task_import`

Create a new task.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `task_id` | string | no | Explicit ID (e.g. `AUTH-42`). Auto-assigned if omitted |
| `title` | string | no | Task title |
| `domain` | string | no | Domain or team context |
| `description` | string | no | Task description |
| `jira` | string | no | Jira ticket URL |
| `figma_url` | string | no | Figma node URL |
| `figma_context` | string | no | Figma design context (pre-fetched via Figma MCP) |
| `flow` | string | no | Pipeline preset: `full`, `fixbug`, `spike`, `docs` |

---

### `get_prompt`

Get the rendered prompt for a phase. The AI reads this and uses it as instructions
for the agent work.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `phase` | string | yes | `spec`, `testspec`, `plan`, `code`, `verify`, `review` |

Returns the rendered prompt text. Call this before dispatching an agent sub-task.

---

### `approve`

Mark an artifact as human-approved. Unblocks downstream phases.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `artifact` | string | yes | `testspec` or `plan` |
| `note` | string | no | Approval note written to the artifact frontmatter |

---

### `verify`

Run the configured test command and record results to `verify.md`.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `command` | string | no | Override the default test command from `config.yaml` |
| `timeout` | number | no | Timeout in seconds (default: 600) |

Returns pass/fail, test counts, and raw output.

---

### `ship`

Combine `Coded` state advance + `verify` in a single round trip. Use when the AI
has already written code and wants to record and test it without two separate calls.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `command` | string | no | Override the default test command |
| `timeout` | number | no | Timeout in seconds (default: 600) |

Idempotent: if the task is already at `Coded`, skips the state advance and runs
verify directly.

---

### `status`

Query task state.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `task_id` | string | no | Specific task. Omit for all tasks |

Returns current state, flow, next command hint, and completion status.

---

## Typical orchestrator pattern (Claude Code)

An AI orchestrator using the MCP tools drives a full pipeline like this:

```
1. task_import(title="Add rate limiting", flow="fixbug")
   → returns task_id: "TASK-005"

2. get_prompt(task_id="TASK-005", phase="spec")
   → returns prompt text

3. [dispatch sub-agent with prompt, wait for spec.md to be written]

4. [call zf spec TASK-005 --done via shell or MCP signal]

5. get_prompt(task_id="TASK-005", phase="testspec")
   → returns prompt text

6. [dispatch sub-agent, wait for testspec.md]

7. [human gate: user approves via CLI or the AI calls approve()]
   approve(task_id="TASK-005", artifact="testspec")

8. [code sub-agent runs]

9. ship(task_id="TASK-005")
   → advances to Coded, runs tests, returns pass/fail

10. status(task_id="TASK-005")
    → State: Verified, complete for fixbug flow
```

## Debugging MCP

Run the server directly to inspect raw JSON-RPC:

```bash
zforge mcp
# type a request on stdin, read response on stdout
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

Or use `--short` status to verify the server can read task state:

```bash
zforge status --short
```
