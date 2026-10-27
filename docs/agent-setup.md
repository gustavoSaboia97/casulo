# Agent setup

Your instructions, skills, subagents, MCP servers and workflows, given to every agent.

## Your agent setup

Each subagent, skill, MCP server and workflow is one file or folder under `~/.casulo/agent/`, and `casulo add|edit|list|delete <agent|skill|mcp|workflow> [name]` manages them, in the style of `kubectl`. `casulo add agent reviewer` opens a commented template in your editor (the one `casulo config` uses). When you close it, the file is checked with the same code `casulo run` uses. If it is invalid, it opens again with the error on top:

```
# casulo: description is empty; say when the main agent should use this subagent
# casulo: fix the file and save it, or close it without changes to keep the last valid version
+++
name = "reviewer"
...
```

You edit a copy, so an invalid file never replaces a valid one. Closing the editor without changes (or emptying the file) saves nothing, like an empty `git commit` message. Names are slugs such as `code-reviewer`: lowercase letters, digits and single hyphens.

```sh
$ casulo list
agent     reviewer
  Reviews the diff for bugs and security problems

skill     workflow-review
  Review the branch. Use when the user asks for the review workflow.

mcp       context7
  npx -y @upstash/context7-mcp

workflow  review
  Review the branch
```

In a terminal each one is a block, with the kind and name in bold blue (set `NO_COLOR` to turn color off). When the output goes to a pipe or a file, it is one tab-separated line per resource instead (kind, name and description), for scripts.

Write instructions for every agent once, in `~/.casulo/agent/AGENTS.md` (`casulo setup` creates it with a commented example):

```markdown
# ~/.casulo/agent/AGENTS.md
Answer in English. Run the tests before saying a task is done.
```

`casulo run` mounts it read-only where each agent reads its global instructions, so the agent can follow it but not change it. `casulo run --dry-run` shows the extra mount:

```
--mount=type=bind,source=/home/you/.casulo/agent/AGENTS.md,target=/home/casulo/.claude/CLAUDE.md,readonly   # Claude Code
--mount=type=bind,source=/home/you/.casulo/agent/AGENTS.md,target=/home/casulo/.codex/AGENTS.md,readonly    # Codex
```

The project's own `CLAUDE.md` or `AGENTS.md` in the workspace still applies on top, as usual.

[Agent Skills](https://agentskills.io) work the same way, and both Claude Code and Codex read the format. Put one folder per skill in `~/.casulo/agent/skills/`:

```markdown
# ~/.casulo/agent/skills/release-notes/SKILL.md
---
name: release-notes
description: Write release notes from the commits since the last tag
---

List the commits with `git log $(git describe --tags --abbrev=0)..HEAD`, group them by type and write one line each.
```

A skill can bring files of its own, such as a template to fill in (`assets/`), longer documentation or examples (`references/`) or scripts to run (`scripts/`). Its `SKILL.md` mentions them by their path relative to the skill's folder, and the agent opens them only when it needs them. `casulo add skill <name> --file <path>` creates one in your editor, and `casulo edit skill <name> --file <path>` changes it:

```sh
casulo add skill release-notes --file assets/template.md
```

The path stays inside the skill's folder, and the skill must exist first. `casulo delete skill` removes the whole folder, files included.

`casulo run` mounts the whole `skills/` folder read-only where each agent looks for skills:

```
--mount=type=bind,source=/home/you/.casulo/agent/skills,target=/home/casulo/.claude/skills,readonly   # Claude Code
--mount=type=bind,source=/home/you/.casulo/agent/skills,target=/home/casulo/.agents/skills,readonly   # Codex
```

Subagents are specialized agents the main agent hands work to. Claude Code and Codex each have their own format, so casulo has one of its own and converts it: one Markdown file per subagent in `~/.casulo/agent/agents/`, with its settings in TOML between `+++` lines:

```markdown
# ~/.casulo/agent/agents/reviewer.md
+++
name = "reviewer"                 # must match the file name
description = "Reviews the diff for bugs and security problems"

[model]                           # optional, per agent; left out = the session model
claude = "haiku"
codex = { model = "gpt-5-codex-mini", effort = "low" }
+++

You are a code reviewer. Read the diff and report only real problems.
```

On every run, casulo checks each file (an invalid one stops the run with its path), writes it in the agent's format to `projects/<name>/generated/<harness>/agents/` and mounts that folder read-only:

```
--mount=type=bind,source=/home/you/.casulo/projects/app/generated/claude/agents,target=/home/casulo/.claude/agents,readonly   # reviewer.md
--mount=type=bind,source=/home/you/.casulo/projects/app/generated/codex/agents,target=/home/casulo/.codex/agents,readonly     # reviewer.toml
```

MCP servers give the agent tools. Write one TOML file per server in `~/.casulo/agent/mcp/`, named after the server. A server is either a command the agent starts or a URL:

```toml
# ~/.casulo/agent/mcp/context7.toml
command = ["npx", "-y", "@upstash/context7-mcp"]
env = ["CONTEXT7_TOKEN"]          # variable names, forwarded into the server
```

```toml
# ~/.casulo/agent/mcp/github.toml
url = "https://api.githubcopilot.com/mcp/"
token_env = "GITHUB_TOKEN"        # sent as Authorization: Bearer
```

Secrets are only ever named. The variables are passed into the container by name (`--env=CONTEXT7_TOKEN`, shown as `CONTEXT7_TOKEN=***` in `--dry-run`), and the agent reads the value from its environment. Claude Code gets a generated JSON file that refers to `${CONTEXT7_TOKEN}`, and Codex gets `-c` overrides:

```
... --mount=type=bind,source=/home/you/.casulo/projects/app/generated/claude/mcp.json,target=/home/casulo/.casulo/mcp.json,readonly ... claude --mcp-config /home/casulo/.casulo/mcp.json
... codex -c 'mcp_servers.context7.command="npx"' -c 'mcp_servers.context7.args=["-y", "@upstash/context7-mcp"]' -c 'mcp_servers.context7.env_vars=["CONTEXT7_TOKEN"]' -c 'mcp_servers.github.url="https://api.githubcopilot.com/mcp/"' -c 'mcp_servers.github.bearer_token_env_var="GITHUB_TOKEN"'
```

A command server runs inside the container, so its program must be there: `npx` is, since the agents are installed with npm.

## Workflows

A workflow chains your subagents into steps the agent follows, written once for every harness. Write one TOML file per workflow in `~/.casulo/agent/workflows/`, or start from a template with `casulo add workflow <name>`:

```toml
# ~/.casulo/agent/workflows/feature.toml
description = "Plan, build and review a feature"

[[step]]
id = "plan"                       # optional, defaults to the subagent's name
agent = "planner"
task = "Write the requirements and plan in docs/plan-<topic>.md"   # optional
approve = true                    # stop and ask you before going on

[[step]]
id = "implement"
agent = "implementer"             # no depends_on: runs after the step above

[[step]]
agent = "reviewer"
depends_on = ["implement"]

[[step]]
agent = "security-auditor"
depends_on = ["implement"]        # may run alongside reviewer
```

Each step hands work to one subagent. It runs after the step above it, or after the steps in its `depends_on` (`[]` for none); steps that wait for the same ones may run in parallel. A workflow has no conditions: you pick the workflow, so each one stays a plain list of steps.

Saving checks the workflow: unknown fields, unknown or repeated ids, steps that wait for each other in a cycle, and subagents that do not exist (`casulo add agent <name>` creates one). Then casulo writes the skill `skills/workflow-<name>/SKILL.md`, which tells the agent how to run it: the steps in order, the subagent and task of each, what each waits for, which may run in parallel and where to stop for your approval. Both Claude Code and Codex read skills and get your subagents, so the same skill works in both. Start a workflow by asking for it:

```
use the workflow-feature skill to add an export to CSV
```

The generated skill says it comes from the workflow, and casulo never overwrites a `workflow-<name>` skill you wrote yourself. Change the TOML with `casulo edit workflow`, not the skill: opening and closing the workflow writes its skill again. `casulo delete workflow` deletes the skill too. If a workflow's skill is missing or out of date, or a subagent it uses is gone, `casulo run` warns and runs anyway.

### Examples

[`examples/`](../examples/) holds ready-made pieces to start from: ten skills (requirements, planning, TDD, review, security audit, docs and more), six subagents built on them (`analyst`, `planner`, `implementer`, `reviewer`, `security-auditor`, `documenter`) and three workflows that chain them (`feature`, `bugfix`, and `quickfix` for a one-line change). Copy what you want:

```sh
mkdir -p ~/.casulo/agent/agents ~/.casulo/agent/skills ~/.casulo/agent/workflows
cp examples/agents/* ~/.casulo/agent/agents/
cp -r examples/skills/* ~/.casulo/agent/skills/
cp examples/workflows/* ~/.casulo/agent/workflows/
casulo edit workflow feature      # close it to write the skill workflow-feature
```

A copied workflow has no skill yet, so open and close each one with `casulo edit workflow <name>` (`casulo run` warns until you do). To update one skill, copy its folder again (`cp -r examples/skills/docs ~/.casulo/agent/skills/`); this replaces your edits to it. Copy an agent or workflow file the same way, and reopen a workflow with `casulo edit workflow <name>`.

## What each agent receives

| Piece | Claude Code | Codex |
|---|---|---|
| `agent/AGENTS.md` | `~/.claude/CLAUDE.md` | `~/.codex/AGENTS.md` |
| `agent/skills/` | `~/.claude/skills/` | `~/.agents/skills/` |
| `agent/agents/*.md` | `~/.claude/agents/*.md` (YAML frontmatter) | `~/.codex/agents/*.toml` |
| `agent/mcp/*.toml` | `--mcp-config` with a generated JSON file | `-c mcp_servers.<name>.<key>=<value>` |
| `agent/workflows/*.toml` | the skill `workflow-<name>`, written when saved | the same skill |
| Session model and effort | `--model`, `--effort` | `--model`, `-c model_reasoning_effort=` |
| Memory session hooks | `--settings` with a generated JSON file | not yet |

Everything is read-only for the agent. Subagents and MCP config are regenerated on every `casulo run`, so a change under `~/.casulo/agent` takes effect on the next run. `casulo harness list` shows the same for every manifest, including your own:

```
$ casulo harness list
claude  built-in        instructions,skills,agents,mcp,hooks,model,effort
codex   built-in        instructions,skills,agents,mcp,model,effort
```
