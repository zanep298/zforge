# AI Coding Agent Contracts

How each AI Coding Agent's CLI binary behaves under zforge's orchestrator. Fill
the blanks (`?`) from real-binary verification before adopting that agent in
production.

## Why this matters

zforge's PR3 orchestrator spawns the agent's CLI binary via
`std::process::Command`:

- Prompt pipes to **stdin** (closed after write to signal EOF).
- Per-phase model selection appends `--model <name>` for known agents.
- `wait_with_output` blocks until exit; captures stdout, stderr, exit code.
- `FallbackPolicy.retryable_exit_codes` + `retryable_stderr_patterns` decide
  whether a failure triggers fallback to the configured `fallback_agent`.

Wrong assumption about any of these = silent prompt loss, runaway retries,
or pipe deadlock. Verify before trusting.

## Verification harness

Real-binary integration tests live in `tests/real_claude_test.rs` (claude
only at PR7 time). Gated by `ZFORGE_TEST_REAL_CLAUDE=1` so CI does not burn
API quota. Run locally:

```bash
ZFORGE_TEST_REAL_CLAUDE=1 cargo test --test real_claude_test -- --nocapture
```

For other agents, copy the file pattern, swap the binary name and env-var
gate (e.g. `ZFORGE_TEST_REAL_CODEX=1`).

## Matrix

Verified 2026-05-25 against `claude` v? (Claude Code) and `codex` v0.133.0.
opencode + agy still TBD — fill when adopted.

| Contract                           | claude                | codex                       | opencode | agy |
|------------------------------------|------------------------|------------------------------|----------|-----|
| Subcommand for non-interactive     | `-p` flag              | `exec` (alias `e`)           | ?        | ?   |
| Prompt delivery: stdin works?      | ✅ yes                 | ✅ yes (prints "Reading prompt from stdin...") | ?        | ?   |
| Prompt delivery: positional arg?   | n/a (uses `-p`)        | ✅ `codex exec "<prompt>"`   | ?        | ?   |
| Model flag syntax                  | `--model X`            | `-m X` / `--model X`          | `--model X` | `--model X` |
| Profile flag (if applicable)       | n/a                    | `-p <name>` (CONFLICTS w/ claude's `-p`!) | n/a      | n/a |
| Exit 0 = success                   | mostly ✅               | ⚠️ **NO** — exit 0 even on API errors | ?        | ?   |
| Exit on invalid model              | exit 1 (verified)      | exit 0 + stdout `ERROR:` JSON | ?        | ?   |
| Exit on auth failure               | depends — OAuth cache overrides env (bogus `ANTHROPIC_API_KEY` → exit 0) | ?  | ?  | ?   |
| Failure message channel            | **stdout** (not stderr!) | stdout (ERROR: JSON)         | ?        | ?   |
| Large prompt (50KB) no deadlock    | ✅ ~6s                  | ?                            | ?        | ?   |
| Approx wall time for haiku/200w    | ~46s (slow!)            | ?                            | ?        | ?   |
| Interactive prompt blocker?        | ⚠️ asks file-write confirm without bypass flag | ⚠️ same — see `-a never` | ?  | ?   |
| Confirmation bypass flag           | `--dangerously-skip-permissions` | `-a never` + `--dangerously-bypass-approvals-and-sandbox` | ? | ? |

## Critical findings 2026-05-25

### 1. Both `claude` and `codex` print failure to STDOUT, not stderr

zforge's `FallbackPolicy.retryable_stderr_patterns` only scans **stderr**. Real
binaries put their error messages in **stdout**:

- claude: `"There's an issue with the selected model (X). It may not exist..."`
- codex:  `ERROR: {"type":"error","status":400,"error":{...}}`

**Implication**: rate-limit / quota / auth failures may NOT trigger fallback
under the current default regex set. Two-part fix needed:
  1. Extend `should_fallback` to scan stdout AS WELL AS stderr.
  2. Add patterns matching the real strings above.

### 2. Codex exits 0 even on API errors

`codex exec -m gpt-5.4-mini` (invalid model) → exit 0 with body
`ERROR: {"type":"error","status":400,...}` in stdout.

**Implication**: zforge cannot trust codex exit codes. Must combine
exit-code check with stdout error-pattern detection for codex. Otherwise
zforge marks failed runs as success.

### 3. `claude` ignores `ANTHROPIC_API_KEY` when OAuth credentials exist

Bogus env var did NOT cause failure. Claude Code reads
`~/.claude/credentials` first, env var only as fallback. Tests that try to
force auth failure via env var won't work — use invalid model instead.

### 4. `claude -p` is fast for ping (`~7s`), slow for 200-word response (`~46s`)

Not unreasonable for an LLM call, but `spawn_agent` has no wall-clock
timeout. A stuck binary hangs zforge indefinitely. Recommended: add
`AgentSpec.timeout_secs` honored by `wait_with_output` (needs streaming
implementation since stdlib's `wait_with_output` has no timeout option).

## How to fill a column

Pick the agent. Run each row's experiment, drop the result.

### 1. Prompt delivery

```bash
# stdin
echo "Respond with PONG only" | <agent> -p
# arg
<agent> -p "Respond with PONG only"
# capture argv binary actually receives (debug)
dtruss -f <agent> -p "test" 2>&1 | grep <agent>     # macOS
strace -f -e execve <agent> -p "test" 2>&1 | grep <agent>  # Linux
```

→ Fill stdin/arg cells. If stdin does NOT work, zforge orchestrator must
inject the prompt into `AgentSpec.args` instead of piping — file a fix
against `src/orchestrator/spawn.rs`.

### 2. Model flag syntax

```bash
echo "Reply with: 1" | <agent> -p --model <cheap-model>
echo "exit: $?"
```

→ Confirm exit 0 + response uses that model. If the binary rejects
`--model`, update `src/orchestrator/model_args.rs::model_args_for_agent`
with the correct flag syntax.

### 3. Failure exit codes & stderr signatures

```bash
# Auth failure
<AGENT_API_KEY_ENV>=invalid-key echo "test" | <agent> -p --model <cheap>
echo "exit: $?"
# capture stderr to a file
<AGENT_API_KEY_ENV>=invalid-key echo "test" | <agent> -p --model <cheap> 2> stderr.log
cat stderr.log
```

```bash
# Rate limit / quota — harder to induce. Options:
#  - hammer the API in a tight loop until quota hits (costly)
#  - check vendor docs for the canonical message
#  - swap to a tier that's already throttled
```

→ Drop the exact exit code into `retryable_exit_codes` if it should
trigger fallback. Add a regex matching the stderr substring into
`retryable_stderr_patterns`.

### 4. Pipe deadlock check

```bash
# Generate 50KB of filler, ask for short reply.
{ head -c 50000 /dev/urandom | base64; echo; echo "Reply: DONE"; } \
  | <agent> -p --model <cheap>
```

→ If the command finishes < 60s, deadlock is not present. If it hangs,
zforge orchestrator must use threaded I/O instead of `wait_with_output`.

### 5. Interactive prompt blocker

Run the agent with a prompt that would normally trigger a confirmation
(e.g. file write). zforge's MCP-driven flow can't answer interactive
prompts — the child blocks forever.

```bash
echo "Write the text HELLO to /tmp/zforge_probe.txt then exit" \
  | <agent> -p --model <cheap>
```

→ If it asks "Allow write to /tmp/zforge_probe.txt? [y/n]" and blocks,
find the non-interactive flag. For Claude Code that's
`--dangerously-skip-permissions`. Add it to the agent's registered
`AgentSpec.args` so every spawn passes it.

## Mapping findings to zforge config

After filling a column:

1. `~/.zforge/registry.yaml`:
   ```yaml
   agents:
     <agent-name>:
       command: <binary>
       args:
         - <permanent flags from rows 2 + 5>
   ```

2. `~/.zforge/models.yaml` (global) or `<project>/.zforge/models.yaml`
   (local):
   ```yaml
   <agent-name>:
     spec: <model>
     plan: <model>
     code: <model>
     review: <model>
   ```

3. `~/.zforge/registry.yaml` `fallback_policy` (only edit if agent's
   failure signature differs from defaults):
   ```yaml
   fallback_policy:
     retryable_exit_codes: [2, 124, 137, <agent-specific>]
     retryable_stderr_patterns:
       - "(?i)rate.?limit"
       - "(?i)quota.?(exceeded|exhausted)"
       - "<agent-specific regex>"
   ```

## Per-agent notes

### claude (Claude Code)

- **Binary**: `claude` (verified at `/Users/zane/.local/bin/claude`)
- **API key env**: `ANTHROPIC_API_KEY` — IGNORED when OAuth creds cached
  (`~/.claude/credentials` wins). Verified 2026-05-25.
- **Cheapest model for verify**: `haiku`
- **Known flags**:
  - `-p <prompt>` — print mode, prompt as arg OR pipe stdin
  - `--model X` — short alias works (`haiku`, `sonnet`, `opus`)
  - `--dangerously-skip-permissions` — skip file-write prompts
  - `--output-format <stream|json|text>`
- **Stdin behavior**: ✅ pipe stdin to `claude -p` delivers prompt. zforge
  orchestrator current implementation works as-is.
- **Failure channel**: stdout (NOT stderr). Exit code reliable for
  invalid model; unreliable when OAuth cache masks env-var auth failure.
- **Special**: subagents loaded from `.claude/agents/*.md`; zforge
  symlinks these in `init`.

### codex (OpenAI Codex CLI)

- **Binary**: `codex` v0.133.0 (verified at `/opt/homebrew/bin/codex`)
- **Auth**: ChatGPT account via `codex login`. NOT env-var driven; no
  `OPENAI_API_KEY` to set. ChatGPT plan restricts model availability
  (e.g. `gpt-5.4-mini` rejected: "not supported when using Codex with a
  ChatGPT account").
- **Non-interactive subcommand**: `codex exec [PROMPT]` (alias `e`). Bare
  `codex` opens the interactive TUI; do NOT use from orchestrator.
- **Stdin behavior**: ✅ `echo "..." | codex exec` works. Prints
  `Reading prompt from stdin...` confirmation to stderr/stdout (mixed).
- **Model selection**:
  - `-m, --model X` — works. Verified.
  - `-p, --profile <name>` — profile from `~/.codex/config.toml`.
    **WARNING**: `-p` means PROFILE in codex but PROMPT-mode in claude.
    `model_args_for_agent` must not pass `-p` blindly across agents.
- **Profile config**: `~/.codex/config.toml`. zforge writes profiles
  `zforge_spec`, `zforge_testspec`, `zforge_plan`, `zforge_code`,
  `zforge_review` at `init --agent codex`.
- **Approval / sandbox**:
  - `-a never` — never ask for human approval (default for zforge worker)
  - `-s read-only|workspace-write|danger-full-access`
  - `--dangerously-bypass-approvals-and-sandbox` — full bypass (DANGEROUS)
- **Failure handling — CRITICAL ⚠️**:
  - **Exit code 0 EVEN ON API ERRORS**. Verified with invalid model.
  - Error printed as JSON to stdout: `ERROR: {"type":"error","status":N,...}`
  - zforge orchestrator MUST scan stdout for `^ERROR:` lines and treat as
    failure regardless of exit code. Currently does NOT — fix pending.
- **Verify**: copy `tests/real_claude_test.rs` pattern, gate by
  `ZFORGE_TEST_REAL_CODEX=1`.

### opencode

- **Binary**: `opencode`
- **API key env**: provider-dependent (Anthropic, OpenAI, OpenRouter, ...).
- **Known flags**: `run "<prompt>"`, `--model X`.
- **Subagents**: `.opencode/agents/*.md` — zforge symlinks at
  `init --agent opencode`.

### agy

- **Binary**: `agy`
- **Treated as standard CLI** by `model_args_for_agent` — `--model X`.
- **Verification pending** — fill matrix when adopting.

## Open contract questions

- Does any agent buffer the prompt unboundedly when reading stdin? (Large
  prompt deadlock test answers this.)
- What's the right `--timeout` story? zforge has none today; agents that
  hang on missing API keys can lock a worker forever. Likely fix: add
  per-phase wall-clock timeout in `FallbackPolicy` or `AgentSpec`.
- Do any agents emit machine-parsable JSON for failure (vs free-form
  stderr)? If yes, prefer JSON parsing over regex stderr matching.

## Updating this doc

Each PR that touches `src/orchestrator/model_args.rs`,
`src/orchestrator/spawn.rs`, or `FallbackPolicy` defaults must update the
matrix row(s) it affects. Stale matrix = subtle production failures.
