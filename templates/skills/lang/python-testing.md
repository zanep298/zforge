# Skill: python-testing

## Purpose
Enforce TDD-first Python testing in this project. Read this before writing any test.
Test command: `{{test_command}}`

## Structure

```
src/
  orders/
    service.py
tests/
  orders/
    test_service.py     ← mirrors src/ structure
  conftest.py           ← shared fixtures
```

## Test Pattern

```python
import pytest
from orders.service import OrderService, InsufficientStockError
from tests.fakes import FakeOrderRepo

class TestOrderService:
    def test_confirms_order_when_stock_is_available(self):
        # Arrange
        repo = FakeOrderRepo(stock=10)
        svc = OrderService(repo)

        # Act
        order = svc.place_order(product_id="p1", qty=2)

        # Assert
        assert order.status == "confirmed"
        assert len(repo.saved_orders) == 1

    def test_raises_when_stock_is_zero(self):
        repo = FakeOrderRepo(stock=0)
        svc = OrderService(repo)

        with pytest.raises(InsufficientStockError) as exc_info:
            svc.place_order(product_id="p1", qty=1)

        assert exc_info.value.product_id == "p1"
```

## Naming

```python
# good
def test_returns_empty_list_when_no_orders_match()
def test_raises_order_not_found_for_unknown_id()
def test_applies_discount_for_premium_tier()

# bad
def test_order()
def test1()
def test_works()
```

## Fixtures

Use `conftest.py` fixtures for shared setup. Prefer function scope unless setup is expensive:

```python
# conftest.py
import pytest
from tests.fakes import FakeOrderRepo

@pytest.fixture
def order_repo() -> FakeOrderRepo:
    return FakeOrderRepo()

@pytest.fixture
def order_service(order_repo: FakeOrderRepo) -> OrderService:
    return OrderService(order_repo)
```

## Fakes Over Mocks

Prefer handwritten fakes — they survive refactors and give clearer failure messages:

```python
class FakeOrderRepo:
    def __init__(self, stock: int = 100) -> None:
        self._stock = stock
        self.saved_orders: list[Order] = []

    def find_stock(self, product_id: str) -> int:
        return self._stock

    def save(self, order: Order) -> None:
        self.saved_orders.append(order)
```

Use `unittest.mock.MagicMock` only for simple call-count assertions on leaf dependencies.

## Parametrize for Multiple Cases

```python
@pytest.mark.parametrize("qty,expected_error", [
    (0,   "qty must be positive"),
    (-1,  "qty must be positive"),
    (1001, "qty exceeds maximum"),
])
def test_rejects_invalid_quantities(qty: int, expected_error: str):
    svc = OrderService(FakeOrderRepo())
    with pytest.raises(ValueError, match=expected_error):
        svc.place_order(product_id="p1", qty=qty)
```

## Test-First Rule

1. Write the failing test — run `{{test_command}}` and confirm it fails
2. Write the minimal production code to make it pass
3. Run `{{test_command}}` again — confirm it passes and all existing tests still pass

## Do Not Do

- No `pytest.skip` without a comment and a reason
- No `assert True` or trivially-passing assertions
- No patching implementation details with `mock.patch` when a fake works
- No production code before the failing test is confirmed
