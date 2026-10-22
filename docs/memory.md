# Project memory

The memory of its code that every agent of a project shares.

Agents forget everything between sessions, and Claude and Codex never see what the other learned. With memory on, every agent of a project shares one memory of its code: which files link to which, a search over code and notes, and notes agents write about what the code does not say (a file's purpose, a convention, a trap, a decision). Turn it on per project, or for every project in the global `casulo.toml`:

```toml
[memory]
enabled = true
```

`casulo build` then also builds the memory image (`casulo run` builds it when it is missing), and `casulo run` starts the memory server before the agent:

```
$ casulo run --dry-run
docker network create --internal casulo-memory-app
docker run --rm --detach --init --name=casulo-memory-app ... --network=casulo-memory-app --network-alias=memory ... casulo memory serve ...
docker create --rm --interactive --tty --name=casulo-app-claude-4242 ... --mount=type=bind,source=.../generated/claude/mcp.json,target=/home/casulo/.casulo/mcp.json,readonly --mount=type=bind,source=.../generated/claude/settings.json,target=/home/casulo/.casulo/settings.json,readonly --mount=type=bind,source=.../generated/claude/memory-hook.mjs,target=/home/casulo/.casulo/memory-hook.mjs,readonly ... --env=CASULO_SESSION=4242 casulo/app:claude claude --mcp-config /home/casulo/.casulo/mcp.json --settings /home/casulo/.casulo/settings.json
docker network connect casulo-memory-app casulo-app-claude-4242
docker start --attach --interactive casulo-app-claude-4242
```

The agent gets a `memory` MCP server with five tools: `recall` (where to start looking), `related` (a file's neighbors and notes), `remember` (write a note), `confirm` (a note still holds for the current code) and `overview` (the last sessions and one line per note, for planning or when asked what was done). How to use them comes with the server, so your instructions file stays as you wrote it.

`recall` and `casulo search memory` match other forms of a word ("mounting" finds `mount`), so a query word may also find an unrelated word with the same stem (`generate` finds `general`). A file linked to another file the search found ranks higher, and the score shown includes that bonus. Links come only from Rust code for now, so in other languages the ranking is by text alone.

Memory is pulled, never pushed: nothing goes into the agent's context unless it calls a tool. A session is one `casulo run`, named by the number in its container name (`4242` above). With Claude Code, casulo also passes `--settings` with a generated settings file, on top of your own settings, whose hooks relay SessionStart, Stop and SessionEnd to the memory server. Codex gets no hooks yet.

- **SessionStart** records the files at the start and injects nothing. `/clear`, a compaction or a resume keeps the same session.
- **Stop** reminds the agent once per session to save one to three notes, the first time a turn ends with files changed since its last note (or since the start, without one). A session that changes nothing is never reminded.
- **SessionEnd** only records what the session did (files changed, notes saved, notes gone stale), which `overview` shows under the last sessions. It is untrusted data, never a note.

The hooks fail open: if the memory server does not answer within 5 seconds, the agent goes on as without them. `casulo describe memory` counts the sessions, those that changed files, saved notes or were reminded.

Memory gives the agent no new access:

- The memory server runs in its own container, hardened like the agent's, with the workspace read-only. It is the only container that mounts `~/.casulo/projects/<name>/memory/`.
- The agent reaches it by name (`memory`) over a network with no route to the internet.
- Notes are data, never instructions: each comes wrapped and marked as written by an agent, and notes cannot hold shell commands.
- The author of a note comes from the URL casulo gives each harness (`/mcp/claude`), never from what the agent says, so no agent can pass for another, or for you.
- The hook script and its settings file are read-only mounts, so the agent cannot change or remove its hooks.
- The session id is only a name, not a permission or a secret: a forged one can at most make the reminder appear or not.

Sessions of the same project share one memory server; the last one to exit stops it. While it runs, commands that change memory from the host (`index`, `edit`, `promote`) wait for it to end; `casulo stop memory` stops it now. A development build of casulo builds the memory image from its own source; a release downloads the Linux binary of its version, checked against a SHA-256 sum built into casulo.
