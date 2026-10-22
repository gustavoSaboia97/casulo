# Harnesses

How each agent logs in, which model it uses, and the manifests that describe agents.

## Login and credentials

Each agent keeps its login, settings and history in one directory per project, `projects/<name>/config/<harness>/`, mounted at its config directory (`~/.claude` for Claude Code, `~/.codex` for Codex) on every run. casulo tells the agent where it is (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`), so Claude Code keeps its `.claude.json` there too. The rest of the container home starts empty every time. The directory is outside the workspace, so a repository can never ship it, and per project, so what one repository's agent writes there (a hook, a permission) never reaches your other projects. The first time a project runs Claude Code, casulo writes a `.claude.json` that skips its first-run setup and trusts `/workspace`; after that, casulo never changes it. `casulo shell` uses the same directory, so `claude /login` there lasts.

Agent credentials are opt-in: `casulo run` forwards the agent's API key variables (such as `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`) by name only when they are set. To use a Claude subscription in every project without logging in to each one, run `claude setup-token` once on the host and export the token as `CLAUDE_CODE_OAUTH_TOKEN`, or put it under `[secrets]` in the global `casulo.toml` (`casulo config`). The file keeps it in plain text and the shell variable wins when both are set. Without either, log in once per project inside the agent.

## Models

The model and reasoning effort of each agent go in the global `casulo.toml`, one table per agent. Names are passed through as written, since only the agent knows its models:

```toml
[harness.claude]
model = "opus"
effort = "high"

[harness.codex]
model = "gpt-5-codex"
effort = "medium"
```

A project overrides them, one value at a time, with `[models.<harness>]` in its own `casulo.toml` (`harness` is already the key that chooses the project's agent). `casulo run --dry-run` then ends with:

```
casulo/app:claude claude --model opus --effort high
casulo/app:codex codex --model gpt-5-codex -c model_reasoning_effort=medium
```

A subagent's model is set in its own file (see [Your agent setup](agent-setup.md#your-agent-setup)) and written into the file each agent reads.

## Agent manifests

Each agent is described by a manifest: how to install it, how to start it and where it keeps its state. The built-in ones are in [`harnesses/`](../harnesses). To change one, or add an agent, put a file with the same fields in `~/.casulo/harnesses/<name>.toml`, then check it with `casulo harness list`:

```toml
# ~/.casulo/harnesses/claude.toml: Claude Code, always verbose
name = "claude"
install = ["npm install -g @anthropic-ai/claude-code"]
command = ["claude", "--verbose"]
auth_env = ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"]
instructions_file = "CLAUDE.md"

# Where the agent keeps its login, settings and history, relative to the
# container home, and the variable that tells it so (optional). casulo keeps
# it in projects/<name>/config/<harness>/ between runs.
config_dir = ".claude"
config_env = "CLAUDE_CONFIG_DIR"

# Written into a new config directory, relative to it, so the agent skips
# its first-run setup. A config directory in use is never changed.
config_files = { ".claude.json" = '{"hasCompletedOnboarding": true, "projects": {"/workspace": {"hasTrustDialogAccepted": true}}}' }

# How to choose the session model and effort. Leave out what the agent lacks.
model_args = ["--model", "{model}"]
effort_args = ["--effort", "{effort}"]

# Where your setup from ~/.casulo/agent goes, relative to the container home.
# Leave out a line and that piece is not given to this agent.
[inject]
instructions = ".claude/CLAUDE.md"
skills = ".claude/skills"
agents = { dir = ".claude/agents", format = "claude-markdown" }   # or "codex-toml"
mcp = "claude-mcp-config"                                          # or "codex-config-overrides"
hooks = "claude-settings"   # memory's session hooks, in a file passed with --settings
```

A manifest replaces the built-in one of the same name entirely, so copy every field you want to keep. Paths must stay inside the container home, and an unknown `format`, `mcp` or `hooks` value is an error that lists the valid ones.
