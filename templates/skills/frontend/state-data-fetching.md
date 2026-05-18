# Skill: frontend-state-data-fetching

## Purpose
Keep frontend state and server data flows predictable. Use this when changing
forms, caches, mutations, optimistic UI, or route data loading.

## Checklist

- [ ] Server state and local UI state are not mixed without a clear reason
- [ ] Cache keys include all inputs that affect returned data
- [ ] Mutations handle pending, success, failure, and rollback behavior
- [ ] Forms define validation timing and submit disabled/loading states
- [ ] Race conditions from stale requests are handled
- [ ] Error messages are actionable and mapped to the right UI surface

## Do Not Do

- Do not duplicate server data into local state unless editing a draft
- Do not invalidate broad caches when a narrow update is enough
- Do not ignore slow network and retry/failure states
