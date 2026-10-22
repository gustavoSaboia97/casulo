# Development

How the quality checks run on the host and in CI, and how a release is published; [AGENTS.md](../AGENTS.md) has the rules for working on casulo.

The quality checks also run as git hooks, from `.pre-commit-config.yaml`, with [prek](https://github.com/j178/prek): file checks and `cargo fmt` on commit, the commit message rule on commit-msg, clippy, tests and the branch name rule on push. Install them once on the host with `prek install`. They never run in the casulo container. On every pull request, CI runs the file checks and checks each commit message and the title.

## Releases

Every push to `main` runs the Release workflow (`.github/workflows/release.yml`). It reads the version in `Cargo.toml`: when tag `v<version>` exists, it does nothing; when it does not and `docs/releases/<version>.md` exists, it runs the CI checks on Linux, macOS and Windows, builds the binaries, then creates the tag and the GitHub release with that file as its notes. CI fails a pull request whose version has no notes, and the Release workflow fails if one reaches `main` anyway. A failed check or build publishes nothing and creates no tag, so the next push to `main` tries again. [AGENTS.md](../AGENTS.md#releases) says how to bump the version and write the notes.
