# Autonomous Software Engineering System

## Product Objective

Produce the highest-quality evidence-backed engineering outcomes possible while
minimizing required human active attention. Quality and safety are hard
constraints; human attention, monetary cost, and elapsed time are optimized only
after those constraints remain satisfied.

The target interaction is that a developer supplies product context, desired
outcomes, and genuine product decisions while zForge independently performs
routine analysis, planning, implementation, verification, correction, and
review. Target v2 supports safe concurrent tasks in the same project, multiple
projects and roles. A batch MAY be serial when dependencies or resources require
it; overnight execution is optional.

Success means accepted, evidence-backed results for the requested scope. Human
active-time tracking and outcomes-per-human-hour metrics are not requirements.

Detailed product semantics are defined in
[Product Direction](../docs/v2/product-direction.md) and
[Fleet and Human Attention](../docs/v2/fleet-and-human-attention.md).

## Requirement Understanding

- Versioned product, repository, and task context
- Context relevance, staleness, and conflict detection
- Automated requirement clarification and normalization
- Detection of missing, conflicting, ambiguous, or unverifiable requirements
- Measurable acceptance criteria
- Automated risk analysis and workflow selection
- Verifiable implementation planning
- Requirement-to-test-to-code-to-evidence traceability
- Task decomposition and dependency management
- External dependency and blocker management
- Consolidated, evidence-backed human questions

## Agent Orchestration

- Capability-based agent and model routing
- Agent-scoped automatic model selection for Codex, Claude Code, and other adapters
- Fully automatic runtime-agent/model-pair selection when policy permits
- Versioned model catalog, task capability profiles, and explainable routing decisions
- Quality-first selection using conservative configured priors initially and measured task-stratified evidence when available
- Explicit native-v2 fixed-model overrides with policy validation
- Shadow evaluation before promoting learned routing policies; conservative configured automatic routing may ship first
- Availability-based agent fallback
- Quality-based fallback for inadequate output
- Independent planner, coder, and reviewer roles
- Adversarial review
- Quality-first task routing and bounded correction
- Required safe task-level concurrency within one project; optional within-task parallelism
- Confidence scoring
- Risk-based human approval
- Escalation for missing information or low confidence
- Human-readable status and decision summaries
- Developer handoff when autonomous resolution is not possible

Automatic model selection is performed by the deterministic assignment router
before an agent process is spawned. A spawned agent cannot choose its own model,
weaken eligibility, or authorize fallback. The complete contract is defined in
[Automatic Model Routing](../docs/v2/model-routing.md).

## Fleet and Human Attention

- Durable work batches independent from sprint or schedule concepts
- Read-only batch preflight and task-readiness analysis
- Dependency, conflict, and knowledge-producing task ordering
- Consolidated Decision Inbox for product input, approval, and conflicts
- Continue unrelated work when one task is blocked
- Task-level isolation before within-task parallelism
- Aggregate outcomes without hiding per-task failure or limitations
- Interaction reasons and consolidated decisions without human active-time measurement

## Isolated and Safe Execution

The supported execution host is macOS only; Linux/Windows host support is outside
v2. Optional local container/emulator/device testing remains separate from host
support. Docker is selected for autonomous agent/repository isolation, with no
separately managed VM and no unrestricted native-host fallback. A Docker-on-macOS
foundation spike must validate the required controls before execution, per
[ADR-003](../docs/v2/decisions/003-macos-host-scope.md).

- Isolated workspace or worktree per task
- File-scope enforcement and change budgets
- Command, network, and permission policies
- Prompt-injection protection
- Secure secret and credential management
- Sandboxing and containment
- Supply-chain security
- Reproducible execution
- Deterministic tooling and dependency pinning
- Test-environment provisioning

## Verification and Code Quality

- Test-driven development and automated test generation
- Unit, integration, and end-to-end testing
- Property-based and fuzz testing
- Regression testing
- Coverage and mutation-testing gates
- Formatter, lint, and type-check gates
- Static analysis and security scanning
- Dependency and license auditing
- API and schema compatibility checking
- Database migration and rollback validation
- Performance and resource-regression testing
- Accessibility and visual UI validation
- Flaky-test detection
- Verifier-driven self-correction
- Root-cause analysis after failures
- Independent implementation-to-spec verification

## Execution Reliability

Persistence uses local Markdown/YAML files, not a database. Immutable YAML commit
manifests make related records, events and outbox intents visible together;
short-lived metadata locks do not serialize task execution. Recovery and
conformance requirements are defined in
[ADR-002](../docs/v2/decisions/002-file-backed-persistence.md).

- Atomic and resumable state
- Idempotent operations and crash recovery
- Task locking and concurrency control
- Retry, timeout, cost, and iteration limits
- SLA enforcement
- Cancellation propagation
- Multi-project and monorepo awareness

## Git and Delivery Automation

- Scope-aware Git staging
- Branch and worktree lifecycle management
- Commit, diff, pull-request, and merge automation
- Merge-conflict handling
- Requested result modes: plan-only, through a named phase, or full implementation
- Phase acceptance and continuation manifests without overstating completion
- Concise review packages with acceptance criteria, test cases, evidence and limitations
- Test-case feedback and targeted high-risk code review
- Documentation-impact assessment and relevant documentation updates
- Development-only CI with no deployment or production side effects

Application deployment to any environment, production connection/credentials
and production debugging are outside all v2 milestones. Local disposable test
stacks and test-device builds are permitted verification infrastructure.

## Observability and Governance

- Logs, metrics, traces, and audit trails
- Token, cost, and execution-time telemetry
- Artifact provenance and integrity
- Policy as code
- Role-based approval
- Agent quality metrics
- Benchmarks and continuous evaluation
- Acceptance quality and evidence validity per requested delivery scope
- Unnecessary-question, unsafe-assumption, rework, and escaped-defect metrics

## Knowledge and Continuous Learning

- Versioned memory with provenance and confidence
- Detection of stale or conflicting knowledge
- Review, development CI and user-supplied defect feedback into requirements and tests

## Project Onboarding and Local Testing

- Per-project knowledge, roles, commands, policies and completion profiles
- Readiness checks with pinned baseline, toolchain and capability evidence
- Versioned environment definitions, fixtures and test-only credential references
- Owned leases for setup, readiness, reset, allocation, teardown and recovery
- Isolation of ports, databases, volumes, devices and test artifacts across tasks
- Reproducible test evidence and one-command replay where supported
- Explicit adapter/platform enforcement guarantees and unsupported capabilities
- Optional Docker, emulator and local-device adapters; unavailable required E2E blocks acceptance

Canonical contracts:
[Development Handoff and Review](../docs/v2/delivery-and-review.md) and
[Project Onboarding and Local Testing](../docs/v2/project-onboarding-and-local-testing.md).
