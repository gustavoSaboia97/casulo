# casulo

Run any AI coding agent in a hardened, disposable container, with your project mounted where you can see every change.

> **Status: early.** The commands work; expect rough edges and breaking changes.

## Why

Running an AI coding agent directly on your machine gives it access to everything you have: SSH keys, cloud credentials, other projects, your shell history. The usual workaround is a hand-written Docker setup for each project, which works but is repetitive and easy to get wrong.

casulo automates that. One command clones a project and builds an isolated environment for it; another runs the agent of your choice inside, with the project mounted from your machine so you can review every change with the tools you already use.

## Goals

- **Any agent.** Claude Code, Codex and others are described by small declarative manifests. Switching agents is a flag, not a new setup.
- **Transparent.** The project lives on your disk and is mounted into the container. You can open it in your editor, run `git diff`, and see exactly what the agent did. `--dry-run` prints the exact container command casulo would run.
- **Closed by default.** Agent credentials and secrets are opt-in: your project's secrets never end up inside the container unless you say so. Outbound network access is still open, so the agent can reach its model's API; a network allowlist is on the [roadmap](docs/roadmap.md).
- **Cross-platform.** Linux, macOS and Windows, distributed as a single binary.

## How it works

`casulo new` clones a repository under `~/.casulo/projects/<name>/`, or uses a directory you already have. `casulo build` generates a Dockerfile and an image for each project and agent. `casulo run` starts the agent in a hardened container where the only host paths it can write are the workspace and the agent's own config directory for this project, so you review its changes on your machine with your own tools. Your instructions, skills, subagents and MCP servers in `~/.casulo/agent` reach every agent, in its own format.

## Install

Your machine only needs git and Docker (Docker Desktop or Colima on macOS). Language toolchains and the agents themselves live inside the container.

Then install casulo in one of two ways.

**From a release.** Each [release](https://github.com/gustavoSaboia97/casulo/releases) has prebuilt binaries for Linux, macOS and Windows. Download the one for your system and its `.sha256` file, check it (`sha256sum -c casulo-<target>.sha256`; `shasum -a 256 -c` on macOS; compare it with `Get-FileHash` in PowerShell on Windows), rename it to `casulo` (`casulo.exe` on Windows) and put it on your `PATH`. A release binary needs nothing else: with memory on, `casulo build` downloads the matching Linux binary for the memory image from the same release. On macOS, Gatekeeper quarantines an unsigned binary downloaded with a browser and refuses to run it: download it with `curl -LO` instead, or remove the quarantine with `xattr -d com.apple.quarantine casulo`.

Known limit: the two `linux-musl` binaries are built for the memory image itself, without the checksums `casulo build` needs to build that image from a release, so with memory on `casulo build` fails with them. On x86_64 Linux, use the `x86_64-unknown-linux-gnu` binary; on arm64 Linux, which has only the `linux-musl` one, build from source to use memory.

**From source.** You also need [Rust](https://rustup.rs).

```sh
git clone https://github.com/gustavoSaboia97/casulo
cd casulo
cargo install --locked --path .
```

Keep the clone: with memory on, `casulo build` builds the memory image from it. The image is built from the clone's current contents, so keep the clone outside any casulo workspace and do not let agents change it.

## Quick start

```sh
casulo doctor                                       # check that git and Docker are ready
casulo setup                                        # create ~/.casulo, then fix what is missing, asking first
cd "$(casulo new https://github.com/you/project)"   # clone into ~/.casulo and enter the workspace
casulo run                                          # run the agent in the hardened container
casulo run --dry-run                                # print the exact docker command instead
```

Log in once per project inside the agent, or give it an API key or token; see [docs/harnesses.md](docs/harnesses.md#login-and-credentials).

Every command and its options: [docs/commands.md](docs/commands.md).

## Commands

| Command | What it does |
|---|---|
| [`casulo new`](docs/commands.md#casulo-new) | Create a project: clone a repository and prepare its environment |
| [`casulo doctor`](docs/commands.md#casulo-doctor) | Check that git and the container runtime are ready, and how to fix them |
| [`casulo setup`](docs/commands.md#casulo-setup) | Create ~/.casulo with documented defaults, then install or fix what casulo needs, asking before each command |
| [`casulo build`](docs/commands.md#casulo-build) | Generate the project's Dockerfile for a harness and build the image |
| [`casulo run`](docs/commands.md#casulo-run) | Run the agent in a hardened container with the workspace mounted |
| [`casulo shell`](docs/commands.md#casulo-shell) | Open a bash shell in the same container the agent runs in |
| [`casulo config`](docs/commands.md#casulo-config) | Open the global casulo.toml, or a project's, in your editor |
| [`casulo add`](docs/commands.md#casulo-add), [`edit`](docs/commands.md#casulo-edit), [`list`](docs/commands.md#casulo-list), [`delete`](docs/commands.md#casulo-delete) | Manage your subagents, skills, MCP servers and workflows, checked as you save |
| [`casulo harness`](docs/commands.md#casulo-harness) | Work with harnesses, the descriptions of how to install and run each agent |
| [`casulo compose`](docs/commands.md#casulo-compose) | Work with the services of the project's docker compose file |
| [`casulo describe`](docs/commands.md#casulo-describe), [`index`](docs/commands.md#casulo-index), [`search`](docs/commands.md#casulo-search), [`promote`](docs/commands.md#casulo-promote), [`stop`](docs/commands.md#casulo-stop) `memory` | Read, search and change the project's memory of its code; `list` and `edit` also take `memory note` and `memory topic` |
| [`casulo home`](docs/commands.md#casulo-home) | Print the casulo home directory |

## Security model

casulo reduces the damage an agent can do by accident or through a malicious dependency: it limits what the agent can read, where it can write and, later, where it can connect. It is not a virtual machine. It does not protect against kernel exploits or a determined attacker with code execution inside the container. For truly hostile code, use a disposable VM.

- The agent runs as an unprivileged user with all capabilities dropped.
- The root filesystem is read-only; `/tmp` and the home are in memory, except the agent's config directory (`~/.claude` or `~/.codex`), which is mounted from the host.
- The only host paths it can write are the workspace and the agent's own config directory for this project (`projects/<name>/config/<harness>/`), which keeps its login and settings between runs, and it has no access to Docker.

Details: [docs/security.md](docs/security.md).

## Documentation

- [docs/commands.md](docs/commands.md): Every casulo command, with its options as `--help` prints them and an example.
- [docs/configuration.md](docs/configuration.md): Where casulo keeps its files, and the global and project `casulo.toml`.
- [docs/agent-setup.md](docs/agent-setup.md): Your instructions, skills, subagents, MCP servers and workflows, given to every agent.
- [docs/harnesses.md](docs/harnesses.md): How each agent logs in, which model it uses, and the manifests that describe agents.
- [docs/projects.md](docs/projects.md): Local directories, the tools a project needs, pushing, and services from docker compose.
- [docs/memory.md](docs/memory.md): The memory of its code that every agent of a project shares.
- [docs/security.md](docs/security.md): What the agent's container gets and the repository config casulo masks. Push, compose and memory describe their own limits in [projects.md](docs/projects.md) and [memory.md](docs/memory.md).
- [docs/development.md](docs/development.md): How the quality checks run on the host and in CI, and how a release is published; [AGENTS.md](AGENTS.md) has the rules for working on casulo.
- [docs/roadmap.md](docs/roadmap.md): What is done and what comes next.
- [docs/releases/](docs/releases/): What each release added, changed and fixed.

## Acknowledgements

casulo is inspired by [ai-jail](https://github.com/akitaonrails/ai-jail) by Fabio Akita, which sandboxes AI agents using the operating system's own isolation features.

## License

[GPL-3.0-only](LICENSE)
