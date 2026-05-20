# GitLab MCP Testing Protocol

This document describes how to verify all Issues, Issue Notes, Branches, Merge Requests, MR Discussions, Repository/Files, and Work Items API functionality against the test project `3kirt1/gitlab-mcp-testing` (numeric ID `82279422`).

**Automated coverage (not retested here):** `encode_project_id`, `encode_path_segment`, `QueryBuilder`, `BodyBuilder`, `enforce_https`, and `GitlabError::to_tool_message()` truncation are covered by unit tests. Error propagation — non-2xx responses from GitLab surfacing as the correct error message — is covered by wiremock tests against `GitlabClient`. For work items, `type_name_to_gid` (all 9 type mappings, case-insensitivity, GID pass-through), `check_mutation_errors` (empty/absent/non-empty arrays), and all five domain functions (success paths, null/not-found errors, mutation error propagation) are covered by wiremock tests. The manual sections below focus exclusively on GitLab's own behavior: field presence, filter correctness, state transitions, hierarchy relationships, and cross-resource consistency.

---

## Shared Patterns

The following behaviors apply across all sections. They are not re-stated per section.

**Numeric project IDs** — `encode_project_id` is unit-tested. Run one live smoke test at the start of any section using `project_id="82279422"` instead of the path form. No need to repeat the numeric-ID variant in every section.

**List response shape (REST)** — Every REST list endpoint returns an envelope object: `{ "items": [...], "page", "per_page", "total", "total_pages", "next_page" }`. The pagination fields are populated from GitLab's `X-*` response headers and any field GitLab omits (e.g. `X-Total` on large endpoints, `X-Next-Page` on the last page) is omitted from the envelope. Item-level invariants in the tables below apply to entries in `items`.

**List response shape (GraphQL / Work Items)** — Work item list responses use cursor-based pagination: `{ "items": [...], "has_next_page": bool, "end_cursor": string | null }`. There are no `page`, `per_page`, or `total` fields. Pass `end_cursor` as `after` in the next call to advance the cursor; `has_next_page: false` with `end_cursor: null` signals the last page. Use `first` to control page size (default 20, max 100).

**Empty results** — Any search, filter, or regex with no matches returns `{"items": []}` (plus whatever pagination fields GitLab populates); no error. Not retested per section.

**Pagination** — Covered in Section 6 (issues). The same `page`/`per_page` logic applies to all list endpoints; not retested per domain.

**Response slimming** — List endpoints apply heavy slimming to every item: `description`, `pipeline`, `head_pipeline`, `diff_stats`, `time_stats`, `_links`, and `references` are stripped, null fields are removed, and user objects (`author`, `assignee`, `reviewers`, etc.) are collapsed to `{id, username, name}`. Single-get, create, and update responses apply a lighter pass: only nulls, `_links`, and `references` are removed, and user objects are still collapsed, but `description` and `pipeline` are preserved. Do not expect stripped fields to be present in list responses; use the corresponding single-get endpoint when full detail is needed.

**Verify-after-delete** — Each delete test ends with a get against the deleted resource and expects a `404` error. This is noted inline, not as a separate numbered step.

---

## Universal Invariants

Check these on every response.

**Issues and merge requests:**

| Property | What to verify |
|---|---|
| `iid` present | Project-scoped `iid` (shown in GitLab UI) |
| `id` present | Global GitLab `id` |
| `project_id` present | Matches the requested project |
| `state` present | Never absent or `null` |
| `title` present | Non-empty string |
| `web_url` present | Non-empty URL |
| List envelope shape | `{items: [...], page, per_page, total?, total_pages?, next_page?}` — invariants apply to each entry in `items` |
| Delete confirmation | Success text message, not a JSON object |
| `description` absent from lists | Stripped by list slimming; present on single-get responses |
| `_links` / `references` absent | Stripped from all responses (list and get) |

**Merge requests only:**

| Property | What to verify |
|---|---|
| `source_branch` | Non-empty string |
| `target_branch` | Non-empty string |
| `author` in lists | Collapsed to `{id, username, name}` only — no `avatar_url`, `web_url`, `state` |
| `author` in single-get | Same collapsed form |
| `pipeline` / `head_pipeline` absent from lists | Stripped by list slimming; present on single-get responses |

**Branches:**

| Property | What to verify |
|---|---|
| `name` | Non-empty string |
| `commit` | Object with at least `id` |
| `merged` | `true` or `false`, never `null` |
| `protected` | `true` or `false`, never `null` |
| `web_url` | Non-empty URL |
| List envelope shape | `{items: [...], page, per_page, total?, total_pages?, next_page?}` — invariants apply to each entry in `items` |
| Delete confirmation | Success text message, not a JSON object |

**Repository tree entries:**

| Property | What to verify |
|---|---|
| `id` | Non-empty SHA |
| `name` | Non-empty string |
| `type` | `"blob"` or `"tree"` |
| `path` | Non-empty string |
| `mode` | Non-empty string |
| List envelope shape | `{items: [...], page, per_page, total?, total_pages?, next_page?}` — invariants apply to each entry in `items` |

**Repository files (GET):**

| Property | What to verify |
|---|---|
| `file_name` | Filename without directory path |
| `file_path` | Full path within repository |
| `size` | Non-negative integer |
| `encoding` | Usually `"base64"` |
| `content` | Non-empty Base64 string |
| `content_sha256` | 64-character hex string |
| `ref` | Matches the requested ref |
| `blob_id` | Non-empty SHA |
| `commit_id` | Non-empty SHA |
| `last_commit_id` | Non-empty SHA |

**MR discussions:**

| Property | What to verify |
|---|---|
| `id` | Non-empty hex string (discussion ID) |
| `individual_note` | `true` or `false`, never `null` |
| `notes` | Non-empty array |
| `notes[].id` | Positive integer (note ID) |
| `notes[].body` | Non-empty string |
| `notes[].author` | Collapsed to `{id, username, name}` in list responses |
| `notes[].resolvable` | `true` or `false` |
| List envelope shape | Standard pagination envelope; invariants apply to each entry in `items` |

**Issue notes:**

| Property | What to verify |
|---|---|
| `id` | Positive integer (note ID) |
| `body` | Non-empty string |
| `author` | Collapsed to `{id, username, name}` in list responses |
| `created_at` | Non-null ISO 8601 datetime |
| `updated_at` | Non-null ISO 8601 datetime |
| `noteable_type` | `"Issue"` |
| List envelope shape | Standard pagination envelope; invariants apply to each entry in `items` |

**Work items (list and get):**

| Property | What to verify |
|---|---|
| `id` | Non-empty global ID string (`gid://gitlab/WorkItem/NNN`) |
| `iid` | Non-empty string representing the project-relative integer |
| `title` | Non-empty string |
| `state` | `"OPEN"` or `"CLOSED"` — uppercase, unlike the REST issues `opened`/`closed` |
| `createdAt` | Non-null ISO 8601 datetime (camelCase, not `created_at`) |
| `updatedAt` | Non-null ISO 8601 datetime |
| `webUrl` | Non-empty URL |
| `workItemType.name` | Non-empty string (e.g. `"Task"`, `"Issue"`, `"Ticket"`) |
| `widgets` | Array of objects each with a `type` field (e.g. `"DESCRIPTION"`, `"ASSIGNEES"`) |
| List envelope shape | `{ items: [...], has_next_page: bool, end_cursor: string\|null }` — no page/total fields |
| Delete confirmation | Success text message, not a JSON object |
| Get-only fields | `author`, `closedAt`, `namespace.fullPath` only present on single-get responses |

> **Key differences from REST:** work item tools require `project_path` (full path string, e.g. `"3kirt1/gitlab-mcp-testing"`) rather than `project_id`. Numeric project IDs are not accepted by the GraphQL API. Get, update, and delete operations require the global ID (`gid://gitlab/WorkItem/NNN`) returned by list and create; the project-relative `iid` is not sufficient.

---

## Seed Data

Perform all seed steps in order. Later steps depend on earlier ones.

### Step 1: Create Test Files on `main`

**1a.** Create the base sample file:
```
gitlab_file_create(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  branch="main",
  commit_message="Add sample.txt for testing",
  content="line one\nline two\nline three"
)
```
Returns `{"file_path": "testing/sample.txt", "branch": "main"}`. Record the commit id (used in Section 25 and 26).

**1b.** Add a second commit to build blame history:
```
gitlab_file_update(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  branch="main",
  commit_message="Add fourth line to sample.txt",
  content="line one\nline two\nline three\nline four"
)
```
`testing/sample.txt` now has 4 lines and 2 commits.

### Step 2: Create Branches

Create all branches from `main`:

| Branch | Purpose |
|---|---|
| `mr-test-open` | Source branch for an open MR |
| `mr-test-draft` | Source branch for a draft MR |
| `mr-test-close` | Source branch for an MR to be closed |
| `mr-test-merge` | Source branch for an MR to be merged (Section 17) |
| `mr-test-scratch` | Reusable scratch branch for Sections 14 and 16 |
| `branch-test-1` | Deleted in Section 10 |

```
gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="<name>", ref="main")
```

### Step 3: Advance `mr-test-merge`

```
gitlab_file_create(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/feature.txt",
  branch="mr-test-merge",
  commit_message="Add feature file",
  content="This file was added in the feature branch."
)
```
`mr-test-merge` is now one commit ahead of `main`.

### Step 4: Create Issues

| # | Title | Labels | Due Date | Final state |
|---|---|---|---|---|
| seed-1 | `Bug: login page crashes on submit` | `bug` | — | opened |
| seed-2 | `Feature: add dark mode support` | `enhancement` | `2026-12-31` | opened |
| seed-3 | `Fix: memory leak in issues API` | `bug,performance` | — | **closed** |
| seed-4 | `Docs: update README with auth instructions` | `documentation` | — | opened |
| seed-5 | `Chore: bump Rust dependencies` | — | — | **closed** |

After seeding: 5 issues total; 3 opened; 2 closed. Record each `iid`.

> Issue `#1` ("Test issue") was created before seed data. Adjust expected counts accordingly.

### Step 5: Create Merge Requests

| # | Title | Source branch | Labels | Draft | Final state |
|---|---|---|---|---|---|
| mr-seed-1 | `Fix: correct off-by-one error` | `mr-test-open` | `bug` | false | opened |
| mr-seed-2 | `Draft: refactor auth module` | `mr-test-draft` | `enhancement` | true | opened (draft) |
| mr-seed-3 | `Chore: update CI config` | `mr-test-close` | — | false | **closed** |
| mr-seed-4 | `Feature: add health check endpoint` | `mr-test-merge` | `enhancement` | false | opened (merged in Section 17) |

After seeding: 4 MRs; 3 opened; 1 closed. Record each `iid`.

### Step 6: Seed an Issue Note

Create a note on seed-1 to serve as a stable seed for Sections 38–42:
```
gitlab_issues_notes_create(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<iid of seed-1>,
  body="Seeded note for issue notes testing."
)
```
Record `id` as `note-issue-seed-1`.

### Step 7: Seed an MR Discussion

Create a plain (non-diff) discussion on mr-seed-1 to serve as a stable seed for Sections 31–36:
```
gitlab_mrs_discussions_create(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-1>,
  body="Seeded review comment for discussion testing."
)
```
Record `id` as `disc-seed-1` and `notes[0].id` as `note-seed-1`.

### Step 8: Create Work Items

**8a.** Create the parent task:
```
gitlab_work_item_create(
  project_path="3kirt1/gitlab-mcp-testing",
  work_item_type="TASK",
  title="Implement login feature",
  description="This is the parent task for hierarchy testing."
)
```
Record `id` (global ID) as `wi-task-1-gid` and `iid` as `wi-task-1-iid`.

**8b.** Create a child task referencing the parent:
```
gitlab_work_item_create(
  project_path="3kirt1/gitlab-mcp-testing",
  work_item_type="TASK",
  title="Write unit tests for login",
  parent_id=<wi-task-1-gid>
)
```
Record `id` as `wi-task-2-gid`.

**8c.** Create a work item of type ISSUE:
```
gitlab_work_item_create(
  project_path="3kirt1/gitlab-mcp-testing",
  work_item_type="ISSUE",
  title="Bug: login page crashes on empty password",
  description="Steps to reproduce: submit login form with empty password field."
)
```
Record `id` as `wi-issue-1-gid`.

After seeding: 3 work items; all `OPEN`; `wi-task-1` has one child (`wi-task-2`).

---

## Section 1: Issues — List

### 1.1 List all open issues (default)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing")
```
Returns an array; GitLab defaults to opened state when no `state` param is sent.

### 1.2 List all issues regardless of state
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all")
```
Returns all 6 issues (5 seed + issue #1). Mix of opened and closed. Repeat with `project_id="82279422"` to smoke-test numeric project ID.

### 1.3 List closed issues only
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="closed")
```
Returns exactly 2 issues: seed-3 and seed-5. All have `state == "closed"`.

### 1.4 Filter by single label
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")
```
Returns seed-1 and seed-3.

### 1.5 Filter by multiple labels (AND logic)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug,performance")
```
Returns only seed-3.

### 1.6 Search by keyword
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="memory")
```
Returns seed-3.

### 1.7 Sort order
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", order_by="created_at", sort="asc")
```
First result is issue #1 (earliest). Each subsequent issue has `created_at >= previous`.

### 1.8 Date range filters
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", created_after="2026-01-01T00:00:00Z")
```
Returns all seed issues (created after Jan 1 2026). Count ≥ 5.

```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", created_after="2030-01-01T00:00:00Z")
```
Returns `[]` (no issues created that far in the future).

```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", created_after="2026-01-01T00:00:00Z", created_before="2030-01-01T00:00:00Z")
```
Same result as the first call above; both bounds applied.

---

## Section 2: Issues — Get

### 2.1 Get an open issue with labels
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>)
```
`title == "Bug: login page crashes on submit"`, `state == "opened"`, `labels` contains `"bug"`.

### 2.2 Get a closed issue
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-3>)
```
`state == "closed"`, `closed_at` non-null.

### 2.3 Get issue with due date
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-2>)
```
`due_date == "2026-12-31"`.

---

## Section 3: Issues — Create

### 3.1 Create with title only
```
gitlab_issues_create(project_id="3kirt1/gitlab-mcp-testing", title="Minimal issue")
```
`state == "opened"`, `description` null or absent. Record `iid`.

### 3.2 Create with all optional fields
```
gitlab_issues_create(
  project_id="3kirt1/gitlab-mcp-testing",
  title="Full issue creation test",
  description="This tests all optional fields.",
  labels="test,automation",
  due_date="2026-06-30"
)
```
All provided fields reflected in response. Record `iid`.

### 3.3 Create with Markdown description
```
gitlab_issues_create(
  project_id="3kirt1/gitlab-mcp-testing",
  title="Markdown description test",
  description="## Summary\n\n- item one\n- item two\n\n**Bold** and _italic_."
)
```
`description` contains the raw Markdown without escaping. Record `iid`.

---

## Section 4: Issues — Update

Operate on a scratch issue from Section 3.

### 4.1 Update title
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, title="Updated title")
```
Returned `title == "Updated title"`.

### 4.2 Update description
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, description="New description.")
```
Returned `description == "New description."`.

### 4.3 Replace labels
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, labels="bug,needs-review")
```
Returned `labels` contains `"bug"` and `"needs-review"`.

### 4.4 Close via state_event
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, state_event="close")
```
Returned `state == "closed"`, `closed_at` non-null.

### 4.5 Reopen via state_event
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, state_event="reopen")
```
Returned `state == "opened"`, `closed_at` null or absent.

### 4.6 Set due date
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, due_date="2027-01-01")
```
Returned `due_date == "2027-01-01"`.

---

## Section 5: Issues — Delete

Create a throwaway issue, record its `iid`, then:
```
gitlab_issues_delete(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<throwaway iid>)
```
Returns a success text message. A subsequent `gitlab_issues_get` with the same `iid` returns a `404` error.

---

## Section 6: Pagination

Tested on issues; the same logic applies to all list endpoints.

### 6.1 First page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=1)
```
`items` contains exactly 2 issues. Envelope reports `page == 1`, `per_page == 2`, and `next_page == 2`.

### 6.2 Second page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=2)
```
`items` contains the next 2 issues; no overlap with page 1. Envelope reports `page == 2`.

### 6.3 Beyond last page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=99)
```
`items == []`; no error. Envelope omits `next_page` (no further pages).

---

## Section 7: Branches — List

### 7.1 List all branches
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing")
```
Returns at least 7 branches (main + 6 seeded). Each satisfies branch universal invariants.

### 7.2 Filter by search string
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", search="mr-test")
```
Returns the 5 `mr-test-*` branches only.

### 7.3 Filter by regex
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", regex="^mr-test-(open|draft)$")
```
Returns exactly `mr-test-open` and `mr-test-draft`.

---

## Section 8: Branches — Get

### 8.1 Get the default (protected) branch
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="main")
```
`name == "main"`, `protected == true`, `default == true`, `commit.id` non-empty.

### 8.2 Get an unprotected branch
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-open")
```
`name == "mr-test-open"`, `merged == false`, `protected == false`.

---

## Section 9: Branches — Create

### 9.1 Create from main
```
gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-scratch", ref="main")
```
`name == "branch-test-scratch"`, `commit.id` matches HEAD of `main`. Delete after testing.

### 9.2 Create from another branch
```
gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-from-seed", ref="mr-test-open")
```
`commit.id` matches HEAD of `mr-test-open`. Delete after testing.

---

## Section 10: Branches — Delete

### 10.1 Delete a branch
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-1")
```
Returns a success text message. A subsequent `gitlab_branches_get` returns a `404` error.

Delete scratch branches from Section 9 (`branch-test-scratch`, `branch-test-from-seed`).

---

## Section 11: Branches — Delete Merged

> Run this section **after Section 17** (MR merge).

Confirm `mr-test-merge` has `merged == true`, then:
```
gitlab_branches_delete_merged(project_id="3kirt1/gitlab-mcp-testing")
```
Returns a success text message. A subsequent `gitlab_branches_get` on `mr-test-merge` returns `404`. A subsequent `gitlab_branches_get` on `mr-test-open` succeeds (unmerged branches are untouched).

---

## Section 12: Merge Requests — List

### 12.1 List open MRs (default)
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing")
```
All results have `state == "opened"`.

### 12.2 List all MRs
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all")
```
Returns all 4 seeded MRs; mix of states.

### 12.3 List closed MRs
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="closed")
```
Returns exactly mr-seed-3; all have `state == "closed"`.

### 12.4 Filter by source branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", source_branch="mr-test-open")
```
Returns exactly mr-seed-1.

### 12.5 Filter by target branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", target_branch="main")
```
Returns all 4 seeded MRs.

### 12.6 Filter by label
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")
```
Returns mr-seed-1 only.

### 12.7 Filter draft MRs
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", draft=true)
```
Returns mr-seed-2 only; `draft == true`.

### 12.8 Search by keyword
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="health")
```
Returns mr-seed-4.

### 12.9 Date range filters
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", created_after="2026-01-01T00:00:00Z")
```
Returns all seed MRs (created after Jan 1 2026). Count ≥ 4.

```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", created_after="2030-01-01T00:00:00Z")
```
Returns `[]` (no MRs created that far in the future).

---

## Section 13: Merge Requests — Get

### 13.1 Get an open MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-1>)
```
`title == "Fix: correct off-by-one error"`, `state == "opened"`, `source_branch == "mr-test-open"`, `draft == false`.

### 13.2 Get a draft MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-2>)
```
`draft == true`.

### 13.3 Get a closed MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-3>)
```
`state == "closed"`.

---

## Section 14: Merge Requests — Create

### 14.1 Create with required fields only
```
gitlab_mrs_create(
  project_id="3kirt1/gitlab-mcp-testing",
  source_branch="mr-test-scratch",
  target_branch="main",
  title="Scratch MR for testing"
)
```
`state == "opened"`, `draft == false`. Record `iid`.

### 14.2 Create with all optional fields
```
gitlab_mrs_create(
  project_id="3kirt1/gitlab-mcp-testing",
  source_branch="mr-test-scratch",
  target_branch="main",
  title="Full MR creation test",
  description="Tests all optional fields.",
  labels="test,automation",
  squash=true,
  draft=true
)
```
`squash == true`, `draft == true`; labels and description reflected. Record `iid`.

### 14.3 Create with Markdown description
```
gitlab_mrs_create(
  project_id="3kirt1/gitlab-mcp-testing",
  source_branch="mr-test-scratch",
  target_branch="main",
  title="Markdown description MR",
  description="## Summary\n\n- item one\n- item two\n\n**Bold** and _italic_."
)
```
`description` contains the raw Markdown. Record `iid`.

---

## Section 15: Merge Requests — Update

Operate on a scratch MR from Section 14.

### 15.1 Update title
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, title="Updated MR title")
```
Returned `title == "Updated MR title"`.

### 15.2 Update description
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, description="New description.")
```
Returned `description == "New description."`.

### 15.3 Replace labels
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, labels="bug,needs-review")
```
Returned `labels` contains `"bug"` and `"needs-review"`.

### 15.4 Toggle draft
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, draft=true)
```
Returned `draft == true`. Then:
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, draft=false)
```
Returned `draft == false`.

### 15.5 Close and reopen via state_event
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, state_event="close")
```
Returned `state == "closed"`. Then:
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, state_event="reopen")
```
Returned `state == "opened"`.

---

## Section 16: Merge Requests — Delete

Create a throwaway MR (source: `mr-test-scratch`), record its `iid`, then:
```
gitlab_mrs_delete(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<throwaway iid>)
```
Returns a success text message. A subsequent `gitlab_mrs_get` returns a `404` error.

---

## Section 17: Merge Requests — Merge

### 17.1 Merge an open MR
```
gitlab_mrs_merge(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-4>)
```
Returned `state == "merged"`, `merged_at` non-null, `merge_commit_sha` present.

> After Section 17, proceed to Section 11 (Branches — Delete Merged).

---

## Section 18: Repository — Tree

### 18.1 List the root of the default branch
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main")
```
Returns an array. At least one entry has `name == "testing"` and `type == "tree"`. Each entry satisfies repository tree universal invariants.

### 18.2 List a subdirectory
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", path="testing")
```
At least one entry: `name == "sample.txt"`, `type == "blob"`, `path == "testing/sample.txt"`. Record the `id` (blob SHA) for Section 19.

### 18.3 List recursively
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", recursive=true)
```
Returns a flat array; `testing/sample.txt` appears directly.

### 18.4 List from a feature branch
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="mr-test-merge", path="testing")
```
Returns both `sample.txt` and `feature.txt`.

---

## Section 19: Repository — Blob Get and Raw

Use the blob SHA from 18.2.

### 19.1 Get blob metadata
```
gitlab_repo_blob_get(project_id="3kirt1/gitlab-mcp-testing", sha=<blob SHA of sample.txt>)
```
`encoding == "base64"`, `size` positive. Decoding `content` produces `"line one\nline two\nline three\nline four"`.

### 19.2 Get raw blob content
```
gitlab_repo_blob_raw(project_id="3kirt1/gitlab-mcp-testing", sha=<blob SHA of sample.txt>)
```
Returns `{"content": "line one\nline two\nline three\nline four"}`.

---

## Section 20: Repository — Compare

### 20.1 Non-empty diff
```
gitlab_repo_compare(project_id="3kirt1/gitlab-mcp-testing", from="main", to="mr-test-merge")
```
`commits` non-empty (includes "Add feature file" commit), `diffs` contains `testing/feature.txt`, `web_url` present.

### 20.2 Empty diff (identical refs)
```
gitlab_repo_compare(project_id="3kirt1/gitlab-mcp-testing", from="main", to="mr-test-open")
```
`commits` is empty, `diffs` is empty.

### 20.3 Straight and unidiff options
```
gitlab_repo_compare(project_id="3kirt1/gitlab-mcp-testing", from="main", to="mr-test-merge", straight=true)
gitlab_repo_compare(project_id="3kirt1/gitlab-mcp-testing", from="main", to="mr-test-merge", unidiff=true)
```
Both return the same structure as 20.1 without errors.

---

## Section 21: Repository — Contributors

### 21.1 List contributors
```
gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing")
```
Returns at least 1 contributor. Each entry has `name`, `email`, `commits` (positive), `additions`, `deletions`.

### 21.2 Order by commits descending
```
gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing", order_by="commits", sort="desc")
```
First contributor has the highest `commits` count.

### 21.3 Scope to a ref
```
gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing", ref_name="main")
```
Returns contributors whose commits exist on `main`.

---

## Section 22: Repository — Merge Base

### 22.1 Common ancestor of two branches
```
gitlab_repo_merge_base(project_id="3kirt1/gitlab-mcp-testing", refs=["main", "mr-test-merge"])
```
Returns a commit object with `id`, `short_id`, `title`, `author_name`, `committed_date`.

### 22.2 Common ancestor of three refs
```
gitlab_repo_merge_base(project_id="3kirt1/gitlab-mcp-testing", refs=["main", "mr-test-open", "mr-test-draft"])
```
Returns a commit object. Since all branches were created from the same `main` commit, the merge base is that commit.

---

## Section 23: Repository — Changelog

> Seed commits were created via API without a `Changelog` trailer, so `notes` will be empty. This is expected.

### 23.1 Generate changelog markdown (read-only)
```
gitlab_repo_changelog_get(project_id="3kirt1/gitlab-mcp-testing", version="0.1.0")
```
Returns `{"notes": "..."}` where `notes` is a string (may be empty). No error.

### 23.2 Commit changelog to a scratch branch (write)
```
gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="changelog-test-scratch", ref="main")

gitlab_repo_changelog_add(
  project_id="3kirt1/gitlab-mcp-testing",
  version="0.1.0",
  branch="changelog-test-scratch",
  file="CHANGELOG-test.md"
)
```
No error. Verify the file exists with `gitlab_file_get(..., file_path="CHANGELOG-test.md", ref_name="changelog-test-scratch")`. Clean up:
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="changelog-test-scratch")
```

---

## Section 24: Repository — Health

> May return `403 Forbidden` for project-scoped tokens. Both outcomes are acceptable.

```
gitlab_repo_health(project_id="3kirt1/gitlab-mcp-testing")
gitlab_repo_health(project_id="3kirt1/gitlab-mcp-testing", generate=true)
```
Either returns health statistics or surfaces a `403`; no crash in either case.

---

## Section 25: Repository Files — Get

### 25.1 Get a seeded file
```
gitlab_file_get(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")
```
All file universal invariants satisfied. `file_name == "sample.txt"`, `file_path == "testing/sample.txt"`, `ref == "main"`, `encoding == "base64"`. Decoding `content` produces `"line one\nline two\nline three\nline four"`. Record `blob_id` for Section 19 if not already done.

### 25.2 Get from a feature branch
```
gitlab_file_get(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/feature.txt", ref_name="mr-test-merge")
```
Content decodes to `"This file was added in the feature branch."`.

### 25.3 Get at a specific commit SHA
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name=<commit_id from seed Step 1a>
)
```
Content decodes to `"line one\nline two\nline three"` (3-line version).

---

## Section 26: Repository Files — Raw

### 26.1 Get raw content
```
gitlab_file_raw(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")
```
Returns `{"content": "line one\nline two\nline three\nline four"}`. Content is plain text, not Base64.

### 26.2 Get at an earlier commit
```
gitlab_file_raw(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name=<commit_id from seed Step 1a>
)
```
`content == "line one\nline two\nline three"`.

### 26.3 Get from a feature branch
```
gitlab_file_raw(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/feature.txt", ref_name="mr-test-merge")
```
`content == "This file was added in the feature branch."`.

---

## Section 27: Repository Files — Blame

### 27.1 Full blame history
```
gitlab_file_blame(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")
```
Returns an array of exactly 2 blame entries. Each has a `commit` object (`id`, `author_name`, `committed_date`) and a `lines` array. First entry covers lines 1–3; second covers line 4 with a more-recent `commit.id`.

### 27.2 Line range
```
gitlab_file_blame(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name="main",
  range_start=4,
  range_end=4
)
```
Returns exactly 1 entry. `lines` contains only `"line four"`. `commit.id` matches the seed Step 1b commit.

---

## Section 28: Repository Files — Create

### 28.1 Create a new file
```
gitlab_file_create(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  branch="main",
  commit_message="Add scratch file for testing",
  content="Hello from the test suite."
)
```
Returns `{"file_path": "testing/scratch.txt", "branch": "main"}`.

### 28.2 Create with Base64 encoding
```
gitlab_file_create(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch-b64.txt",
  branch="main",
  commit_message="Add base64-encoded scratch file",
  content="SGVsbG8gZnJvbSBCYXNlNjQu",
  encoding="base64"
)
```
`gitlab_file_raw(..., file_path="testing/scratch-b64.txt")` returns `content == "Hello from Base64."`.

---

## Section 29: Repository Files — Update

Operate on `testing/scratch.txt` from Section 28.

### 29.1 Update content
```
gitlab_file_update(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  branch="main",
  commit_message="Update scratch file",
  content="Updated content."
)
```
Returns `{"file_path": "testing/scratch.txt", "branch": "main"}`. `gitlab_file_raw(...)` returns `content == "Updated content."`.

### 29.2 Update with last_commit_id guard
Get the file, record `last_commit_id`, then:
```
gitlab_file_update(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  branch="main",
  commit_message="Update with commit guard",
  content="Guarded update.",
  last_commit_id=<recorded>
)
```
Succeeds.

---

## Section 30: Repository Files — Delete

### 30.1 Delete a file
```
gitlab_file_delete(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  branch="main",
  commit_message="Remove scratch file"
)
```
Returns success. A subsequent `gitlab_file_get` for the same path returns a `404` error.

### 30.2 Delete the Base64 scratch file
```
gitlab_file_delete(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch-b64.txt",
  branch="main",
  commit_message="Remove base64 scratch file"
)
```
Returns success.

---

## Section 31: MR Discussions — List

### 31.1 List all discussions
```
gitlab_mrs_discussions_list(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-1>)
```
Returns an envelope with at least 1 item (the seeded disc-seed-1). Each item satisfies MR discussion universal invariants.

### 31.2 Paginate discussions
```
gitlab_mrs_discussions_list(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-1>, per_page=1, page=1)
```
`items` contains exactly 1 discussion. Envelope reports `page == 1`, `per_page == 1`.

---

## Section 32: MR Discussions — Get

### 32.1 Get the seeded discussion
```
gitlab_mrs_discussions_get(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-1>,
  discussion_id=<disc-seed-1>
)
```
`id == disc-seed-1`, `notes[0].body == "Seeded review comment for discussion testing."`, `notes[0].id == note-seed-1`.

---

## Section 33: MR Discussions — Create

### 33.1 Create a top-level comment
```
gitlab_mrs_discussions_create(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-1>,
  body="Top-level comment for create testing."
)
```
`notes[0].body == "Top-level comment for create testing."`, `notes[0].resolvable == false`. Record `id` as `disc-create-1` for use in Sections 35–37.

### 33.2 Create a diff note with position params

First get the diff refs from the MR that has actual code changes:
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-4>)
```
Record `diff_refs.base_sha`, `diff_refs.head_sha`, `diff_refs.start_sha`.

Then create a diff note on the added file:
```
gitlab_mrs_discussions_create(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-4>,
  body="Inline comment on feature.txt.",
  position_base_sha=<base_sha>,
  position_head_sha=<head_sha>,
  position_start_sha=<start_sha>,
  position_new_path="testing/feature.txt",
  position_new_line=1
)
```
`notes[0].resolvable == true`. `position_type` defaults to `"text"` in the assembled body. Record `id` as `disc-diff-1`.

---

## Section 34: MR Discussions — Resolve

Uses the resolvable diff discussion `disc-diff-1` created in Section 33.2.

### 34.1 Resolve a thread
```
gitlab_mrs_discussions_resolve(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-4>,
  discussion_id=<disc-diff-1>,
  resolved=true
)
```
Returned `notes[0].resolved == true`.

### 34.2 Unresolve a thread
```
gitlab_mrs_discussions_resolve(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-4>,
  discussion_id=<disc-diff-1>,
  resolved=false
)
```
Returned `notes[0].resolved == false`.

> Non-diff (plain) discussions have `notes[0].resolvable == false`; attempting to resolve them returns a `400` error from GitLab. Only diff notes are resolvable.

---

## Section 35: MR Discussions — Note Create

Operates on `disc-create-1` from Section 33.1.

### 35.1 Add a reply note
```
gitlab_mrs_discussions_note_create(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-1>,
  discussion_id=<disc-create-1>,
  body="Reply note for testing."
)
```
Returns a note object with `body == "Reply note for testing."`, `id` is a positive integer. Record this `id` as `note-reply-1`.

The parent discussion now has 2 notes: a `gitlab_mrs_discussions_get` call returns `notes` with 2 entries.

---

## Section 36: MR Discussions — Note Update

Operates on `note-reply-1` from Section 35.

### 36.1 Update note body
```
gitlab_mrs_discussions_note_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-1>,
  discussion_id=<disc-create-1>,
  note_id=<note-reply-1>,
  body="Updated reply text."
)
```
Returned `body == "Updated reply text."`.

---

## Section 37: MR Discussions — Note Delete

### 37.1 Delete the reply note
```
gitlab_mrs_discussions_note_delete(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-1>,
  discussion_id=<disc-create-1>,
  note_id=<note-reply-1>
)
```
Returns a success text message. A subsequent `gitlab_mrs_discussions_get` on `disc-create-1` returns only 1 note (the original).

---

## Cross-Tool Workflows

### Workflow A: Issue lifecycle (create → update → delete)
1. `gitlab_issues_create(project_id="3kirt1/gitlab-mcp-testing", title="Workflow A scratch")` — record `iid`
2. `gitlab_issues_update(..., title="Updated", labels="test")`
3. `gitlab_issues_update(..., state_event="close")` — verify `state == "closed"`
4. `gitlab_issues_delete(...)` — verify success message
5. `gitlab_issues_get(...)` — verify `404` error

### Workflow B: Branch and MR lifecycle (create → close → delete)
1. `gitlab_branches_create(..., branch="workflow-b", ref="main")`
2. `gitlab_mrs_create(..., source_branch="workflow-b", target_branch="main", title="Workflow B MR")` — record `iid`
3. `gitlab_mrs_list(..., source_branch="workflow-b")` — confirm MR appears
4. `gitlab_mrs_update(..., state_event="close")` — confirm `state == "closed"`
5. `gitlab_mrs_delete(...)` — verify success message; confirm `404` on get
6. `gitlab_branches_delete(..., branch="workflow-b")` — confirm `404` on get

### Workflow C: Tree → blob → file (SHA consistency)
1. `gitlab_repo_tree(..., ref="main", path="testing")` — record `id` (blob SHA) of `sample.txt`
2. `gitlab_repo_blob_get(..., sha=<blob SHA>)` — confirm `encoding == "base64"`, decode content
3. `gitlab_repo_blob_raw(..., sha=<blob SHA>)` — confirm plain text content
4. `gitlab_file_get(..., file_path="testing/sample.txt", ref_name="main")` — confirm `blob_id` matches the SHA from step 1

### Workflow D: Create file on branch → compare → blame → delete
1. `gitlab_branches_create(..., branch="workflow-d", ref="main")`
2. `gitlab_file_create(..., file_path="testing/wd.txt", branch="workflow-d", content="workflow d content")`
3. `gitlab_repo_compare(..., from="main", to="workflow-d")` — confirm `diffs` contains `testing/wd.txt`
4. `gitlab_file_raw(..., file_path="testing/wd.txt", ref_name="workflow-d")` — confirm `content == "workflow d content"`
5. `gitlab_file_blame(..., file_path="testing/wd.txt", ref_name="workflow-d")` — confirm 1 blame entry
6. `gitlab_file_delete(..., file_path="testing/wd.txt", branch="workflow-d")`; confirm `404` on get
7. `gitlab_branches_delete(..., branch="workflow-d")`

### Workflow E: Compare → merge base → contributors
1. `gitlab_repo_compare(..., from="main", to="mr-test-merge")` — record first commit `id` in `commits`
2. `gitlab_repo_merge_base(..., refs=["main", "mr-test-merge"])` — record merge base `id`
3. Verify: merge base `id` differs from the commit on `mr-test-merge` (it has commits ahead of the base)
4. `gitlab_repo_contributors(..., order_by="commits", sort="desc")` — top contributor has ≥ 3 commits (seed Steps 1a, 1b, 3)

### Workflow G: Issue note lifecycle (create → get → update → delete)
1. `gitlab_issues_notes_create(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-2>, body="Workflow G note.")` — record `id` as `note-wg`
2. `gitlab_issues_notes_get(..., issue_iid=<iid of seed-2>, note_id=<note-wg>)` — confirm `body == "Workflow G note."`
3. `gitlab_issues_notes_update(..., note_id=<note-wg>, body="Updated workflow G note.")` — confirm `body == "Updated workflow G note."`
4. `gitlab_issues_notes_list(..., issue_iid=<iid of seed-2>)` — confirm updated note appears in `items`
5. `gitlab_issues_notes_delete(..., note_id=<note-wg>)` — confirm success message
6. `gitlab_issues_notes_get(..., note_id=<note-wg>)` — confirm `404` error

### Workflow F: Discussion lifecycle (create → reply → resolve → delete)
1. `gitlab_mrs_discussions_list(..., merge_request_iid=<iid of mr-seed-1>)` — confirms seeded disc-seed-1 is present
2. `gitlab_mrs_discussions_create(..., merge_request_iid=<iid of mr-seed-1>, body="Workflow F thread.")` — record `id` as `disc-wf`
3. `gitlab_mrs_discussions_note_create(..., discussion_id=<disc-wf>, body="Reply to workflow F.")` — record returned `id` as `note-wf`
4. `gitlab_mrs_discussions_get(..., discussion_id=<disc-wf>)` — confirm `notes` has 2 entries
5. `gitlab_mrs_discussions_note_update(..., note_id=<note-wf>, body="Edited reply.")` — confirm `body == "Edited reply."`
6. `gitlab_mrs_discussions_note_delete(..., note_id=<note-wf>)` — confirm success message
7. `gitlab_mrs_discussions_get(..., discussion_id=<disc-wf>)` — confirm `notes` has 1 entry (original thread-starter only)

---

## Section 38: Issue Notes — List

### 38.1 List notes on an issue with a seeded note
```
gitlab_issues_notes_list(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>)
```
Returns an envelope with at least 1 item (note-issue-seed-1). Each item satisfies issue note universal invariants.

### 38.2 Sort ascending by created_at
```
gitlab_issues_notes_list(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>, order_by="created_at", sort="asc")
```
First note has the earliest `created_at`.

### 38.3 Paginate
```
gitlab_issues_notes_list(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>, per_page=1, page=1)
```
`items` contains exactly 1 note. Envelope reports `page == 1`, `per_page == 1`.

### 38.4 List on a non-existent issue
```
gitlab_issues_notes_list(project_id="3kirt1/gitlab-mcp-testing", issue_iid=99999)
```
Returns a `404` error.

---

## Section 39: Issue Notes — Get

### 39.1 Get the seeded note
```
gitlab_issues_notes_get(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<iid of seed-1>,
  note_id=<note-issue-seed-1>
)
```
`body == "Seeded note for issue notes testing."`, `noteable_type == "Issue"`, `id == note-issue-seed-1`.

### 39.2 Get a non-existent note
```
gitlab_issues_notes_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>, note_id=99999)
```
Returns a `404` error.

---

## Section 40: Issue Notes — Create

### 40.1 Create a note with body only
```
gitlab_issues_notes_create(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<iid of seed-2>,
  body="Test note for create testing."
)
```
Returned `body == "Test note for create testing."`, `id` is a positive integer, `noteable_type == "Issue"`. Record `id` as `note-create-1`.

### 40.2 Create a Markdown note
```
gitlab_issues_notes_create(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<iid of seed-2>,
  body="## Findings\n\n- item one\n- item two"
)
```
`body` contains the raw Markdown without escaping.

---

## Section 41: Issue Notes — Update

Operates on `note-create-1` from Section 40.

### 41.1 Update note body
```
gitlab_issues_notes_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<iid of seed-2>,
  note_id=<note-create-1>,
  body="Updated note body."
)
```
Returned `body == "Updated note body."`, `updated_at` is later than `created_at`.

### 41.2 Update a deleted note
Delete the note first, then attempt to update:
```
gitlab_issues_notes_update(..., note_id=<note-create-1>, body="Should fail.")
```
Returns a `404` error.

---

## Section 42: Issue Notes — Delete

Create a throwaway note, record its `id`, then:
```
gitlab_issues_notes_delete(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<iid of seed-1>,
  note_id=<throwaway note id>
)
```
Returns a success text message. A subsequent `gitlab_issues_notes_get` with the same `note_id` returns a `404` error.

---

## Section 43: Work Items — List

### 43.1 List all work items (no filter)
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing")
```
Returns at least 3 items (the 3 seeded in Step 8). Each satisfies work item universal invariants. Envelope has `has_next_page` and `end_cursor` fields.

### 43.2 Filter by single type
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", types=["TASK"])
```
Returns `wi-task-1` and `wi-task-2` only. Each item's `workItemType.name == "Task"`.

### 43.3 Filter by multiple types
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", types=["TASK", "ISSUE"])
```
Returns all 3 seeded work items.

### 43.4 Filter by type — returns only matching type
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", types=["ISSUE"])
```
Returns only `wi-issue-1`. `workItemType.name == "Issue"`.

### 43.5 Filter by state
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", state="opened")
```
Returns all 3 seeded work items (all `OPEN`). Then:
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", state="closed")
```
Returns `[]` (none closed yet).

### 43.6 Search by keyword
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", search="login")
```
Returns `wi-task-1` and `wi-issue-1` (both contain "login" in their titles). Does not return `wi-task-2`.

### 43.7 Filter by IID
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", iids=[<wi-task-1-iid>])
```
Returns exactly 1 item matching `wi-task-1`.

### 43.8 Cursor pagination
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", first=1)
```
`items` contains exactly 1 work item. `has_next_page == true`, `end_cursor` is a non-null string. Then:
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", first=1, after=<end_cursor>)
```
Returns the second work item; no overlap with the first page. On the final page, `has_next_page == false` and `end_cursor == null`.

### 43.9 Hierarchy widget in list response
```
gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", iids=[<wi-task-2-iid>])
```
The result for `wi-task-2` has a widget with `type == "HIERARCHY"` where `parent.id == <wi-task-1-gid>` and `parent.title == "Implement login feature"`.

---

## Section 44: Work Item — Get

### 44.1 Get a task by global ID
```
gitlab_work_item_get(id=<wi-task-1-gid>)
```
`id == <wi-task-1-gid>`, `title == "Implement login feature"`, `state == "OPEN"`, `workItemType.name == "Task"`. The response includes `author`, `namespace.fullPath`, and a `widgets` array. A widget with `type == "HIERARCHY"` is present with `hasChildren == true` and at least one entry in `children.nodes`.

### 44.2 Get an issue-type work item
```
gitlab_work_item_get(id=<wi-issue-1-gid>)
```
`workItemType.name == "Issue"`. A widget with `type == "DESCRIPTION"` contains `description == "Steps to reproduce: submit login form with empty password field."`.

### 44.3 Get a non-existent work item
```
gitlab_work_item_get(id="gid://gitlab/WorkItem/999999999")
```
Returns a GraphQL error (not a JSON work item object).

---

## Section 45: Work Item — Create

### 45.1 Create with required fields only
```
gitlab_work_item_create(
  project_path="3kirt1/gitlab-mcp-testing",
  work_item_type="TASK",
  title="Minimal task"
)
```
Returned `title == "Minimal task"`, `state == "OPEN"`, `workItemType.name == "Task"`, `id` is a non-empty global ID string. Record `id` as `wi-scratch-gid`.

### 45.2 Create with description and assignee
```
gitlab_work_item_create(
  project_path="3kirt1/gitlab-mcp-testing",
  work_item_type="TASK",
  title="Task with description",
  description="## Details\n\nFull description here.",
  assignee_usernames=["3kirt1"]
)
```
`title == "Task with description"`. Confirm via `gitlab_work_item_get`: a widget with `type == "DESCRIPTION"` has `description` containing `"## Details"`. A widget with `type == "ASSIGNEES"` has a non-empty `assignees.nodes`. Record `id` as `wi-desc-gid`.

### 45.3 Create with start and due dates
```
gitlab_work_item_create(
  project_path="3kirt1/gitlab-mcp-testing",
  work_item_type="TASK",
  title="Task with dates",
  start_date="2026-06-01",
  due_date="2026-06-30"
)
```
Confirm via `gitlab_work_item_get`: a widget with `type == "START_AND_DUE_DATE"` has `startDate == "2026-06-01"` and `dueDate == "2026-06-30"`. Record `id` as `wi-dates-gid`.

### 45.4 Create with parent (hierarchy)
```
gitlab_work_item_create(
  project_path="3kirt1/gitlab-mcp-testing",
  work_item_type="TASK",
  title="Child of scratch task",
  parent_id=<wi-scratch-gid>
)
```
Confirm via `gitlab_work_item_get(id=<wi-scratch-gid>)`: the HIERARCHY widget now has `hasChildren == true` and the new task appears in `children.nodes`.

---

## Section 46: Work Item — Update

Operate on `wi-scratch-gid` from Section 45.1 unless otherwise noted.

### 46.1 Update title
```
gitlab_work_item_update(id=<wi-scratch-gid>, title="Updated task title")
```
Returned `title == "Updated task title"`.

### 46.2 Update description
```
gitlab_work_item_update(id=<wi-scratch-gid>, description="Updated description.")
```
Confirm via `gitlab_work_item_get`: DESCRIPTION widget has `description == "Updated description."`.

### 46.3 Close via state_event
```
gitlab_work_item_update(id=<wi-scratch-gid>, state_event="CLOSE")
```
Returned `state == "CLOSED"`.

### 46.4 Reopen via state_event
```
gitlab_work_item_update(id=<wi-scratch-gid>, state_event="REOPEN")
```
Returned `state == "OPEN"`.

### 46.5 Replace assignees
```
gitlab_work_item_update(id=<wi-desc-gid>, assignee_usernames=["3kirt1"])
```
Confirm via `gitlab_work_item_get`: ASSIGNEES widget has exactly one entry with `username == "3kirt1"`. Then clear:
```
gitlab_work_item_update(id=<wi-desc-gid>, assignee_usernames=[])
```
Confirm via `gitlab_work_item_get`: ASSIGNEES widget has `assignees.nodes == []`.

### 46.6 Update dates
```
gitlab_work_item_update(id=<wi-dates-gid>, start_date="2026-07-01", due_date="2026-07-31")
```
Confirm via `gitlab_work_item_get`: START_AND_DUE_DATE widget has `startDate == "2026-07-01"` and `dueDate == "2026-07-31"`.

---

## Section 47: Work Item — Delete

### 47.1 Delete a work item
Create a throwaway work item, record its `id`, then:
```
gitlab_work_item_delete(id=<throwaway-gid>)
```
Returns a success text message. A subsequent `gitlab_work_item_get(id=<throwaway-gid>)` returns a GraphQL error (work item not found).

Delete the scratch items from Section 45 (`wi-scratch-gid`, `wi-desc-gid`, `wi-dates-gid`) once testing is complete.

---

## Workflow H: Work item lifecycle (create → get → update → close → delete)

1. `gitlab_work_item_create(project_path="3kirt1/gitlab-mcp-testing", work_item_type="TASK", title="Workflow H task")` — record `id` as `wi-h-gid`
2. `gitlab_work_item_get(id=<wi-h-gid>)` — confirm `title == "Workflow H task"`, `state == "OPEN"`
3. `gitlab_work_item_update(id=<wi-h-gid>, title="Workflow H task — updated", description="Added in step 3.")` — confirm both fields returned
4. `gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", types=["TASK"])` — confirm `wi-h-gid` appears in results
5. `gitlab_work_item_update(id=<wi-h-gid>, state_event="CLOSE")` — confirm `state == "CLOSED"`
6. `gitlab_work_items_list(project_path="3kirt1/gitlab-mcp-testing", state="closed")` — confirm `wi-h-gid` appears
7. `gitlab_work_item_delete(id=<wi-h-gid>)` — confirm success message
8. `gitlab_work_item_get(id=<wi-h-gid>)` — confirm GraphQL error (not found)

