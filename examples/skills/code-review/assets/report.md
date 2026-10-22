# Review: <branch or pull request>

**Verdict:** <Ready to merge | Fix blocking findings first | Questions to answer first>

<One or two sentences: what the change does and the overall impression.>

## Blocking

1. **`<file>:<line>`**: <the problem, in one sentence>
   - **Fails when:** <concrete input or call, and the wrong result>
   - **Fix:** <one line, or a small snippet>

## Should fix

1. **`<file>:<line>`**: <problem>
   - **Fails when:** <...>
   - **Fix:** <...>

## Nits

- `<file>:<line>`: <one line>

## Duplication refactored

- <commit>: <what was merged, where, and the risk level (low/medium)>

## Refactor now (low or medium risk, for the caller)

- **Where:** `<file>:<line>`, `<file>:<line>`: <what to merge, risk level, tests that cover it>

## Refactor later (high risk)

1. **<short title>**
   - **Where:** `<file>:<line>`, `<file>:<line>`
   - **Why it is risky:** <public API, many callers, copies differ, no tests>
   - **Suggested approach:** <how to merge it safely, and tests to add first>

## Questions

- <something the author must clarify before a finding can be judged>

<Delete empty sections. If there are no findings at all, keep only the verdict
and the summary.>
