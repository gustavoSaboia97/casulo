# <Project name>: spec

<Precise enough that someone could rebuild a project that behaves the same,
without seeing the code. Describe behavior from the outside; no function or
file names.>

## Purpose

<What it is, for whom, which problem it solves, and how to tell that it works.>

## Non-goals

- <what it deliberately does not do>

## Concepts

- **<Term>**: <what it means in this project, in one or two sentences.>

## Behavior

### <Command, endpoint or feature>

- **Input:** <arguments, request, files read>
- **Does:** <what happens, step by step if order matters>
- **Output:** <what the user sees or gets back; exit code or status>
- **Errors:** <each failure and the message or status it gives>

## Constraints

- <rules that must never break: security, trust boundaries, platforms,
  compatibility, performance limits>

## Architecture

<The main parts and the boundaries between them, in a few lines.>

## Decisions

- [NNNN <title>](adr/NNNN-<title>.md): <one line>
