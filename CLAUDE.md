# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
cargo build                      # debug build
cargo build --release            # release build
cargo test --all --locked        # run all tests
cargo test <test_name>           # run a single test by name
cargo clippy --locked -- -D warnings   # lint (CI enforces zero warnings)
cargo fmt                        # format code
cargo fmt --check                # check formatting without writing
cargo run -- --help              # show CLI flags
```

To run (requires env vars or `~/.gitlab_mcp.json`):
```sh
GITLAB_URL=https://gitlab.com GITLAB_TOKEN=glpat-xxx cargo run
```

## Architecture

The server runs in stdio transport mode. The token is read from config at startup; `GitlabMcpServer::new_stdio()` initialises the client immediately.

### Request flow

```
MCP client → rmcp transport (stdio)
           → GitlabMcpServer (tool_router macro dispatch)
           → domain function in tools/issues.rs, tools/merge_requests.rs, etc.
           → GitlabClient (reqwest, PRIVATE-TOKEN header)
           → GitLab REST API
```

### Key modules

**`src/client.rs`** — thin `reqwest` wrapper. Sends `PRIVATE-TOKEN: <token>` on every request. REST methods: `get`, `list` (with query params and pagination metadata), `post`, `put`, `delete` (expects 204), `delete_json` (DELETE endpoints that return a response body), `delete_with_body`, `get_text`. All JSON methods return `serde_json::Value` so tools pass payloads straight through to the MCP client without intermediate typed structs. `GitlabError` variants: `Api { status, body }` (HTTP error from GitLab), `Http` (reqwest transport error), `Other(String)` (validation failure or malformed response). `GitlabError::to_tool_message()` truncates API error bodies to 300 chars.

**`src/tools/mod.rs`** — MCP server struct and shared glue. Contains:
- `GitlabMcpServer` struct with `new_stdio` constructor
- `tool_router()` — sums the per-domain sub-routers (`Self::tool_router_<domain>()`, one per module) into the router that `#[tool_handler]` and `new_stdio` consume. The `combined_router_exposes_every_tool` test asserts the exact tool count, so it fails if a new domain's router is forgotten here (or two tools collide on a name)
- Delegation macros (`delegate_list!`, `delegate_get!`, `delegate_create!`, `delegate_update!`, `delegate_delete!`, `delegate_unit!`, `delegate_text!`, plus the lower-level `delegate_json!`) that fetch the client, call the domain function, and map the result to `CallToolResult`. They are defined *before* the `pub mod` declarations so they are in textual macro scope inside every domain module, and their bodies use `$crate::`/`rmcp::` paths so they expand correctly there
- `QueryBuilder` / `BodyBuilder` — fluent helpers for building query param slices and JSON request bodies
- `PaginationParams` — shared `page`/`per_page` struct flattened into list param structs
- `project_path()` / `group_path()` — build the `/api/v4/projects/{id}` / `/api/v4/groups/{id}` prefix every scoped endpoint starts from (callers append their suffix)
- `unwrap_404_as_empty_array()` — turns a 404 from a supplemental embedded fetch into `[]` while propagating every other error (used when a `get` embeds related sub-resources); `unwrap_404_or_403_as_empty_array()` additionally swallows 403 for tier-gated embeds

**`src/tools/slim.rs`** — response slimming, applied to every tool result before serialization. `slim_get` (single-get/create/update) removes `null` fields, strips `_links`/`references`, and collapses user objects to `id`/`username`/`name`. `slim_list` additionally strips bulk-expensive fields from each list item (`description`, `pipeline`, `assignees`, vote counts, …) — they remain available via the single-get tools. **Tests that assert on response contents must account for this**: a field present in the GitLab fixture may be absent from the tool output by design.

**`src/tools/issues.rs`** — Issues domain module. Each operation has a `*Params` struct (derives `Deserialize` + `JsonSchema`) and an `async fn` that builds the URL path, assembles query params or a JSON body, and calls the appropriate `GitlabClient` method. The `#[tool(...)]` shims for the domain live at the bottom of the same file in a `#[tool_router(router = tool_router_issues)]` impl block. Also covers issue links (`issue_links_list`, `issue_link_get`, `issue_link_create`, `issue_link_delete` against `/issues/:iid/links`). `issue_get` enriches the GitLab payload with `linked_issues` (from the links endpoint) and `closed_by` (MRs that close the issue when merged), via `unwrap_404_as_empty_array`.

The remaining domain modules (`commits.rs`, `pipelines.rs`, `pipeline_schedules.rs`, `jobs.rs`, `runners.rs`, `repositories.rs`, `repository_files.rs`, `snippets.rs`, `search.rs`, `groups.rs`, `projects.rs`, `emoji_reactions.rs`, `issue_discussions.rs`, `metadata.rs`) all follow the same pattern; the ones with notable quirks are described below.

**`src/tools/merge_requests.rs`** — Merge Requests domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, update, delete, and merge (accept) operations.

**`src/tools/branches.rs`** — Branches domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, delete, and delete-merged operations. Branch names containing slashes are percent-encoded via the shared `encode_path_segment()` helper from `src/tools/mod.rs`.

**`src/tools/discussions.rs`** — MR Discussions domain module. Implements list, get, create, resolve, note-create, note-update, and note-delete. The `build_position()` helper assembles the nested `position` object for diff notes from flat params, returning `None` when no position fields are set.

**`src/tools/issue_notes.rs`** — Issue Notes domain module. Implements list, get, create, update, and delete for notes on issues.

**`src/tools/epics.rs`** — Epics domain module. Hits the REST Epics API (`/api/v4/groups/:id/epics[/:iid]`) — deprecated since GitLab 17.0 but still fully functional on EE 18.x, where epics haven't been migrated to work items and the work-items GraphQL API rejects epic GIDs. Each operation has a `*Params` struct and an `async fn` mirroring the issues pattern; `group_id: String` accepts a numeric ID or full namespace path, `epic_iid: u64` is the per-group IID from the URL. Two module-local helpers reduce duplication between create and update: `resolve_epic_id()` converts a `parent_epic_iid` to the numeric global ID the REST `parent_id` field expects (an extra GET); `apply_epic_dates()` appends the `*_is_fixed` + `*_fixed` widget pair when a start/due date is set. `parent_epic_iid = 0` on update clears the existing parent. `epic_get` enriches the response with `issues` (child issues of the epic from `/epics/:iid/issues`) via `unwrap_404_as_empty_array`.

**`src/config.rs`** — loads `~/.gitlab_mcp.json`; env vars `GITLAB_URL` / `GITLAB_TOKEN` take precedence. Rejects world-readable config files on Unix. Enforces HTTPS (localhost/127.0.0.1 exempted).

**`src/test_util.rs`** (`#[cfg(test)]`) — shared wiremock fixtures, currently `mock_client()`; use it instead of redefining a local helper in new test modules.

### Adding a new API domain

1. Create `src/tools/<domain>.rs` with `*Params` structs and `async fn` domain functions following the pattern in `issues.rs`.
2. Add `pub mod <domain>;` to `src/tools/mod.rs`.
3. At the bottom of the new module, add the `#[tool(...)]` shim methods in a `#[tool_router(router = tool_router_<domain>, vis = "pub(crate)")] impl GitlabMcpServer` block, each calling the appropriate delegation macro (see the tail of `issues.rs`).
4. Add `+ Self::tool_router_<domain>()` to `tool_router()` in `src/tools/mod.rs` and bump the count in the `combined_router_exposes_every_tool` test — it fails until both are done.

### Writing tool descriptions (LLM discoverability)

The `#[tool(description = ...)]` text is the only thing an LLM matches against when choosing a tool, so write it for the *searcher's* intent and vocabulary, not GitLab's internal API nomenclature:

- **Lead with the verb + noun a user would say** ("Comment on a merge request", "List branches") before explaining the mechanism.
- **Bridge synonyms** where GitLab's term diverges from common usage — GitLab's "note" and "discussion thread" are what users call a "comment". Include both, e.g. "Comment on a GitLab merge request (creates a note / starts a discussion thread)". This is why the MR *discussions* tools — not a separate `gitlab_mrs_notes_*` family — are the way to comment on an MR (see issue #9: the capability existed but wasn't discoverable).
- **Cross-reference near-equivalent tools** so a model searching for the wrong name still lands somewhere useful (e.g. `gitlab_issues_discussions_list` points at `gitlab_issues_notes_list` for the flat view).
- **State the common-case shortcut** when a tool has advanced params — e.g. "pass only `body` for a plain comment; add `position_*` for an inline diff comment".

### Namespace ID encoding

Project- and group-scoped endpoints both accept either a numeric ID (`"42"`) or a namespace path (`"mygroup/myrepo"`). Domain modules build their URL prefix via `project_path()` / `group_path()` from `src/tools/mod.rs`, which percent-encode the namespace through `encode_namespace_id()`. Values used as a single path segment (branch names, file paths, commit refs) go through `encode_path_segment()`, which percent-encodes `/` plus every character that would otherwise corrupt the URL (`#` starts a fragment, `?` starts the query string, literal `%` would be misread as an escape, plus space and the other reserved characters).

## Testing

The overall strategy — unit (wiremock) tests vs. live integration tests, and what each layer verifies — is described in [`docs/testing.md`](docs/testing.md).

Unit tests live alongside each module (`#[cfg(test)] mod tests`) and run by default with `cargo test --all --locked`. End-to-end verification against a real GitLab instance lives under [`src/tools/live/`](src/tools/live/) (one module per API area plus a shared `harness`; Issues — including links, notes, and discussions — Merge Requests, MR Discussions, Branches, Repository Files, Snippets, and Emoji Reactions domains so far), gated behind the `live-tests` cargo feature. Run with `cargo test --features live-tests` plus `GITLAB_URL`/`GITLAB_TOKEN` in the env (against test project `3kirt1/gitlab-mcp-testing`); see [`docs/testing.md`](docs/testing.md) for details. The release process runs this suite as a required gate.
