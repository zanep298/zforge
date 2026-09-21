# Autonomous Software Engineering System — Implementation Plan

## Objective

Evolve zForge from a gated AI development workflow orchestrator into a bounded autonomous software engineering system that can:

- maximize accepted engineering quality before optimizing human attention, cost, or elapsed time;
- assemble and pin relevant product, repository, and task context;
- turn a sufficiently clear task into verifiable engineering contracts;
- execute each task in an isolated and policy-controlled environment;
- produce code and evidence through deterministic quality gates;
- use independent evaluation before accepting agent output;
- request human input only when risk, uncertainty, or policy requires it;
- consolidate genuine product decisions instead of interrupting at fixed phases;
- manage a quality-first fleet with safe same-project task concurrency; overnight is optional;
- deliver reviewed plans, phase-scoped results, or full implementation with concise evidence-backed handoffs.

The required capability inventory is defined in [autonomous-software-engineering-system.md](./autonomous-software-engineering-system.md).
The product optimization order and fleet semantics are defined in
[Product Direction](../docs/v2/product-direction.md) and
[Fleet and Human Attention](../docs/v2/fleet-and-human-attention.md).

## Initial Autonomy Boundary

V2 targets macOS hosts only, as accepted in
[ADR-003](../docs/v2/decisions/003-macos-host-scope.md). Linux/Windows host
support is out of scope. Pin minimum macOS version and tested CPU/adapter
combinations in the foundation spike; no universal macOS compatibility is assumed.

Docker is selected for autonomous agent/repository execution; macOS controllers
retain Git, YAML state and runtime authority. No separate user/zForge-managed VM
is required. Basic Docker runner support belongs in P0, not the optional P4
expansion of multi-service Compose/emulator/device test adapters. Native-only
checks block until an explicit host adapter satisfies their required controls.

The first usable product target is:

> Given a clear task in a supported repository, zForge creates an isolated workspace, derives structured requirements, plans and implements the change, runs configured quality gates, obtains an independent review, and opens a pull request containing complete evidence. A developer reviews the decision and merges.

The next target is a single-user work batch that prepares multiple tasks,
consolidates material questions, continues unrelated work when one task blocks,
and reports accepted scope, evidence, limitations and outstanding decisions. Scheduling MAY remain
serial; task quality and integration safety take precedence over throughput.

Application deployment to any environment, production connection/credentials and
production debugging are outside every v2 milestone. Disposable local test
stacks and allocated test devices are verification infrastructure.

## Principles

- Correctness and safety before completeness, human attention, cost, or speed.
- Accepted outcome quality, not agent activity or raw throughput, is the product result.
- Safety before autonomy.
- Deterministic evidence before LLM judgment.
- Structured contracts as the source of truth; Markdown remains the human-readable view.
- Planner, coder, verifier, and reviewer responsibilities remain independent.
- Human intervention is risk-based, not phase-based.
- Approved context is reused; humans are not asked to repeat derivable information.
- Human requests are consolidated, evidence-backed, and actionable.
- Fleet means multi-task outcome management, not mandatory concurrency or scheduling.
- Every autonomous action is bounded by scope, permissions, time, attempts, and cost.
- Every state transition is resumable, auditable, and idempotent.
- Task profiles use native-v2 contracts; legacy flow routing is not required.
- New autonomous behavior is opt-in until its evaluation thresholds are met.

## Target Architecture

```text
Task Intake
    │
    ▼
Context Assembly ──► Requirement Analyzer ──► Risk and Policy Engine
    │
    ▼
Executable Contract and Traceability Graph ──► Decision Inbox
    │
    ▼
Execution DAG Scheduler
    │
    ├──► Isolated Workspace
    ├──► Capability Profile and Model Router
    │         ├──► Planner Agent
    │         └──► Coder Agent
    └──► Deterministic Quality Gates
                         │
                         ▼
                 Independent Reviewer
                         │
                         ▼
                  Evidence and Judge
                         │
              ┌──────────┴──────────┐
              ▼                     ▼
         Pull Request         Retry or Escalate
              │
              ▼
     Development Handoff and Feedback
```

## Cross-Cutting Data Model

Introduce versioned records that survive retries and process restarts:

- `ContextSnapshot`: exact product, repository, and task context selected for a run.
- `WorkBatch`: a revisioned selection of tasks, priorities, and dependencies.
- `BatchRun`: one execution of a pinned work-batch revision.
- `TaskContract`: requirements, acceptance criteria, assumptions, constraints, and risk.
- `TraceLink`: requirement-to-test-to-plan-to-change-to-evidence relationships.
- `ExecutionPlan`: DAG nodes, dependencies, resources, and policies.
- `TaskRun`: one end-to-end execution of a task.
- `StepAttempt`: one attempt for one DAG node.
- `NodeCapabilityProfile`: model/tool/context/independence requirements for one node attempt.
- `ModelCatalogSnapshot`: immutable selectable-model identities, capabilities, availability, and evaluation references.
- `ModelRoutingDecision`: candidate outcomes and the selected runtime-agent/model/reasoning profile.
- `WorkspaceLease`: isolated workspace ownership and lifecycle.
- `ChangeManifest`: allowed and observed file changes.
- `GateResult`: deterministic verification result and evidence.
- `ReviewDecision`: independent review findings and confidence.
- `PolicyDecision`: `allow`, `deny`, or `require_approval` with structured reasons.
- `ArtifactProvenance`: source task, phase, agent, model, hash, and timestamp.

The Decision Inbox is a projection over clarification questions, approval
requests, blockers, scope changes, integration conflicts, and human reviews. It
does not become a second source of task or policy truth.

All schemas must be versioned native-v2 contracts. Legacy `.state.yaml` is not a supported state input; Markdown source context does not confer lifecycle authority.

---

# P0 — Safe Autonomous Execution

## Goal

Make unattended execution safe, isolated, bounded, recoverable, and auditable before increasing agent autonomy.

## P0.1 — Task Run and Workspace Lifecycle

Implement a `WorkspaceManager` that creates one Git worktree or equivalent isolated workspace per task run.

Required changes:

- Add workspace identity and lease information to task/job state.
- Create, reuse, reconcile, and clean task worktrees idempotently.
- Run agent and verification commands only inside the assigned workspace.
- Prevent multiple active runs from owning the same workspace.
- Reconcile abandoned leases after worker crashes.
- Preserve failed workspaces for inspection according to retention policy.

Primary areas:

- `src/state/`
- `src/job/`
- `src/cli/git.rs`
- new `src/workspace/`

Acceptance criteria:

- [ ] Two tasks can run concurrently without sharing a working tree.
- [ ] Two workers cannot acquire the same workspace lease.
- [ ] Restarting a worker resumes or safely reconciles the existing workspace.
- [ ] Failure never modifies the developer's original working tree.
- [ ] Workspace cleanup is explicit, idempotent, and auditable.

## P0.2 — Scoped Change Manifest and Git Operations

Replace repository-wide staging with manifest-based change control.

Required changes:

- Derive an initial allowed file scope from the approved plan.
- Record files created, modified, renamed, and deleted by each attempt.
- Reject or escalate changes outside the allowed scope.
- Stage only files accepted by the final `ChangeManifest`.
- Preserve task artifacts separately from production-code scope.
- Generate a scope-drift report before commit or pull request creation.

Primary areas:

- `src/cli/git.rs`
- new `src/change/`
- plan and code artifact schemas

Acceptance criteria:

- [ ] `git add -A` is not used by autonomous delivery paths.
- [ ] Out-of-scope changes block delivery or require an explicit policy decision.
- [ ] Unrelated developer changes cannot enter an autonomous commit.
- [ ] Every committed file maps to a plan step or approved scope amendment.

## P0.3 — Execution Policy Engine

Introduce policy checks for filesystem, command, network, credential, and agent permissions.

Use the bounded YAML language accepted in
[ADR-004](../docs/v2/decisions/004-bounded-yaml-policy.md): explicit action
selectors, typed facts, fixed Boolean/comparison operators and deny-by-default
combination. No scripting, dynamic expressions or external policy service is
required. Pin the language and fact-registry versions in policy bundles.

Required changes:

- Define phase-specific command allowlists and deny rules.
- Define writable path policies relative to the task workspace.
- Require explicit capability grants for network and secrets.
- Treat Jira, Figma, repository text, memory, and generated artifacts as untrusted prompt context.
- Record every policy decision and denial.
- Replace unconditional headless permission bypass with policy-derived runner arguments.
- Compile/validate all rule shapes and types before activation; invalid policy never falls back silently to a permissive configuration.
- Resolve applicable facts before Boolean evaluation; missing/null/stale/invalid facts deny admission, including under negation or a true sibling branch.
- Enforce deny precedence, accumulated obligations and exact scoped approval reuse without permitting approval to replace required evidence.
- Add fixtures for operator/type limits, malformed YAML, no-match denial, rule reordering, conflicting obligations and approval expiry/amendment.

Primary areas:

- `src/orchestrator/`
- `src/runner/`
- `src/job/worker.rs`
- new `src/policy/`

Acceptance criteria:

- [ ] Every spawned command has an evaluated policy decision.
- [ ] Network and secret access are denied by default.
- [ ] Agent-requested scope expansion requires policy approval or human escalation.
- [ ] Prompt context has provenance and trust classification.
- [ ] Headless execution cannot silently expand its permissions.

## P0.4 — Budgets, Provenance, and Recovery

Storage uses Markdown/YAML files, not a database, as selected in
[ADR-002](../docs/v2/decisions/002-file-backed-persistence.md). Implement immutable
YAML record/event batches and one durable YAML commit manifest per command under
a short-lived project writer lock. Current task/run YAML and Markdown views are
rebuilt from accepted records and committed history.

Required changes:

- Add per-task budgets for wall time, attempts, tokens, cost, and spawned processes.
- Hash every input and output artifact.
- Persist task-run and step-attempt events atomically.
- Add worker heartbeat and stale-worker reconciliation.
- Make cancellation propagate through the full execution tree.
- Produce an audit bundle for every terminal run.

Acceptance criteria:

- [ ] Budget exhaustion stops execution with a structured reason.
- [ ] Every artifact can be traced to its task, phase, attempt, agent, and model.
- [ ] A process crash cannot produce a false-success state.
- [ ] Terminal runs contain sufficient evidence to reproduce the decision path.
- [ ] Related records, events and outbox intents become visible together through one committed manifest.
- [ ] Crash injection covers staging, manifest publication/durability, acknowledgement and projection repair.
- [ ] Concurrent commands cannot overspend shared budgets or allocate the same exclusive resource.
- [ ] Agents/tests run concurrently while short metadata commits serialize; no lock spans agent execution.
- [ ] No database backend or authoritative JSONL state log is introduced.

## P0.5 — Bootstrap Analysis and Project Readiness

- Add bounded `AnalysisSession` ownership before an approved execution plan exists.
- Pin context, configuration, conservative risk policy, model routing and budgets.
- Persist attempts and decisions in the owning task/batch stream without fake runs.
- Add project profiles and baseline readiness reports for declared task classes.
- Publish an adapter/platform enforcement matrix; unsupported prevention blocks use.
- Scope initial sandbox conformance to Docker on macOS, and file-store conformance to the macOS controller; pin versions, image digests and CPU/adapter combinations.
- Prove container agent authentication/model selection, output parsing, egress/mount restrictions and cancellation without host credential-directory or Docker-socket mounts.
- Keep Docker provisioning in trusted host controllers; unsupported workloads do not fall back to a separate VM or an unrestricted host process.
- Do not require Linux/Windows backends or conformance tests for v2 acceptance.

## P0 Exit Gate

- All autonomous execution occurs in isolated workspaces.
- Autonomous commits contain only manifest-approved files.
- Permission and budget policies are enforced for every spawn.
- Crash, cancellation, and reconciliation integration tests pass.
- Autonomous mode remains opt-in.

---

# P1 — Machine-Verifiable Engineering Contracts

## Goal

Make approved context, requirements, plans, tests, changes, and evidence
structurally traceable and automatically validatable.

## P1.1 — Product and Repository Context Assembly

Required changes:

- Define product, repository, task, and external context classes.
- Select only relevant eligible sources for each assignment.
- Record source identity, revision or retrieval time, hash, trust, and relevance.
- Detect material stale or conflicting knowledge.
- Pin an immutable `ContextSnapshot` before execution.
- Allow agents to propose but not silently apply canonical knowledge updates.

Acceptance criteria:

- [ ] Humans are not asked to repeat facts available in eligible approved context.
- [ ] Every selected source has provenance and a relevance explanation.
- [ ] Material context conflicts create a decision request rather than an arbitrary choice.
- [ ] Context changes invalidate only dependent plans, attempts, and evidence.

## P1.2 — Versioned Artifact Schemas

Ownership is accepted in
[ADR-001 — Canonical Schema Ownership](../docs/v2/decisions/001-canonical-schema.md):
JSON Schema owns persisted/exchanged data shapes; Rust implements compatible
domain behavior; Markdown explains the contract. Shared envelope/reference and
event schemas needed by P0 land with the foundation, not only after P0 finishes.

Define structured schemas for:

- task requirements;
- acceptance criteria;
- assumptions and exclusions;
- test cases;
- plan steps;
- expected file scope;
- risk factors;
- verification evidence;
- review findings.

Markdown artifacts remain readable projections of the structured records.

Primary areas:

- canonical `schemas/v2/` and shared reference definitions;
- schema/type compatibility fixtures and CI checks;
- new `src/artifact/schema/`
- `src/fs/`
- `src/prompt/`
- `templates/*.tmpl`

Acceptance criteria:

- [ ] Every contract element has a stable ID.
- [ ] Schemas are versioned and validated before state advancement.
- [ ] Legacy state is rejected without changing original data; new-task source intake uses native validation.
- [ ] Invalid structured artifacts produce actionable validation errors.
- [ ] Minimum/full valid and invalid fixtures exist for each implemented schema.
- [ ] Rust serialization/deserialization and complete documentation examples agree with the canonical schema.
- [ ] Structural validation and cross-record/domain validation have distinct tests.
- [ ] `EvidenceRecord` uses typed `subjects`; obsolete `subject_refs` is rejected.
- [ ] Initial implementation does not depend on building a code generator.

## P1.3 — Traceability Graph

Implement links across:

```text
Requirement → Acceptance Criterion → Test Case → Plan Step → Change → Gate Evidence
```

Required validation:

- every acceptance criterion is measurable;
- every acceptance criterion has one or more test cases or an approved evidence strategy;
- every plan step maps to an acceptance criterion;
- every changed file maps to a plan step;
- every completed criterion has verification evidence.

Primary areas:

- new `src/traceability/`
- `src/cli/approve.rs`
- `src/cli/verify.rs`
- `src/cli/review.rs`
- MCP responses and status output

Acceptance criteria:

- [ ] Delivery is blocked when required trace links are missing.
- [ ] `zforge status` reports traceability coverage and missing evidence.
- [ ] Review output identifies uncovered requirements and unexplained changes.
- [ ] Traceability survives retries and scope amendments.

## P1.4 — Requirement Analysis and Clarification Loop

Required changes:

- Detect ambiguous, conflicting, missing, or unverifiable requirements.
- Classify assumptions separately from confirmed facts.
- Generate targeted clarification questions.
- Consolidate equivalent questions and show evidence already inspected.
- Prevent implementation when a blocking ambiguity remains.
- Allow non-blocking assumptions only when policy permits them.

Acceptance criteria:

- [ ] Blocking ambiguities cannot be silently converted into implementation assumptions.
- [ ] Human answers update the structured contract and provenance.
- [ ] Requirement changes invalidate affected downstream artifacts.
- [ ] Waiting for one answer does not block unrelated ready tasks.

## P1.5 — Risk Classification and Workflow Compilation

Replace keyword-only flow selection with structured risk classification while preserving flow presets.

Risk inputs include:

- affected component count;
- public API or schema changes;
- database migrations;
- authentication, authorization, cryptography, or secret handling;
- local environment and integration impact;
- test coverage availability;
- reversibility;
- confidence and requirement ambiguity.

Acceptance criteria:

- [ ] Every task has a recorded risk score and explanation.
- [ ] Risk policy selects required phases, gates, reviewers, and approvals.
- [ ] Explicit user policy can increase rigor but cannot silently weaken mandatory organizational policy.

## P1.6 — Requested Results and Phased Decomposition

- Record `plan_only`, `through_phase`, or `full_implementation` in the task contract.
- Define phase outcomes, dependencies, integration criteria and continuation manifests.
- Accept planning evidence without claiming implementation or creating an execution run.
- Amend scope explicitly when continuing beyond a completed requested boundary.

## P1 Exit Gate

- Structured contracts are the source of truth for new autonomous tasks.
- Required traceability coverage is 100% before delivery.
- Risk and ambiguity decisions are recorded and explainable.
- Existing manual flows remain operational.

---

# P2 — Quality Gate and Independent Evaluation

## Goal

Make successful execution mean more than a single test command passing.

## P2.1 — Pluggable Quality Gate Engine

Define a common gate interface with:

- command or adapter;
- phase and language applicability;
- required or optional status;
- timeout and retry policy;
- output parser;
- severity;
- baseline comparison;
- produced evidence.

Initial gate types:

- formatting;
- linting;
- type checking;
- unit tests;
- integration tests;
- end-to-end tests;
- coverage;
- mutation testing;
- static security analysis;
- dependency and license audit;
- API/schema compatibility;
- migration validation;
- performance regression;
- accessibility and visual regression;
- flaky-test detection.

Primary areas:

- refactor `src/runner/`
- `src/cli/verify.rs`
- new `src/gate/`
- language profiles under templates/configuration

Acceptance criteria:

- [ ] Repositories can configure ordered gate sets.
- [ ] Gate results use a common evidence schema.
- [ ] Required gate failures block delivery.
- [ ] Baseline failures can be distinguished from regressions introduced by the task.
- [ ] Gate plugins cannot bypass execution policy.

## P2.2 — Independent Reviewer

Required changes:

- Introduce explicit planner, coder, verifier, and reviewer assignments.
- Support policy requiring a different agent or model for review.
- Give reviewers the original task, structured contract, final diff, and gate evidence.
- Do not provide private coder reasoning as reviewer context.
- Add specialist reviewers for security, database, API, performance, and UI risk classes.

Acceptance criteria:

- [ ] High-risk tasks cannot be self-reviewed by the coding agent.
- [ ] Review findings map to requirements, files, or evidence.
- [ ] Reviewer disagreement triggers correction or escalation.

## P2.3 — Semantic Judge and Confidence

Combine deterministic validation with independent LLM assessment.

Judge dimensions:

- completeness;
- correctness;
- traceability;
- scope compliance;
- risk coverage;
- evidence strength;
- reviewer confidence.

Possible decisions:

- pass;
- retry the current phase;
- quality fallback to another agent;
- amend the plan;
- request human input;
- fail terminally.

Acceptance criteria:

- [ ] A judge cannot override a failed mandatory deterministic gate.
- [ ] Every decision includes structured reasons and confidence.
- [ ] Quality fallback is distinct from provider-availability fallback.
- [ ] Retry loops are bounded and cannot oscillate indefinitely.

## P2.4 — Generalized Self-Correction

Extend the verifier loop beyond failed test names.

Feedback may include:

- gate failures;
- missing trace links;
- scope violations;
- reviewer findings;
- compatibility regressions;
- performance regressions.

Acceptance criteria:

- [ ] Each retry receives only relevant structured feedback.
- [ ] Attempts preserve their own diff and evidence.
- [ ] Repeated failure triggers root-cause analysis before further retries.
- [ ] Correction cannot silently rewrite approved requirements.

## P2.5 — Automatic Model Routing

Replace fixed `(assistant, phase) -> model` dispatch with a capability-based,
quality-first assignment router.

Required changes:

- compile a `NodeCapabilityProfile` from role, task/risk, repository, context,
  output contract, independence, and prior-attempt evidence;
- resolve a trusted, immutable `ModelCatalogSnapshot` through versioned runtime
  agent adapters;
- support explicit fixed, agent-scoped automatic, and fully automatic selection;
- filter hard eligibility before comparing quality, attention, cost, or latency;
- select through task-stratified quality evidence and conservative cold-start
  policy;
- record every candidate outcome and selected model in a
  `ModelRoutingDecision` before assignment authorization;
- keep availability fallback separate from quality escalation;
- support shadow routing that cannot affect the executed assignment;
- support policy-validated native fixed-model overrides without a legacy configuration adapter.

Acceptance criteria:

- [ ] `agent=codex, model=auto` selects only candidates invokable through the Codex adapter.
- [ ] `agent=claude, model=auto` selects only candidates invokable through the Claude Code adapter.
- [ ] `agent=auto, model=auto` may select the eligible runtime-agent/model pair.
- [ ] Core routing code contains no provider model-name defaults.
- [ ] Every choice is reproducible and explainable from pinned inputs.
- [ ] Unknown or insufficient evidence follows explicit cold-start policy.
- [ ] Quality escalation cannot silently downgrade capability or bypass failed evidence.
- [ ] Reviewer independence is checked against resolved model/provider/family identity.
- [ ] Shadow recommendations cannot alter execution, prompts, or budgets.

## P2.6 — Review Package and Local Test Lifecycle

- Link criteria to readable cases, implemented tests and observed gate evidence.
- Generate concise rationale, trade-offs, risk pointers, limitations and reproduction steps.
- Route reviewer feedback by test case/criterion into bounded correction or amendment.
- Assess documentation impact and update affected API, behavior, setup or architecture docs.
- Implement environment leases, fixtures, readiness/reset/teardown and crash recovery
  for at least one declared local stack; richer adapters remain optional P4 work.
- Bind evidence to environment/toolchain/fixture versions and actual tested tree.

## P2 Exit Gate

- Required quality gates run through the common engine.
- Independent review is enforced according to risk policy.
- Quality fallback and bounded self-correction are operational.
- Automatic model routing works with conservative configured cold-start rules
  and fixed selection retained. Statistical optimization and project-specific
  promotion are incremental and do not block P3 fleet delivery.
- Evaluation demonstrates better defect detection than the current single-command verify path.

---

# P3 — Quality-First Fleet, Scheduling, and Pull Requests

## Goal

Allow zForge to manage multiple outcomes with minimal human active attention and
deliver evidence-backed pull requests without weakening task-level quality.

## P3.1 — Work Batch, Preflight, and Decision Inbox

Required changes:

- Add versioned `WorkBatch`, `BatchRun`, and batch-item models.
- Analyze readiness, context gaps, dependencies, conflicts, risk, environment,
  expected decisions, budget, and integration order before execution.
- Project clarification, approval, amendment, conflict, blocker, and review
  records into one consolidated Decision Inbox.
- Continue unrelated ready tasks when one task waits for input or approval.
- Report accepted scope, evidence and decisions without hiding per-task status or tracking human time.

Acceptance criteria:

- [ ] Batch execution may run fully serially.
- [ ] Equivalent questions are consolidated without broadening their semantics.
- [ ] Blocked tasks preserve their work and do not stop unrelated ready tasks.
- [ ] Every batch item retains independent task state, evidence, and delivery truth.
- [ ] A batch result distinguishes completed, limited, blocked, failed, deferred, and cancelled work.

## P3.2 — Execution DAG

Compile the selected workflow into a DAG containing:

- step dependencies;
- required resources;
- workspace requirements;
- agent assignment;
- gate requirements;
- retry and compensation behavior;
- approval conditions.

Primary areas:

- evolve `src/state/flow.rs`
- new `src/execution/`
- `src/job/`
- `src/orchestrator/`

Acceptance criteria:

- [ ] Independent nodes can run serially or concurrently according to evaluated policy.
- [ ] Dependent nodes never run before prerequisites pass.
- [ ] Partial failure does not corrupt successful evidence.
- [ ] Execution resumes from durable checkpoints after restart.
- [ ] Independent same-project tasks run concurrently with resource isolation and combined-tree gates.
- [ ] Within-task parallel nodes are optional; serial task execution remains valid when dependencies or safety require it.

## P3.3 — Risk-Based Human Supervision

Support policy levels such as:

| Risk | Default supervision |
|---|---|
| Low | Run through pull-request creation |
| Medium | Require plan or final-diff approval |
| High | Require specification, plan, and merge approval |
| Critical | Analyze only; implementation or delivery requires explicit authority |

Acceptance criteria:

- [ ] Human requests occur only at policy-defined decision points.
- [ ] Escalation includes the decision required, evidence, alternatives, and impact.
- [ ] Approval is scoped and cannot authorize unrelated later actions.
- [ ] Policy changes are versioned and audited.
- [ ] Approved context and authoritative evidence are inspected before asking a human.
- [ ] No human active-duration telemetry or per-human-hour metric is required.

## P3.4 — Pull Request and CI Controller

Required changes:

- Create scoped commits from the accepted change manifest.
- Generate pull-request descriptions from contracts and evidence.
- Push only with explicit repository policy authorization.
- Monitor CI and ingest failures into the correction loop.
- Process review feedback as structured change requests.
- Detect and escalate merge conflicts.

Primary areas:

- new `src/delivery/`
- `src/cli/git.rs`
- MCP delivery tools
- job lifecycle and status

Acceptance criteria:

- [ ] Pull requests include requirement, test, risk, change, and gate summaries.
- [ ] CI failures are linked to the originating task run.
- [ ] Review feedback cannot modify scope without contract amendment.
- [ ] No autonomous force push or destructive branch operation is permitted.

## P3.5 — Multi-Project and Monorepo Coordination

Required changes:

- Model repository and component ownership.
- Detect cross-project dependencies.
- Coordinate multiple isolated workspaces and pull requests.
- Support component-specific policies and gates.
- Preserve parent/submodule or dependency update order.

Acceptance criteria:

- [ ] Cross-project execution has explicit dependency ordering.
- [ ] Each repository retains independent evidence and delivery decisions.
- [ ] A failure in one project cannot silently commit partial changes in another.

## P3 Exit Gate

- Low-risk supported tasks can run from intake to pull-request creation without phase-by-phase supervision.
- A single-user, single-repository work batch can preflight, run, continue on
  unrelated blockers, execute independent tasks concurrently, integrate safely,
  and produce an aggregate outcome.
- Pull requests contain complete and reproducible evidence.
- CI and reviewer feedback feed bounded correction loops.
- Correctness, safety, evidence and autonomy meet stratified release thresholds;
  operator feedback assesses supervision without human-time tracking.

---

# P4 — Multi-Project Maturity, Local Testing and Continuous Learning

## Goal

Extend proven development autonomy across project/role profiles, richer local
test environments and governed learning. No application deployment or production
access is introduced.

## P4.1 — Additional Local Test Adapters

Basic onboarding and local test lifecycle land in P0/P2. This optional expansion
adds supported Docker Compose, emulator and registered local-device adapters;
do not delay core development autonomy for unsupported hardware/stacks.

Required capabilities for each declared adapter:

- pinned environment/toolchain and deterministic fixture definitions;
- test-only credentials, readiness, reset, teardown and crash reconciliation;
- per-task ports, volumes, databases, queues and exclusive device allocation;
- captured evidence and reproducible reviewer test replay;
- an adapter/platform enforcement matrix and adversarial boundary fixtures.

Acceptance criteria:

- [ ] Required tests blocked by missing capabilities cannot be reported passed.
- [ ] Parallel tasks cannot collide through shared test services or devices.
- [ ] Retained failure environments have bounded leases and safe cleanup.
- [ ] No local test adapter accesses production or triggers an application release.

## P4.2 — Governed Memory

Each memory entry must include:

- source task and commit;
- repository and component scope;
- provenance;
- confidence;
- creation and expiry time;
- applicability conditions;
- superseded or conflicting entries.

Acceptance criteria:

- [ ] Unscoped, expired, or conflicting memory is not silently injected.
- [ ] Memory updates require evidence and preserve history.
- [ ] A task can report exactly which memory influenced each decision.

## P4.3 — Development Feedback Loop

Required changes:

- Ingest review feedback, development CI failures and user-supplied defects.
- Link signals to originating contracts, changes, test cases and evidence.
- Generate regression tests or proposed policy/knowledge updates.
- Track whether corrective knowledge prevents recurrence.

Acceptance criteria:

- [ ] Escaped defects become traceable regression evidence.
- [ ] Feedback cannot directly mutate trusted policy or canonical knowledge.
- [ ] No production monitoring, incident or debugging connector is introduced.
- [ ] Recurrence is available by component and failure class.

## P4.4 — Continuous Evaluation

Track at minimum:

- autonomous task completion rate;
- pull-request acceptance rate;
- accepted outcomes by requested plan/phase/full scope;
- intervention reasons and rate, without human active-time tracking;
- unnecessary and duplicate question rate;
- unsafe assumption and decision-reuse error rate;
- development-reported escaped defects, reverts and major rework;
- scope-violation rate;
- gate and reviewer defect-detection rate;
- retry and fallback effectiveness;
- cost, tokens, and cycle time per task;
- model and agent performance by task class;
- memory usefulness and stale-memory incidents.

Maintain fixed benchmark tasks for supported languages and task classes.

Acceptance criteria:

- [ ] Autonomy levels are enabled only after benchmark and live metrics meet policy thresholds.
- [ ] Model or policy regressions can be detected before global rollout.
- [ ] Evaluation results are reproducible from stored artifacts and configuration.

## P4 Exit Gate

- Multiple project/role profiles preserve independent context, authority and evidence.
- Declared local test adapters pass lifecycle, isolation and replay tests.
- Development feedback is traceable to contracts and test cases.
- Governed knowledge proposals and continuous evaluation are operational.
- All application deployment and production access remain prohibited.

---

# Native-v2 Delivery and Feature Rollout

## Feature Flags

Introduce optional capabilities incrementally. These illustrative rollout switches
are not a v1/v2 engine selector. Native execution must refuse admission until its
mandatory isolation, contracts, policy and gate foundations are enabled; setting
one of those to `false` cannot activate an unguarded execution path:

```yaml
autonomy:
  enabled: false
  isolated_workspace: false
  context_snapshots: false
  structured_contracts: false
  quality_gate_engine: false
  independent_review: false
  shadow_model_routing: false
  automatic_model_routing: false
  decision_inbox: false
  work_batches: false
  execution_dag: false
  parallel_execution: false
  pull_request_delivery: false
  local_test_adapters: false
```

## Native-v2 Data and Interfaces

[ADR-005](../docs/v2/decisions/005-native-v2-no-migration.md) accepts a native-v2-only
implementation. The owner handles old data manually; no migration utility,
legacy reader, command aliases or dual-engine routing is required.
Unsupported stores/configuration fail validation without converting, overwriting
or deleting original files. Source documents may enter normal new-task intake,
without inherited completion, approvals or evidence.

Native schema versioning and capability discovery remain required. This decision
does not remove API/schema compatibility or database-migration testing for the
projects zForge works on.

## Rollout Levels

| Level | Native-v2 capability |
|---|---|
| L2 | Isolated, policy-controlled, self-correcting execution |
| L3 | Independent evaluation, quality-first work batches, and autonomous pull-request creation |
| L4 | Multi-project/role maturity, supported local test adapters and governed learning |

These are capability milestones, not selectable v1/v2 engines. Projects enable
only evaluated capabilities. Mandatory native safety and evidence controls cannot
be disabled to enter a lower level.

---

# Testing Strategy

The initial application fixture is [Taskboard](../examples/v2-taskboard/README.md).
Its Docker baseline and task briefs cover single-task, parallel-feature and
plan/phase scenarios as evaluation inputs. Wiring them to the native orchestrator,
fake/real agents, crash injection and protected acceptance evaluation remains
implementation work; passing the fixture suite alone does not satisfy v2 exit gates.

Each phase requires:

- unit tests for schemas, policies, state transitions, and parsers;
- integration tests using fake agents and temporary Git repositories;
- crash and resume tests;
- concurrent task isolation tests;
- adversarial prompt and scope-escape tests;
- real-agent contract tests behind explicit environment gates;
- end-to-end benchmark tasks for every supported language profile;
- unsupported legacy input rejection and preservation of original files.

No autonomy level advances based only on unit-test coverage; it must pass end-to-end evaluation against its exit gate.

# Cross-Cutting Acceptance Scenarios

- A plan-only request ends with a reviewed decomposition, not a claimed implementation.
- A large request stops at an authorized phase with integrated evidence and resumable follow-up scope.
- A reviewer can inspect expected behavior, negative cases and current results without reading all code.
- A changed criterion invalidates affected tests/reviews; a changed environment invalidates dependent evidence.
- Two independent tasks run simultaneously in one project without workspace, database or port collisions.
- Production endpoints and deployment-triggering Git/CI workflows are denied even when requested by an agent.

See [Development Handoff and Review](../docs/v2/delivery-and-review.md) and
[Project Onboarding and Local Testing](../docs/v2/project-onboarding-and-local-testing.md).

# Suggested Epic Order

1. `ASE-001` — Isolated worktree and workspace lease manager.
2. `ASE-002` — Execution policy and enforceable bounded runner permissions.
3. `ASE-003` — Versioned `AnalysisSession`, `TaskRun`, `StepAttempt`, and transactional event model.
4. `ASE-004` — Change manifest and scoped Git staging.
5. `ASE-005` — Budgets, provenance, heartbeat, and recovery.
6. `ASE-101` — Structured artifact schemas, requested result modes and phased decomposition.
7. `ASE-102` — Product/repository context snapshots and conflict detection.
8. `ASE-103` — Traceability graph and semantic validators.
9. `ASE-104` — Requirement clarification, question consolidation, and risk classifier.
10. `ASE-201` — Pluggable quality gates, project readiness and basic local test lifecycle.
11. `ASE-202` — Generalized self-correction and structured diagnosis.
12. `ASE-203` — Independent review, test-case review package and documentation-impact gates.
13. `ASE-204` — Quality fallback and conditional semantic judge.
14. `ASE-205` — Capability profiles, model catalog, shadow routing, and automatic model selection.
15. `ASE-301` — Work batch, preflight, and Decision Inbox.
16. `ASE-302` — Dependency-aware serial task scheduler and continue-on-block.
17. `ASE-303` — Risk-based supervision and consolidated decision provenance.
18. `ASE-304` — Pull-request, CI, and serial integration controller.
19. `ASE-305` — Evaluated task-level concurrency.
20. `ASE-306` — Multi-project and monorepo coordination.
21. `ASE-401` — Additional local Docker/emulator/device test adapters.
22. `ASE-402` — Governed memory.
23. `ASE-403` — Development review and test-feedback loop.
24. `ASE-404` — Continuous evaluation and autonomy-level promotion.

# Definition of Done

The implementation plan is complete when:

- [ ] L3 is proven for supported repositories through repeatable benchmark and live-task evidence.
- [ ] A low-risk task can reach a reviewable pull request without phase-by-phase human supervision.
- [ ] A work batch can produce accepted task outcomes while consolidating human decisions and continuing unrelated work.
- [ ] Every change is isolated, policy-compliant, traceable, independently reviewed, and backed by deterministic evidence.
- [ ] Human escalation occurs for risk and uncertainty rather than routine orchestration.
- [ ] Requested plan/phase/full results have honest completion status and reproducible review packages.
- [ ] Recovery from timeout, crash, cancellation, provider failure, and quality failure is tested.
- [ ] Automatic model selection is explainable, quality-gated, and reversible to fixed routing.
- [ ] No phase permits application deployment, production access or production debugging.
