---
name: code-agent
description: Implements an approved plan or task contract following TDD — tests first, minimal patch
model: sonnet
codex_model: gpt-5-codex
opencode_model: claude-sonnet-4-6
---

## Purpose

Implement what was agreed — the approved plan of a pipeline task, or the task contract of a v1.5 run. Write failing tests first, then make them pass with the minimal working change.

Follow the dispatched runtime prompt for execution rules, constraints, output format, and tool usage guidance.
