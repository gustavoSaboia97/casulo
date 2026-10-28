---
name: git-workflow
description: Creates branches, small Conventional Commits and pull request descriptions, safely. Use when committing, writing a commit message, creating a branch, pushing, opening or describing a pull request, or resolving a merge conflict.
---

# Git workflow

Goal: a history anyone can read and revert, one logical change at a time,
without ever losing someone's work.

## 1. Before starting

- Read the project's rules first (`AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`):
  they win over this skill.
- Check where you are: `git status` and `git branch --show-current`.
- Check the identity commits will carry: `git config user.name` and
  `git config user.email`. If either is empty, stop and ask the user; never
  invent or set one yourself.
- Never work on `main` (or `master`, `develop`). Create a branch from an
  up-to-date base: `<type>/<short-description>`, such as `feat/skill-files`
  or `fix/windows-mount`.

## 2. Commit

1. Review what changed: `git diff`. Stage only files that belong to this
   change, by name (`git add <path>`), not `git add -A` blindly.
2. Review what is staged: `git diff --staged`. Look for secrets, debug code,
   generated files and unrelated edits; leave them out.
3. Run the project's checks (format, lint, tests) before committing.
4. Commit with `--signoff` (`-s`), always, and a Conventional Commit message.
   It adds `Signed-off-by: <user.name> <user.email>` from the configured
   identity:

   ```
   <type>(<optional scope>): <what changed, imperative, lowercase, no period>

   <why, optional: at most 3 lines>
   ```

   Keep the subject under 72 characters. Add a body only when the subject
   does not explain why; then at most 3 short lines on the reason, not on what
   the diff already shows. No lists of changed files, no long explanations.

   Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, `ci`.
   Add `!` after the type for a breaking change, and explain it in the body.

One commit is one logical change that builds and passes tests on its own.
Split unrelated changes into separate commits; do not mix a refactor with a fix.

Count commits by logical change, never by plan step or by round of review.
A fix to a commit that is not pushed yet belongs in that commit: commit it
with `git commit --signoff --fixup <commit>`, then, before you finish, fold
the fixups in with `git rebase -i --autosquash <base>`, where `<base>` is
the commit the branch started from (`git merge-base HEAD main`). Set
`GIT_SEQUENCE_EDITOR=true` when no editor can open. Folding keeps the order
of the commits, so the branch's last commit stays last. Once a branch is
pushed, add new commits instead.

## 3. Push and pull request

- Push only when the user asked, or the project's rules allow it.
- Code reaches the server only through `git push`. Never create commits or
  files through an API (such as the GitHub MCP tools that write files): they
  skip the signoff, the hooks and the protected branches.
- If a push is rejected (protected branch, hook), do not work around it:
  push another branch, or tell the user.
- Open the pull request with the tool you have for it (the GitHub MCP
  server, or `gh`). Fill `assets/pull-request.md` for the description,
  unless the repository has its own template
  (`.github/pull_request_template.md`): then use that.
- Keep a pull request to one purpose. If it grows, propose splitting it.

## 4. Never, unless the user explicitly asks

- Push to `main` or another protected branch.
- Rewrite shared history: `push --force`, `rebase` or `commit --amend` on a
  branch someone else uses. On your own unpushed branch it is fine; prefer
  `--force-with-lease` if you must.
- Destroy work: `reset --hard`, `clean -fd`, `checkout -- <file>`,
  `branch -D`. Check `git status` first and say what would be lost.
- Commit secrets (`.env`, keys, tokens). If one was committed, stop and tell
  the user: removing it from history is not enough, it must be revoked.
- Skip hooks or checks (`--no-verify`).
- Commit without `--signoff`, or with someone else's identity.

## 5. Merge conflicts

Merge the base into the branch (`git merge --signoff origin/main`; on
`merge`, `-s` means strategy, so write `--signoff` in full) rather than
rebasing a shared branch. Resolve each conflict by understanding both sides;
if both changed the same logic and keeping either loses behavior, ask the
user. Regenerate lockfiles with the project's tool, never by hand. Run the
checks again before committing the merge.

## When done

Reply in a few lines: branch, commits made (one line each), whether it was
pushed, and the pull request link if one was opened.
