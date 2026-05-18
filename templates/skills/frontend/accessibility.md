# Skill: frontend-accessibility

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
