# Skill: typescript-testing

## Purpose
Enforce TDD-first TypeScript testing in this project. Read this before writing any test.
Test command: `{{test_command}}`

## Structure

```
src/
  orders/
    service.ts
    service.test.ts    ← unit test next to the module
tests/
  order-flow.test.ts   ← integration / e2e test
```

## Test Pattern (Arrange / Act / Assert)

```ts
describe('OrderService', () => {
  describe('placeOrder', () => {
    it('confirms order when stock is available', async () => {
      // Arrange
      const repo = fakeOrderRepo({ stock: 10 })
      const svc = new OrderService(repo)

      // Act
      const result = await svc.placeOrder({ productId: 'p1', qty: 2 })

      // Assert
      expect(result.status.kind).toBe('confirmed')
      expect(repo.savedOrders).toHaveLength(1)
    })

    it('throws OrderError.insufficientStock when stock is zero', async () => {
      const repo = fakeOrderRepo({ stock: 0 })
      const svc = new OrderService(repo)

      await expect(svc.placeOrder({ productId: 'p1', qty: 1 }))
        .rejects
        .toBeInstanceOf(InsufficientStockError)
    })
  })
})
```

## Naming

```ts
// good
it('returns empty array when no orders match the filter')
it('throws when required field is missing')
it('applies 10% discount for premium users')

// bad
it('works')
it('test order')
```

## Fakes Over Mocks

Prefer handwritten fakes for dependencies — they survive refactors better than auto-mocked spies:

```ts
function fakeOrderRepo(opts: { stock: number }): OrderRepository {
  const orders: Order[] = []
  return {
    async findStock(productId: string) { return opts.stock },
    async save(order: Order) { orders.push(order) },
    get savedOrders() { return orders },
  }
}
```

Use `jest.fn()` / `vi.fn()` for simple call assertions, not for complex dependency simulation.

## Test-First Rule

1. Write the failing test — run `{{test_command}}` and confirm it fails
2. Write the minimal production code to make it pass
3. Run `{{test_command}}` again — confirm it passes and all existing tests still pass

## Do Not Do

- No `test.skip` without a comment and a ticket reference
- No `expect(fn).not.toThrow()` as the only assertion — also assert the return value
- No snapshot tests for non-UI output — assert specific fields instead
- No production code before the failing test is confirmed
