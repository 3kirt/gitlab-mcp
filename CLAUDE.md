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

**`src/client.rs`** — thin `reqwest` wrapper. Sends `PRIVATE-TOKEN: <token>` on every request. REST methods: `get`, `list` (with query params and pagination metadata), `post`, `put`, `delete` (expects 204), `delete_json` (DELETE endpoints that return a response body), `delete_with_body`, `get_text`. There is also one GraphQL method: `graphql(query, variables)` POSTs to `/api/graphql` and returns the unwrapped `data` object. Because GitLab's GraphQL endpoint returns HTTP 200 even on query failure, it surfaces a non-empty top-level `errors` array as `GitlabError::Api { status: 200, .. }`. Note this only catches *query-level* errors — mutation-payload errors (a populated `data.<mutation>.errors` with a null result) must be checked by the calling domain function. All JSON methods return `serde_json::Value` so tools pass payloads straight through to the MCP client without intermediate typed structs. `GitlabError` variants: `Api { status, body }` (HTTP error from GitLab), `Http` (reqwest transport error), `Other(String)` (validation failure or malformed response). `GitlabError::to_tool_message()` truncates API error bodies to 300 chars.

**`src/tools/mod.rs`** — MCP server struct and shared glue. Contains:
- `GitlabMcpServer` struct with `new_stdio` constructor
- `tool_router()` — sums the per-domain sub-routers (`Self::tool_router_<domain>()`, one per module) into the router that `#[tool_handler]` and `new_stdio` consume. The `combined_router_exposes_every_tool` test asserts the exact tool count, so it fails if a new domain's router is forgotten here (or two tools collide on a name)
- Delegation macros (`delegate_list!`, `delegate_get!`, `delegate_create!`, `delegate_update!`, `delegate_delete!`, `delegate_unit!`, `delegate_text!`, plus the lower-level `delegate_json!`) that fetch the client, call the domain function, and map the result to `CallToolResult`. They are defined *before* the `pub mod` declarations so they are in textual macro scope inside every domain module, and their bodies use `$crate::`/`rmcp::` paths so they expand correctly there
- `QueryBuilder` / `BodyBuilder` — fluent helpers for building query param slices and JSON request bodies
- `PaginationParams` — shared `page`/`per_page` struct flattened into list param structs
- `project_path()` / `group_path()` — build the `/api/v4/projects/{id}` / `/api/v4/groups/{id}` prefix every scoped endpoint starts from (callers append their suffix)
- `unwrap_404_as_empty_array()` — turns a 404 from a supplemental embedded fetch into `[]` while propagating every other error (used when a `get` embeds related sub-resources); `unwrap_404_or_403_as_empty_array()` additionally swallows 403 for tier-gated embeds
- `call_tool` override — wraps router dispatch in the `PROGRESS_CTX` task-local scope (per-page progress notifications during `fetch_all`), and enriches invalid-params errors via `enrich_invalid_params()`: when parameter deserialization fails, the tool's accepted fields (required first, from its input schema) are appended to the error message so an LLM caller can self-correct without a schema lookup

**`src/tools/slim.rs`** — response slimming, applied to every tool result before serialization. `slim_get` (single-get/create/update) removes `null` fields, strips `_links`/`references`, and collapses user objects to `id`/`username`/`name`. `slim_list` additionally strips bulk-expensive fields from each list item (`description`, `pipeline`, `assignees`, vote counts, …) — they remain available via the single-get tools. **Tests that assert on response contents must account for this**: a field present in the GitLab fixture may be absent from the tool output by design.

**`src/tools/issues.rs`** — Issues domain module. Each operation has a `*Params` struct (derives `Deserialize` + `JsonSchema`) and an `async fn` that builds the URL path, assembles query params or a JSON body, and calls the appropriate `GitlabClient` method. The `#[tool(...)]` shims for the domain live at the bottom of the same file in a `#[tool_router(router = tool_router_issues)]` impl block. Also covers issue links (`issue_links_list`, `issue_link_get`, `issue_link_create`, `issue_link_delete` against `/issues/:iid/links`). `issue_get` enriches the GitLab payload with `linked_issues` (from the links endpoint) and `closed_by` (MRs that close the issue when merged), via `unwrap_404_as_empty_array`.

The remaining domain modules (`commits.rs`, `pipelines.rs`, `pipeline_schedules.rs`, `jobs.rs`, `runners.rs`, `repositories.rs`, `repository_files.rs`, `snippets.rs`, `search.rs`, `groups.rs`, `projects.rs`, `emoji_reactions.rs`, `issue_discussions.rs`, `metadata.rs`) all follow the same pattern; the ones with notable quirks are described below.

**`src/tools/merge_requests.rs`** — Merge Requests domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, update, delete, and merge (accept) operations.

**`src/tools/branches.rs`** — Branches domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, delete, and delete-merged operations. Branch names containing slashes are percent-encoded via the shared `encode_path_segment()` helper from `src/tools/mod.rs`.

**`src/tools/discussions.rs`** — MR Discussions domain module. Implements list, get, create, resolve, note-create, note-update, and note-delete. The `build_position()` helper assembles the nested `position` object for diff notes from flat params, returning `None` when no position fields are set.

**`src/tools/issue_notes.rs`** — Issue Notes domain module. Implements list, get, create, update, and delete for notes on issues.

**`src/tools/metadata.rs`** — instance metadata plus `gitlab_tool_schema_get`, a per-tool schema introspection tool: given a tool name it returns the description and parameter JSON Schema straight from `self.tool_router` (serialized directly, bypassing `slim`); unknown names get token-based "similarly named tools" suggestions.

**`src/tools/epics.rs`** — Epics domain module. Hits the REST Epics API (`/api/v4/groups/:id/epics[/:iid]`) — deprecated since GitLab 17.0 but still fully functional on EE 18.x, where epics haven't been migrated to work items and the work-items GraphQL API rejects epic GIDs. Each operation has a `*Params` struct and an `async fn` mirroring the issues pattern; `group_id: String` accepts a numeric ID or full namespace path, `epic_iid: u64` is the per-group IID from the URL. Two module-local helpers reduce duplication between create and update: `resolve_epic_id()` converts a `parent_epic_iid` to the numeric global ID the REST `parent_id` field expects (an extra GET); `apply_epic_dates()` appends the `*_is_fixed` + `*_fixed` widget pair when a start/due date is set. `parent_epic_iid = 0` on update clears the existing parent. `epic_get` enriches the response with `issues` (child issues of the epic from `/epics/:iid/issues`) via `unwrap_404_as_empty_array`.

**`src/tools/work_items.rs`** — Work Items domain module, and the **only GraphQL-backed module** (everything else is REST). Work items are GitLab's unified successor to the deprecated Issues/Epics REST APIs (issue, task, epic, incident, objective/OKR, key result). Full CRUD plus comments: `work_item_get`, `work_items_list`, `work_item_create`, `work_item_update`, `work_item_delete`, and the notes family `work_item_notes_list` / `work_item_note_create` / `work_item_note_update` / `work_item_note_delete` (notes = comments, read via the NOTES widget, written via the generic `createNote`/`updateNote`/`destroyNote` mutations with the work item's GID as `noteableId`; note IDs are global `gid://gitlab/Note/N`, so update/delete take that GID directly and need no namespace/IID). Differs from the REST modules in three ways worth knowing: (1) it addresses namespaces by **full path string** (`namespace_path`, a project *or* group path) via the GraphQL `namespace(fullPath:)` field, not the numeric-or-path `project_id`/`group_id`; (2) response field names are GraphQL **camelCase** (`createdAt`, `webUrl`, `workItemType`), passed through verbatim; (3) the **widget architecture** — attributes beyond `title`/`state` arrive in a typed `widgets[]` array, which `flatten_work_item()` lifts to the top level (description, assignees, labels→title strings, hierarchy parent/children, start/due dates) so callers see a flat object. List uses GraphQL **cursor pagination** (`first`/`after` + a returned `pageInfo { hasNextPage, endCursor }`), not the REST `paginate`/`PaginationMeta`/`fetch_all` machinery, so it returns a plain `{ nodes, pageInfo }` Value via `delegate_json!` rather than `delegate_list!`. The mutations add several module-local **GID-resolution** helpers so the LLM-facing params stay friendly (names/IIDs, never raw GIDs): `resolve_work_item_type_id` (type name → `WorkItems::Type` GID for create), `resolve_work_item_gid` (namespace IID → `WorkItem` GID for update/delete and for the `parent_work_item_iid` hierarchy param — an extra query, like `resolve_epic_id` in epics.rs), `resolve_user_ids` (assignee usernames → `User` GIDs, one query), and `resolve_label_ids` (label names → `Label` GIDs; fetches the namespace's labels by trying the path as both `project` (with ancestor groups) and `group` in one aliased query, then matching titles case-insensitively, bounded at 100). All mutations run their payload through `check_mutation_errors` — GitLab returns business-logic failures in a payload `errors` array at HTTP 200, which `graphql` can't catch (see the `graphql` note under `client.rs`). `work_item_create` accepts a friendly type *name* (case-insensitive) plus `labels`/`assignees`/`parent_work_item_iid`; `work_item_update` adds `add_labels`/`remove_labels`/`assignees`/`parent_work_item_iid`, maps REST-style `state_event` "close"/"reopen" to the GraphQL `WorkItemStateEvent` CLOSE/REOPEN, and routes `description` through `descriptionWidget` (update has no top-level description field, unlike create). Both also take `start_date`/`due_date`, `milestone_id` (numeric — the `Milestone` GID is built directly, no lookup), and `weight`, applied by the shared `apply_scalar_widgets` helper: dates send `isFixed: true` (work items distinguish fixed vs rolled-up dates, like the REST epics `*_is_fixed` pair). **`weight` needs Premium/Ultimate** — on Free the WEIGHT widget is absent and the mutation rejects it, so it's wired and unit-tested but not live-verified (cf. epics). `WORK_ITEM_FIELDS`/`flatten_work_item` also read milestone and weight. Note GitLab enforces hierarchy rules (e.g. Issue→Task is allowed, Issue→Issue is not).

**`src/config.rs`** — loads `~/.gitlab_mcp.json`; env vars `GITLAB_URL` / `GITLAB_TOKEN` take precedence. Rejects world-readable config files on Unix. Enforces HTTPS (localhost/127.0.0.1 exempted).

**`src/test_util.rs`** (`#[cfg(test)]`) — shared wiremock fixtures, currently `mock_client()`; use it instead of redefining a local helper in new test modules.

### Adding a new API domain

1. Create `src/tools/<domain>.rs` with `*Params` structs and `async fn` domain functions following the pattern in `issues.rs`. Name identifier params `<resource>_iid` (per-project/group numbers visible in URLs) or `<resource>_id` (globally unique IDs) — never bare `id`, so a name learned on one tool transfers to every other (issue #10; the `initialize` instructions in `get_info` document this convention to clients).
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

Project- and group-scoped endpoints both accept either a numeric ID (`"42"`) or a namespace path (`"mygroup/myrepo"`). The `project_id` / `group_id` parameters are typed with the **`ProjectId` / `GroupId` newtypes** (in `src/tools/mod.rs`), not `String`: each is `#[serde(transparent)]` over a string (wire shape unchanged) and `Deref`s to `str` (so `project_path(&p.project_id)` works via deref coercion), and its `JsonSchema` impl carries the parameter description in **one** place instead of a `#[schemars(description = …)]` literal repeated across ~130 structs. New `*Params` structs should use these newtypes for those fields (no per-field description needed); reserve plain `String` for project/group params that need a *different* description (e.g. `target_project_id`). Domain modules build their URL prefix via `project_path()` / `group_path()` from `src/tools/mod.rs`, which percent-encode the namespace through `encode_namespace_id()`. Values used as a single path segment (branch names, file paths, commit refs) go through `encode_path_segment()`, which percent-encodes `/` plus every character that would otherwise corrupt the URL (`#` starts a fragment, `?` starts the query string, literal `%` would be misread as an escape, plus space and the other reserved characters).

## Testing

The overall strategy — unit (wiremock) tests vs. live integration tests, and what each layer verifies — is described in [`docs/testing.md`](docs/testing.md).

Unit tests live alongside each module (`#[cfg(test)] mod tests`) and run by default with `cargo test --all --locked`. End-to-end verification against a real GitLab instance lives under [`src/tools/live/`](src/tools/live/) (one module per API area plus a shared `harness`; Issues — including links, notes, and discussions — Merge Requests, MR Discussions, Branches, Repository Files, Snippets, Emoji Reactions, and Work Items domains so far), gated behind the `live-tests` cargo feature. The Work Items live module is a *cross-API equivalence* check rather than a plain lifecycle test: it seeds an issue over REST, reads it back through both `issues` (REST) and `work_items` (GraphQL), and asserts the overlapping fields agree — this is the primary verification that the GraphQL queries/shaping in `work_items.rs` are correct against a real instance. Run with `cargo test --features live-tests` plus `GITLAB_URL`/`GITLAB_TOKEN` in the env (against test project `3kirt1/gitlab-mcp-testing`); see [`docs/testing.md`](docs/testing.md) for details. The release process runs this suite as a required gate.
