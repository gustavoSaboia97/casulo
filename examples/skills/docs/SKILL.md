---
name: docs
description: Writes and updates project documentation (README, project spec, ADRs, CHANGELOG, code comments) so it stays short, true and in sync with the code. Use when creating or changing docs, recording an architecture decision, preparing a release, describing what a project is, or when a change alters behavior that docs describe.
---

# Docs

Goal: a reader (person or agent) understands the project from the docs alone,
and every line is still true. Wrong docs are worse than none: agents trust them.

## Rules for every document

- Write for the reader: what they need to use or change the project, in that
  order.
- One source of truth: each fact lives in one place; elsewhere, link to it.
- Examples must run: copy them from a real run or a test, never from memory.
- Docs change with the code, in the same pull request.
- Describe **what** and **why**. The **how** lives in the code.

Before writing, read `references/style.md` (how to write: words, sentences,
structure).

## Where each thing lives

| Document | Says | Template |
|---|---|---|
| `README.md` | What it is (short), install, quick start, links | `assets/readme.md` |
| `docs/spec.md` | The whole project: purpose, concepts, behavior, constraints | `assets/spec.md` |
| `docs/adr/NNNN-<title>.md` | Why one decision was made | `assets/adr.md` |
| `CHANGELOG.md` | What changed for users, per release | `assets/changelog.md` |
| Code comments | Why the code is the way it is | (below) |

Per-feature documents come from other skills: `docs/requirements/` from the
`requirements` skill, `docs/plans/` from the `implementation-plan` skill.
Follow the project's own layout if it already has one.

## README

Start with **What it is**: 2 to 4 sentences on what, for whom and which
problem, then **Non-goals**. Then install, a quick start that works when
copied, and links to the spec and other docs. Keep details out; link instead.

## Spec

The full description of the project, precise enough that someone could
rebuild a project that behaves the same without seeing the code.
- Describe behavior from the outside: commands or endpoints, inputs, outputs,
  errors. No function or file names: they go stale at every refactor.
- Name the constraints that must never break (security, trust, platforms).
- Link ADRs for the reasons; do not repeat them.
- When a change alters behavior, update the spec in the same pull request.
  The tests prove the behavior; the spec states the intent.

## ADR

Write one when a decision is hard to reverse, has real alternatives, or
someone will later ask "why is it like this?" (a dependency, a data format,
an architecture boundary, a security trade-off). Number them in order. Never
edit an accepted ADR's decision: write a new one that supersedes it, and mark
the old one "Superseded by NNNN".

## CHANGELOG

Follows Keep a Changelog. Add entries under `Unreleased` as changes land, in
words a user understands; build them from the `feat:` and `fix:` commits
(see the `git-workflow` skill), and skip `refactor:`, `test:` and `chore:`
unless users notice them. Mark breaking changes clearly.

## Code comments

- Public items: a doc comment saying what it does, not how.
- Other comments explain **why**: a constraint, a workaround, a non-obvious
  choice. Never restate what the code says.
- Delete comments that are no longer true. No commented-out code.

## Before finishing

1. Check every command, path and example you wrote against the code.
2. Reread what you wrote against `references/style.md` and cut what the
   reader does not need.
3. Search the docs for statements your change made false, and fix them.
4. Reply in a few lines: files changed and anything left undocumented.
