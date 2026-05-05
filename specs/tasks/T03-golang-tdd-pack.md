# T03 — golang-tdd Pack Content

**Phase:** 2  
**Priority:** High  
**Depends on:** T01  
**Status:** TODO

## Goal

Populate `packs/golang-tdd/` with all agents, skills, rules, hooks, and templates for the Golang TDD workflow.

## Directory Layout

```
packs/golang-tdd/
├── pack.yaml
├── agents/
│   ├── spec-writer.md
│   ├── test-designer.md
│   ├── planner.md
│   ├── go-implementer.md
│   ├── go-tester.md
│   ├── go-reviewer.md
│   └── refactor-agent.md
├── skills/
│   ├── write-table-driven-test.md
│   ├── create-gomock.md
│   ├── implement-repository.md
│   ├── implement-service.md
│   ├── implement-handler.md
│   └── handle-error-go.md
├── rules/
│   ├── golang.md
│   └── tdd.md
├── hooks/
│   ├── pre-validate-spec.sh
│   ├── pre-validate-coverage.sh
│   ├── post-run-tests.sh
│   └── post-generate-report.sh
└── templates/
    ├── spec.tmpl
    ├── testspec.tmpl
    ├── plan.tmpl
    ├── code.tmpl
    ├── verify-analysis.tmpl
    └── review.tmpl
```

## Agent Responsibilities

| Agent | Input | Output | Gate |
|-------|-------|--------|------|
| spec-writer | raw task description | structured spec.md | human approve |
| test-designer | spec.md | testspec.md (table-driven) | human approve |
| planner | spec + testspec | plan.md (steps) | human approve |
| go-implementer | plan + testspec | Go source files | — |
| go-tester | source files | `go test ./...` result | — |
| go-reviewer | source + tests | review.md with issues | — |
| refactor-agent | review findings | refactored source | — |

## Rules Content

### rules/golang.md

- Always use `context.Context` as first arg
- Table-driven tests with `t.Run`
- No global state (use dependency injection)
- Error wrapping: `fmt.Errorf("...: %w", err)`
- Interface-first design (define interface before implementation)
- `errors.Is` / `errors.As` for inspection

### rules/tdd.md

- No implementation file before test file exists
- Tests must fail on first run (RED verified)
- Minimal implementation only (no gold plating)
- Refactor only after green
- Coverage gate: 80% minimum

## Skills Content

| Skill | Purpose |
|-------|---------|
| write-table-driven-test | Pattern for `[]struct{ name, input, want }` tests |
| create-gomock | Generate mock from interface via `mockgen` |
| implement-repository | Interface + concrete DB impl pattern |
| implement-service | Business logic layer with injected repos |
| implement-handler | HTTP handler with request parsing + response writing |
| handle-error-go | Sentinel errors, wrapping, custom error types |

## Hook Scripts

### pre-validate-spec.sh
- Check spec.md exists for task
- Check spec.md has required sections: Goal, Inputs, Outputs, Acceptance Criteria
- Exit 1 with message if missing

### pre-validate-coverage.sh
- Run `go test -coverprofile=coverage.out ./...`
- Check coverage ≥ 80%
- Exit 1 if below threshold

### post-run-tests.sh
- Run `go test ./...`
- Output PASS/FAIL summary
- Write result to `.zforge/tasks/<ID>/test-result.txt`

### post-generate-report.sh
- Aggregate spec + testspec + plan + test-result
- Write `.zforge/tasks/<ID>/report.md`

## Templates

Extend existing `.tmpl` files in `templates/` with Go-specific prompts:

- `spec.tmpl` → add Go struct/interface identification section
- `testspec.tmpl` → require table-driven test format, mockgen usage
- `code.tmpl` → require context.Context, no global state
- `review.tmpl` → check golang.md rules compliance

## pack.yaml

```yaml
name: golang-tdd
version: "0.1.0"
language: go
description: TDD-first workflow pack for Golang backend projects
test_command: go test ./...
agents:
  - spec-writer
  - test-designer
  - planner
  - go-implementer
  - go-tester
  - go-reviewer
  - refactor-agent
```

## Files to Create

All files under `packs/golang-tdd/` (new directory, ~20 files).

## Test Cases

- After `zf init --pack golang-tdd`: all agent/skill/rule files present in `.zforge/`
- `packs/golang-tdd/pack.yaml` parseable and valid
- Hook scripts executable
- Templates render without errors
