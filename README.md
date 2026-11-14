# gitlab-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server that connects Claude and other MCP-compatible AI clients to the [GitLab API](https://docs.gitlab.com/api/rest/).

Ask things like *"List open issues assigned to me in my-org/my-project"*, *"Create a merge request from feature-branch to main"*, or *"Close MR #42"* — the server translates them into real GitLab API calls and returns structured results.

- **Full CRUD** — create, read, update, and delete GitLab resources
- **Broad coverage of common GitLab workflows** — issues and work items, merge requests, code and repositories, CI/CD, epics, wikis, snippets, search, and more
- **Token-efficient responses** — list results are automatically slimmed (descriptions, pipelines, and other bulk fields stripped); use single-get tools when full detail is needed
- **Request tracing** — tool errors and (with `--debug`) every GitLab request are logged to stderr or a file via `--log-file`, for diagnosing API failures

---

## Table of contents

- [Requirements](#requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Claude Code setup](#claude-code-setup)
- [Available tools](#available-tools)
- [Development](#development)

---

## Requirements

- A GitLab instance (gitlab.com or self-hosted) and a personal access token with `api` scope
- To build from source: Rust stable toolchain (`rustup install stable`)
- To use a pre-built binary: nothing — grab it from the [releases page](https://github.com/3kirt/gitlab-mcp/releases)
- Optional: GnuPG, to keep the token encrypted instead of in plaintext ([GPG-encrypted token](#gpg-encrypted-token-pass))

---

## Installation

### Pre-built binary (recommended)

Download the binary for your platform from the [releases page](https://github.com/3kirt/gitlab-mcp/releases) and place it somewhere on your `$PATH`.

### From source

```sh
git clone https://github.com/3kirt/gitlab-mcp
cd gitlab-mcp
cargo install --path .
```

Installs the `gitlab-mcp` binary to `$CARGO_HOME/bin` (typically `~/.cargo/bin`).

---

## Configuration

gitlab-mcp reads credentials from `~/.gitlab_mcp.json`:

```json
{
  "url": "https://gitlab.com",
  "token": "your-personal-access-token"
}
```

Environment variables take precedence over the config file (a variable that is
set but empty counts as unset):

| Variable | Description |
|---|---|
| `GITLAB_URL` | GitLab base URL (e.g. `https://gitlab.com` or `https://gitlab.example.com`) |
| `GITLAB_TOKEN` | GitLab personal access token |

A custom config file path can be specified with `--config`:

```sh
gitlab-mcp --config /path/to/config.json
```

### Obtaining a personal access token

1. In GitLab, go to **User Settings → Access Tokens**
2. Create a token with the `api` scope
3. Copy the token — it is only shown once

For self-hosted GitLab instances, replace `https://gitlab.com` with your instance URL.

### GPG-encrypted token (pass)

*Requires gitlab-mcp v0.39.0 or later, on Linux (including WSL) or macOS.*

Instead of a plaintext `token`, gitlab-mcp can keep the token GPG-encrypted in a
[`pass`](https://www.passwordstore.org/)-compatible password store
(`~/.password-store`, or `$PASSWORD_STORE_DIR`). Only `gpg` is required; the
`pass` CLI is optional, but `pass show` reads the entry if you have it.

#### 1. Set up GnuPG

Install GnuPG if you don't have it: your package manager on Linux, or Homebrew
or GPG Suite on macOS.

You need a GPG key that can encrypt. Run `gpg --list-secret-keys` and look for
a key whose subkey shows `[E]`. If you don't have one, create it:

```sh
gpg --quick-gen-key "Your Name <you@example.com>" default default 2y
```

Then configure gpg-agent in `~/.gnupg/gpg-agent.conf` (or under `$GNUPGHOME`
if you set it). The default cache lifetimes (10 minutes, 2 hours at most) are
short for unattended use. This caches an unlock for an
8-hour work day:

```
default-cache-ttl 28800
max-cache-ttl 28800
```

Apply changes with `gpgconf --kill gpg-agent`; the agent restarts on its next
use.

**Which pinentry.** `gitlab-mcp unlock` prompts through gpg-agent's pinentry
(the server itself never prompts). In a normal terminal, a terminal pinentry
works as long as gpg knows the terminal: add `export GPG_TTY=$(tty)` to your
shell profile. To unlock from somewhere without a real
terminal, such as an agent's shell (Claude Code's `!` prefix), you need a
graphical pinentry, set with a `pinentry-program <path>` line in
`gpg-agent.conf`:

| Platform | Graphical pinentry |
|---|---|
| Linux | `pinentry-gnome3` or `pinentry-qt` from your package manager |
| macOS | `pinentry-mac` (Homebrew); it can also save the passphrase in the Keychain |
| Linux under WSL | `pinentry-gtk2` or `pinentry-qt`, which display through WSLg on Windows 11; without WSLg, unlock from a WSL terminal instead |

#### 2. Store the token

```sh
gitlab-mcp login --gpg-id <your-key-id>   # first time: no .gpg-id in the store yet
gitlab-mcp login                          # later: reuses the store's .gpg-id
```

`login` prompts for the token (no echo; `--token-stdin` reads it from a pipe),
checks it against GitLab, encrypts it to `gitlab-mcp/<host>.gpg`, and updates the
config file. It also removes any plaintext `token` from that file:

```json
{
  "url": "https://gitlab.com",
  "pass_entry": "gitlab-mcp/gitlab.com",
  "password_store_dir": "/home/you/.password-store"
}
```

Because the plaintext token is removed, every gitlab-mcp the config is used
with must be v0.39.0 or later; an older one can't start without a token.

#### 3. Leave `GITLAB_TOKEN` out of your MCP client config

`GITLAB_TOKEN` takes precedence over `pass_entry`. If your MCP client config
passes it (for example via `--env GITLAB_TOKEN=…` in `claude mcp add`), the
server uses that value and never touches the encrypted token. Pass only
`GITLAB_URL`, or nothing, since the config file has the URL. A
`"${GITLAB_TOKEN}"` placeholder is fine as long as the variable isn't
exported, because an empty value counts as unset.

Run `gitlab-mcp status` to check which token source applies:

```
GitLab URL:   https://gitlab.com
Token source: pass entry gitlab-mcp/gitlab.com (/home/you/.password-store/gitlab-mcp/gitlab.com.gpg)
Recipients:   0123456789ABCDEF0123456789ABCDEF01234567
gpg-agent:    unlocked (passphrase cached in gpg-agent)
```

#### Day to day

The server decrypts the token on first use and never prompts (gpg runs with
`--pinentry-mode error`), so it starts even while your key is locked. Until
gpg-agent has your passphrase cached, tool calls fail with a message asking
you to unlock. Your agent relays that, and the next call after you unlock
succeeds without restarting the server. After that first decrypt the token
stays in memory for the life of the server process, so the cache only matters
when a server starts.

```sh
gitlab-mcp unlock   # caches the passphrase in gpg-agent (prompts if needed)
gitlab-mcp status   # token source, recipients, locked/unlocked; never decrypts
gitlab-mcp logout   # deletes the encrypted entry
```

Plugin installs keep the binary under
`~/.cache/gitlab-mcp/v<version>/gitlab-mcp` (or `$XDG_CACHE_HOME/gitlab-mcp/…`).
Run `login`/`unlock` from there if `gitlab-mcp` isn't on your `PATH`.

#### Config keys

| Config key | Description |
|---|---|
| `pass_entry` | Store entry holding the token (mutually exclusive with `token`) |
| `gpg` | Full path to `gpg`, for MCP clients that launch servers with a minimal `PATH` (e.g. `/opt/homebrew/bin/gpg`); `gpgconf` is looked up next to it |
| `password_store_dir` | Store root; overrides `$PASSWORD_STORE_DIR` (written by `login`) |
| `gnupg_home` | GnuPG home passed to gpg as `--homedir`; overrides `$GNUPGHOME` (written by `login` when one is set) |

`login` records the store and keyring it used. That way a server started by a
GUI client, which doesn't see variables exported only in your shell, reads the
same ones. `~/` is expanded in these values.

---

## Claude Code setup

### Plugin install (recommended)

The plugin bundles the server configuration and fetches the matching release
binary automatically — no manual install step. In Claude Code:

```
/plugin marketplace add 3kirt/gitlab-mcp
/plugin install gitlab-mcp@gitlab-mcp
```

On first start the plugin downloads the prebuilt binary for your platform
(macOS arm64, Linux amd64/arm64; macOS x86_64 from v0.34.0 onward) from the
GitHub release matching the plugin version, verifies it against the release
checksums, and caches it under `~/.cache/gitlab-mcp/` (or `$XDG_CACHE_HOME`). Plugin updates roll the
binary forward automatically; on unsupported platforms (or when the download
fails) a `gitlab-mcp` already on `PATH` is used instead.
Supply credentials via `GITLAB_URL`/`GITLAB_TOKEN` environment variables or
`~/.gitlab_mcp.json`, optionally with the token GPG-encrypted
([Configuration](#configuration)).

### Manual setup

Register the server with the `claude` CLI:

```sh
claude mcp add --transport stdio \
  --env GITLAB_URL=https://gitlab.com \
  --env GITLAB_TOKEN=your-personal-access-token \
  gitlab -- gitlab-mcp
```

With a [GPG-encrypted token](#gpg-encrypted-token-pass), drop the
`--env GITLAB_TOKEN=…` line: an explicit token overrides the encrypted one.

To share the configuration with your team, commit a `.mcp.json` in the repo
root instead. Claude Code expands `${VAR}` (and `${VAR:-default}`) in `env`
values from each user's environment, so the token never lands in the file —
users just export `GITLAB_TOKEN` before starting Claude Code:

```json
{
  "mcpServers": {
    "gitlab": {
      "command": "gitlab-mcp",
      "args": [],
      "env": {
        "GITLAB_URL": "${GITLAB_URL:-https://gitlab.com}",
        "GITLAB_TOKEN": "${GITLAB_TOKEN}"
      }
    }
  }
}
```

(This repository's own [`.mcp.json`](.mcp.json) does exactly that, running the
server from source via `cargo run` — contributors only need to export
`GITLAB_TOKEN` and start Claude Code in the checkout.)

Verify the server is connected:

```sh
claude mcp list
```

Once connected, beyond the tools themselves you get:

- **Slash commands** — the server's MCP prompts appear as
  `/mcp__gitlab__review-mr`, `/mcp__gitlab__summarize-issue`, and
  `/mcp__gitlab__create-mr-description`, each pre-loading its GitLab context
  (diffs, comments, commits) into the conversation.
- **Resources** — GitLab data can be attached as context via `gitlab://`
  URIs (e.g. `@gitlab:gitlab://mygroup%2Fmyproject/issues/42`); the resource
  list offers your recently active projects.
- **Argument completion** — project paths, branch names, and issue/MR
  numbers are completed from live GitLab data where the client supports MCP
  completions.

> Other MCP clients (Claude Desktop, IDE plugins, etc.) work too — point them at
> the `gitlab-mcp` binary with the same `GITLAB_URL` and `GITLAB_TOKEN` env
> vars.

---

## Available tools

The server exposes 191 tools. Each one describes its own parameters to the
client, so you rarely need to look anything up. To inspect a tool directly,
`gitlab_tool_schema_get` returns its description and parameter schema. What
you can do, by area:

- **Issues and work items**: create, update, close, and search issues; comment
  and reply in threads; link related or blocking issues. Work items (GitLab's
  newer model for issues, tasks, epics, incidents, and OKRs) support full
  CRUD, comments, links, parent/child hierarchy, and reactions.
- **Merge requests**: create, update, approve, and merge; comment, including
  inline diff comments; resolve review threads.
- **Code and repositories**: browse the tree; read, blame, create, update, and
  delete files; manage branches; create multi-file commits, cherry-pick,
  revert, and inspect diffs, refs, statuses, and signatures; compare refs;
  read and update changelogs.
- **CI/CD**: list and inspect pipelines and jobs, read job logs and test
  reports, retry, cancel, or play manual jobs, and manage pipeline schedules
  and their variables. The `*_wait` tools block until a pipeline or job
  finishes, which suits a background "watch the pipeline" step. Runners are
  read-only.
- **Planning and docs**: epics and wikis (project wikis on every tier; epics
  and group wikis need Premium/Ultimate).
- **Projects, groups, and users**: read-only details and listings, including
  a user's public SSH keys.
- **Snippets and reactions**: personal snippets with raw content, and emoji
  reactions on issues, merge requests, snippets, and their comments.
- **Search**: global, group, and project search across code, issues, merge
  requests, commits, wikis, and more.
- **Instance**: version metadata, and license details for admin tokens on
  self-managed instances.

A few conventions hold across all tools:

- `project_id` and `group_id` accept a numeric ID (`42`) or a namespace path
  (`mygroup/myrepo`).
- Per-project numbers from URLs are `<resource>_iid` (`issue_iid`,
  `merge_request_iid`); globally unique IDs are `<resource>_id` (`pipeline_id`,
  `job_id`).
- List tools take `page`/`per_page` (or `fetch_all` to merge every page) and
  return the items with GitLab's pagination fields alongside. List items are
  slimmed to save tokens, and the matching get tool returns the full record.
  `next_page` is absent on the last page, so use it to tell whether more
  results exist.
- Tier-gated features (Premium/Ultimate) return GitLab's own error, or an empty
  result for embedded data, on lower tiers.

---

## Development

```sh
cargo build                          # debug build
cargo build --release                # release build
cargo test --all --locked            # run tests
make lint                            # clippy, all targets and feature sets
cargo fmt --check                    # format check
make check                           # full pre-release gate
```

Live integration tests against a real GitLab instance are described in
[`docs/testing.md`](docs/testing.md).

---

## License

[GPL-3.0](LICENSE)
