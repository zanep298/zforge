# Skill: frontend-accessibility

> General checklist. The project's own conventions — its accepted `conventions.md`
> and `rules.md` (a run's prompt shows the ones that apply) — and the nearest
> existing code in the module you change override it. Follow it only where the
> project is silent; never restructure code to match it.

## Purpose
Check frontend changes for practical accessibility. Use this for forms, dialogs,
navigation, custom controls, tables, and Figma-to-code work.

## Checklist

- [ ] Every interactive element is keyboard reachable
- [ ] Focus order is logical and focus is restored after modals/menus close
- [ ] Controls have accessible names and visible labels where appropriate
- [ ] Errors are announced and linked to the affected field
- [ ] Color is not the only way to convey state
- [ ] Contrast, target size, and reduced-motion behavior are acceptable
- [ ] Semantic elements are preferred over div-based controls

## Do Not Do

- Do not suppress outlines without providing a visible replacement
- Do not use `aria-*` to paper over incorrect HTML semantics
- Do not ship custom controls without keyboard behavior
