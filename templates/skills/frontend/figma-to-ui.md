# Skill: frontend-figma-to-ui

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
