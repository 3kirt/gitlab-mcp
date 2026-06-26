# Changelog

All notable changes to gitlab-mcp are documented here.

---

## [0.29.0] — 2026-06-25

Maintenance release — a security fix for a transitive dependency.

### Security
- **quinn-proto 0.11.14 → 0.11.15** — resolves [RUSTSEC-2026-0185](https://rustsec.org/advisories/RUSTSEC-2026-0185)
  (high, 7.5): remote memory exhaustion from unbounded out-of-order stream
  reassembly. Pulled in transitively via `reqwest`.

### Changed
- **anyhow 1.0.102 → 1.0.103** — routine in-range patch bump.

## [0.28.0] — 2026-06-18

Request tracing — opt-in debug logging to diagnose GitLab API failures against a
real instance.

### Added
- **`--debug` / `--log-file` request tracing** — opt-in logging of every GitLab
  request (method + URL, and for GraphQL the query and variables) plus full,
  untruncated error response bodies, to help diagnose API failures (e.g. the
  group-epic `work_item_get` 500) against a real instance:
  - `--debug` raises this crate's log level to `debug`; `RUST_LOG` still
    overrides it for finer control (e.g. `gitlab_mcp=trace` to also log success
    response bodies).
  - `--log-file <PATH>` writes the JSON trace to a file — the reliable way to
    capture output when an MCP client spawns the server (its stderr is otherwise
    unreachable). The file is created owner-only (`0600`), since the trace can
    contain private GitLab content.
  - The `PRIVATE-TOKEN` is never logged (headers are excluded), and REST request
    bodies are not logged either.

## [0.27.0] — 2026-06-15

Users API — a read-only tool family for looking up GitLab users and their SSH keys.

### Added
- **Users tools** — the `gitlab_users_*` family:
  - `list` — `GET /users` with filters for username (exact, case-insensitive),
    search (fuzzy on name/username/public email), active, blocked, external,
    humans (exclude bots/internal), and created-before/after ranges; admin-only
    `order_by` / `sort`; pagination.
  - `get` — a single user's full profile, by numeric ID *or* username.
  - `keys_list` — a user's public SSH keys (e.g. to populate `authorized_keys`
    when provisioning infrastructure), by numeric ID *or* username.
  - `get` and `keys_list` resolve a username to its numeric ID via an extra
    lookup, since those endpoints only accept the numeric ID.

### Fixed
- **Single-get slimming** no longer collapses a top-level user resource to
  `id`/`username`/`name` — `gitlab_users_get` now returns the full profile while
  nested user *references* inside any response are still collapsed as before.

## [0.26.0] — 2026-06-14

GraphQL Work Items — a full tool family for GitLab's unified successor to the
deprecating Issues/Epics REST APIs.

### Added
- **Work Items (GraphQL) tools** — the `gitlab_work_items_*` family, covering the
  unified work-item model (issues, tasks, epics, incidents, objectives/OKRs, key
  results) via GitLab's GraphQL API:
  - CRUD: `get`, `list`, `create`, `update`, `delete`. Create/update take
    *friendly* inputs — type name, label/assignee names, numeric milestone id,
    ISO start/due dates, weight, parent IID — and resolve them to GraphQL global
    IDs internally. Update can detach a parent (`parent_work_item_iid = 0`).
  - Comments: `notes_list` / `note_create` / `note_update` / `note_delete`, with
    threaded replies (`discussion_id`) and internal notes.
  - Linked items: `link_add` / `link_remove` (relates-to / blocks / is-blocked-by).
  - Emoji reactions: `emoji_add` / `emoji_remove` on items and
    `notes_emoji_add` / `notes_emoji_remove` on comments.
  - `list` supports rich filtering (author, assignees, labels, milestone,
    confidentiality, created/updated/due date ranges), `sort`, and `fetch_all`.
  - Read responses surface description, assignees, labels, hierarchy
    (parent/children + counts), dates, milestone, weight, iteration, health
    status, linked items, emoji reactions, comment count, and the merge requests
    that close the item. Output keys are snake_case and values normalized to
    match the REST tools; list responses drop bulk arrays to save tokens.
- **GraphQL client method** — `GitlabClient::graphql` posts to `/api/graphql` and
  surfaces GitLab's HTTP-200-with-`errors` responses (and mutation-payload
  errors) as proper failures.
- **Rate-limit resilience** — every request retries on `429 Too Many Requests`,
  honoring the `Retry-After` header (bounded retries/backoff), so the server
  rides out GitLab's rate limits instead of failing the call.

### Changed
- **`project_id` / `group_id` parameter descriptions** are now defined once via
  `ProjectId` / `GroupId` newtypes instead of being copy-pasted across ~130 tool
  parameter structs. No change to the wire format or accepted values.

---

## [0.25.0] — 2026-06-11

LLM-interaction reliability improvements from issue #10.

### Added
- **`gitlab_tool_schema_get`** — lightweight per-tool schema introspection. Pass
  a tool name and get back its description and parameter JSON Schema (with
  required fields marked) without a full `tools/list` round-trip. Unknown names
  return suggestions of similarly named tools.
- **Expected fields in invalid-params errors** — when tool parameters fail to
  deserialize (e.g. a missing or misnamed field), the error now appends the
  tool's accepted fields, required ones first, so a caller can self-correct in
  the same turn: `... missing field 'issue_iid'. Expected fields: issue_iid
  (required), project_id (required), ...`.
- **Server instructions** — `initialize` now returns instructions documenting
  the parameter naming conventions (`project_id`/`group_id` accept ID or path;
  `<resource>_iid` for URL-visible numbers vs `<resource>_id` for global IDs).

### Changed
- **Consistent identifier parameter names** — the runner tools now take
  `runner_id` (was `id`) and the snippet tools `snippet_id` (was `id`), matching
  the `<resource>_id` convention used everywhere else (e.g. the emoji-reaction
  snippet tools already said `snippet_id`). The old `id` spelling is still
  accepted as a deserialization alias, so existing callers keep working.

---

## [0.24.0] — 2026-06-11

### Fixed
- **URL path-segment encoding** — branch names and file paths containing characters beyond `/` (spaces, `%`, `{`, `}`, `#`, `?`, and others) are now correctly percent-encoded using the `percent-encoding` crate with a full RFC 3986 path-segment character set. Previously only forward slashes were encoded, so branches like `fix/some issue` would produce malformed URLs.

### Changed
- **Per-domain tool routers** — the 157-shim monolith in `src/tools/mod.rs` has been split into per-domain `#[tool_router]` blocks co-located with their domain functions (one per domain file). `mod.rs` now assembles them via a composing `tool_router()` function; a guard test enforces the expected tool count and catches accidental omissions or name collisions.
- **Shared project and group path helpers** — `project_path()` and `group_path()` helpers centralise the `/api/v4/projects/{id}` and `/api/v4/groups/{id}` prefix construction that was previously repeated across every domain module.
- **HTTP status-check de-duplicated** — a `check_status()` helper in `src/client.rs` replaces five copies of the same 4-line error-extraction block in `list`, `post_void`, `get_text`, `delete`, and `handle_response`.
- **Shared test fixture** — `mock_client()` extracted to a new `src/test_util.rs`, eliminating a copy of the function from every domain module's test section.

---

## [0.23.0] — 2026-06-07

This release is test infrastructure, documentation, and developer process only —
there are no changes to tool behaviour or output.

### Added
- **Live integration test suite** — a new `src/tools/live/` suite (gated behind a
  `live-tests` cargo feature) exercises the tools against a real GitLab instance,
  verifying request/response fidelity that the wiremock unit tests structurally
  cannot. Covers Issues (including issue links, notes, and discussions), Merge
  Requests, MR Discussions, Branches, Repository Files, Snippets, and Emoji
  Reactions. Each test is self-seeding and self-cleaning, so runs are idempotent.
- **Live tests are now a required release gate** — the release process runs the
  live suite with real credentials and refuses to tag if it fails or silently
  skips.

### Removed
- **Manual testing protocol retired** — `docs/testing-protocol.md` and the
  `/test-api` skill that drove it are removed, superseded by the deterministic,
  scriptable live suite above.

### Documentation
- Added `docs/testing.md` documenting the two-layer testing strategy — wiremock
  unit tests vs. live integration tests, and what each layer verifies.

---

## [0.22.0] — 2026-06-07

### Fixed
- **Error logging on two tools** — failures from `gitlab_mrs_unapprove` and
  `gitlab_jobs_get_trace` are now forwarded to the MCP client via the logging
  protocol, matching every other tool. Previously these two hand-rolled handlers
  returned the error to the caller but never emitted a log notification.
- **IPv6 loopback over HTTP** — `http://[::1]` GitLab URLs are now exempt from
  HTTPS enforcement, alongside the existing `localhost` and `127.0.0.1`
  exemptions.

### Changed
- **List pagination de-duplicated** — every list tool now routes through a
  shared `list_paginated` helper instead of repeating the
  `page`/`per_page` + pagination-walk boilerplate, removing a class of
  copy-paste drift across the domain modules. No change to tool behaviour or
  output.

---

## [0.21.0] — 2026-06-04

### Changed
- **Comment-tool descriptions rewritten for discoverability** — the issue, merge
  request, and commit discussion tools now lead with "comment/note" intent
  rather than GitLab's internal "discussion thread" wording, and cross-reference
  their equivalents. This makes the existing `gitlab_mrs_discussions_*` tools
  discoverable as the way to comment on a merge request (resolves #9: posting an
  MR comment never required a separate `gitlab_mrs_notes_*` family).

### Documentation
- Added a "Writing tool descriptions" section to `CLAUDE.md` codifying the
  intent-led, synonym-bridging description principles for future tools.

---

## [0.20.0] — 2026-06-01

### Added
- **`fetch_all` auto-pagination** — every list tool now accepts an optional
  `fetch_all` flag. When set, the server walks every page (at 100 items each),
  merges the results into one array, and returns them as a single complete
  page, ignoring `page`/`per_page`. A page-count guard bounds runaway loops,
  and termination tolerates GitLab omitting `X-Total`/`X-Next-Page` on large
  endpoints.
- **Per-page progress notifications** — during a `fetch_all` walk the server
  emits a `notifications/progress` update after each page when the client
  supplied a `progressToken`, with `progress`/`total` as absolute item counts
  (`total` reported when GitLab provides `X-Total`, otherwise omitted).

---

## [0.19.0] — 2026-05-29

### Added
- **MCP logging protocol** — the server now declares the `logging` capability
  and implements `logging/setLevel`. Tool errors are forwarded to the MCP
  client as structured `notifications/message` notifications (`level=error`,
  `logger="gitlab-mcp"`, `data.message` containing the GitLab error text),
  so failures surface in client log panels without requiring stderr inspection.
  Minimum level defaults to `warning`; clients can lower it via
  `logging/setLevel`.

### Changed
- **rmcp upgraded 1.5 → 1.7** — picks up better stdio error handling (parse
  errors now reply `-32700` instead of closing the connection) and runtime
  tool-disabling support.
- Removed the manual `initialize` override; the SDK default now correctly
  records peer info on handshake.

### Documentation
- README: add MCP logging feature bullet; fix dev commands to include
  `--locked`.
- Testing protocol: note MCP logging wire behaviour is covered by the rmcp
  conformance suite; add Section 93 (MCP Logging) with three smoke tests.

---

## [0.18.0] — 2026-05-28

### Added
- **Issue Discussions** — six new tools mirroring the existing MR
  discussions family for threaded comments on issues:
  `gitlab_issues_discussions_list`, `gitlab_issues_discussions_get`,
  `gitlab_issues_discussions_create`,
  `gitlab_issues_discussions_note_create` (reply to an existing thread by
  `discussion_id`), `gitlab_issues_discussions_note_update`, and
  `gitlab_issues_discussions_note_delete`. Closes
  [#8](https://github.com/3kirt/gitlab-mcp/issues/8). Issue discussions
  are non-resolvable and do not support diff-note positions or `commit_id`,
  so the surface is intentionally narrower than the MR equivalent.

### Documentation
- Document issue discussions in the README and in the testing protocol
  (sections 87–92 + Workflow O), and bring the `test-api` skill's
  area→section mapping back in sync with the protocol — adds rows for
  `mr_approvals`, `epic_issues`, `groups`, `projects`, `runners`, and
  `issue_discussions`, plus the new seed-placeholder lookups.

---

## [0.17.0] — 2026-05-27

### Added
- **Runners domain (read-only)** — seven new tools covering all runner scopes:
  `gitlab_runners_list` (runners available to the current user),
  `gitlab_runners_all_list` (all runners on the instance; admin only),
  `gitlab_runners_get` (full details for a single runner),
  `gitlab_runners_jobs_list` (jobs processed by a specific runner),
  `gitlab_runners_managers_list` (individual machine instances registered
  under a runner), `gitlab_runners_list_for_project` (runners available to a
  project), and `gitlab_runners_list_for_group` (runners available to a
  group). All list tools support `page`/`per_page` pagination; filter
  parameters include `type`, `status`, `paused`, `tag_list`, and
  `version_prefix` where applicable.

### Changed
- Updated `reqwest` to `0.13.4`; removed the obsolete `webpki-roots` feature
  (dropped upstream in that release).

### Documentation
- README Available Tools section and intro updated for runners support.
- Testing protocol extended with runner universal invariants (Sections 80–86)
  and Workflow N (runner discovery across scopes).
- Release skill updated to include dependency audit (`cargo outdated`) and
  security check (`cargo audit`) steps before the quality gate.

---

## [0.16.0] — 2026-05-27

### Added
- **Epic-issue linking** — two new tools: `gitlab_epics_issue_assign`
  (`POST /groups/:id/epics/:iid/issues/:issue_id`) and
  `gitlab_epics_issue_remove`
  (`DELETE /groups/:id/epics/:iid/issues/:epic_issue_id`). Assign takes the
  **global** issue ID (not the project-scoped IID); remove takes the
  **association** ID returned in the `id` field of the issues array from
  `gitlab_epics_get` (or from the assign response).
- **MR approvals** — two new tools: `gitlab_mrs_approve`
  (`POST /projects/:id/merge_requests/:iid/approve`, returns the updated
  approval state with `approvals_left` and `approved_by`; optional `sha`
  guards against approving a since-updated MR) and `gitlab_mrs_unapprove`
  (`POST /projects/:id/merge_requests/:iid/unapprove`, no response body).
- **Projects domain (read-only)** — one new tool: `gitlab_projects_get`
  (`GET /projects/:id`), accepting a numeric ID or full namespace path.
  Optional `statistics=true` embeds commit and storage counts (requires
  Reporter role or higher).

### Internal
- New `GitlabClient::post_void` helper for `POST` endpoints that return no
  response body (used by `mr_unapprove`); covered by dedicated wiremock
  tests for both success and non-2xx propagation.

### Documentation
- README Epics, Merge Requests, and Projects sections updated for the new
  tools. The Epics section calls out the global-issue-ID vs association-ID
  distinction for assign/remove.
- Testing protocol extended with Section 17B (MR approve/unapprove),
  Section 47B (epic-issue assign/remove), and Section 79 (project get),
  plus a Projects universal-invariants table. Workflow H now exercises
  epic-issue assign and remove as part of the epic lifecycle.

---

## [0.15.0] — 2026-05-26

### Added
- **Groups domain (read-only)** — two new tools: `gitlab_groups_list` and
  `gitlab_groups_get`. `gitlab_groups_list` supports `search`, `owned`,
  `all_available`, `min_access_level`, `top_level_only`, plus standard
  pagination and sorting. `gitlab_groups_get` accepts a numeric ID or full
  namespace path, with an optional `with_projects` flag (defaults to `false`
  to keep responses compact — GitLab's upstream default is `true`, which
  would embed up to 100 projects on every fetch).

### Documentation
- Testing protocol extended with Sections 77–78 (Groups list and get) and
  the groups universal-invariants table.

---

## [0.14.0] — 2026-05-25

### Added
- **Emoji Reactions domain** — twenty-four new tools covering GitLab's
  award_emoji surface across six resource types (issues, merge requests,
  project snippets, and notes on each), each with list, get, create, and
  delete operations: `gitlab_emoji_reactions_issues_*`,
  `gitlab_emoji_reactions_mrs_*`, `gitlab_emoji_reactions_snippets_*`,
  `gitlab_emoji_reactions_issue_notes_*`, `gitlab_emoji_reactions_mr_notes_*`,
  `gitlab_emoji_reactions_snippet_notes_*`. Emoji `name` follows GitLab's
  no-colons convention (e.g. `"thumbsup"`, `"tada"`); deletion requires the
  reaction author or an administrator.

### Documentation
- Testing protocol extended with seed step 13 (emoji reaction seeding),
  Sections 71–76 (one per resource family, with list/get/create/delete
  subsections), and Workflow M (emoji lifecycle across an issue).
- Fixed a label mix-up in Step 13d and Section 74 of the testing protocol
  where the issue note seeded in Step 6 was referenced as `note-seed-1`
  instead of `note-issue-seed-1` (the former is the MR discussion note
  seeded in Step 7).

### Developer experience
- Added a `/test-api` skill (`.claude/commands/test-api.md`) that maps an
  API-area argument to the corresponding section range in the testing
  protocol, resolves seed placeholders via MCP tool lookups, and reports
  per-case PASS/FAIL results. Forbids shell/curl invocations — every
  GitLab interaction must go through an MCP tool call.

---

## [0.13.0] — 2026-05-24

### Added
- **Snippets domain** — ten new tools: `gitlab_snippets_list`,
  `gitlab_snippets_public_list`, `gitlab_snippets_all_list`,
  `gitlab_snippets_get`, `gitlab_snippets_raw`, `gitlab_snippets_file_raw`,
  `gitlab_snippets_create`, `gitlab_snippets_update`, `gitlab_snippets_delete`,
  `gitlab_snippets_user_agent_detail`. Covers personal snippet CRUD, raw and
  per-file content retrieval, and multi-file snippet management (create, update,
  move, delete actions on individual files).

### Fixed
- `gitlab_snippets_create` now always sends `visibility` in the request body,
  defaulting to `"private"` when not specified. Previously the field was omitted,
  causing GitLab.com to select `internal` visibility — which is restricted —
  resulting in a 403 error.

### Documentation
- Testing protocol extended with seed step 12, Sections 63–70 (Snippets list,
  get, raw, file raw, create, update, delete, user agent detail), and Workflow L.

---

## [0.12.0] — 2026-05-24

### Added
- `gitlab_mrs_get` now embeds `closes_issues` (issues that will be closed when the MR
  merges) and `related_issues` (issues linked to the MR), mirroring the enrichment
  already present in `gitlab_issues_get`.
- Embedded sub-resource fetches in `issue_get` and `mr_get` are now performed in
  parallel, reducing latency when multiple supplemental endpoints are queried.

### Documentation
- Testing protocol extended with coverage for `mr_get` and `issue_get` embedded arrays.

---

## [0.11.0] — 2026-05-24

### Added
- **Search domain** — three new tools: `gitlab_search_global`, `gitlab_search_group`,
  `gitlab_search_project`. Supports searching across projects, issues,
  merge requests, milestones, snippets, users, wiki blobs, commits, and blobs.
  Includes filtering by scope, search type (basic/advanced/zoekt), state, and
  confidentiality.
- **Pipeline Schedules domain** — twelve new tools: `gitlab_pipeline_schedules_list`,
  `gitlab_pipeline_schedules_get`, `gitlab_pipeline_schedules_pipelines_list`,
  `gitlab_pipeline_schedules_create`, `gitlab_pipeline_schedules_update`,
  `gitlab_pipeline_schedules_delete`, `gitlab_pipeline_schedules_take_ownership`,
  `gitlab_pipeline_schedules_play`, and variable management tools
  (`gitlab_pipeline_schedules_variables_create`, `_get`, `_update`, `_delete`).
- **Metadata API** — new `gitlab_metadata_get` tool returns GitLab instance
  metadata: version, revision, enterprise status, and Kubernetes agent
  server (KAS) information.

### Documentation
- Testing protocol extended with Section 52 (Metadata), Sections 53–59
  (Pipeline Schedules + variables, plus Workflow J), and Sections 60–62
  (Search global/group/project, plus Workflow K).

---

## [0.10.0] — 2026-05-24

### Added
- **Issue links domain** — four new tools: `gitlab_issues_links_list`,
  `gitlab_issues_links_get`, `gitlab_issues_links_create`,
  `gitlab_issues_links_delete`. Supports all three GitLab link types:
  `relates_to`, `blocks`, and `is_blocked_by`.
- `gitlab_issues_get` now embeds a `linked_issues` array (all issue links
  with their `link_type` and `issue_link_id`) and a `closed_by` array
  (merge requests that will close the issue when merged).
- `GitlabClient::delete_json` — new client method for DELETE endpoints that
  return a response body rather than 204 No Content.
- `unwrap_404_as_empty_array` helper in `tools/mod.rs` for graceful
  degradation of supplemental fetches embedded in a primary response.

### Changed
- **Epics migrated from GraphQL to REST API** (`/api/v4/groups/:id/epics`).
  Fixes `gitlab_epics_get` returning 500 on GitLab EE 18.x where the
  work-items GraphQL API rejects Epic GIDs. All five epic tools are
  unaffected at the call surface; the GraphQL plumbing is fully replaced.
- `encode_project_id` renamed to `encode_namespace_id` to reflect its use
  for both project and group IDs.

### Fixed
- Removed unsupported GraphQL widgets (`linkedItems`, `notes`) from the
  `epic_get` query that caused failures on GitLab EE 18.x.

### Documentation
- Testing protocol updated with seed step 9 (issue link seeding), universal
  invariant tables for issue links, Sections 48–51 (list, get, create,
  delete), and Workflow I.
- Testing protocol updated with EE 18.x regression notes for epics and
  removed widget references.

---

## [0.9.0] — 2026-05-21

### Added
- **Epics domain** — five REST-style tools for group-level epics, backed by
  GitLab's GraphQL API: `gitlab_epics_list`, `gitlab_epics_get`,
  `gitlab_epics_create`, `gitlab_epics_update`, `gitlab_epics_delete`. Inputs
  mirror the rest of the toolset: `group_id` accepts a numeric ID or a full
  namespace path, and `epic_iid` is the IID from the URL — global
  `gid://gitlab/WorkItem/NNN` strings never appear in the tool surface.
  Numeric `group_id` is resolved internally via a REST lookup.
- `gitlab_epics_get` widget enrichment: `linkedItems` (issues/work items
  linked via the GitLab UI; first 20) and `notes` (first 20 discussions with
  their notes). Closes [#6](https://github.com/3kirt/gitlab-mcp/issues/6).
- `parent_epic_iid=0` on update clears the existing hierarchy parent
  (mirroring REST `milestone_id=0`).

### Removed
- **Breaking:** the five `gitlab_work_items_*` tools introduced in 0.8.0 are
  removed. The create/update/delete primitives plus the shared helpers
  (`type_name_to_gid`, `usernames_to_ids`, `check_mutation_errors`,
  `add_shared_widgets`) are retained in `src/tools/work_items.rs` as
  `pub(crate)` building blocks used by `epics.rs`; the unused project-scoped
  list/get code was deleted.

### Documentation
- README: replaced the Work Items section with an Epics section; updated tool
  table accordingly.
- Testing protocol: replaced sections 43–47 (Work Items) with a new Epics
  section covering list, get, create, update (including parent clearing via
  iid=0), and delete against the seeded test group.

---

## [0.8.0] — 2026-05-20

### Added
- **Work Items domain** — five new tools covering tasks, epics, tickets,
  incidents, test cases, requirements, objectives, and key results via the
  GraphQL API: `gitlab_work_items_list`, `gitlab_work_items_get`,
  `gitlab_work_items_create`, `gitlab_work_items_update`,
  `gitlab_work_items_delete`. List pagination is cursor-based (`first` /
  `after`) and tools accept the full `project_path` rather than `project_id`.
- `GitlabClient::graphql()` — wraps `POST /api/graphql`, returns the `data`
  field, maps top-level GraphQL errors to `GitlabError::Graphql`, and leaves
  mutation-level errors for callers to check via `check_mutation_errors()`.

### Changed
- `assignee_usernames` on work item create/update now resolves names to user
  IDs via GraphQL before submitting the mutation. Unknown usernames cause the
  call to fail with `"unknown username(s): …"` rather than being silently
  dropped from the assignee list. Match is case-insensitive.

### Documentation
- README: added Work Items section explaining `project_path`, cursor pagination,
  and the global-ID requirement; bumped headline from eight to nine domains.
- CLAUDE.md: added `work_items.rs` to key modules, documented the `graphql()`
  client method, and generalized the request-flow diagram to include GraphQL.
- Testing protocol: added Work Items coverage across seed setup, sections 45–47,
  workflow H, and §46.6 covering the unknown-username error path.

---

## [0.7.0] — 2026-05-20

### Changed
- Issue list responses now strip ten additional low-signal fields: `assignees`,
  `blocking_issues_count`, `confidential`, `downvotes`, `has_tasks`, `imported`,
  `imported_from`, `issue_type`, `severity`, `task_status`, `upvotes`. These fields
  are almost always zero/false/duplicate/unknown and are still available on single-get
  responses via `gitlab_issues_get`.

### Added
- `--version` flag prints the current version and exits
- `--help` output expanded with environment variable documentation and quickstart
  setup instructions

---

## [0.6.0] — 2026-05-19

### Added
- **Issue Notes domain** — five new tools covering full CRUD on issue comments:
  `gitlab_issues_notes_list`, `gitlab_issues_notes_get`, `gitlab_issues_notes_create`,
  `gitlab_issues_notes_update`, `gitlab_issues_notes_delete`

### Fixed
- `created_at` field description corrected to "requires administrator or Owner role"
  in both the Issue Notes and MR Discussions tool schemas (was overstated as Reporter)

### Documentation
- README: added Issue Notes and MR Discussions tools tables (Discussions table was missing)
- CLAUDE.md: added `discussions.rs` and `issue_notes.rs` to key modules section
- Testing protocol: added seed step 6, sections 38–42, and workflow G for Issue Notes

---

## [0.5.0] — 2026-05-19

### Added
- **MR Discussions domain** — seven new tools for merge request code review workflows:
  `gitlab_mrs_discussions_list`, `gitlab_mrs_discussions_get`,
  `gitlab_mrs_discussions_create`, `gitlab_mrs_discussions_resolve`,
  `gitlab_mrs_discussions_note_create`, `gitlab_mrs_discussions_note_update`,
  `gitlab_mrs_discussions_note_delete`
- Diff-note position support in `gitlab_mrs_discussions_create` — inline code comments
  can be anchored to a specific file, line, and commit range

---

## [0.4.0] — 2026-05-19

### Changed
- All list endpoints now return a pagination envelope instead of a bare array:
  ```json
  { "items": [...], "page": 1, "per_page": 20, "total": 49, "total_pages": 3, "next_page": 2 }
  ```
  `total`, `total_pages`, and `next_page` are omitted when GitLab does not supply them.
  **Breaking change** — callers that indexed the array directly must now read `response["items"]`.

---

## [0.3.1] — 2026-05-19

### Added
- Date range filters on all list endpoints: `created_after`, `created_before`,
  `updated_after`, `updated_before` (ISO 8601)

### Changed
- List responses are now slimmed to reduce token usage: `description`, `pipeline`,
  `head_pipeline`, `diff_stats`, `time_stats`, `_links`, and `references` stripped;
  null fields removed; user objects collapsed to `{id, username, name}`

---

## [0.3.0] — 2026-05-18

### Changed
- HTTP transport disabled pending a secure OAuth implementation (stdio only)

### Removed
- HTTP transport source files and associated dead dependencies

---

## [0.2.0] — 2026-05-18

### Added
- **Commits domain** — 15 tools: list, create, get, diff, refs, sequence check,
  cherry-pick, revert, comments list/create, discussions list, statuses list/set,
  merge requests for commit, and GPG/SSH signature
- **Pipelines domain** — 11 tools: list, get, get latest, variables, test report,
  test report summary, create, retry, cancel, delete, update metadata
- **Jobs domain** — 9 tools: list, list for pipeline, list bridges, get, get trace,
  cancel, retry, erase, play
- Unit tests for `BodyBuilder`, `QueryBuilder`, `encode_project_id`,
  `encode_path_segment`, `json_list_result`, `to_tool_message`, and
  `GitlabClient` HTTP behaviour (via wiremock)
- CI now requires `cargo clippy` to pass before release builds

### Fixed
- MR draft toggle: `draft` param now uses the title `Draft:` prefix accepted by the
  GitLab API (the `wip` body field used previously was rejected)
- `enforce_https` localhost prefix-bypass: host is now compared exactly against
  `"localhost"` / `"127.0.0.1"` via `url::Url` parsing rather than bare prefix match

### Changed
- `BodyBuilder` introduced — eliminates ~200 lines of repetitive `json!` body
  construction across all domain modules
- Path-segment encoders consolidated to a single `encode_path_segment()` in
  `tools/mod.rs`; module-local duplicates removed
- Delegation macros deduplicated via a shared `delegate_json!` core
- `GitlabClient` method names cleaned up

---

## [0.1.1] — 2026-05-17

### Fixed
- Formatting: applied `cargo fmt` to resolve CI lint failure introduced in v0.1.0

---

## [0.1.0] — 2026-05-17

Initial release.

### Added
- **Issues domain** — 5 tools: `gitlab_issues_list`, `gitlab_issues_get`,
  `gitlab_issues_create`, `gitlab_issues_update`, `gitlab_issues_delete`
- **Merge Requests domain** — 6 tools: list, get, create, update, delete, merge
- **Branches domain** — 5 tools: list, get, create, delete, delete-merged
- **Repository Files domain** — 6 tools: get, raw, blame, create, update, delete
- **Repositories domain** — 9 tools: tree, blob get, blob raw, compare, contributors,
  merge base, changelog get/add, health
- Numeric project ID and namespace path (`mygroup/myrepo`) both accepted on all tools
- `GITLAB_URL` / `GITLAB_TOKEN` env vars; `~/.gitlab_mcp.json` config file fallback
- HTTPS enforcement (localhost/127.0.0.1 exempted)
- stdio transport (MCP)
