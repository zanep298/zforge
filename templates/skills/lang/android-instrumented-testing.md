# Skill: android-instrumented-testing

## Purpose
Use Android instrumented tests only when JVM tests cannot cover the behavior. Use
this for device APIs, navigation, Compose UI, permissions, and platform integration.

## Checklist

- [ ] Prefer local JVM tests for domain, ViewModel, and repository behavior
- [ ] Use instrumented tests for platform APIs, real navigation, permissions, and Compose UI
- [ ] Tests isolate app state and clean up persisted data
- [ ] Idling, coroutine dispatchers, and clocks are controlled where needed
- [ ] Assertions use user-visible text, content descriptions, or semantics
- [ ] CI device/emulator requirements are documented

## Do Not Do

- Do not move all tests to instrumented tests because one case needs a device
- Do not depend on execution order or shared emulator state
- Do not use sleeps instead of proper synchronization
