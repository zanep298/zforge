# Requirements Index

Specification and task breakdown for the **Task-Level Agent Assignment & Global Orchestration** feature.

## Master spec

- [global-orchestration.md](./global-orchestration.md) — full requirements, schemas, acceptance criteria.

## Implementation tasks

Three PRs, sequenced. Each PR depends on the previous. Tasks within a PR can be executed in roughly numbered order (some are parallelizable — see each task's `Depends on` field).

### PR 1 — Global Registry + Auto-Register on Init

Foundation. Registry CRUD, `zforge project` subcommands, auto-register from `init`. No FSM/state changes.

- [pr1-global-registry/01-add-dependencies.md](./pr1-global-registry/01-add-dependencies.md)
- [pr1-global-registry/02-registry-schema.md](./pr1-global-registry/02-registry-schema.md)
- [pr1-global-registry/03-registry-io.md](./pr1-global-registry/03-registry-io.md)
- [pr1-global-registry/04-registry-validate.md](./pr1-global-registry/04-registry-validate.md)
- [pr1-global-registry/05-auto-register.md](./pr1-global-registry/05-auto-register.md)
- [pr1-global-registry/06-cli-project-subcommands.md](./pr1-global-registry/06-cli-project-subcommands.md)
- [pr1-global-registry/07-init-integration.md](./pr1-global-registry/07-init-integration.md)
- [pr1-global-registry/08-status-global.md](./pr1-global-registry/08-status-global.md)
- [pr1-global-registry/09-mcp-registry-tools.md](./pr1-global-registry/09-mcp-registry-tools.md)
- [pr1-global-registry/10-integration-tests.md](./pr1-global-registry/10-integration-tests.md)

### PR 2 — Task-Level Agent Assignment

Extends `.state.yaml` schema with agent fields. Adds `--agent` / `--fallback` flags to `task import`. No orchestrator yet.

- [pr2-task-agent-assignment/01-state-schema-extension.md](./pr2-task-agent-assignment/01-state-schema-extension.md)
- [pr2-task-agent-assignment/02-task-import-flags.md](./pr2-task-agent-assignment/02-task-import-flags.md)
- [pr2-task-agent-assignment/03-mcp-task-import-extension.md](./pr2-task-agent-assignment/03-mcp-task-import-extension.md)
- [pr2-task-agent-assignment/04-integration-tests.md](./pr2-task-agent-assignment/04-integration-tests.md)

### PR 3 — Fallback Orchestration

Phase commands route through orchestrator. Retryable failures swap to fallback agent.

- [pr3-fallback-orchestration/01-fake-agent-test-stub.md](./pr3-fallback-orchestration/01-fake-agent-test-stub.md)
- [pr3-fallback-orchestration/02-orchestrator-spawn.md](./pr3-fallback-orchestration/02-orchestrator-spawn.md)
- [pr3-fallback-orchestration/03-fallback-decision.md](./pr3-fallback-orchestration/03-fallback-decision.md)
- [pr3-fallback-orchestration/04-history-append.md](./pr3-fallback-orchestration/04-history-append.md)
- [pr3-fallback-orchestration/05-run-phase.md](./pr3-fallback-orchestration/05-run-phase.md)
- [pr3-fallback-orchestration/06-cli-route-through-orchestrator.md](./pr3-fallback-orchestration/06-cli-route-through-orchestrator.md)
- [pr3-fallback-orchestration/07-integration-tests.md](./pr3-fallback-orchestration/07-integration-tests.md)

## Status legend

Each task file ends with a status block:

```yaml
status: todo       # todo | in_progress | blocked | done
owner: ""          # filled when picked up
pr: ""             # PR number once opened
```

Update inline as work progresses.

## Resolved decisions

1. **Test home override env var name** — `ZFORGE_HOME` (locked in PR1-02 `paths.rs`).
2. **`status --global` parallelism** — one thread per project + `recv_timeout`; serial enumeration. Revisit after benchmark at >50 projects.
3. **`auto_register` rename behavior** — silent + info log; preserves `registered_at`, `registered_by`, `agent_overrides`.
