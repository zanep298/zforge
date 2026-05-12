# Skill: performance-optimize

## Purpose

Identify and fix a measurable performance problem using data. No speculation —
profile first, change one thing, measure again.

## When to Use

- When a feature is measurably slow against a defined target
- When profiling or benchmarks show a specific hotspot
- Before shipping a high-traffic code path

## Required Inputs

- A baseline measurement (benchmark output, profile, or latency report)
- The target — what "fast enough" means (p99 latency, throughput, memory limit)
- The relevant source files

## Expected Outputs

A `perf-report.md` containing:

- **Baseline** — measured before any change (numbers, not impressions)
- **Hotspot** — specific function or query identified as the bottleneck
- **Hypothesis** — why this is slow and what change should help
- **Change** — what was modified (minimal, one thing at a time)
- **Result** — measured after the change, compared to baseline and target
- **Verdict** — target met / further work needed / diminishing returns

## Checklist

- [ ] Baseline measured and recorded before touching code
- [ ] Hotspot identified via profiler or benchmark — not guessed
- [ ] One change made per measurement cycle
- [ ] Result measured with same method as baseline
- [ ] Change does not regress correctness — tests still pass
- [ ] If target not met, next highest-impact hotspot identified

## Constraints

- No optimization without a baseline — "feels slow" is not a measurement
- Change one variable per cycle — otherwise the cause of improvement is unknown
- Micro-optimizations that do not move the target metric are not worth the diff

## Do Not Do

- Do not optimize code that is not on the measured hot path
- Do not trade correctness for performance without explicit approval
- Do not report improvement without numbers
- Do not optimize prematurely — profile first, then act
