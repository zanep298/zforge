---
title: ADR-004 Bounded YAML Policy Rules
status: accepted
document_type: architecture-decision
schema_version: 2
---

# ADR-004 — Bounded YAML Policy Rules

## Decision

The product owner accepted YAML rules with a small fixed condition language and
`allow | deny | require_approval` outcomes. Rules never execute scripts; unknown
fields/operators and invalid inputs fail closed. This defines the initial
language and acceptance contract, not an implemented engine.

YAML is parsed as data and validated using JSON Schema from ADR-001. Rust
compiles/evaluates the typed tree. No general-purpose expression engine, external
policy service, embedded scripting language or code generator is required.

## Rule Shape and Applicability

A rule contains `id`, `actions`, `when`, `decision`, `obligations` and
`reason_code`; only declared namespaced extensions may add metadata. IDs are
unique within a versioned source; audit qualifies them with source ID/version.
`actions` is a nonempty list from the protected-action registry. Select applicable
rules by action before resolving condition facts.

```yaml
rules:
  - id: high-risk-completion
    actions: [complete_run]
    when:
      fact: risk.overall_level
      op: in
      value: [high, critical]
    decision: require_approval
    obligations: [independent_review, evidence_coverage_100]
    reason_code: HIGH_RISK_COMPLETION
```

This is a rule fragment, not a complete policy-bundle fixture.

## Closed Condition Grammar

An expression contains exactly one of these shapes:

| Shape | Meaning |
|---|---|
| `all_of: [expression, ...]` | Every child is true; nonempty array |
| `any_of: [expression, ...]` | At least one child is true; nonempty array |
| `not: expression` | Negate one valid Boolean result |
| `fact: name`, `op: operator`, `value: literal` | Typed fact comparison |

Comparison operators are `equals | in | lt | lte | gt | gte`. The versioned fact
registry declares string, Boolean or signed 64-bit integer facts. `equals`
compares the same type; `in` takes a nonempty list of that type; ordering accepts
integers only. No implicit coercion, floating-point values or overflow is allowed.
Numeric facts declare units such as bytes, milliseconds or cost microunits.

No regex, glob, interpolation, environment expansion, function calls, filesystem,
shell, network, templates or model calls exist in conditions. Deterministic
validators perform path/host matching and expose typed facts such as
`scope.within_grant`. Agent output cannot populate trusted facts by name alone.

## Invalid Inputs and Bounds

- Validate the entire bundle's rule shapes, action/fact/operator/obligation names,
  literal types and limits before activation. Invalid policy cannot issue grants;
  failed activation is visible, never a silent fallback to permissive policy.
- Resolve every fact referenced by action-applicable rules before Boolean
  evaluation. Missing, null, stale, untrusted or wrong-type facts deny admission
  with a stable diagnostic, not `false`, a guessed default or an approval bypass.
- `not` cannot turn an unknown value into permission. A true `any_of` branch
  cannot mask invalid facts elsewhere in an applicable condition. Unrelated
  action rules do not demand their facts for the current request.
- Reject duplicate YAML keys, custom object tags, recursive aliases and unknown
  protected fields before admitting policy data.
- Initial ceilings are 256 KiB per source, 1,000 rules per bundle, 32 nested
  expression levels, 10,000 expression nodes per bundle, 256 literals per `in`
  and 4 KiB per string literal. Exhaustion yields no grant. These are initial
  design limits, not measured performance claims; changing them requires a
  versioned evaluator change and conformance tests, not a task override.

Evaluation uses an immutable trusted fact snapshot and performs no side effects.
Controllers recheck expiry, lease ownership, resource availability and evidence
freshness immediately before action admission under the applicable lock.

## Combination and Approval

Rule order does not grant priority. Combine all matched rules as follows:

1. Any matching `deny` denies the action, regardless of allow/approval rules.
2. Accumulate registered obligations from matching allow/approval rules.
3. Unmet mandatory obligations deny admission with reasons. Approval cannot
   replace passing tests, current evidence, valid scope or deterministic facts.
4. An unresolved matching `require_approval` requests scoped authority.
5. With obligations satisfied and required approvals valid, a matching `allow`
   or satisfied conditional-approval rule may issue the exact bounded grant.
6. No authorizing match means deny. Safe reads require explicit scoped built-in
   allow rules, not an implicit permissive default.

Deny/allow overlap is valid and resolved by this precedence. Duplicate identities
and malformed definitions are compile errors; incompatible combined obligations
deny admission with diagnostics. They never create permission.

Approval binds authorized actor, exact request/input hash, policy revision,
matched approval-rule identities and expiry. Re-evaluation recognizes a valid
approval instead of prompting forever; changes to scope/policy or expiry require
a new decision. Agent output cannot approve. Exceptions require explicit,
versioned, audited handling; they cannot bypass a matching deny in the effective
bundle or built-in production/deployment prohibitions.

## Audit and Foundation Acceptance

Bundle and decision records pin `language_version`, compiler/evaluator versions,
the trusted fact registry and source hashes. Decisions retain matched rule
identities, fact provenance, reasons, obligations, approvals and exact grant
constraints without leaking secrets. Same pinned inputs produce the same semantic
outcome/reasons regardless of rule order; IDs/timestamps may differ.

Foundation tests cover each operator/type, unknown/missing/null/stale facts,
malformed YAML, bounds, no-match denial, deny precedence, obligation conflicts,
rule reordering, approval reuse/expiry/amendment and emergency revocation. Add
canonical expression/rule/fact-registry schemas with valid/invalid fixtures.
Verify that allowed actions are actually constrained by the Docker/controller
adapter: a policy decision alone is not enforcement evidence.

Related: [Policy and Risk](../policy-and-risk.md),
[Configuration](../configuration.md), [Data Model](../data-model.md),
[Implementation Plan](../../../requirements/autonomous-software-engineering-implementation-plan.md).
