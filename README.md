# gitlab-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server that connects Claude and other MCP-compatible AI clients to the [GitLab API](https://docs.gitlab.com/api/rest/).

Ask things like *"List open issues assigned to me in my-org/my-project"*, *"Create a merge request from feature-branch to main"*, or *"Close MR #42"* — the server translates them into real GitLab API calls and returns structured results.

- **Full CRUD** — create, read, update, and delete GitLab resources
- **Two transports** — stdio (local subprocess) or HTTP (remote/shared)
- **Issues, Merge Requests & Branches** — full CRUD plus merge, draft, and branch management support

---

## Table of contents

- [Requirements](#requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Client setup](#client-setup)
  - [Claude Desktop](#claude-desktop)
  - [Claude Code](#claude-code)
- [Remote MCP (HTTP transport)](#remote-mcp-http-transport)
- [Available tools](#available-tools)
- [Development](#development)

---

## Requirements

- A GitLab instance (gitlab.com or self-hosted) and a personal access token with `api` scope
- To build from source: Rust stable toolchain (`rustup install stable`)
- To use a pre-built binary: nothing — grab it from the [releases page](https://github.com/3kirt/gitlab-mcp/releases)

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

Environment variables take precedence over the config file:

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

---

## Client setup

### Claude Desktop

Add the following to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "gitlab": {
      "command": "gitlab-mcp",
      "env": {
        "GITLAB_URL": "https://gitlab.com",
        "GITLAB_TOKEN": "your-personal-access-token"
      }
    }
  }
}
```

Config file location:

| OS | Path |
|---|---|
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Linux | `~/.config/Claude/claude_desktop_config.json` |
| Windows | `%APPDATA%\Claude\claude_desktop_config.json` |

### Claude Code

Register via the CLI:

```sh
claude mcp add --transport stdio \
  --env GITLAB_URL=https://gitlab.com \
  --env GITLAB_TOKEN=your-personal-access-token \
  gitlab -- gitlab-mcp
```

To share the configuration with your team (writes to `.mcp.json`):

```sh
claude mcp add --transport stdio --scope project \
  --env GITLAB_URL=https://gitlab.com \
  gitlab -- gitlab-mcp
```

---

## Remote MCP (HTTP transport)

gitlab-mcp can run as a remote MCP server over the Streamable HTTP transport. Each session authenticates with its own GitLab personal access token via an `Authorization: Bearer` header — no server-side token is configured.

### Running from the binary

```sh
GITLAB_URL=https://gitlab.com gitlab-mcp --listen 0.0.0.0:8080
```

### Registering with Claude Code (HTTP)

```sh
claude mcp add --transport http \
  --header "Authorization: Bearer your-personal-access-token" \
  gitlab https://gitlab-mcp.example.com/mcp
```

### Health endpoints

| Endpoint | Purpose | Success response |
|---|---|---|
| `GET /healthz` | Liveness — server is running | `{"status":"ok","version":"..."}` |
| `GET /readyz` | Readiness — GitLab hostname resolves | HTTP 200 |

> **TLS note:** The HTTP listener does not terminate TLS. In production, place it behind a reverse proxy (nginx, Caddy) or a platform that provides HTTPS.

---

## Available tools

All tools accept `project_id` as either a numeric ID (e.g. `42`) or a URL-encoded namespace path (e.g. `mygroup%2Fmyproject`).

### Issues

| Tool | Method | Description |
|---|---|---|
| `gitlab_issues_list` | GET | List issues for a project. Filters: `state`, `labels`, `search`, `scope`, `assignee_id`, `author_id`, `order_by`, `sort`. Paginate with `page`/`per_page`. |
| `gitlab_issues_get` | GET | Get a single issue by project ID and issue IID. |
| `gitlab_issues_create` | POST | Create a new issue. Required: `project_id`, `title`. Optional: `description`, `labels`, `assignee_ids`, `milestone_id`, `due_date`. |
| `gitlab_issues_update` | PUT | Update an existing issue. Use `state_event: "close"` or `"reopen"` to change state. |
| `gitlab_issues_delete` | DELETE | Permanently delete an issue. Requires Maintainer role or higher. |

### Merge Requests

| Tool | Method | Description |
|---|---|---|
| `gitlab_mrs_list` | GET | List merge requests for a project. Filters: `state`, `source_branch`, `target_branch`, `author_id`, `assignee_id`, `reviewer_id`, `labels`, `search`, `draft`, `scope`, `order_by`, `sort`. Paginate with `page`/`per_page`. |
| `gitlab_mrs_get` | GET | Get a single merge request by project ID and MR IID. |
| `gitlab_mrs_create` | POST | Create a new merge request. Required: `project_id`, `source_branch`, `target_branch`, `title`. Optional: `description`, `assignee_id`, `reviewer_ids`, `labels`, `milestone_id`, `squash`, `remove_source_branch`, `draft`. |
| `gitlab_mrs_update` | PUT | Update an existing merge request. Use `state_event: "close"` or `"reopen"` to change state; `draft: true/false` to toggle draft status. |
| `gitlab_mrs_delete` | DELETE | Permanently delete a merge request. Requires Maintainer role or higher. |
| `gitlab_mrs_merge` | PUT | Accept and merge a merge request. Optional: `merge_commit_message`, `squash`, `should_remove_source_branch`, `merge_when_pipeline_succeeds`. |

### Branches

| Tool | Method | Description |
|---|---|---|
| `gitlab_branches_list` | GET | List branches for a project, sorted alphabetically. Optional: `search` (substring), `regex` (re2 pattern). Paginate with `page`/`per_page`. |
| `gitlab_branches_get` | GET | Get a single branch by name. Returns commit details and protection status. |
| `gitlab_branches_create` | POST | Create a new branch. Required: `project_id`, `branch` (new name), `ref` (source branch or commit SHA). |
| `gitlab_branches_delete` | DELETE | Delete a branch by name. Cannot delete default or protected branches. |
| `gitlab_branches_delete_merged` | DELETE | Delete all branches that have been merged into the default branch (protected branches are excluded). |

---

## Development

```sh
cargo build                    # debug build
cargo build --release          # release build
cargo test --all               # run tests
cargo clippy -- -D warnings    # lint
cargo fmt --check              # format check
```

---

## License

[GPL-3.0](LICENSE)
