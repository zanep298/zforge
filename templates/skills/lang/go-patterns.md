# Skill: go-patterns

## Purpose
Enforce idiomatic Go in this project: error handling, interfaces, composition,
and package organization. Read this before implementing any Go task.

## Error Handling

- Return `error` as the last return value — never ignore it
- Wrap errors with context using `fmt.Errorf("doing X: %w", err)`
- Define sentinel errors with `errors.New` for cases callers need to check
- Use `errors.Is` / `errors.As` for matching — never string comparison

```go
// define
var ErrNotFound = errors.New("not found")

// wrap with context
func (s *OrderService) Get(id int64) (*Order, error) {
    o, err := s.repo.FindByID(id)
    if err != nil {
        return nil, fmt.Errorf("get order %d: %w", id, err)
    }
    return o, nil
}

// check
if errors.Is(err, ErrNotFound) { ... }
```

## Interfaces

- Define interfaces at the point of use, not at the point of implementation
- Keep interfaces small — one or two methods is ideal
- Accept interfaces, return concrete types

```go
// good — defined where it's consumed
type OrderRepository interface {
    FindByID(ctx context.Context, id int64) (*Order, error)
    Save(ctx context.Context, o *Order) error
}

type OrderService struct {
    repo OrderRepository  // depends on interface
}
```

## Struct Design

- Use constructor functions (`NewOrderService`) — not bare struct literals in calling code
- Validate inputs in the constructor, not scattered across methods
- Unexport fields by default — expose behaviour through methods

```go
func NewOrderService(repo OrderRepository) (*OrderService, error) {
    if repo == nil {
        return nil, errors.New("repo is required")
    }
    return &OrderService{repo: repo}, nil
}
```

## Package Organization

- Organize by feature: `internal/orders/`, `internal/auth/`
- `internal/` for packages that must not be imported outside the module
- Avoid `util/`, `common/`, `helpers/` — put code where it belongs

## Context

- Accept `context.Context` as the first parameter on any function that does I/O
- Never store a context in a struct — pass it per call
- Respect cancellation: check `ctx.Err()` in loops and before expensive operations

## Do Not Do

- No naked `_` on error returns — always handle or explicitly document the ignore
- No `interface{}` / `any` when a concrete type or typed interface works
- No global mutable state
- No `init()` functions for anything other than registration patterns
