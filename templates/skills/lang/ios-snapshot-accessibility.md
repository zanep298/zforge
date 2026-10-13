# Skill: ios-snapshot-accessibility

> General checklist. The project's own conventions — its accepted `conventions.md`
> and `rules.md` (a run's prompt shows the ones that apply) — and the nearest
> existing code in the module you change override it. Follow it only where the
> project is silent; never restructure code to match it.

## Purpose
Add iOS snapshot and accessibility checks where they provide real regression
coverage. Use this for SwiftUI screens, reusable components, and visual states.

## Checklist

- [ ] Snapshot tests cover stable, high-value visual states only
- [ ] Dynamic data, dates, animations, and network output are deterministic
- [ ] Accessibility labels, hints, traits, and focus order are checked for custom controls
- [ ] Dynamic Type behavior is considered for text-heavy screens
- [ ] VoiceOver-visible strings are meaningful and localized where required
- [ ] Tests run on a documented simulator/device target

## Do Not Do

- Do not snapshot entire flows when component-level snapshots are enough
- Do not accept snapshot diffs without inspecting the visual change
- Do not use snapshots as a substitute for behavior tests
