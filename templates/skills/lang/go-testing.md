# Skill: go-testing

## Purpose
Enforce TDD-first Go testing in this project. Read this before writing any test.
Test command: `{{test_command}}`

## Structure

- Test files sit next to the code they test: `orders/service_test.go`
- Use `package orders_test` (black-box) for public API tests
- Use `package orders` (white-box) only when testing unexported internals

## Table-Driven Tests

Prefer table-driven tests for any function with multiple cases:

```go
func TestOrderService_Get(t *testing.T) {
    tests := []struct {
        name    string
        id      int64
        want    *Order
        wantErr error
    }{
        {
            name:    "returns order when found",
            id:      1,
            want:    &Order{ID: 1, Status: StatusPending},
        },
        {
            name:    "returns ErrNotFound for missing order",
            id:      999,
            wantErr: ErrNotFound,
        },
    }

    for _, tc := range tests {
        t.Run(tc.name, func(t *testing.T) {
            svc := newTestService(t)
            got, err := svc.Get(context.Background(), tc.id)
            if tc.wantErr != nil {
                if !errors.Is(err, tc.wantErr) {
                    t.Fatalf("got error %v, want %v", err, tc.wantErr)
                }
                return
            }
            if err != nil {
                t.Fatalf("unexpected error: %v", err)
            }
            // assert got == tc.want
        })
    }
}
```

## Test Helpers

Use `t.Helper()` in helpers so failures point to the call site:

```go
func assertNoError(t *testing.T, err error) {
    t.Helper()
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
}
```

## Test Interfaces (Fakes Over Mocks)

Prefer handwritten fakes over generated mocks for complex dependencies:

```go
type fakeOrderRepo struct {
    orders map[int64]*Order
    err    error  // set to simulate failures
}

func (f *fakeOrderRepo) FindByID(_ context.Context, id int64) (*Order, error) {
    if f.err != nil { return nil, f.err }
    o, ok := f.orders[id]
    if !ok { return nil, ErrNotFound }
    return o, nil
}
```

## Test-First Rule

1. Write the failing test — run `{{test_command}}` and confirm it fails
2. Write the minimal production code to make it pass
3. Run `{{test_command}}` again — confirm it passes and all existing tests still pass

## Do Not Do

- No `t.Skip` without a comment explaining when it will be unskipped
- No assertions using string comparison on errors — use `errors.Is`
- No shared mutable state between test cases in a table
- No production code before the failing test is confirmed
