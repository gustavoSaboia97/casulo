# Accessibility checklist

Target: WCAG 2.2 level AA. Most items are cheap when done from the start and
expensive later.

## Structure

- [ ] Semantic elements: `<header>`, `<nav>`, `<main>`, `<footer>`,
      `<button>` for actions, `<a href>` for navigation.
- [ ] One `<h1>` per page; headings in order, never skipping a level for size.
- [ ] `lang` set on `<html>`; each page has a unique `<title>`.
- [ ] Lists are `<ul>`/`<ol>`, tables of data are `<table>` with headers.

## Keyboard

- [ ] Everything works with Tab, Shift+Tab, Enter, Space and Escape.
- [ ] Focus is always visible (never `outline: none` without a replacement).
- [ ] Focus order follows the visual order; no positive `tabindex`.
- [ ] Dialogs move focus inside when opened, trap it, close on Escape, and
      return focus to what opened them.

## Text and color

- [ ] Contrast at least 4.5:1 for text, 3:1 for large text and for icons,
      borders and focus rings that carry meaning.
- [ ] Color is never the only signal: errors also have text or an icon.
- [ ] Text can grow to 200% without breaking the layout; sizes in `rem`.

## Images and icons

- [ ] Informative images have `alt` describing them; decorative ones `alt=""`.
- [ ] Icon-only buttons have an accessible name (`aria-label`).

## Forms

- [ ] Every field has a `<label for>` (a placeholder is not a label).
- [ ] Errors are linked to their field (`aria-describedby`) and announced
      (`aria-live="polite"` or moving focus to the first error).
- [ ] Required fields are marked in text, not only with color or `*`.
- [ ] `autocomplete` set on personal data fields (name, email, password).

## Dynamic content

- [ ] Loading and result messages are announced (`aria-live`, `role="status"`).
- [ ] Custom widgets (tabs, menus, comboboxes) follow the WAI-ARIA Authoring
      Practices pattern, or are replaced by native elements.
- [ ] `prefers-reduced-motion` turns off non-essential animation.

## How to check

1. Use the page with the keyboard only.
2. Run an automated checker if the project has one (axe, Lighthouse); it
   finds about a third of the problems, not all.
3. Read the page with a screen reader (VoiceOver, NVDA) for key flows.
