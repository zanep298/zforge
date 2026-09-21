---
title: ADR-003 macOS Host and Docker Isolation
status: accepted
document_type: architecture-decision
schema_version: 2
---

# ADR-003 — macOS Host and Docker Isolation

## Decision and Status

The product owner selected macOS as the only supported v2 orchestration host
and Docker as the isolation runtime. The CLI, controllers and file-backed state
store run on macOS; supported autonomous agent and repository-command workloads
run in Linux containers through the local Docker runtime. Linux and Windows
orchestration hosts are outside v2 scope; do not build their host backends or
require their host conformance suites for the v2 release.

Docker is accepted; a separate user-installed or zForge-managed VM is not required
or introduced. This is a runtime-boundary decision, not a claim that containment
is already proven. The Docker-on-macOS conformance spike remains a foundation
prerequisite for autonomous execution.

Docker Desktop itself runs its daemon/containers in a Docker-managed Linux VM.
"No VM" here means no additional VM workflow managed by the user or zForge, not
an assertion that Docker uses no virtualization internally.
[Docker's macOS permission documentation](https://docs.docker.com/desktop/setup/install/mac-permission-requirements/)
explains that boundary. zForge uses the Docker interface and does not provision
or operate a separate guest OS.

## Host versus Test Target

The host OS is distinct from a project's target platform or test environment.
Docker isolation for agent/repository execution is separate from optional
multi-service Docker Compose E2E stacks, emulators and registered-device testing.
A simple task may need only its runner container, with no additional test stack.
Read-only CLI queries and trusted controllers do not need containers.

Linux-container test results do not prove native macOS behavior. Tasks requiring
macOS-native tools, simulators or device access require a separately declared,
conformance-tested host adapter. They are blocked/unsupported for those required
checks until such an adapter exists; there is no silent unsandboxed-host fallback.
The Docker decision does not authorize unrestricted host command execution.

The minimum macOS version and supported CPU/version combinations are pinned in
the foundation compatibility matrix after validation. Do not infer support for
every macOS release or architecture merely from the macOS-only product decision.
Unknown or unsupported combinations fail preflight explicitly.

## Docker Runner Contract

- Allocate an isolated container/workspace per active task execution owner;
  preserve ownership and resource identity across attempts and cleanup.
- Pin image digest, agent/toolchain version and container architecture. Validate
  agent execution, authentication, output/usage parsing and cancellation in the
  container before enabling that adapter. Host CLI availability is not proof.
- Use a non-root container user, no privileged mode and the minimum tested
  capabilities, mounts and resource limits. Fail rather than broaden permissions
  implicitly when a workload is incompatible.
- Expose only prepared task code, approved context and allocated output paths.
  Do not mount the user's home, host keychain/credential directories, authoritative
  YAML store, shared Git administration storage or Docker control socket.
- Host controllers own Git publication, state writes, Docker lifecycle and local
  test-service provisioning. Agent requests use typed, scoped controller actions;
  Docker/Compose access is not granted through a raw socket or arbitrary host shell.
- Mount/export boundaries must protect the file-backed store even when the repo
  contains a `.zforge` directory. A worktree path alone is not a complete boundary.
- Network access follows enforced destination policy for model providers,
  registries and allocated test services; Docker bridge networking alone is not
  an egress allowlist. Uncontrolled host-service access and production endpoints
  remain denied. The spike must test bypass attempts, not just proxy configuration.
- Provide only scoped development/provider credential access required by the
  assignment, with explicit lifetime and redaction. Never copy ambient host auth
  wholesale into the container or bake credentials into images.
- Freeze/quiesce container writers before accepting a diff or hashing evidence;
  reconcile and terminate old owners before reusing task resources.

## Required macOS Foundation Spike

Implement and test the Docker process/sandbox adapter on macOS. For each supported
agent and gate runner, document the mechanism and actual guarantee for:

- task-workspace write scope and protection of the user's other files;
- protection of authoritative YAML state, commit manifests and writer locks;
- network destination restrictions, including production-access denial;
- test-only credential exposure and inherited process environment;
- child-process containment, cancellation, fencing and crash reconciliation;
- local filesystem locking, atomic publication and durability required by ADR-002.

Each control reports `enforced | detect_after | unsupported`. Prompt instructions,
worktree separation, permission flags and post-run diff inspection cannot be
treated as proof of controls they do not enforce. Missing required prevention
blocks adapter eligibility; macOS-only scope does not weaken safety policy.

The spike outputs an explicit supported macOS/Docker/image/architecture
configuration, enforcement matrix, adversarial fixtures and repeatable results.
If Docker cannot meet a required control, mark the workload unsupported/blocked
and report the gap. Do not add a separate VM or fall back to unrestricted native
execution. Alternative Docker-compatible runtimes are supported only after
equivalent conformance, not assumed from their CLI compatibility alone.

## Implementation Consequences

- Onboarding and doctor report host OS/version/architecture and tested adapter
  support before cost-bearing or state-changing execution.
- Foundation acceptance runs on macOS; cross-platform host abstractions are not
  a prerequisite. Keep domain/schema code independent where practical without
  building unrequested platform backends.
- Validate two concurrent same-project tasks, process cancellation and restart,
  file-store crash recovery, out-of-scope writes and forbidden network/credential
  access on the declared macOS configurations.
- Optional test devices and container/emulator environments retain their own
  readiness, isolation and lifecycle requirements.

Related: [Local Testing](../project-onboarding-and-local-testing.md),
[Runtime](../deterministic-runtime.md),
[File-Backed Persistence](./002-file-backed-persistence.md),
[Implementation Plan](../../../requirements/autonomous-software-engineering-implementation-plan.md).
