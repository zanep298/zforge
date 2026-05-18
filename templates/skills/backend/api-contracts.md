# Skill: backend-api-contracts

## Purpose
Design and review backend API changes with stable contracts, explicit errors, and
backward compatibility. Use this before implementing routes, RPC methods, or
public service interfaces.

## Checklist

- [ ] Request and response schemas are explicit, typed, and documented
- [ ] Error responses have stable codes/messages callers can handle
- [ ] Required vs optional fields are clear
- [ ] New fields are additive unless the spec explicitly allows a breaking change
- [ ] Pagination, filtering, sorting, and idempotency behavior are defined when relevant
- [ ] Auth and ownership requirements are stated per endpoint
- [ ] Compatibility impact on existing clients is called out

## Do Not Do

- Do not change a public contract silently
- Do not expose internal errors or stack traces through API responses
- Do not rely on undocumented defaults for caller-visible behavior
