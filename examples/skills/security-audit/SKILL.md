---
name: security-audit
description: Audits code for exploitable security problems (injection, broken access control, leaked secrets, unsafe input handling, vulnerable dependencies) and reports only verified findings with severity and fix. Use when asked for a security review or audit, before a release, or when a change touches authentication, permissions, secrets, user input, file paths, commands or network calls.
---

# Security audit

Goal: find the risks the author is not seeing, prove each path, and say what
it takes to close it. Follows the OWASP Top 10 and the OWASP Cheat Sheets.

## 1. Set the scope

- A change: the diff against its base (`git diff <base>...HEAD`) and the code
  it touches. A whole project: start from the entry points (below).
- Read the project's rules (`AGENTS.md`, `SECURITY.md`): they may define what
  is trusted and what is not.

## 2. Map the attack surface

Before reading line by line, write down in a few lines:
- **Entry points**: where outside data comes in (CLI arguments, HTTP
  requests, files, environment, config, messages, other repositories).
- **Trust boundaries**: which data is trusted and which is not, and where one
  becomes the other.
- **Sensitive assets**: secrets, user data, permissions, the host system.

## 3. Follow untrusted data

For each entry point, follow the data to where it is used and check it
against `references/checklist.md`, organized by OWASP Top 10 category. The
highest-risk places, first:
- running commands, building queries, file paths, HTML or templates;
- authentication and permission checks (missing, bypassable, done client-side);
- secrets: in code, logs, error messages, command-line arguments, the repo;
- deserialization, redirects, outgoing requests built from input.

## 4. Check what remains after sanitization

Where input is validated or sanitized, do not stop there: look for the risk
that is left. Use the "Sanitization" section of the checklist: allowlist or
denylist, the context it was sanitized for versus where it is used, decoding
and normalization order, and values that change between the check and the
use.

Run the dependency audit tool the project has (`cargo audit`, `npm audit`,
`pip-audit`), if it is installed and the network allows it. Say if it could
not run.

## 5. Verify every finding

A finding needs a concrete path: which input, through which code
(`file:line`), causes which harm. Confirm it by reading the code or with a
local test. Never run an exploit against a real or shared system. If the path
depends on something you cannot see, report it as a question, not a finding.

## 6. Rate the risk

Rate likelihood and impact (low, medium, high) and read the severity
(low, medium, high, critical) from the matrix in `references/risk-rating.md`,
based on the OWASP Risk Rating Methodology. Write both ratings and the reason
next to each finding. Report a critical one at the top, at once.

## 7. What each severity needs

- **High and critical**: must be fixed. For each, say what the fix needs:
  the change, where, the effort (small, medium, large), anything it depends
  on (a dependency upgrade, a config change, a migration) and the test that
  proves it.
- **Medium and low**: may be accepted as a risk to fix later, but never
  silently. Record each with the reason, the suggested fix and when to
  revisit it.

Do not change code: the author applies the fixes. Exception: a secret found
in the repository. Stop and tell the user right away; removing it is not
enough, it must be revoked.

## Output

Fill `assets/report.md`: the attack surface in a few lines, then findings by
severity, each with its OWASP category, `file:line`, the attack path, the
impact and what the fix needs; then the accepted risks. End with what was
not covered (areas skipped, tools that could not run).

If nothing exploitable was found, say so first, in one line, and list what
was checked. Do not inflate low findings to fill the report.
