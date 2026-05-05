# Skill: python-patterns

## Purpose
Enforce idiomatic Python in this project: type hints, error handling,
dataclasses, and module organization. Read this before implementing any task.

## Type Hints

- Annotate all function signatures — parameters and return types
- Use `from __future__ import annotations` for forward references
- Use `X | Y` (Python 3.10+) or `Optional[X]` for nullable types
- Prefer `Sequence[T]` over `List[T]` for read-only parameters

```python
from __future__ import annotations
from collections.abc import Sequence

def place_order(items: Sequence[OrderItem], user_id: str) -> Order:
    ...

def find_order(order_id: str) -> Order | None:
    ...
```

## Error Handling

- Define custom exception classes for domain errors — never raise bare `Exception`
- Include context in exception messages — include the value that caused the failure
- Catch specific exceptions — never bare `except:` or `except Exception:`

```python
class OrderNotFoundError(Exception):
    def __init__(self, order_id: str) -> None:
        super().__init__(f"order {order_id!r} not found")
        self.order_id = order_id

# catch specific
try:
    order = repo.find(order_id)
except OrderNotFoundError:
    return Response(status=404, body={"error": "not found"})
```

## Dataclasses and Immutability

- Use `@dataclass(frozen=True)` for value objects
- Use `@dataclass` with explicit `__init__` validation for entities
- Prefer returning new objects over mutating in place

```python
from dataclasses import dataclass

@dataclass(frozen=True)
class Money:
    amount: int   # cents
    currency: str

    def add(self, other: Money) -> Money:
        if self.currency != other.currency:
            raise ValueError(f"currency mismatch: {self.currency} vs {other.currency}")
        return Money(self.amount + other.amount, self.currency)
```

## Module Organization

- Organize by feature: `orders/`, `auth/`, `payments/`
- Each package exposes its public API from `__init__.py` — keep it small
- Use `_` prefix for internal helpers — not for everything private

## Dependencies and Injection

- Accept dependencies as constructor arguments — not imported globals
- Use `Protocol` for structural typing of dependencies (no `ABC` boilerplate)

```python
from typing import Protocol

class OrderRepository(Protocol):
    def find_by_id(self, order_id: str) -> Order | None: ...
    def save(self, order: Order) -> None: ...

class OrderService:
    def __init__(self, repo: OrderRepository) -> None:
        self._repo = repo
```

## Do Not Do

- No mutable default arguments: `def fn(items=[])` is a bug — use `None` and initialize inside
- No `type: ignore` without a comment explaining why
- No catching and silently swallowing exceptions
- No module-level code with side effects (network calls, file I/O) at import time
