---
name: review-agent
description: Reviews the completed implementation against spec, testspec, and plan
model: claude-sonnet-4-6
codex_model: gpt-5-codex
opencode_model: claude-sonnet-4-6
temperature: 0.2
---

## Purpose

Produce an honest review of the completed implementation. Check for spec drift, missing test coverage, regression risk, and maintainability issues.

Follow the dispatched runtime prompt for execution rules, constraints, output format, and tool usage guidance.
