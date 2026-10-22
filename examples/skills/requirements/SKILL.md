---
name: requirements
description: Turns a request or idea into clear, testable requirements, with scope, user stories and Given/When/Then acceptance criteria. Use when starting a new feature or project, when a request is vague, or when asked for requirements, user stories, acceptance criteria or a feature spec.
---

# Requirements gathering

Goal: anyone can plan, build and test the feature from this document alone,
without guessing. Describe **what** and **why**, never **how** (that is planning).

## 1. Understand the problem

Before writing anything, find out:
- **Who** has the problem (the user or system that benefits).
- **What** they cannot do today, or what hurts.
- **Why now**: what changes when it is solved, and how we will know.

Read what already exists first: the request, the README, the project
spec (`docs/spec.md`), related code and docs.
Do not ask what you can find out yourself.

## 2. Ask only what blocks

Ask the user only questions whose answer changes the scope or the behavior.
Ask them together, in one message, each answerable in a word or a pick from
options, with your suggested answer marked.

For anything else, choose a reasonable default and record it as an
**assumption**, so the user can correct it later.

## 3. Draw the scope

- **In scope**: what this delivery does.
- **Out of scope**: what it deliberately does not do, even if it seems related.
  This list prevents scope creep; keep it explicit.

Prefer the smallest version that solves the problem. Put the rest under
"Later".

## 4. Write the stories and criteria

One story per user goal:

```
As a <who>, I want <what>, so that <why>.
```

Each story gets acceptance criteria in Given/When/Then. They become the test
scenarios (see the `tdd` skill), so each one must be:
- **observable**: says what the user sees or gets, not internal state;
- **specific**: real values and messages, not "works correctly" or "is fast";
- **one behavior** per criterion.

Cover the main path, the boundaries of each rule and each error the user sees.

## 5. Non-functional, only when relevant

Add only those that apply, with a number or a clear rule: security (who can do
what, sensitive data), performance, compatibility (platforms, browsers),
accessibility, limits. Skip the section if none applies.

## Output

Fill `assets/template.md` and save it where the project keeps requirements; if it has
none, use `docs/requirements/<feature-name>.md`. Then reply in a few lines with
the scope, the assumptions to confirm and the open questions.

Do not start planning or coding in this step.
