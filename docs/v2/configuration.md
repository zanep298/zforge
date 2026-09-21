---
title: zForge v2 Configuration
status: draft
document_type: normative-design
schema_version: 2
---

# zForge v2 Configuration

## Status

Normative design draft for configuration sources, precedence, validation,
versioning, secrets, profiles, immutable run snapshots, and format boundaries.

Related documents:

- [Product Direction](./product-direction.md)
- [Fleet and Human Attention](./fleet-and-human-attention.md)
- [Data Model](./data-model.md)
- [Execution DAG](./execution-dag.md)
- [Policy and Risk](./policy-and-risk.md)
- [Agents and Subagents](./agents.md)
- [Automatic Model Routing](./model-routing.md)
- [Skills](./skills.md)
- [Quality Gates](./quality-gates.md)
- [Workspace and Git](./workspace-and-git.md)

## Objective

Make every runtime decision reproducible from validated, explainable, immutable
configuration while keeping secrets outside ordinary configuration records.

## Principles

- Configuration is data, not executable code.
- Every field has one owner, type, default, and override policy.
- Unknown fields fail validation unless explicitly namespaced extensions.
- Effective configuration is explainable field by field.
- Runs pin an immutable configuration snapshot.
- Active runs never silently reload changed configuration.
- Secrets are referenced, not embedded.
- Local configuration cannot weaken mandatory policy.

V2 storage is local Markdown/YAML according to
[ADR-002](./decisions/002-file-backed-persistence.md), with one shared project
metadata store across task worktrees. Configuration may select supported local
store/artifact locations and retention limits, not a database backend or weaker
locking/durability behavior. JSON Schema validation does not require JSON files
or a database. Draft input edits become new accepted revisions through explicit
validated commands; runtime snapshots are not manually writable authority.

## Configuration Layers

Policy sources use the bounded YAML language accepted in
[ADR-004](./decisions/004-bounded-yaml-policy.md). Compilation rejects unknown
actions/facts/operators/obligations, invalid types and expression-limit violations.
The effective bundle pins language/compiler/evaluator and fact-registry versions.
Rule ordering cannot override a deny. Configuration templates may select trusted
facts but cannot interpolate shell, environment values or executable expressions
into policy conditions. Invalid candidate policy activation is reported visibly;
it never silently authorizes work through a weaker fallback.

From lowest-level default to highest-level permitted specialization:

```text
built_in -> installation -> organization -> user -> project -> profile -> task -> CLI request
```

This order does not imply unrestricted override. Every schema field declares one
of:

```text
fixed | strengthen_only | narrow_only | replace | append_unique | forbidden
```

Built-in safety and organization policy may mark fields non-overridable.

## Resolution Flow

```mermaid
flowchart LR
    Sources[Configuration Sources] --> Parse[Parse and Schema Validate]
    Parse --> Trust[Verify Source Trust]
    Trust --> Merge[Typed Restrictive Merge]
    Merge --> References[Resolve Versioned References]
    References --> Cross[Cross-Field Validation]
    Cross --> Secrets[Validate Secret References]
    Secrets --> Snapshot[Canonical Effective Snapshot]
    Snapshot --> Hash[Configuration Hash]
    Hash --> Run[Pin Into Run]
```

Resolution failure prevents state-changing work.

## Source Types

| Layer | Example location | Purpose |
|---|---|---|
| Built-in | compiled defaults | Safe minimum behavior and schema defaults |
| Installation | global zForge config | Runtime binaries, stores, shared adapters |
| Organization | signed/managed policy source | Mandatory policy, risk, retention, providers |
| User | user config directory | Non-security preferences and local runtime paths |
| Project | `.zforge/config.yaml` | Repository capabilities and project behavior |
| Profile | `.zforge/profiles/*.yaml` | Feature, fixbug, docs, spike specialization |
| Task | structured task override | Narrow task-specific choices |
| CLI request | allowlisted flags | One invocation override with provenance |

Environment variables are transport for explicitly declared fields or secret
references. Arbitrary environment enumeration is prohibited.

## Root Configuration

```yaml
schema_version: 2
project:
  id: project_01J...
  name: zforge
  repository_root: .
  default_branch: main

execution:
  max_parallel_nodes_per_run: 1
  max_attempts_per_node: 3
  default_timeout_seconds: 900

fleet:
  max_parallel_task_runs: 1
  allow_serial_execution: true
  continue_unaffected_on_block: true
  scheduling_policy_ref: fleet-quality-first@1

human_attention:
  policy_ref: attention-default@1
  consolidate_questions: true
  prefer_approved_context: true

workspace:
  strategy: git_worktree
  root: .zforge/workspaces
  preserve_on_failure: true

agents:
  registry_ref: agents@2
  routing_policy_ref: routing@3

models:
  catalog_sources:
    - installation
    - project
  selection_default: auto
  catalog_staleness_seconds: 3600
  shadow_routing: false

skills:
  registries:
    - project
    - installed

gates:
  catalog_ref: gates@4
  default_groups:
    feature: feature-default@2

risk:
  rules_ref: risk-rules@4

policy:
  bundle_sources:
    - built-in-safety
    - organization-policy@4
    - project-policy@2

budget:
  policy_ref: budget-default@2

delivery:
  default_boundary: pull_request
  adapter_ref: git-host@2

retention:
  policy_ref: retention-default@1
```

## Configuration Domains

The root schema delegates to versioned domains:

```text
project | execution | workspace | git | agents | models | skills | tools |
gates | risk | policy | budget | artifacts | evidence | delivery |
security | retention | telemetry | evaluation | context |
fleet | human_attention
```

Each domain has a schema owner and cannot reuse another domain's field for a
different meaning.

## Typed Merge Semantics

- Scalars use the field's declared override mode.
- Objects merge only declared child fields.
- Ordered arrays replace unless declared appendable.
- Set-like arrays use canonical unique merge and stable sort.
- Maps of named definitions merge by stable key and validate version conflicts.
- `null` is not delete unless schema explicitly defines tombstone semantics.
- Type changes are errors, not coercions.
- Relative paths resolve against the source document's declared base, then are
  normalized and validated against allowed roots.

Example explanation:

```yaml
field: execution.max_attempts_per_node
effective_value: 2
source: task:SSO-102
merge_mode: narrow_only
overridden_values:
  - source: project
    value: 3
constraints:
  organization_maximum: 4
```

## Configuration Snapshot

Every run pins the fully resolved snapshot, not a list of mutable source paths.

```yaml
schema_version: 2
record_type: configuration_snapshot
record_id: config_snapshot_01J...
revision: 1
status: immutable
project_id: project_01J...
task_id: SSO-102
source_refs:
  - config-source:builtin@2.0.0
  - config-source:project@sha256:abc...
resolved_references:
  policy_bundle: policy_bundle_01J...@1
  gate_catalog: gate-catalog@4
  agent_registry: agents@2
effective_configuration_artifact_ref: artifact_01J...
redacted_explanation_artifact_ref: artifact_02J...
configuration_hash: sha256:effective-config...
resolver_version: 2.0.0
created_at: 2026-08-25T00:30:00Z
created_by:
  actor_type: system
  actor_id: configuration-resolver
updated_at: 2026-08-25T00:30:00Z
content_hash: sha256:config-snapshot-record...
provenance_refs: []
```

Snapshots are immutable. A configuration change creates a new snapshot and,
when execution semantics change, a new plan revision or run.

## Versioned References

Definitions such as roles, skills, gates, policies, prompts, price plans, and
adapters are referenced by stable ID, version, and content hash.

Floating references such as `latest` MAY be accepted in source configuration but
MUST resolve to pinned references before plan compilation or execution.

Resolution records registry identity, selected version, trust level, and hash.

## Profiles

Profiles specialize normal behavior without encoding a fixed pipeline.

```yaml
schema_version: 2
profile_id: fixbug
inherits:
  - base-executable@2
requirements:
  require_reproducer: true
evidence:
  require_regression_evidence: true
execution:
  preferred_roles:
    - requirements
    - diagnostic
    - implementation
    - review
```

The graph compiler combines profile, risk, policy, and repository facts. A
profile cannot remove a mandatory control.

## Repository Capabilities

Project configuration declares detected and confirmed capabilities:

```yaml
repository_capabilities:
  languages:
    - rust
  package_managers:
    - cargo
  commands:
    format: [cargo, fmt, --check]
    test: [cargo, test]
  integration_tests: true
  git_worktree: true
  generated_paths:
    - src/generated/**
  protected_paths:
    - .github/workflows/**
```

Auto-detection produces proposed facts with provenance. Security-sensitive or
ambiguous capabilities require confirmation or policy validation.

## Context, Fleet, and Human Attention

Context configuration declares eligible product and repository knowledge sources,
trust, staleness rules, selection limits, and update authority. It never grants a
source authority merely because it is local.

Fleet configuration controls task-level concurrency, continue-on-block behavior,
shared resource groups, scheduling policy, and integration strategy. A fleet
profile MAY set concurrency to one; serial execution is fully supported.

Human-attention configuration controls question consolidation, permitted
reversible assumptions, and interrupt-versus-queue presentation. It cannot grant product decisions, approval authority, secrets,
permissions, or external effects.

## Agent, Model, Prompt, and Skill Configuration

Configuration keeps these separate:

- role definition: engineering responsibility and output contract;
- runtime agent: executable adapter and capabilities;
- model definition: provider identity, context and usage capabilities;
- prompt template: versioned runtime instruction;
- skill: versioned methodology/domain package;
- assignment routing: deterministic mapping under policy and budget.

Assignments pin resolved values. Provider fallback uses an explicit compatible
set through a fresh attempt/decision/assignment; it never selects an arbitrary
installed command or changes the model of an authorized assignment.

Model selection supports:

```text
fixed | agent_scoped_auto | fully_auto
```

Provider model identifiers belong to trusted, versioned catalog sources rather
than built-in router defaults. `fixed` pins an explicit runtime-agent/model
pair. `agent_scoped_auto` selects within one adapter such as Codex or Claude
Code. `fully_auto` may select the pair. All modes remain subject to eligibility,
policy, independence, availability, and budget validation.

Routing configuration owns:

- trusted model-catalog sources and staleness limits;
- quality floors, evidence strata, confidence rules, and cold-start behavior;
- runtime-agent scope and permitted provider fallback;
- reasoning-profile mappings supported by each adapter;
- availability fallback and quality-escalation policy;
- shadow-routing and bounded low-risk exploration;
- explicit native-v2 fixed-model selections.

The effective configuration snapshot pins routing policy and catalog source
references. Each assignment separately pins the concrete catalog snapshot and
routing decision used for that attempt.

## Gate Configuration

Gate definitions use executable plus argument arrays, never shell strings by
default. Working directory, environment allowlist, parser, timeout, required
artifacts, and input dependency rules are explicit. Detailed semantics belong in
the quality-gates document.

## Secrets

```yaml
delivery:
  credential_ref:
    provider: environment
    key: ZFORGE_GIT_TOKEN
```

Rules:

- Secret values never enter configuration snapshots, hashes, logs, errors, or
  generated Markdown.
- A snapshot stores only provider, key identity, version metadata when available,
  and required access classification.
- Secret existence and access are validated at use time under policy.
- Empty or missing secrets are errors for required operations.
- CLI flags cannot carry raw secrets unless a dedicated secure input mechanism
  explicitly supports it.

## CLI Overrides

Only schema-declared non-secret fields may be overridden. Every override records
actor, command, time, original value, new value, and merge authority.

Examples normally allowed:

- output format;
- dry-run;
- bounded timeout reduction;
- lower concurrency;
- narrower task selection.

Examples normally denied:

- disabling mandatory gates;
- expanding filesystem/network/secret permission;
- increasing hard budget without approval;
- changing accepted contract or risk through a flag;
- bypassing reviewer independence.

## Reload and Change Detection

Long-running workers MAY reload discovery metadata for new work but continue
using the run's pinned snapshot. A changed source emits a configuration-change
observation. Policy decides whether active runs continue, block, revoke unsafe
grants, or require a new run.

Emergency revocation is a policy event, not silent configuration reload.

## Validation

Validation stages:

1. Syntax and supported schema version.
2. Unknown field and extension namespace validation.
3. Type, range, enum, path, and format validation.
4. Source trust and override authority.
5. Typed merge constraints.
6. Reference existence, compatibility, and hash validation.
7. Cross-domain invariants.
8. secret-reference shape without reading values.
9. canonical serialization and snapshot hash.

Warnings never substitute for errors on security, policy, state, or reproducibility fields.

## Failure Semantics

```text
CONFIG_NOT_FOUND | CONFIG_SCHEMA_UNSUPPORTED | CONFIG_UNKNOWN_FIELD |
CONFIG_TYPE_MISMATCH | CONFIG_OVERRIDE_FORBIDDEN | CONFIG_REFERENCE_MISSING |
CONFIG_REFERENCE_HASH_MISMATCH | CONFIG_PATH_OUTSIDE_ROOT |
CONFIG_SECRET_EMBEDDED | CONFIG_CROSS_FIELD_INVALID | CONFIG_SNAPSHOT_STALE |
CONFIG_MODEL_CATALOG_UNTRUSTED | CONFIG_ROUTING_POLICY_INVALID
```

Errors include source identity and safe field path but redact sensitive values.

## Storage Layout

```text
.zforge/
├── config.yaml
├── profiles/
├── roles/
├── prompts/
├── gates/
├── policies/
├── models/
│   ├── catalogs/
│   └── routing-policies/
├── skills/
└── tasks/
    └── SSO-102/
        └── runs/
            └── run_01J.../
                └── configuration-snapshot.yaml
```

## Rust Modules

```text
src/configuration/
├── model.rs
├── source.rs
├── schema.rs
├── merge.rs
├── reference.rs
├── resolve.rs
├── explain.rs
├── snapshot.rs
├── secret_ref.rs
├── validate.rs
└── version.rs
```

## Native-v2 Configuration Boundary

Per [ADR-005](./decisions/005-native-v2-no-migration.md), v2 accepts only native-v2 configuration. No v1
adapter, migration command, deprecated-field alias or implicit legacy default is
required. Unsupported formats fail validation without rewriting source files.
The user handles any old configuration manually. Native schema versioning,
reference pinning and immutable run snapshots remain required.

## Testing Strategy

Tests cover every merge mode, layer precedence, forbidden weakening, unknown
fields, extensions, type conflicts, arrays/maps, path resolution, floating
reference pinning, hash mismatch, secret exclusion, CLI override allowlists,
cross-field invariants, snapshot determinism, reload behavior, and unsupported-format rejection.

Golden fixtures MUST prove field-level explanation and canonical snapshot hashes.

## Acceptance Criteria

- [ ] Every configuration field has a type, owner, default, and override mode.
- [ ] Unknown fields and unsupported versions fail validation.
- [ ] Mandatory policy cannot be weakened by local configuration.
- [ ] Effective values are explainable back to exact sources.
- [ ] Runs pin immutable canonical configuration snapshots.
- [ ] Floating references resolve and pin before execution.
- [ ] Secret values never enter snapshots or normal artifacts.
- [ ] CLI overrides are allowlisted, bounded, and auditable.
- [ ] Active runs do not silently reload changed semantics.
- [ ] Fleet configuration supports fully serial quality-first execution.
- [ ] Human-attention configuration changes presentation, not authority.
- [ ] Context source eligibility and staleness rules are pinned and explainable.
- [ ] Model catalog sources, routing policy, and cold-start behavior are typed and versioned.
- [ ] Provider model names are trusted catalog or validated explicit configuration, not hidden core defaults.
- [ ] Unsupported legacy configuration is rejected without changing original files.

## Project, Delivery and Local Environment Profiles

Per-project profiles pin approved product knowledge, conventions, role policies,
build/test commands, completion rules and supported task classes. Organization
templates may be shared; project secrets, fixtures and mutable resources are
isolated. Discovery produces proposed facts, while onboarding executes explicit
granted checks and records a `ProjectReadinessReport`.

`EnvironmentProfile` is trusted versioned configuration with adapter identity,
toolchain/image pins, fixtures, resource requirements, test-only credential refs
and setup/readiness/reset/teardown command refs. See
[Project Onboarding and Local Testing](./project-onboarding-and-local-testing.md).
Runtime resource IDs belong to leases, not reusable configuration. Each adapter
publishes `enforced | detect_after | unsupported` guarantees; required prevention
cannot be satisfied by post-run detection.

Task contracts pin `requested_result` and `documentation_impact` according to
[Development Handoff and Review](./delivery-and-review.md). Profiles supply
defaults only; they cannot silently narrow requested implementation into a plan.
Native task contracts accept only `requested_result.implementation_boundary`;
the old top-level task `delivery_boundary` alias is unsupported. This does not
remove the separate batch-level `delivery_boundary` policy setting.

Application deployment, production endpoints/credentials and production debugging
are built-in v2 denials, not enablement flags. Development Git/CI actions require
proof that triggered workflows have no deployment or production effects; unknown
capability falls back to a local handoff.
