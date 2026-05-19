# gitlab-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server that connects Claude and other MCP-compatible AI clients to the [GitLab API](https://docs.gitlab.com/api/rest/).

Ask things like *"List open issues assigned to me in my-org/my-project"*, *"Create a merge request from feature-branch to main"*, or *"Close MR #42"* — the server translates them into real GitLab API calls and returns structured results.

- **Full CRUD** — create, read, update, and delete GitLab resources
- **Eight domains** — Issues, Merge Requests, Branches, Pipelines, Jobs, Commits, Repository Files, and Repositories
- **Token-efficient responses** — list results are automatically slimmed (descriptions, pipelines, and other bulk fields stripped); use single-get tools when full detail is needed

---

## Table of contents

- [Requirements](#requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Client setup](#client-setup)
  - [Claude Desktop](#claude-desktop)
  - [Claude Code](#claude-code)
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

### Docker

```sh
docker build -t gitlab-mcp .
docker run -e GITLAB_URL=https://gitlab.com -e GITLAB_TOKEN=your-personal-access-token gitlab-mcp
```

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

## Available tools

All tools accept `project_id` as either a numeric ID (`42`) or a namespace path (`mygroup/myrepo`).

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
Pagination fields are populated from GitLab's `X-*` response headers. `total` and `total_pages` are omitted by GitLab on large endpoints; `next_page` is omitted on the last page. Use the presence of `next_page` to detect "more results exist."

### Issues

| Tool | Description |
|---|---|
| `gitlab_issues_list` | List issues. Filters: `state`, `labels`, `search`, `scope`, `assignee_id`, `author_id`, `created_after`/`created_before`, `updated_after`/`updated_before` (ISO 8601). |
| `gitlab_issues_get` | Get a single issue by IID. |
| `gitlab_issues_create` | Create an issue (`title` required). |
| `gitlab_issues_update` | Update an issue. Use `state_event: "close"` or `"reopen"` to change state. |
| `gitlab_issues_delete` | Delete an issue (Maintainer role required). |

#### Issue Notes

| Tool | Description |
|---|---|
| `gitlab_issues_notes_list` | List notes (comments) on an issue. Optional `order_by` and `sort`. |
| `gitlab_issues_notes_get` | Get a single note by note ID. |
| `gitlab_issues_notes_create` | Post a new note on an issue (`body` required). |
| `gitlab_issues_notes_update` | Update the body of a note. |
| `gitlab_issues_notes_delete` | Delete a note (permanent). |

### Merge Requests

| Tool | Description |
|---|---|
| `gitlab_mrs_list` | List MRs. Filters: `state`, `source_branch`, `target_branch`, `draft`, `labels`, `scope`, `created_after`/`created_before`, `updated_after`/`updated_before` (ISO 8601). |
| `gitlab_mrs_get` | Get a single MR by IID. |
| `gitlab_mrs_create` | Create an MR (`source_branch`, `target_branch`, `title` required). |
| `gitlab_mrs_update` | Update an MR. Use `state_event` to close/reopen; `draft` to toggle draft status. |
| `gitlab_mrs_delete` | Delete an MR (Maintainer role required). |
| `gitlab_mrs_merge` | Accept and merge an MR. |

#### MR Discussions

| Tool | Description |
|---|---|
| `gitlab_mrs_discussions_list` | List discussion threads on an MR. Each thread has an `individual_note` flag and a `notes[]` array. |
| `gitlab_mrs_discussions_get` | Get a single discussion thread by discussion ID (hex string). |
| `gitlab_mrs_discussions_create` | Start a new discussion thread (`body` required). Supports optional diff-note position params for inline code review comments. |
| `gitlab_mrs_discussions_resolve` | Resolve or unresolve a discussion thread (`resolved: true/false`). Requires Developer role or being the change author. |
| `gitlab_mrs_discussions_note_create` | Add a reply note to an existing discussion thread. |
| `gitlab_mrs_discussions_note_update` | Update a note body or resolved state (provide exactly one of `body` or `resolved`). |
| `gitlab_mrs_discussions_note_delete` | Delete a note from a discussion thread (permanent). |

### Branches

| Tool | Description |
|---|---|
| `gitlab_branches_list` | List branches. Optional `search` (substring) or `regex` filter. |
| `gitlab_branches_get` | Get a branch with commit details and protection status. |
| `gitlab_branches_create` | Create a branch from a source branch or commit SHA. |
| `gitlab_branches_delete` | Delete a branch (protected branches excluded). |
| `gitlab_branches_delete_merged` | Delete all merged branches (protected branches excluded). |

### Pipelines

| Tool | Description |
|---|---|
| `gitlab_pipelines_list` | List pipelines. Filters: `status`, `source`, `ref`, `sha`. |
| `gitlab_pipelines_get` | Get a single pipeline by ID. |
| `gitlab_pipelines_get_latest` | Get the latest pipeline for a ref. |
| `gitlab_pipelines_get_variables` | List variables for a pipeline. |
| `gitlab_pipelines_get_test_report` | Get the full test report for a pipeline. |
| `gitlab_pipelines_get_test_report_summary` | Get a summary of the test report. |
| `gitlab_pipelines_create` | Trigger a new pipeline on a ref. |
| `gitlab_pipelines_retry` | Retry failed jobs in a pipeline. |
| `gitlab_pipelines_cancel` | Cancel all running jobs in a pipeline. |
| `gitlab_pipelines_delete` | Delete a pipeline. |
| `gitlab_pipelines_update_metadata` | Update pipeline metadata (e.g. name). |

### Jobs

| Tool | Description |
|---|---|
| `gitlab_jobs_list` | List jobs for a project. |
| `gitlab_jobs_list_for_pipeline` | List jobs for a specific pipeline. |
| `gitlab_jobs_list_bridges` | List bridge (trigger) jobs for a pipeline. |
| `gitlab_jobs_get` | Get a single job by ID. |
| `gitlab_jobs_get_trace` | Get the raw log output for a job. |
| `gitlab_jobs_cancel` | Cancel a running job. |
| `gitlab_jobs_retry` | Retry a failed or canceled job. |
| `gitlab_jobs_erase` | Erase a job's artifacts and trace. |
| `gitlab_jobs_play` | Trigger a manual job. |

### Commits

| Tool | Description |
|---|---|
| `gitlab_commits_list` | List commits for a branch or path. |
| `gitlab_commits_create` | Create a commit with multiple file actions. |
| `gitlab_commits_get` | Get a single commit by SHA. |
| `gitlab_commits_diff` | Get the diff for a commit. |
| `gitlab_commits_refs` | List branches and tags a commit belongs to. |
| `gitlab_commits_sequence` | Check if one commit is an ancestor of another. |
| `gitlab_commits_cherry_pick` | Cherry-pick a commit onto a branch. |
| `gitlab_commits_revert` | Revert a commit. |
| `gitlab_commits_comments_list` | List comments on a commit. |
| `gitlab_commits_comment_create` | Add a comment to a commit. |
| `gitlab_commits_discussions_list` | List threaded discussions on a commit. |
| `gitlab_commits_statuses_list` | List CI statuses for a commit. |
| `gitlab_commits_status_set` | Set a CI status on a commit. |
| `gitlab_commits_merge_requests` | List MRs that include a commit. |
| `gitlab_commits_signature` | Get the GPG/SSH signature for a commit. |

### Repository Files

| Tool | Description |
|---|---|
| `gitlab_file_get` | Get file content and metadata at a ref. |
| `gitlab_file_raw` | Get raw file content at a ref. |
| `gitlab_file_blame` | Get blame information for a file. |
| `gitlab_file_create` | Create a new file with a commit message. |
| `gitlab_file_update` | Update an existing file with a commit message. |
| `gitlab_file_delete` | Delete a file with a commit message. |

### Repositories

| Tool | Description |
|---|---|
| `gitlab_repo_tree` | List files and directories at a path. |
| `gitlab_repo_blob_get` | Get a blob by SHA (metadata + content). |
| `gitlab_repo_blob_raw` | Get raw blob content by SHA. |
| `gitlab_repo_compare` | Compare two refs (diff, commits, diffs). |
| `gitlab_repo_contributors` | List repository contributors. |
| `gitlab_repo_merge_base` | Find the common ancestor of two refs. |
| `gitlab_repo_changelog_get` | Get the changelog for a version. |
| `gitlab_repo_changelog_add` | Generate and commit a changelog entry. |
| `gitlab_repo_health` | Check repository health status. |

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
