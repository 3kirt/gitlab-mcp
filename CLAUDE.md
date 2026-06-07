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

**`src/tools/mod.rs`** — MCP server struct and all glue. Contains:
- `GitlabMcpServer` struct with `new_stdio` constructor
- `#[tool_router]` impl block — one `async fn` per tool, each calling a delegation macro
- Delegation macros (`delegate_list!`, `delegate_get!`, `delegate_create!`, `delegate_update!`, `delegate_delete!`, plus the lower-level `delegate_json!`) that fetch the client, call the domain function, and map the result to `CallToolResult`
- `QueryBuilder` / `BodyBuilder` — fluent helpers for building query param slices and JSON request bodies
- `PaginationParams` — shared `page`/`per_page` struct flattened into list param structs
- `unwrap_404_as_empty_array()` — turns a 404 from a supplemental embedded fetch into `[]` while propagating every other error (used when a `get` embeds related sub-resources)

**`src/tools/issues.rs`** — Issues domain module. Each operation has a `*Params` struct (derives `Deserialize` + `JsonSchema`) and an `async fn` that builds the URL path, assembles query params or a JSON body, and calls the appropriate `GitlabClient` method. Also covers issue links (`issue_links_list`, `issue_link_get`, `issue_link_create`, `issue_link_delete` against `/issues/:iid/links`). `issue_get` enriches the GitLab payload with `linked_issues` (from the links endpoint) and `closed_by` (MRs that close the issue when merged), via `unwrap_404_as_empty_array`.

**`src/tools/merge_requests.rs`** — Merge Requests domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, update, delete, and merge (accept) operations.

**`src/tools/branches.rs`** — Branches domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, delete, and delete-merged operations. Branch names containing slashes are percent-encoded via the shared `encode_path_segment()` helper from `src/tools/mod.rs`.

**`src/tools/discussions.rs`** — MR Discussions domain module. Implements list, get, create, resolve, note-create, note-update, and note-delete. The `build_position()` helper assembles the nested `position` object for diff notes from flat params, returning `None` when no position fields are set.

**`src/tools/issue_notes.rs`** — Issue Notes domain module. Implements list, get, create, update, and delete for notes on issues.

**`src/tools/epics.rs`** — Epics domain module. Hits the REST Epics API (`/api/v4/groups/:id/epics[/:iid]`) — deprecated since GitLab 17.0 but still fully functional on EE 18.x, where epics haven't been migrated to work items and the work-items GraphQL API rejects epic GIDs. Each operation has a `*Params` struct and an `async fn` mirroring the issues pattern; `group_id: String` accepts a numeric ID or full namespace path, `epic_iid: u64` is the per-group IID from the URL. Two module-local helpers reduce duplication between create and update: `resolve_epic_id()` converts a `parent_epic_iid` to the numeric global ID the REST `parent_id` field expects (an extra GET); `apply_epic_dates()` appends the `*_is_fixed` + `*_fixed` widget pair when a start/due date is set. `parent_epic_iid = 0` on update clears the existing parent. `epic_get` enriches the response with `issues` (child issues of the epic from `/epics/:iid/issues`) via `unwrap_404_as_empty_array`.

**`src/config.rs`** — loads `~/.gitlab_mcp.json`; env vars `GITLAB_URL` / `GITLAB_TOKEN` take precedence. Rejects world-readable config files on Unix. Enforces HTTPS (localhost/127.0.0.1 exempted).

### Adding a new API domain

1. Create `src/tools/<domain>.rs` with `*Params` structs and `async fn` domain functions following the pattern in `issues.rs`.
2. Add `pub mod <domain>;` to `src/tools/mod.rs`.
3. Add `#[tool(...)]` shim methods to the `#[tool_router]` impl block, each calling the appropriate delegation macro.

### Writing tool descriptions (LLM discoverability)

The `#[tool(description = ...)]` text is the only thing an LLM matches against when choosing a tool, so write it for the *searcher's* intent and vocabulary, not GitLab's internal API nomenclature:

- **Lead with the verb + noun a user would say** ("Comment on a merge request", "List branches") before explaining the mechanism.
- **Bridge synonyms** where GitLab's term diverges from common usage — GitLab's "note" and "discussion thread" are what users call a "comment". Include both, e.g. "Comment on a GitLab merge request (creates a note / starts a discussion thread)". This is why the MR *discussions* tools — not a separate `gitlab_mrs_notes_*` family — are the way to comment on an MR (see issue #9: the capability existed but wasn't discoverable).
- **Cross-reference near-equivalent tools** so a model searching for the wrong name still lands somewhere useful (e.g. `gitlab_issues_discussions_list` points at `gitlab_issues_notes_list` for the flat view).
- **State the common-case shortcut** when a tool has advanced params — e.g. "pass only `body` for a plain comment; add `position_*` for an inline diff comment".

### Namespace ID encoding

Project- and group-scoped endpoints both accept either a numeric ID (`"42"`) or a namespace path (`"mygroup/myrepo"`). `encode_namespace_id()` in `src/tools/mod.rs` (pub crate) URL-encodes the slash when a path is provided and is shared by all domain modules (projects in `issues.rs`/`merge_requests.rs`/etc., groups in `epics.rs`).

## Testing

The overall strategy — unit (wiremock) tests vs. live integration tests, and what each layer verifies — is described in [`docs/testing.md`](docs/testing.md).

Unit tests live alongside each module (`#[cfg(test)] mod tests`) and run by default with `cargo test --all --locked`. End-to-end verification against a real GitLab instance lives under [`src/tools/live/`](src/tools/live/) (one module per API area plus a shared `harness`; Issues — including notes and discussions — Merge Requests, MR Discussions, Branches, Repository Files, and Emoji Reactions domains so far), gated behind the `live-tests` cargo feature. Run with `cargo test --features live-tests` plus `GITLAB_URL`/`GITLAB_TOKEN` in the env (against test project `3kirt1/gitlab-mcp-testing`); see [`docs/testing.md`](docs/testing.md) for details. The release process runs this suite as a required gate.
