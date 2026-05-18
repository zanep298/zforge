# Skill: frontend-testing

## Purpose
Write frontend tests that assert user-visible behavior instead of implementation
details. Use this before adding or changing UI tests.

## Checklist

- [ ] Tests query by role, label, text, or accessible name where possible
- [ ] Happy path, validation errors, loading, empty, and failure states are covered
- [ ] Network/data dependencies are faked at the boundary
- [ ] Tests avoid brittle snapshots for dynamic UI
- [ ] Critical flows have integration or e2e coverage when unit tests are insufficient
- [ ] Assertions verify behavior and visible output, not component internals

## Do Not Do

- Do not assert private state or hook implementation details
- Do not mock the component under test
- Do not use snapshots as the only assertion for interactive UI
