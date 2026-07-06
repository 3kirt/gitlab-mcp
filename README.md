# gitlab-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server that connects Claude and other MCP-compatible AI clients to the [GitLab API](https://docs.gitlab.com/api/rest/).

Ask things like *"List open issues assigned to me in my-org/my-project"*, *"Create a merge request from feature-branch to main"*, or *"Close MR #42"* — the server translates them into real GitLab API calls and returns structured results.

- **Full CRUD** — create, read, update, and delete GitLab resources
- **Broad coverage of common GitLab workflows** — issues, merge requests, branches, commits, repository files, pipelines, jobs, runners, epics, groups, snippets, emoji reactions, search, and more
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
(macOS arm64/x86_64, Linux amd64/arm64) from the GitHub release matching the
plugin version, verifies it against the release checksums, and caches it under
`~/.cache/gitlab-mcp/`. Plugin updates roll the binary forward automatically.
Supply credentials via `GITLAB_URL`/`GITLAB_TOKEN` environment variables or
`~/.gitlab_mcp.json` ([Configuration](#configuration)).

### Manual setup

Register the server with the `claude` CLI:

```sh
claude mcp add --transport stdio \
  --env GITLAB_URL=https://gitlab.com \
  --env GITLAB_TOKEN=your-personal-access-token \
  gitlab -- gitlab-mcp
```

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

The server covers the GitLab API surface most teams reach for day-to-day:
issues, merge requests, branches, commits, repository files, pipelines, jobs,
runners, epics, groups, snippets, emoji reactions, search, and more. All tools accept
`project_id` (or `group_id` for group-scoped endpoints) as either a numeric ID
(`42`) or a namespace path (`mygroup/myrepo`).

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

Full CRUD on issues, plus notes (comments), threaded discussions (start a
thread, reply, edit a reply, delete a reply), and issue links (`relates_to`,
`blocks`, `is_blocked_by`). `gitlab_issues_get` enriches the GitLab payload
with a `linked_issues` array and a `closed_by` array (MRs that close the issue
on merge). Filters on list include state, labels, search text, scope,
assignee/author IDs, and ISO 8601 created/updated date ranges.

### Merge Requests

Full CRUD plus accept/merge, approve/unapprove, and a discussions subsystem
covering threaded comments, inline diff notes with `position` (file, line,
base/head/start SHA), note add/edit/delete, and resolve/unresolve.
`gitlab_mrs_approve` approves a merge request on behalf of the current user and
returns the updated approval state (`approvals_left`, `approved_by`); an
optional `sha` parameter guards against approving a since-updated MR.
`gitlab_mrs_unapprove` removes the current user's approval.
`gitlab_mrs_get` enriches the GitLab payload with a `closes_issues` array
(issues this MR will close on merge) and a `related_issues` array (all linked +
closing issues; Premium/Ultimate — empty on lower tiers).

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

### Runners

Read-only access to GitLab runners. Seven tools covering all runner scopes:

- `gitlab_runners_list` — runners available to the current user (filtered by `type`, `status`, `paused`, `tag_list`, or `version_prefix`)
- `gitlab_runners_all_list` — all runners on the instance (administrators only; same filters)
- `gitlab_runners_get` — full detail for a single runner: architecture, platform, version, tag list, projects, access level, and last contact time
- `gitlab_runners_jobs_list` — jobs that a specific runner has processed (filter by `status` or `system_id`; sort `asc`/`desc`)
- `gitlab_runners_managers_list` — individual machine instances (runner managers) registered under a runner, with system ID, version, and contact info
- `gitlab_runners_list_for_project` — runners available to a project (`project_id` accepts a numeric ID or namespace path)
- `gitlab_runners_list_for_group` — runners available to a group (`group_id` accepts a numeric ID or namespace path)

All list tools support `page`/`per_page` pagination.

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

### Projects

Read-only access to GitLab projects. `gitlab_projects_get` returns full details
for a project by numeric ID or full namespace path (e.g. `mygroup/myrepo`).
Optional `statistics=true` embeds commit and storage counts (requires Reporter
role or higher).

### Groups

Read-only access to GitLab groups. `gitlab_groups_list` returns all groups accessible
to the token with optional filters: `search` (by name or path), `owned` (only groups
the token's user owns), `all_available` (include all accessible groups, not just member
groups), `min_access_level`, and `top_level_only` (exclude subgroups). `gitlab_groups_get`
returns full details for a group by numeric ID or full namespace path. Set
`with_projects=true` to embed the group's projects (up to 100) in the response.

### Epics

Group-level epics via the REST Epics API (`/api/v4/groups/:id/epics`). `group_id`
accepts a numeric ID or a full namespace path; `epic_iid` is the per-group IID
shown in the URL. Full CRUD plus `state_event`-based open/close, parent epic
linking (`parent_epic_iid=0` clears the parent), and date widget management.
`gitlab_epics_get` embeds an `issues` array (the epic's child issues) via the
epic's `/issues` endpoint. Issues can be linked to or unlinked from an epic via
`gitlab_epics_issue_assign` (takes the **global** issue ID) and
`gitlab_epics_issue_remove` (takes the **association** ID — the `id` field of
each entry in the embedded `issues` array, not the issue's own ID).
Pagination is standard `page`/`per_page`. The REST endpoint is
deprecated since GitLab 17.0 but remains functional on EE 18.x, where it is
the only working surface for epics.

### Snippets

Full CRUD on personal snippets, plus raw content retrieval and admin helpers.
`gitlab_snippets_list` returns the current user's snippets;
`gitlab_snippets_public_list` lists all public snippets; `gitlab_snippets_all_list`
lists all snippets accessible to the authenticated user (administrators and
auditors see every snippet on the instance). `gitlab_snippets_raw` fetches the
raw text content of a snippet as `{"content": "..."}`;
`gitlab_snippets_file_raw` does the same for a specific file within a
multi-file snippet repository (slashes in `file_path` are percent-encoded
automatically). Create and update accept a `files` array — each entry specifies
`content` and `file_path` on create, and `action` (`create`, `update`,
`delete`, or `move`), optional `file_path`, `previous_path`, and `content` on
update. `gitlab_snippets_user_agent_detail` is an admin-only endpoint that
returns the IP address and user-agent string used to create the snippet.

### Emoji Reactions

Award emoji (e.g. `thumbsup`, `tada`) on issues, merge requests, project
snippets, and notes on each. Twenty-four tools — list, get, create, delete —
across `gitlab_emoji_reactions_issues_*`, `gitlab_emoji_reactions_mrs_*`,
`gitlab_emoji_reactions_snippets_*`, `gitlab_emoji_reactions_issue_notes_*`,
`gitlab_emoji_reactions_mr_notes_*`, and `gitlab_emoji_reactions_snippet_notes_*`.
Emoji names are passed without surrounding colons. Only the reaction author
or an administrator can delete a reaction.

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
cargo build                          # debug build
cargo build --release                # release build
cargo test --all --locked            # run tests
cargo clippy --locked -- -D warnings # lint
cargo fmt --check                    # format check
```

---

## License

[GPL-3.0](LICENSE)
