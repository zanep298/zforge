# Skill: debug

## Purpose

Diagnose the root cause of a bug or unexpected behavior before writing any fix.
Evidence-first: confirm the cause before touching code.

## When to Use

- When a task involves a bug report or unexpected behavior
- When a test fails and the reason is not immediately obvious
- Before writing a fix — to ensure the fix targets the actual cause

## Required Inputs

- Error message, stack trace, or failing test output
- Steps to reproduce (or a failing test as a reproduction)
- The relevant source files

## Expected Outputs

A `debug-report.md` containing:

- **Reproduction** — confirmed repro with exact steps or test
- **Isolation** — smallest failing case identified
- **Root cause** — specific line(s) or logic responsible
- **Evidence** — log output, assertion failure, or trace proving the cause
- **Fix boundary** — which files/functions need to change and why

## Checklist

- [ ] Bug is reproduced consistently before investigating
- [ ] Problem scope is narrowed — unrelated code ruled out
- [ ] Root cause is a specific statement, not a vague description
- [ ] At least one piece of evidence (log, assertion, trace) supports the cause
- [ ] Fix boundary is documented — no changes outside this boundary without flagging
- [ ] If multiple hypotheses existed, discarded ones are noted with reasons

## Constraints

- Do not write a fix before root cause is confirmed
- One hypothesis at a time — changing multiple things at once obscures the cause
- "It works now" is not a diagnosis — document why it works

## Do Not Do

- Do not assume the cause from the symptom description alone
- Do not change unrelated code while investigating
- Do not skip reproduction — a fix without a confirmed repro is a guess
- Do not document the symptom as the root cause ("crashes" ≠ root cause)
