# Skill: typescript-patterns

## Purpose
Enforce idiomatic TypeScript in this project: type safety, error handling,
async patterns, and module organization. Read this before implementing any task.

## Type Safety

- No `any` — use `unknown` for truly unknown input, then narrow it
- Prefer `type` for unions/intersections, `interface` for object shapes that may be extended
- Use discriminated unions to model state — not optional fields scattered everywhere

```ts
// good — discriminated union, no ambiguous optionals
type OrderStatus =
  | { kind: 'pending' }
  | { kind: 'confirmed'; confirmedAt: Date }
  | { kind: 'cancelled'; reason: string }

// bad — unclear which combinations are valid
interface Order {
  status: string
  confirmedAt?: Date
  reason?: string
}
```

## Error Handling

- Never throw raw strings — always throw `Error` instances or custom subclasses
- At async boundaries, use `Result`-style returns or typed catch blocks
- Validate external data at the boundary with a schema library (zod, valibot)

```ts
class OrderNotFoundError extends Error {
  constructor(id: string) {
    super(`order ${id} not found`)
    this.name = 'OrderNotFoundError'
  }
}

// validate at boundary
const body = OrderSchema.parse(req.body)  // throws with structured message if invalid
```

## Async Patterns

- Use `async/await` — avoid raw `.then()` chains
- Always `await` promises — never fire-and-forget unless explicitly intended and documented
- Use `Promise.all` for independent parallel operations

```ts
// good — parallel, explicit
const [user, orders] = await Promise.all([
  getUser(userId),
  getOrders(userId),
])

// bad — sequential when parallel is possible
const user = await getUser(userId)
const orders = await getOrders(userId)
```

## Immutability

- Prefer `const` for all declarations — use `let` only when reassignment is required
- Use spread operators or `structuredClone` over mutation
- Mark object properties `readonly` when they should not change after construction

```ts
// good
const updated = { ...order, status: 'confirmed', confirmedAt: new Date() }

// bad
order.status = 'confirmed'
order.confirmedAt = new Date()
```

## Module Organization

- Organize by feature: `src/orders/`, `src/auth/`
- One public entry point per feature: `src/orders/index.ts` re-exports the public API
- Keep types co-located with the code that uses them — not in a global `types.ts`

## Do Not Do

- No `as any` to silence type errors — fix the type
- No non-null assertions (`!`) without a comment explaining why it is safe
- No mixing `Promise` and callback styles in the same function
- No `export *` from barrel files without knowing what you're re-exporting
