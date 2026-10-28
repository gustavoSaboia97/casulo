---
name: refactoring
description: Restructures existing code without changing its behavior, in small steps with the tests green after each one. Use when asked to refactor, clean up, simplify, rename, extract, move or reorganize code, or to work on items recorded as "Refactor later".
---

# Refactoring

Goal: better structure, same behavior. The tests prove the behavior; the
`design-principles` skill says what "better" means.

## 1. Know the target

- Say in one line what the refactor is for (remove duplication, split a
  module, simplify a function) and which code it touches. A "Refactor later"
  item from a review is a good starting point.
- Never mix it with a feature or a bug fix. If you find a bug, write it down
  and finish the refactor first, or stop and fix the bug separately.

## 2. Get a safety net first

Run the tests for the code in scope. If they are missing or weak, first write
**characterization tests**: tests that pin down what the code does today,
even where it looks wrong, following the `tdd` skill. Commit them on their
own before changing anything.

## 3. Move in small steps

One move at a time, such as rename, extract a function, inline, move to
another module, merge duplicates or replace a condition with a `match`.
After each move:

1. Run the tests (at least those for the code in scope).
2. Green: continue, and commit each finished logical change, as the
   `git-workflow` skill says.
3. Red: undo the last move instead of debugging forward, then take a smaller
   step.

Do not change a test's expected result during a refactor. If a test must
change, the behavior changed: stop and say so.

## 4. Know when to stop

Stop when the target in step 1 is reached. Anything else you notice ("while
I am here") goes on a list for later, not into this change. If the refactor
grows beyond the target or touches much more code than planned, stop and
propose splitting it.

## Output

Commits of type `refactor:`, made as the `git-workflow` skill says, with the
tests green on each. Reply in a few lines: what changed in the structure, the
tests that cover it (and any characterization tests added), and what was
left for later.
