# gitlab-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server that connects Claude and other MCP-compatible AI clients to the [GitLab API](https://docs.gitlab.com/api/rest/).

Ask things like *"List open issues assigned to me in my-org/my-project"*, *"Create a merge request from feature-branch to main"*, or *"Close MR #42"* — the server translates them into real GitLab API calls and returns structured results.

- **Full CRUD** — create, read, update, and delete GitLab resources
- **Broad coverage of common GitLab workflows** — issues, merge requests, branches, commits, repository files, pipelines, jobs, epics, search, and more
- **Token-efficient responses** — list results are automatically slimmed (descriptions, pipelines, and other bulk fields stripped); use single-get tools when full detail is needed

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

## Claude Code setup

Register the server with the `claude` CLI:

```sh
claude mcp add --transport stdio \
  --env GITLAB_URL=https://gitlab.com \
  --env GITLAB_TOKEN=your-personal-access-token \
  gitlab -- gitlab-mcp
```

To share the configuration with your team (writes to `.mcp.json` in the repo
root, omit the token so each user supplies their own):

```sh
claude mcp add --transport stdio --scope project \
  --env GITLAB_URL=https://gitlab.com \
  gitlab -- gitlab-mcp
```

Verify the server is connected:

```sh
claude mcp list
```

> Other MCP clients (Claude Desktop, IDE plugins, etc.) work too — point them at
> the `gitlab-mcp` binary with the same `GITLAB_URL` and `GITLAB_TOKEN` env
> vars.

---

## Available tools

The server covers the GitLab API surface most teams reach for day-to-day:
issues, merge requests, branches, commits, repository files, pipelines, jobs,
epics, search, and more. All tools accept `project_id` (or `group_id` for
group-scoped endpoints) as either a numeric ID (`42`) or a namespace path
(`mygroup/myrepo`).

List operations support `page`/`per_page` pagination and return an envelope:
```json
{
  "items": [ /* slimmed records */ ],
  "page": 2,
  "per_page": 20,
  "total": 49,
  "total_pages": 3,
  "next_page": 3
}
```
Pagination fields are populated from GitLab's `X-*` response headers. `total`
and `total_pages` are omitted by GitLab on large endpoints; `next_page` is
omitted on the last page — use its presence to detect "more results exist."

### Issues

Full CRUD on issues, plus notes (comments) and issue links (`relates_to`,
`blocks`, `is_blocked_by`). `gitlab_issues_get` enriches the GitLab payload
with a `linked_issues` array and a `closed_by` array (MRs that close the issue
on merge). Filters on list include state, labels, search text, scope,
assignee/author IDs, and ISO 8601 created/updated date ranges.

### Merge Requests

Full CRUD plus accept/merge, and a discussions subsystem covering threaded
comments, inline diff notes with `position` (file, line, base/head/start SHA),
note add/edit/delete, and resolve/unresolve.

### Branches

List, get (with commit details and protection status), create (from a source
branch or SHA), delete a single branch, and bulk delete-merged. Protected
branches are excluded from destructive operations.

### Pipelines

List/get/get-latest, fetch pipeline variables, get full and summary test
reports, create on a ref, retry/cancel/delete, and update pipeline metadata
(e.g. name).

### Pipeline Schedules

Full CRUD on schedules, plus `take_ownership` and `play` (run immediately),
listing pipelines triggered by a schedule, and CRUD on per-schedule variables
(`env_var` and `file` types).

### Jobs

List (project-wide, per-pipeline, and bridge/trigger jobs), get, fetch the raw
trace log, cancel/retry/erase, and play a manual job.

### Commits

List/get/create commits (with multi-file actions), diff, refs (branches/tags
containing a commit), ancestry check, cherry-pick, revert, comments and
threaded discussions, CI status get/set, find MRs that include a commit, and
GPG/SSH signature lookup.

### Repository Files

Get file content + metadata at a ref, get raw content, get blame, and create,
update, or delete a file with a commit message.

### Repositories

Tree listing, blob get (with metadata + content) and raw blob, compare refs
(commits + diffs), contributors, merge-base, changelog get/add, and repository
health.

### Epics

Group-level epics via the REST Epics API (`/api/v4/groups/:id/epics`). `group_id`
accepts a numeric ID or a full namespace path; `epic_iid` is the per-group IID
shown in the URL. Full CRUD plus `state_event`-based open/close, parent epic
linking (`parent_epic_iid=0` clears the parent), and date widget management.
`gitlab_epics_get` embeds an `issues` array (the epic's child issues) via the
epic's `/issues` endpoint. Pagination is standard `page`/`per_page`. The REST endpoint is
deprecated since GitLab 17.0 but remains functional on EE 18.x, where it is
the only working surface for epics.

### Search

Three scopes — global instance, group, and project search — over `projects`,
`issues`, `merge_requests`, `milestones`, `snippet_titles`, `users`,
`wiki_blobs`, `commits`, `blobs`, and `notes`. Supports `search_type` selection
(`basic`/`advanced`/`zoekt`), confidentiality and state filtering, and field
restriction (Premium/Ultimate).

### Metadata

Returns GitLab instance metadata: version, revision, enterprise flag, and KAS
(Kubernetes Agent Server) information.

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
