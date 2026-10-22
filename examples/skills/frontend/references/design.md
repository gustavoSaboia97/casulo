# Design guide

Each section lists what to **do**, what to **avoid**, and the default when
unsure. The default always leans to simple and consistent.

## Tokens

**Do:** keep every visual value as a token in one file:

```css
:root {
  --color-bg: #fbfaf7;
  --color-text: #1d1b16;
  --color-muted: #6b665c;
  --color-primary: #0b6e4f;
  --color-danger: #b42318;
  --space-1: 4px; --space-2: 8px; --space-3: 12px; --space-4: 16px;
  --space-6: 24px; --space-8: 32px; --space-12: 48px;
  --text-sm: 0.875rem; --text-base: 1rem; --text-lg: 1.25rem; --text-xl: 1.75rem;
  --radius: 8px;
}
```

**Avoid:** `margin: 13px`, `color: #333` inside components; a new shade for
each screen.

**Default:** spacing in multiples of 4px; 4 to 6 font sizes; one accent
color plus neutrals and status colors (success, warning, danger).

## Visual direction

**Do:** pick a direction and keep it: one or two typefaces (one for headings,
one for text), a palette with a clear accent, a density (airy or compact).
Write it at the top of the tokens file.

**Avoid:** the look every generated app has (system font only, gray cards
with shadows everywhere, purple-to-blue gradients, emoji as icons). Mixing
several icon sets or styles.

**Default:** a readable text face at 16px, line height 1.5, lines of 60 to 80
characters; one icon set.

## Hierarchy

**Do:** make the main action the only filled, accent-colored button. Show
importance by size, weight and space. Align to a grid; group related items.

**Avoid:** several primary buttons; bold, color and borders all at once;
centered body text; boxes inside boxes.

**Default:** one primary action; others as secondary or text buttons.

## States

**Do:** design each one:
- **Loading:** a skeleton or spinner where the content will be; keep the
  layout from jumping.
- **Empty:** say why it is empty and what to do next, with the action.
- **Error:** what went wrong and how to fix it, in plain words, near the
  problem; keep what the user typed.
- **Success:** confirm briefly; do not block with a modal.

**Avoid:** a blank area, "Something went wrong" with no way out, losing
input on error.

## Forms

**Do:** a visible label above each field; mark optional fields rather than
required ones; validate on blur or submit, and show the error under the field.
Disable submit only while sending, and show that it is sending.

**Avoid:** placeholders as labels; errors only in a toast; clearing the form.

## Responsive

**Do:** design for 360px first, then add columns as space grows. Touch
targets at least 44×44px. Images with set `width` and `height`.

**Avoid:** horizontal scroll, text that needs zoom, hover-only actions.

## Motion

**Do:** short transitions (150 to 250ms) that explain a change. Respect
`prefers-reduced-motion`.

**Avoid:** animation that only decorates, or delays the user.

## Final checklist

- [ ] No raw color, size or spacing outside the tokens file.
- [ ] One primary action per screen.
- [ ] Loading, empty, error and success states exist.
- [ ] Works at 360px and at desktop width, no horizontal scroll.
- [ ] `references/accessibility.md` checked.
