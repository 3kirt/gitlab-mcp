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
           → GitLab REST API (or GraphQL API for epics / work items)
```

### Key modules

**`src/client.rs`** — thin `reqwest` wrapper. Sends `PRIVATE-TOKEN: <token>` on every request. REST methods: `get`, `list` (with query params), `post`, `put`, `delete`. GraphQL: `graphql(query, variables)` posts to `/api/graphql` and returns the `data` field; top-level GraphQL `errors` are mapped to `GitlabError::Graphql`, mutation-level errors must be checked by the caller. All methods return `serde_json::Value` so tools pass JSON straight through to the MCP client without intermediate typed structs. `GitlabError::to_tool_message()` truncates API error bodies to 300 chars.

**`src/tools/mod.rs`** — MCP server struct and all glue. Contains:
- `GitlabMcpServer` struct with `new_stdio` constructor
- `#[tool_router]` impl block — one `async fn` per tool, each calling a delegation macro
- Five delegation macros (`delegate_list!`, `delegate_get!`, `delegate_create!`, `delegate_update!`, `delegate_delete!`) that fetch the client, call the domain function, and map the result to `CallToolResult`
- `QueryBuilder` — fluent helper for building `&[(&str, String)]` query param slices
- `PaginationParams` — shared `page`/`per_page` struct flattened into list param structs

**`src/tools/issues.rs`** — Issues domain module. Each operation has a `*Params` struct (derives `Deserialize` + `JsonSchema`) and an `async fn` that builds the URL path, assembles query params or a JSON body, and calls the appropriate `GitlabClient` method.

**`src/tools/merge_requests.rs`** — Merge Requests domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, update, delete, and merge (accept) operations.

**`src/tools/branches.rs`** — Branches domain module. Follows the same pattern as `issues.rs`. Implements list, get, create, delete, and delete-merged operations. Branch names containing slashes are percent-encoded via a module-local `encode_branch_name()` helper.

**`src/tools/discussions.rs`** — MR Discussions domain module. Implements list, get, create, resolve, note-create, note-update, and note-delete. The `build_position()` helper assembles the nested `position` object for diff notes from flat params, returning `None` when no position fields are set.

**`src/tools/issue_notes.rs`** — Issue Notes domain module. Implements list, get, create, update, and delete for notes on issues.

**`src/tools/epics.rs`** — Epics domain module (user-facing). Uses the GraphQL API for group-level work items but exposes a REST-style surface: `group_id: String` (numeric or path) and `epic_iid: u64`. Each operation has a `*Params` struct plus its own GraphQL query/mutation. List and get queries are group-scoped (`group(fullPath:) { workItems(...) }` with `types: [EPIC]`). Two resolvers bridge to the work-items plumbing: `resolve_group_path()` converts a numeric `group_id` via REST `GET /api/v4/groups/:id`; `resolve_epic_gid()` converts `(group_path, epic_iid)` to the global gid via a one-shot GraphQL lookup. Create/update/delete compose against the internal `work_item_*` functions after resolving. `parent_epic_iid = 0` on update clears the existing hierarchy parent (mirroring REST `milestone_id = 0`).

**`src/tools/work_items.rs`** — Internal GraphQL primitives (no public MCP tools). Holds the `pub(crate)` work-item create/update/delete domain functions, their mutation constants, and shared helpers used by `epics.rs`: `type_name_to_gid()` maps short type names (`TASK`, `EPIC`, ...) to `gid://gitlab/WorkItems::Type/<id>`; `usernames_to_ids()` resolves `assignee_usernames` to numeric IDs via a `users(usernames: …)` query and errors if any input doesn't resolve; `check_mutation_errors()` surfaces mutation-level `errors[]` as `GitlabError::Graphql`; `add_shared_widgets()` assembles the widget fields shared by create and update (its `parent_id: Option<Value>` parameter accepts `Value::Null` to clear).

**`src/config.rs`** — loads `~/.gitlab_mcp.json`; env vars `GITLAB_URL` / `GITLAB_TOKEN` take precedence. Rejects world-readable config files on Unix. Enforces HTTPS (localhost/127.0.0.1 exempted).

### Adding a new API domain

1. Create `src/tools/<domain>.rs` with `*Params` structs and `async fn` domain functions following the pattern in `issues.rs`.
2. Add `pub mod <domain>;` to `src/tools/mod.rs`.
3. Add `#[tool(...)]` shim methods to the `#[tool_router]` impl block, each calling the appropriate delegation macro.

### project_id encoding

All GitLab endpoints are project-scoped. The `project_id` field accepts either a numeric ID (`"42"`) or a namespace path (`"mygroup/myrepo"`). `encode_project_id()` in `src/tools/mod.rs` (pub crate) URL-encodes the slash when a path is provided and is shared by all domain modules.

## Testing

End-to-end tool verification is documented in [`docs/testing-protocol.md`](docs/testing-protocol.md). It covers seed data setup, per-tool test cases, cross-tool workflows, and error-handling checks for Issues, Issue Notes, Branches, Merge Requests, MR Discussions, Repository/Files, and Epics endpoints against the test project `3kirt1/gitlab-mcp-testing` (and parent group `3kirt1` for epics).
