# Skill: android-compose-ui

## Purpose
Implement Android Jetpack Compose UI with accessible, testable, state-driven
components. Use this for Android screens, design implementation, and UI refactors.

## Checklist

- [ ] Composables are stateless where practical and receive state/events as parameters
- [ ] ViewModels expose immutable UI state and one-shot effects separately
- [ ] Loading, empty, error, disabled, and success states are represented
- [ ] Semantics and content descriptions exist for custom or icon-only controls
- [ ] Touch targets meet Material guidance
- [ ] Preview data covers meaningful states
- [ ] UI tests use semantics rather than fragile tree structure

## Do Not Do

- Do not perform network or database work in composables
- Do not pass mutable collections directly into UI state
- Do not rely on visual-only indicators for errors or selection
