# Skill: flutter-patterns

## Purpose
Enforce maintainable Flutter and Dart patterns. Use this before implementing any
Flutter feature or refactor.

## Architecture

- Organize by feature first, then by concern inside the feature
- Keep widgets small and presentation-focused
- Put business rules in services, controllers, notifiers, blocs, or use cases
- Use immutable models and explicit copy/update methods
- Inject dependencies instead of reading globals directly

## Dart Rules

- Prefer `final` by default
- Model states with sealed classes or explicit unions where available
- Avoid `dynamic`; parse external data into typed models at the boundary
- Keep async errors explicit and testable

## Do Not Do

- Do not put network/database calls inside widget build methods
- Do not call `setState` after async work without checking `mounted`
- Do not create deeply nested widget trees when extracted widgets improve clarity
