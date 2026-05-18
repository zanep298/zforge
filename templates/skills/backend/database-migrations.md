# Skill: backend-database-migrations

## Purpose
Plan and implement database changes safely. Use this when a task touches schema,
indexes, data backfills, or persistence behavior.

## Checklist

- [ ] Migration is reversible or has an explicit rollback note
- [ ] Large-table changes avoid long locks where the database requires it
- [ ] Backfill, deploy ordering, and compatibility with old/new code are documented
- [ ] New constraints are validated against existing data before enforcement
- [ ] Indexes support the query patterns introduced by the task
- [ ] Tests cover persistence behavior, not just in-memory service logic

## Safe Rollout Pattern

1. Add nullable columns or additive tables first.
2. Deploy code that writes both old and new shapes if needed.
3. Backfill existing data.
4. Enforce constraints only after data is valid.
5. Remove old shapes in a later task.

## Do Not Do

- Do not combine destructive schema changes with unrelated feature code
- Do not add indexes without naming the query they support
- Do not assume local test data represents production cardinality
