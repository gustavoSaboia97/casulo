# Writing style

Follow the project's existing style first; these rules are the defaults.

They apply to everything this skill writes: docs, READMEs, architecture
decision records (ADRs), changelogs and code comments.

## Words

- Talk to the reader as "you", not "we".
- Pick one term for each concept and keep it everywhere.
- Spell out an abbreviation the first time: "pull request (PR)".
- Format code, commands, paths, file names and options as `code`.
- Cut filler: "please", "simply", "just", "easy", "basically", "currently",
  "eventually". Write "to", not "in order to", and "lets you", not "allows
  you to".
- Write "for example" and "that is", not "e.g." and "i.e.".
- Skip idioms and jokes: many readers translate them word by word.
- Leave out marketing words: "powerful" or "seamless" tells the reader
  nothing to check.

## Sentences

- Give each sentence one idea.
- Aim for 20 words or fewer in steps and 25 in descriptions. These are
  targets, not limits: split a longer sentence when you can.
- Prefer active voice: name who acts. "casulo mounts the project", not "the
  project is mounted".
- Say what is, not what is not: "fails", not "does not succeed".
- Choose concrete words: "stops after 30 seconds", not "may take a while".
- Put the condition before the action:
  "If the build fails, run `cargo clean`."
- Open with the subject, not "There is" or "There are".
- Write "it" only when the reader knows what it means.
- Place the subject close to its verb, and a modifier next to what it
  modifies.

## Paragraphs and lists

- Lead with the point, then the details.
- Give each paragraph one topic, and open it with a sentence that names it.
- Start a page with what the reader can do there, not "This page shows…".
- When a sentence lists more than about five items, or items that each need
  explaining, make it a list.
- Write list items in the same grammatical form.
- Give each numbered step one action.

## Headings and links

- Write headings in sentence case: "Install the binary", not "Install The
  Binary".
- Make link text say where the link goes: "see the [security model](...)",
  never "click here" or "this link".

## Before and after

- "In order to run the tests, you will simply need to execute the following
  command." becomes "To run the tests, run `cargo test`."
- "The configuration file is read at startup." becomes "casulo reads the
  configuration file at startup."
- "This section describes how secrets are handled." becomes "casulo passes
  secrets to the container through the environment, never as a command-line
  argument."
- The comment `// wait 2 seconds` becomes
  `// the server needs 2 seconds to open its port`.
- "To release, bump the version in `Cargo.toml`, which also changes
  `Cargo.lock`, write the release notes, which CI checks for, and merge to
  `main`, where a workflow tags the release." becomes:

  ```markdown
  To release:
  1. Bump the version in `Cargo.toml`; this also changes `Cargo.lock`.
  2. Write the release notes; CI checks that they exist.
  3. Merge to `main`; a workflow tags the release.
  ```

Sources: rules paraphrased from the [Google developer documentation style guide](https://developers.google.com/style) ([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)), William Strunk Jr., [The Elements of Style](https://www.gutenberg.org/ebooks/37134) (1918, public domain, Project Gutenberg) and ideas from [ASD-STE100](https://www.asd-ste100.org/) Simplified Technical English.
