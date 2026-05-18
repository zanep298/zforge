# Skill: flutter-ui-patterns

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
