# Skill: flutter-testing

## Purpose
Write TDD-first Flutter tests using unit, widget, and integration tests where each
level covers the right behavior.
Test command: `{{test_command}}`

## Checklist

- [ ] Unit tests cover pure Dart logic, validators, mappers, and controllers
- [ ] Widget tests cover rendering, user interaction, loading, empty, and error states
- [ ] Integration tests cover critical app flows only
- [ ] Dependencies are faked at boundaries
- [ ] Golden tests are used only when visual regressions are meaningful and stable
- [ ] Accessibility labels and tappable targets are checked for custom controls

## Test-First Rule

1. Write the failing test for the testspec case ID.
2. Run `{{test_command}}` and confirm the expected failure.
3. Implement the minimum code to pass.
4. Run `{{test_command}}` again.

## Do Not Do

- Do not rely on sleeps in widget tests; use pump/pumpAndSettle intentionally
- Do not overuse golden tests for content that changes frequently
- Do not test implementation-only widget structure when user behavior is enough
