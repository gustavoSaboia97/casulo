---
name: implementation-plan
description: Turns requirements into a short implementation plan of small, verifiable steps, each with the files it touches and the test scenarios it covers. Use after requirements are clear and before coding, or when asked to plan, break down or estimate a change.
---

# Implementation plan

Goal: a plan anyone can follow step by step, where each step leaves the
project working and tested. Describe **how**; the **what** comes from the
requirements.

## 1. Start from the requirements

Read the requirements document (see the `requirements` skill). If there is
none and the change is more than trivial, write it first. Every step of the
plan must trace back to a story or acceptance criterion.

## 2. Explore the code

Find out how things work today before deciding anything:
- the files and modules the change touches;
- existing patterns to follow (similar features, naming, tests);
- the project's conventions (`AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`),
  its spec (`docs/spec.md`) and past decisions (`docs/adr/`).

If the exploration is large and a read-only exploration subagent is available,
delegate it and ask for a summary of files, patterns and risks. Do not let it
decide anything.

## 3. Decide the approach, with the user

When there is more than one reasonable way, show the options in a few lines
each (what it is, what it costs, what it risks), recommend one and ask.
Do not ask when one option is clearly better; state it and move on.

Prefer the simplest approach that meets the requirements, and weigh options
with the `design-principles` skill. Reuse what exists before adding something
new. Any new dependency needs the user's approval. A decision that is hard to
reverse gets an ADR (see the `docs` skill).

## 4. Break it into steps

Each step:
- is small enough to review on its own (one idea, a handful of files);
- leaves the project building and its tests passing;
- lists the files it touches and the scenarios it covers (these become the
  tests in the `tdd` skill);
- says how to verify it (command to run, behavior to check).

If the change alters behavior, include a step to update the docs it makes
false (spec, README, CHANGELOG).

Order: what others depend on first; the main path before edge cases; risky or
unknown parts early, so surprises appear when they are cheap.

## 5. Name the risks

List what could go wrong or is still unknown (migrations, breaking changes,
performance, security) and how the plan handles each. Skip what does not apply.

## Output

Fill `assets/template.md` and save it where the project keeps plans; if it
has none, use `docs/plans/<feature-name>.md`. Reply in a few lines with the approach, the
number of steps and anything the user must decide.

Do not start coding until the user approves the plan.
