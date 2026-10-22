# Commands

Every casulo command, with its options as `--help` prints them and an example.

Inside a project's workspace, casulo knows which project you mean; elsewhere, pass `--project <name>`. The agent comes from `--harness`, then the project config, then the global default.

## casulo new

```text
$ casulo new --help
Create a project: clone a repository and prepare its environment

Usage: casulo new [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>  Git URL to clone, or a directory to use in place, such as `.` or ./app

Options:
      --name <NAME>  Project name [default: the repository or directory name]
  -h, --help         Print help
```

```sh
cd "$(casulo new https://github.com/you/project)"   # clone into ~/.casulo and enter the workspace
casulo new .                                  # or use the current directory in place
```

More in [projects.md](projects.md#projects-from-a-local-directory).

## casulo doctor

```text
$ casulo doctor --help
Check that git and the container runtime are ready, and how to fix them

Usage: casulo doctor

Options:
  -h, --help  Print help
```

```sh
casulo doctor                                 # check that git and Docker are ready
```

More in [README.md](../README.md#quick-start).

## casulo setup

```text
$ casulo setup --help
Create ~/.casulo with documented defaults, then install or fix what casulo needs, asking before each command

Usage: casulo setup

Options:
  -h, --help  Print help
```

```sh
casulo setup                                  # create ~/.casulo, then fix what is missing, asking first
```

More in [configuration.md](configuration.md#files).

## casulo build

```text
$ casulo build --help
Generate the project's Dockerfile for a harness and build the image

Usage: casulo build [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
      --harness <HARNESS>  Harness to use [default: the project's, else the global default]
  -h, --help               Print help
```

```sh
casulo build                                  # build the image for the default agent (Claude Code)
```

More in [projects.md](projects.md#tools-the-project-needs).

## casulo run

```text
$ casulo run --help
Run the agent in a hardened container with the workspace mounted

Usage: casulo run [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
      --harness <HARNESS>  Harness to use [default: the project's, else the global default]
      --dry-run            Print the exact container command instead of running it
  -h, --help               Print help
```

```sh
casulo run                                    # run the agent in the hardened container (builds the image first if missing)
casulo run --dry-run                          # print the exact docker command instead
casulo build --harness codex && casulo run --harness codex   # same project, another agent
```

More in [security.md](security.md#what-the-container-gets).

## casulo shell

```text
$ casulo shell --help
Open a bash shell in the same container the agent runs in

Usage: casulo shell [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
      --harness <HARNESS>  Harness to use [default: the project's, else the global default]
      --dry-run            Print the exact container command instead of running it
  -h, --help               Print help
```

```sh
casulo shell                                  # open bash in the same container
```

More in [harnesses.md](harnesses.md#login-and-credentials).

## casulo harness

```text
$ casulo harness --help
Work with harnesses, the descriptions of how to install and run each agent

Usage: casulo harness <COMMAND>

Commands:
  list  List the available harnesses, where each one comes from and what it receives
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo harness list --help
List the available harnesses, where each one comes from and what it receives

Usage: casulo harness list

Options:
  -h, --help  Print help
```

```sh
casulo harness list                           # available agents, where they come from and what they receive
```

More in [harnesses.md](harnesses.md#agent-manifests).

## casulo compose

```text
$ casulo compose --help
Work with the services of the project's docker compose file

Usage: casulo compose <COMMAND>

Commands:
  check  Check the compose file and show what would run, without starting anything
  up     Check, approve and start the services in the background
  down   Stop and remove the services (named volumes are kept)
  ps     Show the state of the services
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo compose check --help
Check the compose file and show what would run, without starting anything

Usage: casulo compose check [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo compose up --help
Check, approve and start the services in the background

Usage: casulo compose up [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo compose down --help
Stop and remove the services (named volumes are kept)

Usage: casulo compose down [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo compose ps --help
Show the state of the services

Usage: casulo compose ps [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```sh
casulo compose check                          # validate the project's docker-compose.yml
casulo compose up                             # start its services (casulo run does it with [compose] enabled)
```

More in [projects.md](projects.md#services-from-docker-compose).

## casulo describe

```text
$ casulo describe --help
Show details of a resource, such as the project's memory

Usage: casulo describe <COMMAND>

Commands:
  memory  What the project's memory holds, or details of one of its notes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo describe memory --help
What the project's memory holds, or details of one of its notes

Usage: casulo describe memory [OPTIONS] [COMMAND]

Commands:
  note  A file's neighbors in the code graph and its notes
  help  Print this message or the help of the given subcommand(s)

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo describe memory note --help
A file's neighbors in the code graph and its notes

Usage: casulo describe memory note [OPTIONS] <PATH>

Arguments:
  <PATH>  File path relative to the repository root, such as src/main.rs

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```sh
casulo describe memory                        # what the project's memory holds
casulo describe memory note src/main.rs       # a file's links in the code graph and its notes
```

More in [memory.md](memory.md).

## casulo index

```text
$ casulo index --help
Update an index, such as the project's memory of its code

Usage: casulo index <COMMAND>

Commands:
  memory  Read what changed in the checkout and the local branches into memory
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo index memory --help
Read what changed in the checkout and the local branches into memory

Usage: casulo index memory [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```sh
casulo index memory                           # read the project's code into its memory
```

More in [memory.md](memory.md).

## casulo search

```text
$ casulo search --help
Search the project's memory, the same way agents do with recall

Usage: casulo search <COMMAND>

Commands:
  memory  Files, their links and notes, and project notes that match the text
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo search memory --help
Files, their links and notes, and project notes that match the text

Usage: casulo search memory [OPTIONS] <TEXT>

Arguments:
  <TEXT>  What to look for, such as "docker run arguments"

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
      --limit <LIMIT>      How many files to return [default: 5]
  -h, --help               Print help
```

```sh
casulo search memory "docker run arguments"   # the same search agents get from recall
```

More in [memory.md](memory.md).

## casulo config

```text
$ casulo config --help
Open the global casulo.toml, or a project's, in your editor

Usage: casulo config [OPTIONS]

Options:
      --project <PROJECT>  Open this project's casulo.toml instead of the global one
      --path               Only print the file's path
  -h, --help               Print help
```

```sh
casulo config                                 # edit the global casulo.toml in your editor
casulo config --project project               # edit a project's casulo.toml
```

More in [configuration.md](configuration.md#files).

## casulo add

```text
$ casulo add --help
Create a subagent, skill, MCP server or workflow from a template, in your editor

Usage: casulo add [OPTIONS] <KIND> <NAME>

Arguments:
  <KIND>
          What to work on

          Possible values:
          - agent:    A subagent, `agents/<name>.md`
          - skill:    An Agent Skill, `skills/<name>/SKILL.md`
          - mcp:      An MCP server, `mcp/<name>.toml`
          - workflow: A workflow of subagents, `workflows/<name>.toml`

  <NAME>
          Its name, such as code-reviewer

Options:
      --file <FILE>
          A file to create inside the skill's folder instead, such as assets/template.md

  -h, --help
          Print help (see a summary with '-h')
```

```sh
casulo add agent reviewer                     # create a subagent from a template, in your editor
casulo add skill release-notes --file assets/template.md   # add a file the skill uses, such as a template
casulo add workflow feature                   # chain subagents into a workflow the agent follows
```

More in [agent-setup.md](agent-setup.md#your-agent-setup).

## casulo edit

```text
$ casulo edit --help
Change a subagent, skill, MCP server or workflow in your editor

Usage: casulo edit <COMMAND>

Commands:
  agent     A subagent
  skill     An Agent Skill, or one of its files with --file
  mcp       An MCP server
  workflow  A workflow of subagents; saving it writes its skill
  memory    Notes in the project's memory
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo edit agent --help
A subagent

Usage: casulo edit agent <NAME>

Arguments:
  <NAME>  Its name, such as code-reviewer

Options:
  -h, --help  Print help
```

```text
$ casulo edit skill --help
An Agent Skill, or one of its files with --file

Usage: casulo edit skill [OPTIONS] <NAME>

Arguments:
  <NAME>  Its name, such as release-notes

Options:
      --file <FILE>  A file inside the skill's folder instead of SKILL.md, such as assets/template.md
  -h, --help         Print help
```

```text
$ casulo edit mcp --help
An MCP server

Usage: casulo edit mcp <NAME>

Arguments:
  <NAME>  Its name, such as code-reviewer

Options:
  -h, --help  Print help
```

```text
$ casulo edit workflow --help
A workflow of subagents; saving it writes its skill

Usage: casulo edit workflow <NAME>

Arguments:
  <NAME>  Its name, such as code-reviewer

Options:
  -h, --help  Print help
```

```text
$ casulo edit memory --help
Notes in the project's memory

Usage: casulo edit memory [OPTIONS] <COMMAND>

Commands:
  note   A file's notes, in your editor; saving settles conflicts
  topic  A project note: general, or of the current branch with --branch
  help   Print this message or the help of the given subcommand(s)

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo edit memory note --help
A file's notes, in your editor; saving settles conflicts

Usage: casulo edit memory note [OPTIONS] <PATH>

Arguments:
  <PATH>  File path relative to the repository root, such as src/main.rs

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo edit memory topic --help
A project note: general, or of the current branch with --branch

Usage: casulo edit memory topic [OPTIONS] <TOPIC>

Arguments:
  <TOPIC>  Its name, such as error-handling

Options:
      --branch             The note of the current branch instead of the general one
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```sh
casulo edit memory note src/main.rs           # change a file's notes in your editor; saving settles conflicts
casulo edit memory topic errors               # a project note (--branch: only for the current branch)
casulo edit skill release-notes               # change a skill
```

More in [agent-setup.md](agent-setup.md#your-agent-setup) and [memory.md](memory.md).

## casulo list

```text
$ casulo list --help
List your subagents, skills, MCP servers and workflows, or the project's memory notes

Usage: casulo list [COMMAND]

Commands:
  agent     Subagents, with their descriptions
  skill     Agent Skills, with their descriptions
  mcp       MCP servers, with their commands or URLs
  workflow  Workflows of subagents, with their descriptions
  memory    Notes in the project's memory
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo list agent --help
Subagents, with their descriptions

Usage: casulo list agent

Options:
  -h, --help  Print help
```

```text
$ casulo list skill --help
Agent Skills, with their descriptions

Usage: casulo list skill

Options:
  -h, --help  Print help
```

```text
$ casulo list mcp --help
MCP servers, with their commands or URLs

Usage: casulo list mcp

Options:
  -h, --help  Print help
```

```text
$ casulo list workflow --help
Workflows of subagents, with their descriptions

Usage: casulo list workflow

Options:
  -h, --help  Print help
```

```text
$ casulo list memory --help
Notes in the project's memory

Usage: casulo list memory [OPTIONS] <COMMAND>

Commands:
  note   One line per file and kind: path, kind, status, author, summary
  topic  Project notes: general, and of the current branch
  help   Print this message or the help of the given subcommand(s)

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo list memory note --help
One line per file and kind: path, kind, status, author, summary

Usage: casulo list memory note [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
      --stale              Only notes written for another version of the file
      --conflict           Only notes with two texts to settle
      --all-versions       Every stored version, including notes kept for other branches
  -h, --help               Print help
```

```text
$ casulo list memory topic --help
Project notes: general, and of the current branch

Usage: casulo list memory topic [OPTIONS]

Options:
      --branch             Only the notes of the current branch
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```sh
casulo list memory note --stale               # notes written for an older version of their file
casulo list                                   # your subagents, skills, MCP servers and workflows
```

More in [agent-setup.md](agent-setup.md#your-agent-setup) and [memory.md](memory.md).

## casulo promote

```text
$ casulo promote --help
Promote a branch note of the project's memory to a general one

Usage: casulo promote <COMMAND>

Commands:
  memory  Turn the current branch's note on a topic into a general project note
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo promote memory --help
Turn the current branch's note on a topic into a general project note

Usage: casulo promote memory [OPTIONS] <COMMAND>

Commands:
  topic  The current branch's notes on this topic
  help   Print this message or the help of the given subcommand(s)

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```text
$ casulo promote memory topic --help
The current branch's notes on this topic

Usage: casulo promote memory topic [OPTIONS] <TOPIC>

Arguments:
  <TOPIC>  Its name, such as runtime

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```sh
casulo promote memory topic runtime           # turn the current branch's note into a general one
```

More in [memory.md](memory.md).

## casulo stop

```text
$ casulo stop --help
Stop a running service, such as the project's memory server

Usage: casulo stop <COMMAND>

Commands:
  memory  Stop the project's memory server now, even with sessions running
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

```text
$ casulo stop memory --help
Stop the project's memory server now, even with sessions running

Usage: casulo stop memory [OPTIONS]

Options:
      --project <PROJECT>  Project to use [default: the project of the current directory]
  -h, --help               Print help
```

```sh
casulo stop memory                            # stop the project's memory server now
```

More in [memory.md](memory.md).

## casulo delete

```text
$ casulo delete --help
Delete a subagent, skill, MCP server or workflow

Usage: casulo delete [OPTIONS] <KIND> <NAME>

Arguments:
  <KIND>
          What to work on

          Possible values:
          - agent:    A subagent, `agents/<name>.md`
          - skill:    An Agent Skill, `skills/<name>/SKILL.md`
          - mcp:      An MCP server, `mcp/<name>.toml`
          - workflow: A workflow of subagents, `workflows/<name>.toml`

  <NAME>
          Its name, such as code-reviewer

Options:
      --yes
          Delete without asking

  -h, --help
          Print help (see a summary with '-h')
```

```sh
casulo delete mcp context7                    # delete one, asking first
```

More in [agent-setup.md](agent-setup.md#your-agent-setup).

## casulo home

```text
$ casulo home --help
Print the casulo home directory

Usage: casulo home

Options:
  -h, --help  Print help
```

```sh
casulo home                                   # print the casulo home directory
```

More in [configuration.md](configuration.md#files).
