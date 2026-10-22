# Risk rating

Based on the OWASP Risk Rating Methodology: **risk = likelihood × impact**.
Rate each side as low, medium or high, then read the severity from the matrix.

## Likelihood: how likely is it to be exploited?

Look at these factors, rate each low, medium or high, and take the level most
of them point to (on a tie, the higher one):

| Factor | Low | Medium | High |
|---|---|---|---|
| Who can reach it | insiders or admins only | authenticated users | anyone, anonymous |
| Skill needed | deep expertise, custom tooling | some security knowledge | none, or a public tool or exploit |
| How easy to find | needs the source and deep reading | visible with some probing | obvious or already public |
| Conditions needed | rare config or a chain of other bugs | a specific but common setup | none, the default setup |

## Impact: what happens if it is exploited?

Same method, with these factors:

| Factor | Low | Medium | High |
|---|---|---|---|
| Confidentiality | non-sensitive data | some internal or personal data | secrets, credentials, all user data |
| Integrity | minor, easy-to-fix changes | real data changed | code execution, any data changed |
| Availability | a slow or failed request | one feature down | the whole service or host down |
| Scope | one user or session | many users | all users, the host, other systems |

## Severity matrix

| Impact ↓ / Likelihood → | Low | Medium | High |
|---|---|---|---|
| **High** | Medium | High | Critical |
| **Medium** | Low | Medium | High |
| **Low** | Low | Low | Medium |

## In the report

For each finding, write the two ratings and the reason in one line, so the
severity can be checked: `Likelihood: high (anonymous, public tool) · Impact:
high (secrets) → Critical`.

## Dependencies with a CVE

The audit tool gives a CVSS score. Use it as the impact side, then rate
likelihood by whether this project actually reaches the vulnerable code: not
reachable lowers the severity, and say why.
