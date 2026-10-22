# Configuration

Where casulo keeps its files, and the global and project `casulo.toml`.

## Files

Everything casulo manages lives in `~/.casulo` (or `$CASULO_HOME`), outside your repositories:

```
~/.casulo/
  config/casulo.toml       # global settings
  harnesses/<name>.toml    # optional: override or add agent manifests
  agent/                   # your agent setup, given to every harness
    AGENTS.md              # instructions for every agent
    skills/<name>/SKILL.md # Agent Skills
    agents/<name>.md       # subagents
    mcp/<name>.toml        # MCP servers
  edits/                   # copies being edited by casulo add and edit
  projects/<name>/
    casulo.toml            # project settings, outside the repository
    workspace/             # the cloned repository, mounted at /workspace (not used with casulo new <dir>)
    build/<harness>/       # the generated Dockerfile
    config/<harness>/      # the agent's login, settings and history, mounted on every run
    masks/                 # empty files that cover masked paths
    push/                  # git config and hook for pushing, only when push is on in [git]
    generated/<harness>/   # your subagents and MCP config rendered for each agent, on every run
    compose/               # the docker compose model you approved
```

`casulo setup` creates the casulo home and writes `config/casulo.toml` with every option commented out next to its default value, like the `.zshrc` oh-my-zsh installs. To change an option, remove the `# ` in front of it. casulo never rewrites the file, so your edits and comments stay:

```toml
# Name or path of the container runtime binary.
# runtime = "docker"

# Agent used when neither --harness nor the project chooses one.
# `casulo harness list` shows the available agents.
# default_harness = "claude"

# Editor for `casulo config`. Split on spaces, so "code --wait" works.
# CASULO_EDITOR overrides it. When neither is set, casulo uses VISUAL, then
# EDITOR, then vi (notepad on Windows).
# editor = "vi"

# [git]
# Identity of the commits the agent makes. Not set by default.
# name = "Your Name"
# email = "you@example.com"
# ...
```

`casulo config` opens this file in your editor (creating it first if needed) and checks it when you close the editor, so a typo shows up right away:

```sh
$ CASULO_EDITOR="code --wait" casulo config
error: the file was saved with an error; run `casulo config` to fix it: invalid configuration in /home/you/.casulo/config/casulo.toml: TOML parse error at line 7, column 11
```

The editor is chosen like git does: `CASULO_EDITOR`, then `editor` in the global `casulo.toml`, then `VISUAL`, then `EDITOR`, else `vi` (`notepad` on Windows). `casulo config --path` only prints the file's path, for scripts, and `--project <name>` works on a project's file instead.

Running `casulo setup` again only creates what is missing. `casulo new` writes each project's `casulo.toml` the same way, with the values it fills in and the rest commented:

```toml
# Git URL the workspace was cloned from.
repository = "https://github.com/you/project"

# Agent for this project, overriding the global default_harness.
# harness = "codex"

# Workspace paths the agent sees as empty and read-only: files that hold
# secrets, and agent config the repository ships, which could otherwise
# grant your agent permissions, hooks, MCP servers or subagents. Remove an
# entry only for a repository you trust.
mask = [".env", ".mcp.json", ".claude/settings.json", ".claude/agents", ".codex/agents"]
```

[Repository agent config](security.md#repository-agent-config) explains the masked agent paths.
