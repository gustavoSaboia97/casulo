---
name: design-principles
description: Applies KISS, SOLID, DRY and YAGNI, in that priority, to design and refactoring decisions, with a rule for when they conflict. Use when designing a module or API, choosing between implementations, adding an abstraction, deciding whether to remove duplication, or judging code structure.
---

# Design principles

Priority: **KISS > SOLID > DRY > YAGNI**. When two pull in different
directions, the one higher in the list wins.

## KISS: the simplest thing that works

- Plain functions and data first; types, traits or classes only when they
  remove real complexity.
- Fewer moving parts: no layer, wrapper or config option nobody asked for.
- Readable beats clever. If a newcomer cannot follow it quickly, simplify.
- Smells: abstraction with one user, generic code for one case, deep
  inheritance or nesting, a helper used once that hides two lines.

## SOLID: well-separated responsibilities

- **S**: one reason to change per module, type or function. Split when a
  unit mixes concerns (parsing and printing, logic and I/O).
- **O**: extend by adding a case or an implementation, not by editing a
  growing `if` chain, but only once there are several cases.
- **L**: an implementation must honor its interface's promises.
- **I**: small interfaces; callers should not depend on methods they ignore.
- **D**: depend on interfaces at real boundaries (network, processes, disk,
  clock) so they can be faked in tests. Not everywhere.

Apply SOLID only as far as KISS allows: an interface with one implementation
and no test fake is complexity, not design.

## DRY: one place for each rule

- Logic that must change together lives in one place.
- Duplication that only looks alike (same shape, different reasons to change)
  is not duplication; leave it.
- Merge copies only when the result is simpler than the copies. Wait for the
  third copy if unsure.

## YAGNI: only what is needed now

- No parameters, options, hooks or extension points for imagined futures.
- Delete dead code instead of keeping it "just in case".

## How to apply

1. Before writing: pick the simplest design that meets the requirement and
   say in one line why, if there was a real alternative.
2. At each design decision (interface, extract a function, merge copies,
   split a module, add a parameter, `match` or implementations, generics, a
   new dependency), read its entry in `references/decisions.md` and follow
   its signals. When unsure, take its default.
3. After writing: check the smells above on the changed code and fix the ones
   that are cheap.
4. In a review: report a broken principle only with the concrete cost it
   causes (harder to test, two places to change, code nobody uses).

Before-and-after examples for each decision: `references/examples.md`
(in Rust; `references/languages.md` maps them to other languages).
