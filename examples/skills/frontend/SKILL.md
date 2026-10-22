---
name: frontend
description: Builds web interfaces that are clear, accessible and consistent, with a deliberate visual direction and simple component code (Vue 3 and TypeScript by default). Use when creating or changing pages, components, layouts, forms or styles, or when asked about UI, UX, visual design or accessibility.
---

# Frontend

Goal: an interface a user understands at a glance, that works with keyboard
and screen reader, on a phone and a desktop, and code as simple as the rest
of the project.

## 1. Know the screen

From the requirements (see the `requirements` skill), write in a few lines:
- who uses the screen and the **one main thing** they come to do;
- the data it shows and the actions it offers;
- its states: loading, empty, error, success (and disabled, if any).

## 2. Use the design system, or start one

- Read what exists: tokens, components, styles. Reuse before creating.
- No tokens yet: define them in one place (CSS custom properties) before any
  component: colors, spacing scale, type scale, radius, shadows.
- Never write a raw color, size or spacing in a component; use a token. A
  value that is missing becomes a new token.
- New project: choose a visual direction (typeface, palette, density, tone)
  and say it in one line. Avoid the generic default look: system font, gray
  cards, purple gradient. See `references/design.md`.

## 3. Lay it out

Follow `references/design.md`:
- one primary action per screen; hierarchy by size, weight and space, not
  by more colors or borders;
- spacing only from the scale; related things close, unrelated things apart;
- mobile first, then wider screens; no horizontal scroll.

## 4. Build the components

Follow `references/vue.md`. In short:
- small components with one job (KISS and SOLID, judged with the
  `design-principles` skill); logic and data fetching in composables or
  services, not in templates;
- semantic HTML first (`<button>`, `<a>`, `<label>`, `<nav>`, `<main>`), ARIA
  only when no element fits;
- every state from step 1 rendered, not only the happy path;
- never `v-html` with data that is not fully trusted (XSS). When untrusted
  data reaches HTML, URLs or storage, also run the `security-audit` skill.

## 5. Test

With the `tdd` skill: test what the user sees and does, finding elements by
role, label and text, never by CSS class or component internals. One test
per scenario and per state that matters (error, empty).

## 6. Check before finishing

Go through `references/accessibility.md` and the checklist at the end of
`references/design.md`. Then, if you can run the app:
- use it with the keyboard only;
- check it at 360px wide and at desktop width;
- check text contrast in every theme the app has.

Say which checks you could not do.

## Output

Reply in a few lines: screens or components changed, tokens added, states
covered, the checks done, and anything left for the user to look at.
