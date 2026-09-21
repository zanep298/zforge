---
title: zForge v2 Product Direction
status: draft
document_type: normative-product-direction
schema_version: 2
---

# zForge v2 Product Direction

## Status

This document defines the product objective, target users, optimization order,
human role, and autonomy boundary for zForge v2. Detailed runtime documents MUST
remain consistent with this direction.

Related documents:

- [Autonomous Flow](./flow.md)
- [Fleet and Human Attention](./fleet-and-human-attention.md)
- [Automatic Model Routing](./model-routing.md)
- [Evaluation](./evaluation.md)
- [Development Handoff and Review](./delivery-and-review.md)
- [Project Onboarding and Local Testing](./project-onboarding-and-local-testing.md)
- [Policy and Risk](./policy-and-risk.md)
- [Autonomous Software Engineering System — Implementation Plan](../../requirements/autonomous-software-engineering-implementation-plan.md)

## Product Objective

zForge turns user-supplied outcomes and product knowledge into high-quality,
evidence-backed engineering results while minimizing the active attention
required from humans.

The objective is not to maximize agent count, parallelism, task throughput, or
unattended runtime. Overnight execution is one optional operating mode, not a
product goal.

The product optimizes in this strict order:

```text
correctness and safety
    > completeness and maintainability
    > human active attention
    > monetary and token cost
    > elapsed time and parallelism
```

A faster or cheaper result is never preferred when it weakens required quality,
evidence, safety, or maintainability. Human attention is reduced only after the
required quality boundary remains satisfied.

## Accepted Outcomes

Success means a correct, safe, maintainable result satisfying the requested
delivery mode, backed by current evidence and clear limitations. A plan-only or
phase-scoped result is accepted against its own contract, never reported as a
completed implementation of the larger goal.

Protected indicators include acceptance-criterion coverage, evidence integrity,
review rejection/major rework, escaped defects reported through development
feedback, unsafe assumptions, and autonomous correction success. Interaction
reason/count diagnostics may identify unnecessary questions.

Human active-time tracking, timers, and outcomes-per-human-hour metrics are not
requirements, telemetry contracts, or release gates. The technical lead judges
the practical reduction in supervision through use.

## Target Users

### Solo Developer

A solo developer works on discrete daily tasks and remains the product authority.
They need fast intake, automatic context assembly, targeted clarification,
high-quality implementation, deterministic verification, independent review,
and a reviewable diff or pull request.

The normal interaction should be one task request, at most one consolidated
clarification round when possible, autonomous execution, and an evidence-backed
result. Internal phases are not exposed as mandatory approval rituals.

### Small Team

A small team works continuously across one or more products. It needs the same
single-task quality guarantees plus shared context, task ownership, scoped
authority, batch planning, dependency management, shared budgets, integration,
and actionable status summaries.

Team support adds governance and coordination. It does not introduce a weaker
quality tier or require a hosted control plane.

### Technical Lead and Power-Solo Fleet Operator

The primary target is a technical lead managing multiple projects and roles,
or a solo developer, who supplies full product context,
selects the outcomes for a sprint or work period, answers genuine product
questions, and delegates the remaining engineering work to zForge.

Fleet means managing multiple outcomes under shared quality and attention policy.
V2 MUST support concurrent independent tasks in the same project with isolated
workspaces and test resources, followed by combined-tree verification. It does
not require every batch to run concurrently or overnight. A fleet scheduler MAY
serialize work when dependency order, knowledge gain, integration risk, provider
quality, or resource contention makes serialization more reliable.

## Human Responsibility

Humans remain authoritative for:

- product intent and business trade-offs;
- information that cannot be derived from approved context;
- approval required by risk, policy, or external authority;
- changes to canonical product knowledge;
- acceptance or merge decisions where policy requires them.

Humans are not required to:

- advance routine internal phases;
- repeat context already available in a valid knowledge source;
- interpret raw logs when zForge can produce a structured diagnosis;
- approve deterministic facts such as a passing test result;
- answer low-impact questions for which policy permits a safe assumption.

## zForge Responsibility

zForge owns:

- context retrieval and relevance selection;
- requirement normalization and executable-contract readiness;
- ambiguity, contradiction, and staleness detection;
- planning, implementation, diagnosis, correction, and review;
- deterministic gates and evidence binding;
- risk-aware agent, model, skill, and workflow routing;
- durable execution, retry, recovery, and cancellation;
- consolidation of human decisions and explanations;
- delivery of reviewable outcomes and limitations.

Agents propose semantic work. Deterministic runtime components retain authority
over state, permissions, budgets, evidence, Git, and external side effects.
Model choice is also a deterministic zForge responsibility: spawned agents may
provide diagnostic evidence but cannot select or replace their own model.

## Product Knowledge Model

Context is separated into three levels:

| Level | Contents | Authority |
|---|---|---|
| Product knowledge | Vision, glossary, business rules, user behavior, durable decisions | Human-approved canonical sources |
| Repository knowledge | Architecture, conventions, commands, ownership, protected paths | Repository evidence plus approved project configuration |
| Task context | Outcome, criteria, references, assumptions, exclusions | Current approved task contract |

Every run pins a `ContextSnapshot` containing the exact selected sources,
versions, hashes, trust classifications, and relevance explanations.

Agent output MAY propose knowledge changes. It MUST NOT silently modify canonical
product knowledge. Conflicting or stale knowledge creates a decision request or a
governed knowledge-update proposal.

## Human-Attention Policy

Before asking a human, zForge MUST attempt applicable safe alternatives:

1. inspect the approved product and repository context;
2. inspect authoritative repository behavior and tests;
3. retrieve an approved external source when policy permits;
4. determine whether a reversible, recorded assumption is allowed;
5. isolate or defer only the affected scope;
6. ask a targeted question when the missing decision remains material.

Questions are classified as:

| Class | Default action |
|---|---|
| Derivable fact | Resolve from evidence without asking |
| Low-impact reversible choice | Apply a policy-approved assumption and record it |
| User preference | Ask only when it materially affects the outcome |
| Product behavior | Request a product decision |
| Security, data, compatibility, or irreversible change | Escalate according to risk policy |
| Missing authority | Request scoped approval |

Questions and approvals are consolidated into a Decision Inbox. One human answer
MAY resolve multiple tasks when all affected contracts and provenance links are
explicit.

## Quality Strategy

Confidence is a routing signal, not proof. Completion is based on current evidence
for the exact candidate change.

Quality is built through:

- measurable acceptance criteria;
- deterministic verification selected from contract and risk;
- actual-diff and scope reconciliation;
- structured failure diagnosis and bounded correction;
- independent review proportional to risk;
- evidence invalidation after material input changes;
- explicit limitations and unresolved findings.

Multiple agents are used only when independence, specialization, or expected
accepted quality justifies them. Agent count is not a quality metric.

## Fleet Strategy

Task-level concurrency is a required target-v2 capability, delivered after the
single-task foundation. Fleet scheduling happens primarily across tasks. Parallel nodes inside one task
are optional and should be introduced only after task-level isolation and
integration are proven.

A fleet scheduler prioritizes:

1. required dependencies and blockers;
2. risk-reducing or knowledge-producing work;
3. task and context readiness;
4. expected correctness and acceptance probability;
5. integration and shared-resource safety;
6. human-decision reuse across tasks;
7. cost and elapsed time.

Blocked tasks do not block unrelated ready tasks. The fleet preserves partial
results, accumulates decision requests, and resumes affected work only after
revalidating current context, policy, evidence, and dependencies.

## Initial Product Boundary

The first native-v2 product target is a high-quality autonomous single task:

```text
clear outcome and approved context
    -> executable contract
    -> isolated implementation
    -> deterministic gates
    -> bounded correction
    -> independent review when required
    -> evidence-backed diff or pull request
```

The next target is a quality-first fleet with safe same-project task concurrency,
multi-project profiles and role governance. Large requests support
`plan_only`, `through_phase`, and `full_implementation`; see
[Development Handoff and Review](./delivery-and-review.md).

V2 ends at a plan/task graph, local changes/commits, or reviewable PR under policy.
Application deployment to any environment, production access/credentials and
production debugging are out of scope for all v2 milestones, not deferred v2
features. Development CI is allowed only when it cannot trigger deployment or
production effects; otherwise hand off locally. Disposable local stacks and
allocated test devices are test infrastructure.

Remote clusters, aggressive within-task parallelism and automatic canonical
knowledge mutation remain deferred. Additional local Compose/device test adapters
are optional per supported stack; required tests cannot silently pass when
unavailable. Docker runner isolation on macOS is a separate foundation decision
in [ADR-003](./decisions/003-macos-host-scope.md), not an optional E2E feature.

## Product Acceptance Criteria

- [ ] Quality and safety gates take precedence over attention, cost, and speed.
- [ ] Human interaction is triggered by material uncertainty or authority, not fixed phases.
- [ ] Context already available in approved sources is not requested again.
- [ ] Every human request is targeted, evidence-backed, and actionable.
- [ ] Autonomous completion is counted only for accepted, evidence-backed outcomes.
- [ ] Fleet execution may be serial and is not coupled to overnight scheduling.
- [ ] Blocked tasks do not stop unrelated ready tasks.
- [ ] Reports show accepted scope, evidence, limitations and unresolved decisions without tracking human active time.
- [ ] Independent tasks in one project can execute concurrently and pass integration verification.
- [ ] Large tasks can stop at an explicitly requested plan or phase with a continuation manifest.
- [ ] Review packages explain behavior, rationale, test cases and documentation impact concisely.
- [ ] Project onboarding identifies supported task classes and reproducible local test capabilities.
- [ ] No v2 delivery path deploys applications or accesses production.
