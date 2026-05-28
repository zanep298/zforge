---
description: Drive the zforge TDD pipeline for a task end-to-end via MCP
argument-hint: <TASK-ID> [--flow full|fixbug|spike|docs] [--agent claude|codex|opencode] [--auto-approve]
---

# /zforge — run task `$ARGUMENTS`

You are the orchestrator for one zforge task. Drive the pipeline through the
`mcp__zforge__*` tools. For each phase, delegate the actual work to the
matching sub-agent (see phase table below). You handle all MCP calls and git
ops — sub-agents only read/write files.

## Arguments

Parse `$ARGUMENTS`:

- First positional token = `TASK_ID` (e.g. `TASK-001`, `PROJ-42`). Required.
- `--flow <full|fixbug|spike|docs>` — pipeline preset. Default `full`.
- `--agent <claude|codex|opencode>` — primary agent. Default `claude`.
- `--fallback <name>` — optional secondary agent.
- `--auto-approve` — branch isolation mode (see below). Skips testspec/plan
  approval prompts and commits each phase to `zforge/<TASK-ID>`.

If `TASK_ID` is missing, ask the user for it. Do not guess.

## Phases by flow

| Flow    | Phases                                                            |
|---------|-------------------------------------------------------------------|
| full    | spec → testspec → (approve testspec) → plan → (approve plan) → code → verify → review |
| fixbug  | spec → testspec → code → verify                                   |
| spike   | spec → code                                                       |
| docs    | code                                                              |

## Step 0 — import + optional branch setup

Call `mcp__zforge__task_import` with:

```json
{
  "task_id": "<TASK_ID>",
  "flow": "<flow or omit for full>",
  "agent": "<agent or omit for claude>",
  "fallback": "<fallback or omit>"
}
```

If the task already exists you'll get an error — ignore and continue at the
phase the task is currently in (use `mcp__zforge__status(task_id=<TASK_ID>)` to
check).

**If `--auto-approve` is set**, additionally run via Bash:

```bash
zforge git check-clean          # refuses if working tree dirty
zforge git init-branch <TASK_ID>  # checkout zforge/<TASK_ID>, create if missing
```

If `check-clean` fails, stop and tell the user to commit/stash first. Do not
try to recover automatically.

## Phase → sub-agent mapping

| Phase     | Sub-agent        |
|-----------|------------------|
| spec      | `spec-agent`     |
| testspec  | `testspec-agent` |
| plan      | `plan-agent`     |
| code      | `code-agent`     |
| review    | `review-agent`   |

## Steps 1..N — pre-code phase loop

For each active phase before `code` (`spec`, `testspec`, and `plan` when present):

1. `mcp__zforge__get_prompt(phase=<PHASE>, task_id=<TASK_ID>)`
2. Invoke the matching sub-agent with the returned prompt as its task
   description. Example for spec phase:
   > Use the `spec-agent` sub-agent. Pass it this prompt: <prompt content>
3. Wait for the sub-agent to finish writing the artifact.
4. **If `--auto-approve` is set**, after the sub-agent finishes:
   ```bash
   zforge git commit-phase <TASK_ID> <phase>
   ```
   This stages everything and commits `zforge(<phase>): <TASK-ID> <title>`.
   No-op if nothing changed.
5. Gate handling (`testspec` / `plan` only on `full` flow):
   - **Without `--auto-approve`**: STOP and ask the user
     `"<phase> looks like this — approve? (y/N)"`. Only call
     `mcp__zforge__approve(task_id=<TASK_ID>, artifact=<phase>)` after explicit
     confirmation.
   - **With `--auto-approve`**: call `mcp__zforge__approve` immediately, no
     prompt. The branch + per-phase commit is the audit trail; the user
     reviews via git diff.

> **Important**: Always call `mcp__zforge__approve` and `mcp__zforge__ship`
> yourself (parent session) — never ask sub-agents to call MCP tools.

## Code + verify

When the phase is `code`:

1. `mcp__zforge__get_prompt(phase="code", task_id=<TASK_ID>)`
2. Invoke `code-agent` sub-agent with the returned prompt.
3. Wait for `code-agent` to finish writing/editing source files.
4. If the active flow ends at `code` (`spike` / `docs`), run
   `zforge code <TASK_ID> --done` via Bash. If `--auto-approve`, then run
   `zforge git commit-phase <TASK_ID> code`. Skip verify/review.
5. Otherwise, if `--auto-approve`: `zforge git commit-phase <TASK_ID> code`.
6. For flows with `verify` (`full` / `fixbug`), call
   `mcp__zforge__ship(task_id=<TASK_ID>)` yourself. This advances to Coded +
   runs verify.
7. On test failure:
   - Read `.zforge/tasks/<TASK_ID>/verify.md`.
   - If `--auto-approve`, undo the previous code commit so retry doesn't stack:
     `git reset --soft HEAD~1`.
   - Invoke `code-agent` again with the original prompt **plus** the verify
     failure content appended.
   - Then `zforge git commit-phase <TASK_ID> code` (if auto-approve) and
     `mcp__zforge__ship` again.
   - Cap at 3 retries. Surface persistent failure to the user.

## Review (full flow only)

1. `mcp__zforge__get_prompt(phase="review", task_id=<TASK_ID>)`
2. Invoke `review-agent` sub-agent with the returned prompt to write
   `review-summary.md`.
3. If `--auto-approve`: run `zforge review <TASK_ID> --done` via Bash, then
   `zforge git commit-phase <TASK_ID> review`.
4. Otherwise, ask the user to confirm review, then run
   `zforge review <TASK_ID> --done` via Bash on confirm.

`zforge review --done` triggers memory extraction and advances the task to
Reviewed.

## After task completes

The user may follow up with one of:

- **"merge <TASK_ID>"** — switch back to base branch, `git merge zforge/<TASK_ID>`,
  then cleanup: `git worktree remove` (if applicable) + `git branch -d zforge/<TASK_ID>`.
- **"merge <TASK_ID> squash"** — `git merge --squash zforge/<TASK_ID>` + manual
  commit, then cleanup.
- **"pr <TASK_ID>"** — `git push -u origin zforge/<TASK_ID>` + `gh pr create`.
  Do NOT cleanup (user reviews via PR).
- **"diff <TASK_ID>"** — show `git log --oneline <base>..zforge/<TASK_ID>` +
  `git diff <base>...zforge/<TASK_ID>` for review.
- **"cleanup <TASK_ID>"** — delete branch without merging.

Before any merge: check `git status` of the target branch is clean. On
conflict, stop and let the user resolve.

## Rules

- Default agent is `claude` — that means you orchestrate and sub-agents
  do the file work. Only pass `--agent codex|opencode` if user explicitly
  says so, which delegates spawn to the orchestrator subprocess instead.
- Without `--auto-approve`: never skip testspec/plan approval gates without
  explicit user "yes".
- With `--auto-approve`: never push to remote and never run destructive git
  ops (reset --hard, push --force, branch -D) unless the user explicitly asks.
- Never invent paths. Always write to the path returned by `get_prompt`.
- On any MCP or git error, show the raw error and ask how to proceed.
- After each phase, give one short status line: `✓ spec written +committed`
  / `✓ tests passing 12/12` / `⏸ awaiting plan approval`.

## Final report

When the task reaches its terminal state, print:

```
✓ <TASK_ID> done (flow=<flow>, branch=zforge/<TASK_ID> if auto-approve)
  spec       .zforge/tasks/<TASK_ID>/spec.md          (omit if skipped)
  testspec   .zforge/tasks/<TASK_ID>/testspec.md      (omit if skipped)
  plan       .zforge/tasks/<TASK_ID>/plan.md          (omit if skipped)
  verify     <N>/<M> tests passing                    (omit if skipped)
  review     .zforge/tasks/<TASK_ID>/review-summary.md (omit if skipped)

next: ask me "merge <TASK_ID>" or "pr <TASK_ID>" or "diff <TASK_ID>"
```
