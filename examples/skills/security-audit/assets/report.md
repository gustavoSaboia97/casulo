# Security audit: <project, branch or pull request>

**Result:** <No exploitable findings | N findings, the worst is <severity>>

## Attack surface

- **Entry points:** <...>
- **Trust boundaries:** <...>
- **Sensitive assets:** <...>
- **Sanitization in place:** <where input is validated, and how>

## Critical

1. **`<file>:<line>`** · <OWASP category>: <the vulnerability, in one sentence>
   - **Attack path:** <input → code → harm>
   - **Impact:** <what an attacker gets>
   - **Rating:** Likelihood <level> (<why>) · Impact <level> (<why>) → <severity>
   - **Fix needs:** <the change and where; effort small/medium/large;
     dependencies such as an upgrade, config or migration; the test that proves it>

## High

1. <same format>

## Accepted risks (medium and low)

| Severity | Where | OWASP category | Risk | Why it is accepted | Suggested fix | Revisit when |
|---|---|---|---|---|---|---|
| <medium/low> | `<file>:<line>` | <...> | <one line> | <reason> | <one line> | <trigger or date> |

## Questions

- <a path that depends on something not visible, and what would confirm it>

## Not covered

- <areas skipped, tools that could not run and why>

<Delete empty sections.>
