ZFLOW – Claude Code Spec (TDD-first Golang)

0. Reference Repository

This project is inspired by:

* https://github.com/affaan-m/everything-claude-code

Use this repo as a reference only, not something to copy fully.

Key ideas to reuse from ECC:

* Agent-based workflow (agents + skills + hooks)
* Config-driven system
* Multi-tool compatibility (Claude Code, Codex, Cursor, etc.)
* MCP integration
* Memory & context system

What NOT to copy:

* Overly generic multi-language support
* Non-TDD workflows
* Prompt-heavy, non-structured logic

⸻

1. Objective

Build zforge: a TDD-first AI workflow system for Golang projects.

zforge should:

* Enforce strict TDD workflow
* Provide structured agents (not just prompts)
* Support MCP (local & remote)
* Be production-ready for backend teams

⸻

2. Core Philosophy

* Test-first (no implementation before tests)
* Human gate for critical steps
* Minimal context → high accuracy
* Deterministic workflows
* Go best practices enforced

⸻

3. High-Level Architecture

zforge
├── cli/
├── runtime/
│   ├── mcp-local/
│   └── mcp-remote/
├── packs/
│   └── golang-tdd/
│       ├── agents/
│       ├── skills/
│       ├── rules/
│       ├── hooks/
│       └── templates/
└── project (.zforge/)

⸻

4. CLI Commands

Required commands

zforge init --pack golang-tdd
zforge task import <TASK_ID>
zforge spec <TASK_ID>
zforge testspec <TASK_ID>
zforge approve <TASK_ID> testspec
zforge plan <TASK_ID>
zforge approve <TASK_ID> plan
zforge implement <TASK_ID>
zforge test <TASK_ID>
zforge review <TASK_ID>

⸻

5. TDD Workflow (STRICT)

Step 1 – Spec

* Define requirement clearly

Step 2 – Test Spec

* Define test cases FIRST
* Must include:
    * happy path
    * edge cases
    * failure cases

Step 3 – Approval (Human Gate)

* No implementation allowed before approval

Step 4 – Plan

* Break into small steps

Step 5 – Implement

* Only write code to satisfy tests

Step 6 – Test

* Run:
    * go test ./...

Step 7 – Review

* Code quality
* TDD compliance

⸻

6. Agents

Required agents

spec-writer
 test-designer
 planner
 go-implementer
 go-tester
 go-reviewer
 refactor-agent

Agent format

Each agent:

* clear responsibility
* strict boundaries
* minimal output

⸻

7. Skills

Skills are reusable building blocks.

Examples:

write_table_driven_test
create_gomock
implement_repository
implement_service
implement_handler
handle_error_go

⸻

8. Rules (IMPORTANT)

Golang rules

* Always use context.Context
* Table-driven tests required
* No global state
* Proper error wrapping

TDD rules

* No implementation without tests
* Tests must fail first
* Refactor only after passing

⸻

9. MCP Tools (CORE)

Required tools:

search_code
get_file
get_function
run_go_test
run_go_test_package
run_lint
get_git_diff
write_memory
read_memory

⸻

10. Hooks

Pre-hooks

* validate spec
* validate test coverage

Post-hooks

* run tests
* generate report

⸻

11. Memory System

.zforge/memory/
  project.md
  tasks/

Used to:

* store decisions
* reduce token usage

⸻

12. MCP Runtime

Local MCP

* Runs on developer machine

Remote MCP

* Shared for team
* Secure

⸻

13. Acceptance Criteria

System is complete when:

* Can run full TDD flow end-to-end
* Tests are always written first
* Agents follow strict roles
* Works with Claude Code
* Works with OpenCode

⸻

14. Roadmap

Phase 1

* CLI + pack system

Phase 2

* TDD workflow

Phase 3

* MCP tools

Phase 4

* Multi-agent support

Phase 5

* Remote MCP

⸻

15. Output Requirement (IMPORTANT)

When Claude Code executes this spec:

* Generate clean Go code
* Minimal changes only
* Follow TDD strictly
* Do not over-engineer

⸻

16. Summary

zforge is NOT:

* a prompt collection
* a Claude-only tool

zforge IS:

* a TDD operating system for Go development