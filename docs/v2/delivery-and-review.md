---
title: zForge v2 Development Handoff and Review
status: draft
document_type: normative-design
schema_version: 2
---

# zForge v2 Development Handoff and Review

## Scope

Define how a technical lead delegates a task or large outcome and receives
concise, verifiable development results. This is a design target for v2.

Related documents:

- [Product Direction](./product-direction.md)
- [Data Model](./data-model.md)
- [State and Events](./state-and-events.md)
- [Artifacts and Traceability](./artifacts-and-traceability.md)
- [Project Onboarding and Local Testing](./project-onboarding-and-local-testing.md)
- [CLI and MCP](./cli-and-mcp.md)

Delivery means preparing a plan/task graph, local change, or reviewable pull
request. Application deployment to any environment, production access,
production credentials, and production debugging are outside v2. Starting a
disposable local test stack or installing a test build on an allocated local
device belongs to testing, not application release.

## Requested Result

Intake records two independent fields:

```yaml
requested_result:
  mode: through_phase
  target_phase_key: callback-and-session
  implementation_boundary: pull_request
```

`mode` is `plan_only | through_phase | full_implementation`.
`target_phase_key` is required only for `through_phase` and identifies a stable
key in the accepted decomposition. `implementation_boundary` is
`local_changes | local_commits | pull_request`; it is null for `plan_only`.

When intent is clear, intake infers and records the mode. Otherwise it asks a
targeted question. It MUST NOT reinterpret a request to implement a feature as
a planning-only request merely because implementation is difficult.

| Mode | Accepted deliverable | What completion means |
|---|---|---|
| `plan_only` | Reviewed plan/decomposition, executable child-task definitions, assumptions and open decisions | The planning request is satisfied; implementation has not been verified |
| `through_phase` | Selected phase and prerequisite outputs, tests, integration evidence and continuation manifest | The requested phase boundary is satisfied; later phases remain unimplemented |
| `full_implementation` | All required outcomes, combined-tree evidence and review package | The entire requested implementation outcome is satisfied |

An accepted plan contains outcomes, scope, dependency order, shared contracts,
test strategies, likely conflicts, phase acceptance conditions and a bounded
next task. Unknowns remain visible. Unresolved questions may be acceptable in a
planning deliverable only when its contract allows them; affected implementation
tasks remain not ready.

## Large-Task Phases

A phase is an embedded value object in a versioned `DecompositionPlan`:

```yaml
phases:
  - key: callback-and-session
    outcome: Login callback creates a valid session
    task_ids: [AUTH-101, AUTH-102]
    depends_on: []
    acceptance_criterion_refs: [ac_01J...]
    integration_task_id: AUTH-199
```

Each phase names its bounded outcome, child tasks, dependencies and integration
criteria. Prefer independently demonstrable behavior or a resolved design
uncertainty. A phase may contain several parallel tasks.

Phase status is derived from accepted child results and integration evidence:
`planned | ready | running | blocked | accepted | superseded`.
Passing child tests alone cannot accept a cross-task phase.

Acceptance of a phase does not mark an unfinished full-implementation request
completed. For `through_phase`, the requested boundary may be completed while
the larger product goal remains visibly incomplete.

The handoff includes a continuation manifest with accepted phase/task/commit
references, remaining tasks, next dependencies, open decisions and environment
requirements. Continuing a completed phase-scoped request creates a linked task
or new explicitly authorized scope; it does not silently expand the original
request or reactivate a terminal task. Resuming an interrupted active request
uses the existing recovery flow and revalidates all pinned inputs.

## Reviewer Package

Every accepted deliverable has a `ReviewPackage` with a concise summary and
linked supporting detail. The default summary SHOULD fit approximately one
screen; policy sets an advisory size budget. Required limitations must never be
truncated to satisfy that budget.

Required content:

1. Requested outcome and actual delivery mode/boundary.
2. Observable behavior added, changed or preserved.
3. Approach, reasons for material choices, and relevant trade-offs.
4. Acceptance criteria linked to test cases and observed results.
5. Risk-focused review pointers to specific code/tests and why they matter.
6. Reproduction instructions with environment and fixture references.
7. Unverified behavior, known limitations and unresolved findings.
8. Documentation changes or an explained not-applicable decision.
9. Remaining phases/tasks and continuation information when applicable.

The explanation is a concise engineering rationale; it does not expose private
agent reasoning. Summaries are generated from validated artifacts. A prose claim
cannot override a failed or missing test result. The reviewer can expand details
without reading raw execution logs as a prerequisite.

## Test-Case Review

The test-design role works from the approved outcome, business rules and
interfaces. It covers happy paths, negative cases, boundaries and regressions
according to risk. Implementation may not silently weaken expected behavior.

Each reviewer-facing case exposes:

```yaml
test_id: test_01J...
title: Expired callback token is rejected without creating a session
criterion_refs: [ac_01J...]
preconditions: [An expired token exists in isolated test fixtures]
action: Submit the login callback with that token
expected_outcome: Request is rejected and no session is created
level: integration
implementation_refs:
  - path: tests/auth/callback_test.rs
    symbol: rejects_expired_token
observed_status: passed
gate_result_refs: [gate_result_01J...]
```

The canonical executable test definition is versioned as a `test_definition`
artifact. Observed status and result references are a projection over current
gate evidence, never fields the test author can self-approve. Status is
`not_run | passed | failed | blocked | not_applicable`; stale evidence projects
as `not_run` with an invalidation reason. `not_applicable` requires a policy
decision and alternative evidence where the criterion still requires proof.

The package shows which checks ran on the base, which ran on the candidate, and
which mocks or environment limitations affect the claim. A fixbug should show
that its regression case detects the intended missing behavior on the base when
practical under the evidence strategy.

Reviewer feedback is a typed command referencing package revision, test ID,
criterion and feedback category:

| Feedback | Route |
|---|---|
| Expected behavior is wrong | Contract clarification/amendment, then invalidate affected tests and outputs |
| Scenario or edge case is missing | Test-design correction within scope, or scope decision if the outcome expands |
| Assertion does not prove the case | Test-implementation correction and rerun |
| Fixture/mock hides relevant behavior | Environment/test correction and new evidence |
| Test result is sufficient but implementation risk remains | Focused independent code review |

Human review of test cases is conditional on risk, ambiguity or an explicit user
request. Routine tasks do not require a manual test-approval ceremony. Test-case
review reduces the code-reading burden but does not remove independent code
review for authorization, migration, concurrency, public interfaces or other
policy-selected risks.

## Documentation Obligations

Each contract includes a documentation-impact decision: `required | not_applicable`.
When required, it identifies audience, changed behavior and target artifacts.

Typical triggers:

- public API or CLI behavior: reference and example updates;
- business/user behavior: usage or behavior documentation;
- significant architectural choice: concise design note or ADR;
- local setup/configuration: setup and testing instructions;
- test environment change: fixtures and reproduction instructions.

Update canonical existing documentation where appropriate; avoid generating a
new long document for every small task. Link checks, relevant examples and
document/code consistency checks are part of acceptance. `not_applicable` has a
short reason visible in the review package.

## Acceptance Criteria

- [ ] Planning-only, phase-scoped and full implementation have distinct completion conditions.
- [ ] An implementation request cannot be counted complete from a plan alone.
- [ ] Accepted phases include integrated evidence and a continuation manifest.
- [ ] Reviewers see behavior, rationale, tests, limitations and reproduction steps first.
- [ ] Test cases map to executable checks and current evidence, including missing or invalidated results.
- [ ] Reviewer feedback has an explicit correction/amendment route.
- [ ] Documentation impact is assessed and required updates are verified.
- [ ] No human-time tracking is required for review or acceptance.
