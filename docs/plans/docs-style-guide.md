# Plan: a writing style guide for the docs example skill

**Status:** done.

## Requirements

Agents that follow the `docs` example skill (`examples/skills/docs/`) write long, padded English: filler words, passive voice, "This page shows…" openings, long sentences. The skill says only "Short sentences, no marketing" (`examples/skills/docs/SKILL.md:14`). Docs written with it should be concise technical English.

Acceptance criteria:

- A new `examples/skills/docs/references/style.md` holds the style rules. It is at most 4096 bytes (`wc -c`), like the other references (1.1 to 4.4 KB).
- Its first line after the title says to follow the project's existing style first and that these rules are defaults (wording after `examples/skills/frontend/references/vue.md:3`). It sets no rule on the serial comma or on spelling.
- It covers everything the `docs` skill writes, code comments included, and says so.
- Its rules are written in our own words, with nothing copied verbatim from any source. They come from:
  - the Google developer documentation style guide: address the reader as "you", not "we"; active voice; the condition before the instruction; sentence-case headings; code font for code, commands, paths and names; descriptive link text; one idea per sentence; a list instead of a sentence that lists; no filler ("please", "simply", "easy", "just", "currently", "eventually"); "to" instead of "in order to"; "lets you" instead of "allows you to"; no "e.g." or "i.e."; no idioms, for a global audience;
  - Strunk, The Elements of Style (1918): omit needless words; positive form; definite, concrete language; one topic per paragraph, topic sentence first; parallel form for parallel ideas; related words together;
  - Simplified Technical English (ideas only, none of its vocabulary): sentence length targets, one term per concept, one action per step, abbreviations defined on first use;
  - general practice: no marketing, no "This page shows…" opening, no "there is" or "there are", no "it" whose referent is unclear, the point first.
- Length limits are targets, not bans: about 20 words or fewer per sentence in steps and procedures, about 25 in descriptions.
- It has 3 to 5 short before/after pairs, written for this guide.
- It ends with one `Sources:` line linking the Google guide (noting CC BY 4.0), Strunk's 1918 text on Project Gutenberg (public domain) and ASD-STE100, saying the rules are paraphrased. The credit lives inside the skill folder because users copy only that folder (precedent: `examples/skills/security-audit/references/risk-rating.md:3`).
- `examples/skills/docs/SKILL.md` tells the agent to read `references/style.md` before writing (pattern: `examples/skills/tdd/SKILL.md:59`) and adds one "Before finishing" check against it. "Short sentences, no marketing" leaves `SKILL.md`; its content lives only in `style.md`. The frontmatter does not change.
- `docs/agent-setup.md`, under Examples, says in one sentence how to update one example: copy its folder again, which replaces your edits to it.
- The examples stay ten skills; `tests/examples.rs` passes unchanged. No Rust, snapshot, `AGENTS.md` or other skill changes. No version bump, so no `docs/releases/` file; the change goes into the next release's notes.

Out of scope: Rust code, `AGENTS.md`, the other example skills and agents, a lint or test that enforces the style, the STE vocabulary, Microsoft's brand voice.

## Approach

Add one reference file to the existing skill and point `SKILL.md` at it, the way `tdd`, `frontend` and `security-audit` already use `references/`. `SKILL.md` stays short and loads the guide only when the agent writes, and the skill count, `tests/examples.rs` and `docs/agent-setup.md:164` ("ten skills") stay true. Nothing in casulo reads `examples/`: `tests/examples.rs` checks only each `SKILL.md`'s frontmatter (`check_skill` in `src/application/shared/domain/resource.rs:50`) and the count, and no snapshot covers the examples, so the change is Markdown only.

Not chosen: a separate `style` skill (changes the count, the test and the docs, and splits one job across two skills); the rules inline in `SKILL.md` (it grows past what an agent should read for a small doc fix, and the other skills keep long guidance in `references/`).

## Outline of style.md

Sections in this order, each a short list of rules:

1. `# Writing style`, then the house-style line and the scope line (docs, READMEs, ADRs, changelogs, code comments).
2. `## Words`: you, not we; one term per concept; define an abbreviation on first use; code font; the filler list and its replacements; no e.g./i.e., no idioms, no marketing.
3. `## Sentences`: one idea each; the length targets; active voice; positive form; concrete words; the condition first; avoid "there is/are" and an unclear "it"; related words together.
4. `## Paragraphs and lists`: the point first; one topic per paragraph, topic sentence first; no "This page shows…"; a list for a sentence that lists; parallel items; one action per step.
5. `## Headings and links`: sentence case; descriptive link text, never "click here" or "this link".
6. `## Before and after`: 3 to 5 pairs, casulo-flavored (a padded procedure step, a passive sentence, a "This section describes…" opening, a vague code comment, a sentence that lists).
7. The `Sources:` line.

The file follows its own rules: the reviewer reads it against them.

## Steps

### 0. Commit this plan

- **Covers:** the Plan phase of `AGENTS.md`.
- **Files:** `docs/plans/docs-style-guide.md`.
- **Do:** on the branch `docs/docs-style-guide` (already created from `main`), commit only this file: `docs(plans): plan a style guide for the docs example skill`, signed off, checked with `cargo xtask commit-msg`.
- **Verify:** `cargo xtask branch-name`; `git show --stat HEAD` lists one file.

### 1. Write references/style.md

- **Covers:** every criterion on `style.md` (location, size, house style, scope, rules, targets, pairs, Sources line).
- **Files:** new `examples/skills/docs/references/style.md`.
- **Do:** write it from the Outline above, in our own words.
- **Scenarios:**
  - Given the file, `wc -c` prints at most 4096.
  - Given the file, line 3 says the project's own style comes first.
  - Given the file, it has between 3 and 5 before/after pairs, and its last line starts with `Sources:` and links the three sources.
  - Given the file, `grep -niE 'serial|oxford|british|american'` finds nothing.
  - Given each rule, no sentence matches a sentence of the Google guide or of Strunk word for word (manual review; the rules are short and written fresh).
- **Verify:** `wc -c examples/skills/docs/references/style.md`; `cargo test --test examples`; manual review against the criteria.
- **Commit:** `docs(examples): add a writing style guide to the docs skill`.

### 2. Point SKILL.md at the guide

- **Covers:** the `SKILL.md` criterion (read before writing, one check, one source of truth).
- **Files:** `examples/skills/docs/SKILL.md`.
- **Do:**
  - Line 13-14: the bullet becomes "Write for the reader: what they need to use or change the project, in that order." ("Short sentences, no marketing." is dropped; `style.md` covers both.)
  - After the "Rules for every document" list: "Before writing, read `references/style.md` (how to write: words, sentences, structure)."
  - "Before finishing": a new step 2, "Reread what you wrote against `references/style.md` and cut what the reader does not need.", the others renumbered.
- **Scenarios:**
  - `grep -c 'references/style.md' examples/skills/docs/SKILL.md` prints 2.
  - `grep -c 'no marketing' examples/skills/docs/SKILL.md` prints 0.
  - `git diff main -- examples/skills/docs/SKILL.md` leaves lines 1-4 (frontmatter) untouched.
- **Verify:** `cargo test --test examples`.
- **Commit:** `docs(examples): read the style guide from the docs skill`.

### 3. Say how to update one example

- **Covers:** the `docs/agent-setup.md` criterion.
- **Files:** `docs/agent-setup.md`.
- **Do:** after the sentence that ends the Examples section ("A copied workflow has no skill yet, …"), add: "To update one example, copy its folder again (`cp -r examples/skills/docs ~/.casulo/agent/skills/`); this replaces your edits to it."
- **Scenarios:** the `cp` command matches the one in the copy block above it (same source and destination form); "ten skills" is still true.
- **Verify:** read the section; `cargo test --test examples` still counts 10 skills.
- **Commit:** `docs: say how to update one copied example`.

### 4. Check the whole change

No commit unless a check finds something.

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- `git diff --stat main` lists only the plan, `style.md`, `SKILL.md` and `docs/agent-setup.md`; `git diff --stat main -- '*.snap' src tests` is empty.
- Read `style.md` and the new `SKILL.md` lines once more against the acceptance criteria.
- Set this plan's Status to `done` in the last commit of the pull request.

## Risks

- **Licensing of the sources:** only Google (CC BY 4.0) and Strunk (public domain) are used, both paraphrased; STE contributes ideas, not text; the obra and sdsheeks repositories are not used at all. The Sources line gives the attribution CC BY asks for.
- **The guide outgrows 4 KB:** the size check in step 1 fails first; cut rules that repeat each other (Strunk's and Google's overlap on active voice and needless words, so each appears once).
- **Agents apply the rules against a project's own style:** the house-style line comes first, and no rule touches spelling or the serial comma.
- **Users with an older copy:** the copied skill does not update itself; step 3 documents how to copy it again, and the next release notes mention the change.

## Decisions for the user

- None open. One small choice made here: "Short sentences, no marketing" is removed from `SKILL.md` rather than replaced by a pointer, since the new "read `references/style.md`" line already points there.
