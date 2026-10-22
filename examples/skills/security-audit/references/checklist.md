# Checklist

Organized by the OWASP Top 10 (2021) categories, plus input sanitization and
isolation. Go through the sections that apply to the code in scope. Each item
names what goes wrong and the safe pattern. For depth on any item, the OWASP
Cheat Sheet Series has a sheet with the same name.

## Sanitization and residual risk

Input validation or sanitization exists. Now check what it misses:
- **Denylist instead of allowlist**: blocking `<script>` or `../` misses
  variants. Safe: accept only the known-good shape (a parse into a type).
- **Wrong context**: sanitized for HTML but used in SQL, a shell, a path or a
  URL. Each sink needs its own encoding or a safe API.
- **Order of operations**: validated before decoding, normalizing (Unicode,
  case, `%2e%2e`), resolving symlinks or joining paths. Safe: decode and
  canonicalize first, then validate, then use that same value.
- **Check then use (TOCTOU)**: the file, record or permission can change
  between the check and the use.
- **Partial coverage**: one entry point validates, another path to the same
  sink does not.
- **Trust after storage**: data validated on write but treated as trusted
  when read back, after another writer could have changed it.

## Broken access control

- A check missing on one route, command or action among several.
- Checks done only in the client or the UI.
- Object ids from the user used without checking ownership (IDOR).
- Defaults that allow everything; anything from an untrusted source that can
  grant itself permissions.
- Path traversal: input in a path (`..`, absolute paths, symlinks, drive
  letters, archive entries). Safe: relative parts only, joined to a fixed
  base, result checked to stay inside it.

## Cryptographic failures

- Sensitive data sent over plain HTTP or with TLS verification disabled.
- Weak or home-made crypto; hard-coded keys or IVs; `==` to compare secrets
  instead of a constant-time comparison.
- Passwords stored without a slow hash (bcrypt, scrypt, argon2).

## Injection

- A shell string built from input (`sh -c`, `os.system`, backticks). Safe: the
  program with an argument list.
- Input that becomes a flag (a value starting with `-`). Safe: `--` before
  positional arguments, or validation.
- SQL, NoSQL, LDAP or GraphQL built by concatenation. Safe: parameters.
- HTML from input without escaping (XSS). Safe: auto-escaping; no
  `innerHTML`, `v-html`, `dangerouslySetInnerHTML` or `|safe` on input.

## Insecure design

- No limits on size, depth, count or rate (memory, CPU or cost exhaustion).
- Trust placed in something the attacker controls (a header, a file in the
  repository, a client-side flag).
- Security that depends on the user doing the right thing by default.

## Security misconfiguration

- Debug modes, verbose errors or stack traces reaching users.
- CORS that allows any origin with credentials; missing security headers.
- Cookies without `HttpOnly`, `Secure`, `SameSite`.
- Unsafe defaults that the user must remember to change.

## Vulnerable and outdated components

- Known vulnerabilities (the project's audit tool).
- Dependencies not pinned by a lockfile; abandoned packages.

## Identification and authentication failures

- Tokens that never expire, are logged, or are guessable.
- No protection against brute force on login or token checks.
- Session not invalidated on logout or password change.

## Software and data integrity failures

- Deserializing untrusted data into types that run code (pickle, YAML tags,
  Java serialization).
- Updates, plugins or scripts downloaded and run without verification.
- CI workflows that run untrusted code with secrets available.

## Security logging and monitoring failures

- Secrets or personal data in logs, error messages, panic output or
  `--dry-run` output.
- Security-relevant events (login failures, permission denials) not logged.

## Server-side request forgery (SSRF)

- Requests to URLs built from input, including redirects and DNS rebinding.
  Safe: an allowlist of hosts, resolved and checked before connecting.

## Containers and isolation (beyond the Top 10)

- Running as root, extra capabilities, the Docker socket mounted,
  `--privileged`, host paths mounted read-write without need.
- Secrets passed as command-line arguments (visible in `ps` and logs). Safe:
  the child process environment or a read-only file.
- Anything from the untrusted workspace that can change how it is isolated.
