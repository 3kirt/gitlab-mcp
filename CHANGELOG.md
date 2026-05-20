# Changelog

All notable changes to gitlab-mcp are documented here.

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
