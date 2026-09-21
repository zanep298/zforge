---
title: zForge v2 Project Onboarding and Local Testing
status: draft
document_type: normative-design
schema_version: 2
---

# zForge v2 Project Onboarding and Local Testing

## Scope

Define how a technical lead prepares several projects for autonomous development
and how tasks obtain reproducible, isolated local test environments. This is a
v2 design target; supported stacks and adapters must be declared explicitly.

Related documents:

- [Product Direction](./product-direction.md)
- [Configuration](./configuration.md)
- [Data Model](./data-model.md)
- [Quality Gates](./quality-gates.md)
- [Workspace and Git](./workspace-and-git.md)
- [Fleet and Human Attention](./fleet-and-human-attention.md)
- [Development Handoff and Review](./delivery-and-review.md)

## Project Onboarding

The first reference application is [Taskboard](../../examples/v2-taskboard/README.md):
a dependency-free Python API with synthetic in-memory data, a digest-pinned Docker
baseline, cross-container HTTP tests and briefs for single/parallel/phased tasks.
It provides test inputs, not an implemented native-v2 onboarding adapter or proof
of agent isolation, routing quality, recovery or scheduler conformance.

V2 runs only on macOS hosts, as selected in
[ADR-003](./decisions/003-macos-host-scope.md). Onboarding checks the host
OS/version/architecture against the tested adapter matrix. Linux and Windows
hosts are out of scope. Docker is the selected agent/repository execution
boundary; no separate user- or zForge-managed VM is required. Optional Compose,
emulator and device test targets remain separate capabilities and do not broaden
host support. Required native macOS/device checks remain blocked until their
explicit host adapter passes conformance; never silently run them unsandboxed.

Each project has its own versioned profile covering repository identity, approved
product knowledge, architecture/conventions, supported roles and model policy,
build/test commands, local environment definitions and completion rules.
Organization-level templates may be reused through restrictive configuration
merging. Project source data, fixtures, secrets and mutable workspaces never leak
into another project merely because both use the same template.

Onboarding performs:

1. Inspect repository and proposed language/toolchain capabilities.
2. Locate approved knowledge and report material gaps or contradictions.
3. Validate commands, dependencies, adapter capabilities and test-only credentials.
4. Prepare a disposable workspace and local environment under policy.
5. Run baseline build/tests and classify pre-existing failures.
6. Store a `ProjectReadinessReport` with pinned inputs and supported task classes.

The report status is `ready | partially_ready | blocked | unsupported`.
Readiness is per task class/environment: unavailable device testing must not block
a documentation task. A changed toolchain, environment profile, base revision or
credential availability triggers targeted revalidation. Discovery alone cannot
claim tests passed or grant permission to execute arbitrary repository scripts.

## Environment Definition

An `EnvironmentProfile` is trusted versioned configuration:

```yaml
environment_id: auth-local
version: 1
adapter: docker_compose
adapter_version: 1
definition_refs: [compose.test.yaml]
toolchain_ref: toolchain:rust-test@1
fixture_refs: ["fixtures:auth-synthetic@1"]
resource_requirements:
  cpu_units: 2
  memory_mb: 4096
  exclusive_devices: []
network_profile_ref: local-test-services@1
credential_refs: []
lifecycle:
  setup_command_ref: command:auth-test-setup@1
  readiness_probe_ref: probe:auth-test-ready@1
  reset_command_ref: command:auth-fixtures-reset@1
  teardown_command_ref: command:auth-test-teardown@1
```

Supported adapter families are `process | docker_compose | emulator | local_device`.
The first supported project may need only `process` tests inside its Docker
runner. Additional Docker Compose services, emulators and device-test support
are optional capabilities; Docker runner isolation itself is selected in ADR-003.
E2E means end-to-end behavior testing and is distinct from encryption protocols.

Commands are registry references resolved into executable/argument arrays, with
working directories, timeouts, environment allowlists and grants. Profiles pin
image digests or exact tool/build identities, dependency definitions, fixture
versions and readiness checks. A command accepting arbitrary shell text is not
an environment isolation mechanism.

## Environment Lease and Lifecycle

Each allocation creates an `EnvironmentLease` scoped to an analysis session or
task run, using the execution ownership rules in the data model.

```text
requested -> provisioning -> checking -> ready -> in_use
in_use -> resetting -> checking
in_use -> retained | releasing
ready -> releasing
provisioning/checking/resetting -> failed
failed/retained -> releasing -> released
```

Every transition records the profile hash, owner, resource identities and
observed result. Failure or cancellation freezes execution, collects permitted
logs and reconciles allocated resources before release. Retry limits and cleanup
timeouts are bounded. Retention records expiry and exact retained resources.

An expired lease does not prove a process or container stopped. Recovery checks
ownership and resource identity, terminates or fences the old owner, and verifies
quiescence before reallocating mutable resources. An uncertain physical device
remains unavailable until reconciled.

## Concurrent Tasks in One Project

An isolated Git worktree is only one part of isolation. Allocations also isolate:

- container/Compose project identity and owned container IDs;
- port allocation, networks, volumes and database/schema names;
- test accounts, fixtures, queues and mutable caches;
- emulator/device identity and writable device state;
- artifact/log output directories.

No test run uses a shared default database or an unscoped cleanup command.
Named exclusive resources serialize only conflicting work. The scheduler may run
two independent tasks simultaneously while queueing a device-exclusive test.
Waiting work does not reserve all project capacity indefinitely.

## Local Devices

Device profiles declare supported platform, OS/build constraints, connectivity,
available storage and allocation policy. Only explicitly registered test devices
are eligible. An ordinary personal device is not presumed disposable.

The adapter allocates the device exclusively, installs an identified test build,
seeds synthetic fixtures, runs the scenario, captures permitted evidence, resets
owned app/test state and releases the lease. Whole-device erasure is never an
implicit reset operation. Disconnect, state drift or failed cleanup produces a
typed environment blocker rather than a passing result.

## Evidence and Reproduction

Gate results link the environment lease/profile, base/candidate tree, toolchain,
image or device/OS identity, fixture revision, test commands and readiness
observations. Browser/device logs, screenshots and recordings are protected
artifacts attached to exact scenarios when useful.

The reviewer package includes a reproducible command or short sequence to prepare
the environment, seed/reset data and run the relevant cases. A replay creates a
fresh workspace/lease; it never silently mutates the reviewed branch or reuses
another task's database. Historical results and replay results are shown
separately. Missing hardware or dependencies are explicit limitations.

Reproduction does not guarantee production correctness. Reports identify mocks,
simulated dependencies, untested cases and unsupported platform combinations.

## Adapter Enforcement

Runtime and environment adapters publish an enforcement matrix for filesystem,
process, network, credentials and device controls on each supported macOS host
configuration and optional test-environment adapter:
`enforced | detect_after | unsupported`.

A task requiring prevention of an external effect is ineligible when that
control is only `detect_after`. Post-run diff inspection cannot establish that
network or credential access was prevented. An initial adapter spike must prove
the selected boundaries with adversarial fixtures before autonomous execution is
promoted on that platform.

The initial spike validates the selected Docker runner on macOS and pins minimum
OS/runtime versions, image digests and tested CPU/container-architecture
combinations. Selecting Docker does not by itself establish filesystem, process,
network or credential controls. Docker's internal virtualization is not a
separately managed VM workflow for zForge.

## Development-Only Boundary

Production endpoints, accounts, credentials and devices are prohibited in v2.
This prohibition cannot be relaxed by project config, agent requests or ordinary
approval. Application deployment jobs and production diagnostic connectors are
not registered capabilities. Git hosting and explicitly allowed development CI
may receive review artifacts, but selected CI workflows must exclude deployment
and production credentials/effects. If that cannot be verified, use local output.

Test stacks use synthetic fixtures and disposable local resources. Network grants
name permitted dependency registries, model providers and development services;
an unrestricted credential-bearing connection is not needed for local testing.

## Acceptance Criteria

- [ ] Each project has a profile and scoped readiness report before supported execution.
- [ ] Baseline failures are distinct from regressions introduced by a task.
- [ ] Two same-project tasks can use independent local test resources concurrently.
- [ ] Conflicting device or shared-resource use queues only affected work.
- [ ] Setup, readiness, reset, teardown, cancellation and crash recovery are specified.
- [ ] Reviewers can reproduce supported cases with pinned environment/fixture inputs.
- [ ] Optional Docker/device capabilities have explicit support and limitation reporting.
- [ ] Production access and application deployment are excluded by policy and adapter eligibility.
