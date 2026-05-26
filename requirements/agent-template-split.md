# Agent / Template Responsibility Split

## Problem

*-agent.md files and *.tmpl files currently overlap in responsibility.

Both define:
- behavioral rules
- constraints
- "do not do" sections
- execution guidance

This creates:
- drift between duplicated instructions
- dead/unrendered template variables
- inconsistent behavior across execution paths
- tool availability gaps

The architecture boundary is currently unclear.

---

# Current Flow

text zforge spec KMS-048   └─ Engine::render(spec.tmpl)        └─ spawn claude binary             └─ stdin = rendered spec.tmpl                  ← spec-agent.md NOT loaded                  ← agent-only tool instructions lost  Claude Code spawns Agent(subagent_type="spec-agent")   └─ spec-agent.md loaded as system prompt        └─ prompt = caller message             ← rendered tmpl context NOT passed             ← {{task_id}} appears literally             ← output schema missing 

---

# Core Issue

The system currently mixes two fundamentally different layers:

| Layer | Responsibility |
|---|---|
| *-agent.md | Agent identity / system personality |
| *.tmpl | Runtime execution instructions |

Because ownership is blurred:
- rules exist in two places
- output schemas only exist in one path
- rendered variables only exist in one path
- tools may silently disappear depending on dispatch mode

---

# Target Architecture

## Principle

agent.md defines:

> WHO the agent is

tmpl defines:

> WHAT the agent must do right now

---

# Target Flow

text zforge spec KMS-048   └─ Engine::render(spec.tmpl)        └─ spawn claude binary             └─ stdin = rendered spec.tmpl  Claude Code spawns Agent(   subagent_type="spec-agent",   prompt=rendered_spec_tmpl )   └─ spec-agent.md loaded as system prompt        └─ rendered tmpl becomes runtime task prompt 

Result:
- system identity remains stable
- runtime instructions always arrive rendered
- both execution paths receive identical task context
- no duplicated rules required

---

# Ownership Model

## *-agent.md

System identity layer only.

Allowed content:
- frontmatter
- agent purpose paragraph

NOT allowed:
- rules
- constraints
- "do not do"
- output schemas
- rendered variables
- phase execution instructions
- tool usage instructions

### Target shape

markdown --- name: spec-agent description: Clarifies task requirements and produces a scoped technical spec model: claude-haiku-4-5-20251001 codex_model: gpt-5-codex opencode_model: claude-haiku-4-5 temperature: 0.2 ---  ## Purpose Turn a rough task description into an unambiguous behavioral specification.  Follow the dispatched runtime prompt for: - execution rules - constraints - output format - tool usage guidance 

---

## *.tmpl

Runtime execution layer.

Owns:
- rendered variables
- execution rules
- constraints
- output schema
- retry logic
- phase-specific behavior
- tool usage instructions

Examples:
- {{task_id}}
- {{context_files}}
- {{output_file}}
- {{if failed_tests}}

---

# Changes Required

## 1. Strip all *-agent.md files

Remove:
- ## Inputs
- ## Rules
- ## Do Not Do
- ## Output
- ## Code Search

Keep:
- frontmatter
- ## Purpose

Applies to:
- spec-agent.md
- testspec-agent.md
- plan-agent.md
- code-agent.md
- review-agent.md

---

## 2. Move all operational instructions into *.tmpl

Each tmpl file must contain:
- execution rules
- do-not-do constraints
- output schema
- tool usage instructions

Add this block to every tmpl:

markdown ## Code Search  When you need to understand existing code: - `mcp__codegraph__query` — find symbols, types, functions by name - `mcp__codegraph__context` — get AI-ready context for a task area - `mcp__codegraph__files` — browse file structure  Prefer these over grep/find for accuracy. 

Applies to:
- spec.tmpl
- testspec.tmpl
- plan.tmpl
- code.tmpl
- review.tmpl

---

## 3. Consolidate duplicated rules into tmpl only

For each phase:
1. Compare agent.md vs tmpl
2. Keep the stronger/more precise wording
3. Move final wording into tmpl
4. Delete duplicated wording from agent.md

Examples:

| Phase | Candidate Rules |
|---|---|
| spec | "Derive everything from the task description" vs "Do not expand scope beyond the task" |
| plan | "Plan the minimal patch" vs "Plan only the minimal patch needed to satisfy the approved testspec" |
| code | "Keep changes minimal" vs "Do not open spec.md unless testspec leaves a gap" |

Final authority must exist only in tmpl.

---

## 4. Verify runtime dispatch behavior

This refactor assumes:

text subagent prompt = rendered tmpl 

NOT:

text subagent prompt = raw caller message 

If current runtime behavior does not pass rendered tmpl into subagents,
runtime dispatch logic must be updated.

This is required to ensure:
- rendered variables arrive correctly
- output schema is preserved
- both execution paths behave identically

---

## 5. Embedded template behavior

embedded.rs already embeds content using include_str!().

All modifications must be made under:

text templates/agents/ 

After changes:

bash zforge install 

must refresh:

text ~/.zforge/agents/ 

with updated content.

---

# Files To Change

text templates/agents/spec-agent.md templates/agents/testspec-agent.md templates/agents/plan-agent.md templates/agents/code-agent.md templates/agents/review-agent.md  templates/agents/spec.tmpl templates/agents/testspec.tmpl templates/agents/plan.tmpl templates/agents/code.tmpl templates/agents/review.tmpl 

Runtime dispatch code may also require updates if rendered tmpl is not currently forwarded into subagent prompts.

---

# Acceptance Criteria

- [ ] Every *-agent.md contains only:
  - frontmatter
  - one short ## Purpose section

- [ ] No rendered variables appear in any *-agent.md

- [ ] No execution rules appear in any *-agent.md

- [ ] Every *.tmpl owns:
  - execution rules
  - constraints
  - do-not-do sections
  - output schema
  - tool usage instructions

- [ ] No rule text is duplicated between an agent and its tmpl

- [ ] mcp__codegraph__* guidance exists only in tmpl files

- [ ] Both execution paths receive rendered tmpl context

- [ ] {{task_id}} never appears literally during runtime execution

- [ ] zforge install refreshes ~/.zforge/agents/

- [ ] Existing phase outputs remain behaviorally compatible:
  - spec
  - testspec
  - plan
  - code
  - review