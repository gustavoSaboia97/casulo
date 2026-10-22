---
name: code-review
description: Reviews a diff, branch or pull request for real problems (bugs, missing tests, security, needless complexity) and reports only verified findings, ranked by severity. Use when asked to review code, a diff, a branch, a commit or a pull request, or before saying a change is ready to merge.
---

# Code review

Goal: find what would hurt if merged, prove each finding, and say it briefly.
A short review with three real problems beats a long one with thirty opinions.

## 1. Know what the change is for

- Get the diff against its base: `git diff <base>...HEAD` (or the pull
  request's diff). Note every file it touches.
- Read the intent: the PR description, the requirements and the plan if they
  exist (see the `requirements` and `implementation-plan` skills).
- Read the project's rules (`AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`).
  A broken project rule is a finding; your own taste is not.

## 2. Read the code, not only the diff

For each changed function, open the file and read its callers and what it
calls. Most real bugs live where the diff meets code it did not change.

## 3. Look for, in this order

1. **Correctness**: wrong logic, unhandled errors, edge cases (empty, zero,
   missing, very large), broken callers, wrong behavior on other platforms.
2. **Tests**: does each new behavior and each fixed bug have a test that
   would fail without the change? Judge them with the `tdd` skill: simple,
   observable, no needless tests.
3. **Security**: untrusted input reaching a command, query, path or HTML;
   secrets in code, logs or errors; weakened permissions. When the change
   touches authentication, permissions, secrets or untrusted input, also run
   the `security-audit` skill.
4. **Duplication**: logic the change repeats, either inside the diff or
   compared with code that already exists (search the codebase for similar
   names and bodies). Handle it as in step 6.
5. **Design principles**: KISS > SOLID > DRY > YAGNI, judged with the
   `design-principles` skill. Report a broken one only with its concrete cost.
6. **Conventions**: naming, error messages and structure the project's rules
   ask for. Skip anything the formatter or linter already enforces. If the
   change alters behavior, the docs it makes false (spec, README, CHANGELOG)
   change too, as the `docs` skill says.

## 4. Verify every finding

Before reporting, trace a concrete path: which input or call leads to which
wrong result. If you cannot describe it, drop the finding or mark it as a
question. Run the tests or a small check when that settles it.

## 5. Rank by severity

- **Blocking**: a bug, a high or critical security problem, a missing test
  for new behavior, a broken project rule. Must be fixed before merge.
- **Should fix**: real but limited impact, including medium or low security
  problems; fix in this change if cheap, or record them as accepted risks.
- **Nit**: small and optional. At most five; leave the rest out.

## 6. Duplicated logic: fix it or record it

First decide whether it is real duplication with "Merge duplicated code?" in
the `design-principles` skill (its `references/decisions.md`). Copies that only
look alike, or would need a flag to merge, stay as they are. For the real
ones, judge the risk of merging:
- **Low**: the copies are in the changed files, behave the same, and tests
  cover them.
- **Medium**: a few callers outside the diff, same behavior, tests cover them.
- **High**: public API or many callers, copies that differ in small ways,
  missing tests, or changes across modules.

Low or medium: do the refactor yourself, following the `refactoring` skill.
High: do not touch it; record it under "Refactor later" in the report, with
the places, why it is risky and the suggested approach.

When you only review and must not edit (for example, as a reviewer
subagent), do not refactor: list the low and medium ones under "Refactor
now" in the report, for the caller to apply with the `refactoring` skill.

## Output

Fill `assets/report.md`. Each finding: severity, `file:line`, the problem in
one sentence, the concrete failure, and a suggested fix in a line or a small
snippet. Apart from step 6, do not rewrite the code yourself unless asked.

If nothing blocks, say so first, in one line. Do not invent findings to fill
the report.
