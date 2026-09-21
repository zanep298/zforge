---
title: ADR-002 Markdown and YAML File-Backed Persistence
status: accepted
document_type: architecture-decision
schema_version: 2
---

# ADR-002 — Markdown and YAML File-Backed Persistence

## Status and Scope

The product owner selected Markdown/YAML storage instead of a database on
2026-09-07. This ADR defines the selected storage direction and the consistency
contract to implement and test. It does not claim that recovery or platform
durability has already been implemented or proven.

V2 uses local files, not SQLite or another embedded/remote database. JSON Schema
remains the validation contract from [ADR-001](./001-canonical-schema.md); YAML
records are parsed into the schema's data model before validation and hashing.

## File Ownership

| File family | Role |
|---|---|
| Markdown | Human-readable task input, specification, plan, test cases, rationale and review |
| YAML records | Typed contracts, attempts, evidence metadata, grants, budgets and leases |
| Immutable YAML event batches and commit manifests | Authoritative accepted lifecycle history and transaction boundaries |
| `task.yaml`, `run.yaml`, `snapshot.yaml`, indexes and generated Markdown | Current views rebuilt from accepted records and committed history |
| Artifact files | Patches, logs, images and test reports in their native formats, with hashes and YAML metadata |

Authoritative records and their task-local views remain in the project directory.
A shared project commit directory coordinates related changes without moving the
task content into an opaque store. Runtime history is YAML, not an authoritative
JSONL stream. JSON/JSONL may remain native tool-output artifacts or exports.

Editing a task's Markdown input is supported: an explicit import/amend command
validates it and records a new accepted revision under policy. Directly editing a
snapshot, evidence result or generated summary does not approve a task or advance
runtime state. Do not silently ingest arbitrary manual changes during a run.

## Local Layout

```text
.zforge/
├── store/
│   ├── write.lock
│   ├── commits/                 # immutable <ordinal>-<transaction-id>.yaml
│   ├── staging/                 # uncommitted temporary files
│   └── index.yaml               # rebuildable commit/idempotency index
├── tasks/
│   └── AUTH-101/
│       ├── task.md
│       ├── task.yaml            # current projection, with commit cursor
│       ├── snapshot.yaml
│       ├── events/              # immutable YAML batches, committed by manifest
│       ├── contracts/           # immutable YAML revisions
│       ├── artifacts/
│       └── runs/
│           └── run_01J.../
│               ├── run.yaml
│               ├── events/
│               ├── snapshot.yaml
│               ├── attempts/
│               └── evidence/
├── batches/                    # YAML records, events and projections
└── projects/                   # project-scoped YAML records and events
```

Paths index records; IDs and hashes identify them. All metadata participating in
one commit lives on the same supported local filesystem. The store resolves to
one shared project location, not a separate mutable copy in each task worktree.
Network filesystems and live cloud-synchronized stores are not initially supported.

## Writer and Transaction Boundary

One short-lived, process-safe project writer lock serializes metadata commits.
It does not serialize agent work, tests or the full lifetime of tasks. Only
controllers write authoritative state; agents operate in their scoped workspaces.
The lock path is stable, excluded from agent writes, and never deleted/replaced
to take over a live owner. Timestamps alone do not prove writer/process death.

A command may update several task/run/project streams in the same project. Its
`FileCommitRequest` contains expected sequences for every affected stream,
immutable record revisions, event batches and outbox intents. Validation and
budget/resource reservation occur under the writer lock against committed truth.

The immutable `file_commit` YAML manifest includes transaction ID, project-local
commit ordinal, previous commit identity/hash, command identity, idempotency scope
and key, input hash, expected/result sequences for affected streams, referenced
record/event/outbox file hashes and the original command result. The manifest
uses the common schema/version/integrity conventions and is a storage envelope,
not a replacement domain event. Register its schema and fixtures in foundation.

## Commit Protocol

1. Acquire the project writer lock; reconcile the last committed cursor. Check
   expected sequences, idempotency, authority, current grants and resource limits.
2. Write new immutable records, event batches and outbox intents to temporary
   files. Validate schemas/hashes, flush file contents and publish their immutable
   paths without overwriting accepted history; flush affected directories.
3. Write one complete commit manifest to a temporary file, flush it, then rename
   it to its unique final name under `store/commits/` and flush that directory.
   Readers/dispatchers admit the entire referenced batch only through this manifest.
4. Acknowledge success only after the commit durability barrier succeeds. Release
   the writer lock before executing agents, tests or external actions.
5. Update current YAML/Markdown projections with atomic temporary-file replacement.
   Projection updates may lag; each carries its committed cursor. Queries needing
   current truth replay committed manifests rather than trusting a stale index.

Readers capture a committed cursor under the store lock and then read immutable
history at that cursor. They ignore files merely present in task event/record
directories unless referenced by committed manifests. No partial batch is visible.
Read-only status may compute current state without rewriting projections; cache
repair is an explicit maintenance operation.

Atomic rename is not by itself a durability guarantee. Platform implementation
must demonstrate the required file/directory flush and lock behavior, including
crash tests. Unsupported guarantees block promotion on that filesystem/platform;
they are not silently replaced by best-effort writes.

## Recovery and External Effects

- Before manifest publication: staged/published-but-unreferenced files are not
  accepted state. Retry may reuse verified immutable bytes; cleanup happens under
  the store lock after proving files are unreferenced.
- After manifest publication but before acknowledgement: recovery validates the
  complete manifest chain and payloads, establishes durability, and replays the
  original outcome for the same key/input. It does not execute the command twice.
- After commit but before projection: rebuild the view from committed history.
- Missing/corrupt committed metadata or sequence gaps block affected work for
  repair; never truncate committed YAML history to manufacture a valid state.
- Outbox intents commit with the state that authorizes them. Dispatch happens
  after durable commit and remains at least once, with stable action IDs,
  idempotency and reconciliation of uncertain spawn/push/PR outcomes.
- Resource lease expiry does not prove old processes stopped. Fence/reconcile
  ownership before allocating resources to another task.

An idempotency index is a cache over committed manifests. The same scoped key
and same input return the original result; a different input is rejected.

## Cross-Project Work, Backup and Retention

There is no implicit transaction across project stores. Cross-project batches
use explicit dependency events and idempotent reconciliation. A host-global
resource such as a device has one authoritative YAML resource store using the
same lock/commit protocol; projects do not independently reserve it. Cross-store
reservations use durable allocation IDs and reconcile leaks before reuse, without
holding one store lock while acquiring another.

Backup/export captures a committed cursor and its referenced files under a
consistent snapshot procedure. Git is not a runtime lock or transaction protocol.
Snapshots/indexes may be rebuilt; committed metadata is immutable. Existing
artifact retention/tombstone policies still apply, and exports disclose missing
retained bytes. Copying only `task.md` is not a complete resumable task export.

## Required Tests and Consequences

Foundation tests inject failure before/after each publish/flush boundary; verify
multi-stream event/record/outbox atomic visibility, idempotent retry, corruption
handling, projection rebuild, live-owner fencing, and concurrent budget/resource
reservation. Include two independent same-project tasks whose agents/tests run
in parallel while metadata commits serialize.

Files remain inspectable and portable without database tooling. zForge must own
the commit/recovery protocol, indexes, locking and its tests. Initial metadata
throughput is bounded by one writer per store; measure it before adding a more
complex locking scheme. No database fallback is introduced in v2.

Related: [Data Model](../data-model.md), [State and Events](../state-and-events.md),
[Workspace and Git](../workspace-and-git.md),
[Implementation Plan](../../../requirements/autonomous-software-engineering-implementation-plan.md).
