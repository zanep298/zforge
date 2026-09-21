# zForge v2 Documentation

zForge v2 is a quality-first autonomous engineering system. It converts
user-supplied outcomes and product knowledge into evidence-backed engineering
results while minimizing human active attention.

It does not optimize for the largest number of agents, maximum parallelism, or
overnight execution. Fleet execution means managing multiple outcomes and may be
serial when dependencies, risk or resources require it. Safe same-project task
concurrency remains a required target-v2 capability.

## Recommended Reading Order

Implementation decisions are recorded separately from draft design details:

- Accepted: [ADR-001 — Canonical Schema Ownership](./decisions/001-canonical-schema.md).
- Accepted storage direction: [ADR-002 — Markdown/YAML File-Backed Persistence](./decisions/002-file-backed-persistence.md); implementation and crash-conformance tests remain required.
- Accepted host/isolation direction: [ADR-003 — macOS + Docker](./decisions/003-macos-host-scope.md); no separately managed VM, with Docker conformance still required.
- Accepted: [ADR-004 — Bounded YAML Policy Rules](./decisions/004-bounded-yaml-policy.md); engine and conformance tests remain to be implemented.
- Accepted: [ADR-005 — Native v2, No v1 Migration](./decisions/005-native-v2-no-migration.md); the owner handles old data manually, with no legacy engine/adapter requirement.
- Reference fixture: [Taskboard API](../../examples/v2-taskboard/README.md), with baseline tests, Docker environment and task briefs. This is not an implemented v2 orchestration harness.
- Pending: executable implementation backlog and wiring the fixture into v2 agent/recovery/concurrency evaluation.

1. [Product Direction](./product-direction.md)
2. [Autonomous Flow](./flow.md)
3. [Fleet and Human Attention](./fleet-and-human-attention.md)
4. [Deterministic Runtime](./deterministic-runtime.md)
5. [Data Model](./data-model.md)
6. [State and Events](./state-and-events.md)
7. [Execution DAG](./execution-dag.md)
8. [Artifacts and Traceability](./artifacts-and-traceability.md)
9. [Quality Gates](./quality-gates.md)
10. [Policy and Risk](./policy-and-risk.md)
11. [Agents and Subagents](./agents.md)
12. [Automatic Model Routing](./model-routing.md)
13. [Skills](./skills.md)
14. [Workspace and Git](./workspace-and-git.md)
15. [Errors and Recovery](./errors-and-recovery.md)
16. [Configuration](./configuration.md)
17. [CLI and MCP](./cli-and-mcp.md)
18. [Agent Token and Cost Accounting](./cost.md)
19. [Security Threat Model](./security-threat-model.md)
20. [Evaluation](./evaluation.md)
21. [Development Handoff and Review](./delivery-and-review.md)
22. [Project Onboarding and Local Testing](./project-onboarding-and-local-testing.md)

## Product Optimization Order

```text
correctness and safety
    > completeness and maintainability
    > human active attention
    > monetary and token cost
    > elapsed time and parallelism
```

Success is an accepted result for the requested scope, backed by current
verification evidence. Human active-time measurement is not required.

## Delivery Sequence

The intended capability progression is:

1. safe single-task execution, project readiness and evidence-backed handoff;
2. contract-driven plan/phase/full results and concise test-case review;
3. explainable automatic model selection with conservative cold-start rules;
4. minimum-human task execution and a single-repository work batch;
5. verified task-level concurrency, resource isolation and integration;
6. multi-project/multi-role governance, governed knowledge and evaluation;
7. optional richer local Docker/device testing adapters.

Statistical routing optimization is incremental, not a prerequisite for fleet.
No v2 milestone includes application deployment, production access or production
debugging. All documents describe design targets, not implemented capabilities.

The detailed implementation order is maintained in
[Autonomous Software Engineering System — Implementation Plan](../../requirements/autonomous-software-engineering-implementation-plan.md).
