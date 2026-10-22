---
name: tdd
description: Develops with TDD driven by BDD scenarios (Given/When/Then), writing only the functional, regression and unit tests that protect real behavior. Use when implementing a feature, fixing a bug, or when asked for tests, TDD, BDD, regression tests or coverage.
---

# Scenario-driven TDD

Every test protects a behavior someone would notice if it broke.
A few right tests are worth more than many obvious ones.

## 1. List the scenarios before the code

```
Scenario: <behavior, in the words of whoever uses it>
  Given <initial state>
  When <action>
  Then <observable result>
```

If the requirements or the plan already list the scenarios, use those.
Otherwise cover only the main path, the boundary of each rule (the value that
passes and the one that fails) and each error the user sees, and for a new or
ambiguous task show the list to the user before going on. For a bug, the
scenario is the bug.

## 2. Pick one level per scenario

- **Functional** (default): through the public entry point (CLI, API, exported function).
- **Unit**: pure logic with many branches, where a functional test cannot reach.
- **Regression**: every fixed bug gets a test that failed before the fix.

## 3. Red, green, refactor

One scenario at a time:

1. Write the test and run it. It must fail **for the expected reason** (an
   assertion, not a compile error). If it passes on the first run, review it.
2. Write the least code that makes it pass.
3. Refactor with the suite green and run it again.

Never change an existing test just to make it pass. If the expected behavior
changed, say so.

## 4. Write the test

Every test must have three blocks, in this order and once each:

```
// Given  (setup)  prepare the state
// When   (run)    one single action
// Then   (assert) check the observable result
```

Tests must be simple: no `if`, loops or logic of their own, little setup and a
single action. If a test does not fit that (huge setup, many mocks, several
actions), the problem is usually the code: propose simplifying the design
(see the `design-principles` skill) instead of complicating the test.

Before writing, read `references/writing.md` (rules and what **not** to test)
and `references/examples.md` (examples per language).

Follow the project's test conventions (`AGENTS.md`, `CLAUDE.md`, existing
tests). Do not add a BDD framework if the project does not use one.

## When done

Run the whole suite and answer in a few lines: scenarios covered and at which
level, what you chose **not** to test and why, and the suite result.
