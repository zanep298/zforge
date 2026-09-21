---
title: ADR-005 Native v2 Without v1 Migration
status: accepted
document_type: architecture-decision
schema_version: 2
---

# ADR-005 — Native v2, No v1 Migration

## Decision

The product owner is building zForge for personal use and will handle any old
data manually. V2 targets native-v2 records, configuration and interfaces only.
No v1 migration or backward-compatibility engineering is required.

This is an accepted implementation-scope decision, not a completed runtime change.

## Included and Excluded

- Implement one native-v2 runtime and its validated Markdown/YAML file store.
- Do not build a v1 state/configuration/cost reader, migration command, import
  adapter, phase-agent alias layer or dual-engine task router.
- Preserving v1 CLI/MCP signatures, defaults and model mappings is not a v2
  acceptance requirement. Native fixed-model overrides remain supported.
- The owner may manually recreate tasks, context and configuration. Ordinary
  source-document intake is supported, but is not a legacy-state converter.
- Unsupported legacy stores and configuration fail validation with actionable
  diagnostics. Initialization/loading must not silently convert, overwrite or
  delete old data; native setup needs a valid, non-conflicting location.
- Manually recreated tasks pass normal native contract, policy, approval and
  evidence checks. Old completion labels and logs do not automatically become
  trusted v2 evidence.

## Boundaries That Remain

Native schema versioning, immutable snapshots, event evolution and schema/Rust
conformance remain required. These are not product-v1 compatibility work.

Project API/schema compatibility checks and local database-migration/rollback
tests remain supported engineering capabilities. This decision only excludes
migration of zForge's own v1 state and configuration.

Historical v1 documentation and existing code may remain in the repository. This
decision neither requires a supported v1 runtime path in v2 nor authorizes
deleting or rewriting existing v1 files. Tested helpers may be reused if they
satisfy native-v2 contracts.

## Acceptance

- The first native-v2 task runs without any legacy state/configuration adapter.
- Unsupported old input is rejected without modifying source files.
- New-task source intake cannot inherit completion, approval or evidence authority.
- No delivery milestone depends on v1 alias support, migration tools or dual engines.
- Normal native schema-version and provenance tests still pass.

Related: [Data Model](../data-model.md), [Configuration](../configuration.md),
[CLI and MCP](../cli-and-mcp.md),
[Implementation Plan](../../../requirements/autonomous-software-engineering-implementation-plan.md).
