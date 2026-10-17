# Skill: flutter-ui-patterns

> General checklist. The project's own conventions — its accepted `conventions.md`
> and `rules.md` (a run's prompt shows the ones that apply) — and the nearest
> existing code in the module you change override it. Follow it only where the
> project is silent; never restructure code to match it.

## Purpose
Build Flutter UI that maps design intent to responsive, accessible widgets. Use
this for screens, reusable components, and Figma-driven UI tasks.

## Checklist

- [ ] Use theme tokens for colors, typography, shape, and spacing
- [ ] Support text scaling and common mobile breakpoints
- [ ] Represent loading, empty, error, disabled, and success states
- [ ] Use semantic widgets and `Semantics` for custom controls
- [ ] Keep tap targets at least 48x48 logical pixels unless platform guidance differs
- [ ] Avoid rebuilding expensive subtrees on unrelated state changes

## Figma Mapping

- Auto-layout vertical frames usually map to `Column` plus explicit spacing
- Auto-layout horizontal frames usually map to `Row` or `Wrap`
- Repeated component variants should become constructor parameters or enums

## Do Not Do

- Do not hardcode one device size
- Do not hide overflow issues by clipping without understanding the layout
- Do not ignore platform gestures and back navigation expectations
