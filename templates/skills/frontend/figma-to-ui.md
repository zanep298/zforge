# Skill: frontend-figma-to-ui

> General checklist. The project's own conventions — its accepted `conventions.md`
> and `rules.md` (a run's prompt shows the ones that apply) — and the nearest
> existing code in the module you change override it. Follow it only where the
> project is silent; never restructure code to match it.

## Purpose
Translate Figma context into maintainable UI code. Use this when a task includes
Figma MCP output or design implementation.

## Checklist

- [ ] Extract design tokens for color, spacing, type, radius, and shadow
- [ ] Map auto-layout frames to flex/grid/layout primitives intentionally
- [ ] Implement all relevant variants: default, hover/pressed, loading, error, disabled
- [ ] Preserve responsive behavior rather than hardcoding one viewport
- [ ] Name components after product concepts, not visual shapes only
- [ ] Document deviations from Figma when platform constraints require them

## Do Not Do

- Do not hardcode every pixel if the project has tokens
- Do not implement only the default variant when Figma defines states
- Do not flatten reusable components into one large screen file
