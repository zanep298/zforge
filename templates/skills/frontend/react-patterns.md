# Skill: frontend-react-patterns

## Purpose
Implement React UI with clear component boundaries, predictable state, and
accessible interactions. Use this for React, Next.js, Remix, or similar projects.

## Checklist

- [ ] Components are organized by feature or route, not broad type buckets
- [ ] Server/client boundaries are explicit in frameworks that support them
- [ ] Local UI state stays local; shared state has a clear owner
- [ ] Data loading, empty, loading, and error states are represented in UI
- [ ] Interactive components expose keyboard and screen-reader behavior
- [ ] Styling follows the project design system or token conventions

## Do Not Do

- Do not add `useMemo` or `useCallback` by default without a measured need
- Do not put business rules inside render-only components
- Do not build a new UI primitive if the design system already has one
