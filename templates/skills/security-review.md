# Skill: security-review

## Purpose

Identify security vulnerabilities in a changeset before it is merged. Covers the
most common categories: secrets exposure, injection, authentication, input handling,
and dependency risk.

## When to Use

- Before approving any feature that handles user input, authentication, or data persistence
- When a task touches external API calls, file I/O, or database queries
- When a dependency is added or upgraded

## Required Inputs

- The implementation diff or changed files
- `spec.md` — intended behavior and trust boundaries

## Expected Outputs

A `security-report.md` with findings grouped by severity:

- **CRITICAL** — must fix before merge (exploitable vulnerability, secret leak)
- **HIGH** — should fix before merge (missing auth check, unvalidated input)
- **MEDIUM** — consider fixing (overly broad permissions, weak defaults)
- **LOW** — informational (style, minor hardening opportunity)

Each finding: location → description → evidence → recommended fix.

## Checklist

- [ ] No hardcoded secrets, tokens, or credentials in changed files
- [ ] All user-supplied input is validated before use
- [ ] SQL queries use parameterized statements — no string concatenation
- [ ] HTML output is escaped — no unsanitized `innerHTML` or equivalent
- [ ] Authentication checks are present on every protected path
- [ ] Authorization checks confirm the caller owns the resource
- [ ] Error messages do not expose stack traces or internal paths to users
- [ ] New dependencies checked for known CVEs
- [ ] File paths from user input are sanitized against traversal

## Constraints

- Every finding requires a file and line reference — no vague warnings
- Do not block on LOW severity findings
- A clean automated scan does not replace manual review of critical categories

## Do Not Do

- Do not approve with "no issues found" without checking every category
- Do not implement fixes — only identify and describe them
- Do not inflate LOW findings into blockers
- Do not ignore dependency additions — they are part of the attack surface
