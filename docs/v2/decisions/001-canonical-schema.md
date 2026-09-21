---
title: ADR-001 Canonical Schema Ownership
status: accepted
document_type: architecture-decision
schema_version: 2
---

# ADR-001 — Canonical Schema Ownership

## Status and Scope

Accepted by the product owner on 2026-09-07. This accepts the ownership and
validation decision, not every draft field or the implementation of schemas/CI.
Persistence, sandbox and policy-language semantics are separate decisions.
[ADR-005](./005-native-v2-no-migration.md) excludes v1 migration and compatibility
work. No runtime changes are authorized or implied by this ADR.

## Decision

Version-controlled JSON Schema under `schemas/v2/` is the canonical contract for
native-v2 persisted and exchanged structured data. Rust types implement domain
behavior and MUST serialize/deserialize compatibly with those schemas. Markdown
explains intent, invariants and examples; it is not a competing wire contract.

Schema validation does not replace domain validation. Reference resolution,
ownership, authority, state-transition guards, evidence freshness and actual
diff reconciliation remain deterministic Rust responsibilities.

### References and Extensibility

- Shared reference definitions live under `schemas/v2/common/`; record-specific
  schemas reuse them instead of inventing incompatible reference shapes.
- Revisioned inputs affecting execution/evidence pin the record revision and
  content hash. Immutable inputs pin identity and content hash. Artifact bytes
  retain their separate `artifact_hash` integrity rule.
- Logical/latest references are discovery or presentation inputs only. Resolve
  them to pinned references before an invocation or acceptance decision.
- Compact references in explanatory prose do not define additional wire formats.
- Execution-controlling and security-sensitive records reject unknown fields.
  Extensions are explicit, namespaced `extensions` objects; they cannot override
  core fields, grant authority or silently weaken validation.

### Evidence Subjects

The canonical evidence field is `subjects`, containing typed identities:

```yaml
subjects:
  - type: acceptance_criterion
    id: ac_01J...
```

This is an illustrative field fragment, not a complete schema-valid fixture.
The type is checked against the evidence subject registry and the referenced
entity. Subject identity does not itself prove freshness: evidence input hashes
and pinned owning records establish the exact version of the claim.

`subject_refs` is not a native-v2 alias. Producers, validators and consumers use
`subjects`; reject the obsolete field rather than supporting two sources of
truth. There is no assumed persisted-v2 migration merely because a draft example
previously used the old name.

## Implementation and CI Contract

For each implemented record schema, provide minimum-valid, full-valid and
invalid fixtures. Invalid cases include missing required fields, incorrect
types/enums, unknown protected fields and invalid reference shapes.

CI MUST check:

1. Valid fixtures pass the canonical schema and Rust deserialization.
2. Rust serialization produces schema-valid data and round trips without losing
   meaningful values or changing their interpretation.
3. Invalid structural fixtures fail schema validation; matching Rust boundary
   validation does not accept them into authoritative state.
4. Separate domain fixtures reject structurally valid but semantically invalid
   references, transitions, grants or evidence.
5. Complete record examples in documentation come from validated fixtures or are
   checked against the same schema. Explanatory fragments and placeholder sketches
   are explicitly identified and cannot be counted as validated record fixtures.

Schema, Rust types, fixtures and affected documentation change together. Runtime
work for a record cannot be declared complete while its consumers use a different
shape. Existing draft sketches must be reconciled when that record is implemented.

No code generator is required initially. The schema dialect, validator library
and fixture harness are pinned and tested during foundation implementation;
those choices cannot change the ownership established here.

## Trade-offs

Maintaining schemas and handwritten Rust types adds compatibility-test work.
It provides one contract for independently implemented producers/consumers and
CLI/MCP/persistence adapters. Rust-only ownership and prose-only contracts were
not selected; generated bindings may be evaluated later without changing this
decision.

## Related Documents

- [Data Model](../data-model.md)
- [Artifacts and Traceability](../artifacts-and-traceability.md)
- [Implementation Plan](../../../requirements/autonomous-software-engineering-implementation-plan.md)
