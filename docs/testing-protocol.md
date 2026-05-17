# GitLab MCP Testing Protocol

This document describes how to use the MCP tools to verify that all Issues, Branches, and Merge Requests API functionality is working correctly against the test project `3kirt1/gitlab-mcp-testing` (numeric ID `82279422`) seeded with the data defined below.

---

## Universal Invariants

Every response from every tool must satisfy these properties. Check them on every call.

**Issues and merge requests:**

| Property | What to verify |
|---|---|
| `iid` present | Every object has a project-scoped `iid` (the number shown in the GitLab UI) |
| `id` present | Every object has a global GitLab `id` |
| `project_id` present | Every object has a `project_id` matching the requested project |
| `state` value | `state` is never absent or `null` |
| `title` present | Every object has a non-empty `title` |
| `web_url` present | Every object has a `web_url` pointing to the GitLab UI |
| List is an array | List responses are JSON arrays, not objects |
| Delete confirmation | Delete returns a success text message, not a JSON object |

**Merge requests only:**

| Property | What to verify |
|---|---|
| `source_branch` present | Every MR object has a non-empty `source_branch` |
| `target_branch` present | Every MR object has a non-empty `target_branch` |
| `author` present | Every MR object has an `author` object with at least `id` and `username` |

**Branches:**

| Property | What to verify |
|---|---|
| `name` present | Every branch object has a non-empty `name` |
| `commit` present | Every branch object has a `commit` object with at least an `id` field |
| `merged` is boolean | `merged` is `true` or `false`, never `null` or absent |
| `protected` is boolean | `protected` is `true` or `false`, never `null` or absent |
| `web_url` present | Every branch object has a `web_url` |
| List is an array | List responses are JSON arrays, not objects |
| Delete confirmation | Delete returns a success text message, not a JSON object |

---

## Seed Data

### Issues

Before running the issue tests, create the following issues in `3kirt1/gitlab-mcp-testing`. Record the `iid` returned for each — the protocol uses these as ground truth for assertions.

| # | Title | Labels | Due Date | State after seed |
|---|---|---|---|---|
| seed-1 | `Bug: login page crashes on submit` | `bug` | — | opened |
| seed-2 | `Feature: add dark mode support` | `enhancement` | `2026-12-31` | opened |
| seed-3 | `Fix: memory leak in issues API` | `bug,performance` | — | opened → **closed** (close it after creating) |
| seed-4 | `Docs: update README with auth instructions` | `documentation` | — | opened |
| seed-5 | `Chore: bump Rust dependencies` | — | — | opened → **closed** (close it after creating) |

After seeding, the project should have:
- 5 issues total
- 3 opened: seed-1, seed-2, seed-4
- 2 closed: seed-3, seed-5
- Labels in use: `bug` (seed-1, seed-3), `enhancement` (seed-2), `performance` (seed-3), `documentation` (seed-4)
- 1 issue with a due date: seed-2

> **Note:** Issue `#1` ("Test issue") was created before the seed data. Adjust expected counts accordingly — the assertions below assume it exists and is opened unless you delete it first.

### Branches

Create the following branches using `gitlab_branches_create` before running either the branch or merge request tests. All branches are created from `main`.

| Branch name | Purpose |
|---|---|
| `mr-test-open` | Source branch for an open MR |
| `mr-test-draft` | Source branch for a draft MR |
| `mr-test-close` | Source branch for an MR that will be closed during testing |
| `mr-test-merge` | Source branch for an MR that will be merged (add a real commit via git or GitLab UI after creating so it is ahead of `main`) |
| `mr-test-scratch` | Reusable source branch for scratch MR tests in Sections 14 and 16 |
| `branch-test-1` | Branch for branch list/get/delete testing; deleted in Section 10 |

Example create call (repeat for each branch):
```
gitlab_branches_create(
  project_id="3kirt1/gitlab-mcp-testing",
  branch="mr-test-open",
  ref="main"
)
```

After seeding, the project should have at least 7 branches: `main` plus the 6 above.

### Merge Requests

After the branch seed above, create the following MRs using `gitlab_mrs_create`. Record the `iid` of each:

| # | Title | Source branch | Labels | Draft | State after seed |
|---|---|---|---|---|---|
| mr-seed-1 | `Fix: correct off-by-one error` | `mr-test-open` | `bug` | false | opened |
| mr-seed-2 | `Draft: refactor auth module` | `mr-test-draft` | `enhancement` | true | opened (draft) |
| mr-seed-3 | `Chore: update CI config` | `mr-test-close` | — | false | opened → **closed** (close after creating) |
| mr-seed-4 | `Feature: add health check endpoint` | `mr-test-merge` | `enhancement` | false | opened (to be merged in Section 17) |

After seeding:
- 4 MRs total
- 3 opened: mr-seed-1 (open), mr-seed-2 (draft), mr-seed-4 (ready to merge)
- 1 closed: mr-seed-3

---

## Section 1: Issues — List

### 1.1 List all issues (no state filter)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns an array
- Returns all issues regardless of state (opened and closed) — GitLab omits the state filter when no `state` param is sent
- To get only opened issues, pass `state="opened"` explicitly

### 1.2 List all issues regardless of state
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all")
```
- Returns all 6 issues (5 seed + issue #1)
- Mix of `state == "opened"` and `state == "closed"`

### 1.3 List closed issues only
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="closed")
```
- Returns exactly 2 issues: seed-3 and seed-5
- All results have `state == "closed"`

### 1.4 Filter by label (single label)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")
```
- Returns seed-1 and seed-3 (both have the `bug` label)
- Does not include seed-2, seed-4, seed-5

### 1.5 Filter by label (multiple labels — AND logic)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug,performance")
```
- Returns only seed-3 (the only issue with both `bug` and `performance`)

### 1.6 Search by title keyword
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="memory")
```
- Returns seed-3 (`Fix: memory leak in issues API`)
- Does not return unrelated issues

### 1.7 Search with no matches
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="xyzzy-nonexistent")
```
- Returns an empty array `[]`
- No error

### 1.8 Order by created_at ascending
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", order_by="created_at", sort="asc")
```
- First result is the earliest-created issue (issue #1)
- Each subsequent issue has `created_at >= previous`

### 1.9 Order by updated_at descending
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", order_by="updated_at", sort="desc")
```
- First result is the most-recently updated issue
- Each subsequent issue has `updated_at <= previous`

### 1.10 Numeric project ID
```
gitlab_issues_list(project_id="82279422", state="all")
```
- Same results as 1.2 (numeric ID and path are equivalent)

---

## Section 2: Issues — Get

### 2.1 Get by IID
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>)
```
- Single issue object (not an array)
- `title == "Bug: login page crashes on submit"`
- `state == "opened"`
- `labels` contains `"bug"`
- `project_id == 82279422`

### 2.2 Get a closed issue
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-3>)
```
- `state == "closed"`
- `closed_at` is non-null

### 2.3 Get issue with due date
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-2>)
```
- `due_date == "2026-12-31"`

### 2.4 Get using numeric project ID
```
gitlab_issues_get(project_id="82279422", issue_iid=<iid of seed-1>)
```
- Same result as 2.1

---

## Section 3: Issues — Create

### 3.1 Create with title only
```
gitlab_issues_create(project_id="3kirt1/gitlab-mcp-testing", title="Minimal issue")
```
- Returns issue object with `title == "Minimal issue"`
- `state == "opened"`
- `description` is null or absent
- Record the `iid` — used in later sections; delete after testing

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
- `title == "Full issue creation test"`
- `description == "This tests all optional fields."`
- `due_date == "2026-06-30"`
- `labels` includes both `test` and `automation`
- `state == "opened"`
- Record the `iid` — delete after testing

### 3.3 Create with Markdown description
```
gitlab_issues_create(
  project_id="3kirt1/gitlab-mcp-testing",
  title="Markdown description test",
  description="## Summary\n\n- item one\n- item two\n\n**Bold** and _italic_."
)
```
- `description` contains the raw Markdown string
- No escaping or stripping of Markdown syntax
- Record the `iid` — delete after testing

---

## Section 4: Issues — Update

For each test in this section, operate on one of the issues created in Section 3 (or a dedicated scratch issue). Verify the returned object reflects the change.

### 4.1 Update title
```
gitlab_issues_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<scratch iid>,
  title="Updated title"
)
```
- Returned `title == "Updated title"`
- Other fields unchanged

### 4.2 Update description
```
gitlab_issues_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<scratch iid>,
  description="New description."
)
```
- Returned `description == "New description."`

### 4.3 Add labels
```
gitlab_issues_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<scratch iid>,
  labels="bug,needs-review"
)
```
- Returned `labels` contains `"bug"` and `"needs-review"`
- Previous labels are replaced (GitLab replaces, not appends)

### 4.4 Close issue via state_event
```
gitlab_issues_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<scratch iid>,
  state_event="close"
)
```
- Returned `state == "closed"`
- `closed_at` is non-null

### 4.5 Reopen issue via state_event
```
gitlab_issues_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<scratch iid>,
  state_event="reopen"
)
```
- Returned `state == "opened"`
- `closed_at` is null or absent

### 4.6 Set due date
```
gitlab_issues_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<scratch iid>,
  due_date="2027-01-01"
)
```
- Returned `due_date == "2027-01-01"`

### 4.7 No-op update (empty body)
```
gitlab_issues_update(
  project_id="3kirt1/gitlab-mcp-testing",
  issue_iid=<scratch iid>
)
```
- **Expected:** GitLab returns `400 Bad Request` — the API requires at least one field in the request body
- Tool surfaces this as an error message containing `400`; no crash
- This is correct GitLab API behavior, not a tool bug

---

## Section 5: Issues — Delete

> Delete requires Maintainer or Owner role on the project.

### 5.1 Delete a scratch issue
Create a throwaway issue, record its `iid`, then:
```
gitlab_issues_delete(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<throwaway iid>)
```
- Returns a success text message (not a JSON object)
- No error

### 5.2 Verify deletion
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<throwaway iid>)
```
- Returns an error message containing `404` or similar
- Does not crash

---

## Section 6: Pagination

### 6.1 First page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=1)
```
- Returns exactly 2 issues
- Response is an array (GitLab REST paginates via `X-Next-Page` headers; the client returns the page contents as-is)

### 6.2 Second page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=2)
```
- Returns the next 2 issues
- No overlap with page 1

### 6.3 Last page (beyond available results)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=99)
```
- Returns an empty array `[]`
- No error

### 6.4 Large per_page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=100)
```
- Returns all issues in a single call (6 total after seeding)
- Array length == total issue count

---

## Cross-Tool Workflows — Issues

### Workflow A: Find and close a bug
1. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", labels="bug")` — find open bugs
2. Pick the `iid` of seed-1
3. `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid>)` — confirm it is `opened`
4. `gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid>, state_event="close")` — close it
5. `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid>)` — confirm `state == "closed"`
6. Reopen: `gitlab_issues_update(..., state_event="reopen")` to restore state

### Workflow B: Create, update, then delete
1. `gitlab_issues_create(project_id="3kirt1/gitlab-mcp-testing", title="Workflow B scratch")` — record `iid`
2. `gitlab_issues_update(..., issue_iid=<iid>, title="Workflow B updated", labels="test")` — verify title and label
3. `gitlab_issues_update(..., issue_iid=<iid>, state_event="close")` — verify closed
4. `gitlab_issues_delete(..., issue_iid=<iid>)` — verify success message
5. `gitlab_issues_get(..., issue_iid=<iid>)` — verify 404 error

### Workflow C: Label-based triage audit
1. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")` — list all bugs
2. For each result: confirm `labels` array/string contains `"bug"`
3. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="closed", labels="bug")` — list resolved bugs
4. Verify seed-3 appears; verify seed-1 does not (it is opened)

### Workflow D: Search and update
1. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="README")` — find docs issue
2. Confirm seed-4 is returned
3. `gitlab_issues_update(..., issue_iid=<iid of seed-4>, labels="documentation,needs-review")` — add label
4. `gitlab_issues_get(..., issue_iid=<iid of seed-4>)` — confirm both labels present

---

## Section 7: Branches — List

### 7.1 List all branches (no filter)
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns an array
- Array contains at least 7 branches (main + 6 seeded)
- Each result satisfies all branch universal invariants

### 7.2 Filter by search string
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", search="mr-test")
```
- Returns exactly the 5 `mr-test-*` branches
- Does not return `main` or `branch-test-1`

### 7.3 Filter by regex
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", regex="^mr-test-(open|draft)$")
```
- Returns exactly `mr-test-open` and `mr-test-draft`
- Does not return other `mr-test-*` branches

### 7.4 Regex with no matches
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", regex="^xyzzy-nonexistent$")
```
- Returns an empty array `[]`
- No error

### 7.5 Search with no matches
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", search="xyzzy-nonexistent")
```
- Returns an empty array `[]`
- No error

### 7.6 Pagination
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", per_page=2, page=1)
```
- Returns exactly 2 branches

```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", per_page=2, page=2)
```
- Returns the next 2 branches; no overlap with page 1

### 7.7 Numeric project ID
```
gitlab_branches_list(project_id="82279422")
```
- Same results as 7.1

---

## Section 8: Branches — Get

### 8.1 Get an existing branch
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="main")
```
- Single branch object (not an array)
- `name == "main"`
- `commit` object is present with a non-empty `id`
- `protected == true` (main is protected by default)
- `default == true`

### 8.2 Get a seeded branch
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-open")
```
- `name == "mr-test-open"`
- `merged == false` (not yet merged)
- `protected == false`

### 8.3 Numeric project ID
```
gitlab_branches_get(project_id="82279422", branch="main")
```
- Same result as 8.1

---

## Section 9: Branches — Create

### 9.1 Create from main
```
gitlab_branches_create(
  project_id="3kirt1/gitlab-mcp-testing",
  branch="branch-test-scratch",
  ref="main"
)
```
- Returns branch object with `name == "branch-test-scratch"`
- `commit.id` matches the HEAD commit of `main`
- `merged == false`
- Record the branch name — delete after testing

### 9.2 Create from another branch
```
gitlab_branches_create(
  project_id="3kirt1/gitlab-mcp-testing",
  branch="branch-test-from-seed",
  ref="mr-test-open"
)
```
- Returns branch object with `name == "branch-test-from-seed"`
- `commit.id` matches the HEAD commit of `mr-test-open`
- Delete after testing

### 9.3 Verify creation with get
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-scratch")
```
- Same `commit.id` as returned in 9.1
- `name == "branch-test-scratch"`

### 9.4 Numeric project ID
```
gitlab_branches_create(
  project_id="82279422",
  branch="branch-test-numeric-id",
  ref="main"
)
```
- Succeeds; returns branch object
- Delete after testing

---

## Section 10: Branches — Delete

> Cannot delete default (`main`) or protected branches.

### 10.1 Delete a branch
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-1")
```
- Returns a success text message (not a JSON object)
- No error

### 10.2 Verify deletion
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-1")
```
- Returns an error message containing `404`
- Does not crash

### 10.3 Attempt to delete a protected branch
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="main")
```
- GitLab returns 403 — cannot delete a protected branch
- Tool surfaces this as an error message containing `403`; no crash

### 10.4 Delete scratch branches created in Section 9
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-scratch")
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-from-seed")
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-numeric-id")
```
- Each returns a success text message

---

## Section 11: Branches — Delete Merged

> Run this section **after Section 17** (MR merge). After merging mr-seed-4, the `mr-test-merge` branch will be the project's first merged non-protected branch and is a safe target for the bulk delete.

### 11.1 Confirm a merged branch exists
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-merge")
```
- `merged == true` (after the merge in Section 17)

### 11.2 Delete all merged branches
```
gitlab_branches_delete_merged(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns a success text message
- No error

### 11.3 Verify merged branch was removed
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-merge")
```
- Returns an error message containing `404`
- Does not crash

### 11.4 Verify unmerged branches are untouched
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-open")
```
- Returns the branch successfully (still exists — its MR was not merged)

---

## Section 12: Merge Requests — List

### 12.1 List all open MRs (default)
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns an array
- All results have `state == "opened"` (GitLab defaults to opened when no state param is sent)

### 12.2 List all MRs regardless of state
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all")
```
- Returns all 4 seeded MRs
- Mix of `state == "opened"` and `state == "closed"`

### 12.3 List closed MRs only
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="closed")
```
- Returns exactly mr-seed-3
- All results have `state == "closed"`

### 12.4 Filter by source branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", source_branch="mr-test-open")
```
- Returns exactly mr-seed-1

### 12.5 Filter by target branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", target_branch="main")
```
- Returns all 4 seeded MRs (all target `main`)

### 12.6 Filter by label
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")
```
- Returns mr-seed-1 only

### 12.7 Filter by draft
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", draft=true)
```
- Returns mr-seed-2 only
- `draft == true` on the result

### 12.8 Search by title keyword
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="health")
```
- Returns mr-seed-4 (`Feature: add health check endpoint`)

### 12.9 Search with no matches
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="xyzzy-nonexistent")
```
- Returns an empty array `[]`
- No error

### 12.10 Numeric project ID
```
gitlab_mrs_list(project_id="82279422", state="all")
```
- Same results as 12.2

### 12.11 Pagination
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=1)
```
- Returns exactly 2 MRs

```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=2)
```
- Returns the next 2 MRs; no overlap with page 1

---

## Section 13: Merge Requests — Get

### 13.1 Get open MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-1>)
```
- Single MR object (not an array)
- `title == "Fix: correct off-by-one error"`
- `state == "opened"`
- `source_branch == "mr-test-open"`
- `target_branch == "main"`
- `draft == false`
- `labels` contains `"bug"`

### 13.2 Get draft MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-2>)
```
- `draft == true`
- Title begins with `"Draft:"` or the `draft` field is `true`

### 13.3 Get closed MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-3>)
```
- `state == "closed"`

### 13.4 Numeric project ID
```
gitlab_mrs_get(project_id="82279422", merge_request_iid=<iid of mr-seed-1>)
```
- Same result as 13.1

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
- Returns MR object with `title == "Scratch MR for testing"`
- `state == "opened"`
- `draft == false`
- `description` is null or absent
- Record the `iid` — delete after testing

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
- `title == "Full MR creation test"`
- `description == "Tests all optional fields."`
- `labels` includes both `test` and `automation`
- `squash == true`
- `draft == true`
- Record the `iid` — delete after testing

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
- `description` contains the raw Markdown string
- Record the `iid` — delete after testing

---

## Section 15: Merge Requests — Update

For each test in this section, operate on a scratch MR created in Section 14. Verify the returned object reflects the change.

### 15.1 Update title
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  title="Updated MR title"
)
```
- Returned `title == "Updated MR title"`
- Other fields unchanged

### 15.2 Update description
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  description="New description."
)
```
- Returned `description == "New description."`

### 15.3 Add labels
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  labels="bug,needs-review"
)
```
- Returned `labels` contains `"bug"` and `"needs-review"`

### 15.4 Mark as draft
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  draft=true
)
```
- Returned `draft == true`

### 15.5 Mark as ready (un-draft)
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  draft=false
)
```
- Returned `draft == false`

### 15.6 Close via state_event
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  state_event="close"
)
```
- Returned `state == "closed"`

### 15.7 Reopen via state_event
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  state_event="reopen"
)
```
- Returned `state == "opened"`

---

## Section 16: Merge Requests — Delete

> Delete requires Maintainer or Owner role on the project.

### 16.1 Delete a scratch MR
Create a throwaway MR (source branch `mr-test-scratch`), record its `iid`, then:
```
gitlab_mrs_delete(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<throwaway iid>)
```
- Returns a success text message (not a JSON object)
- No error

### 16.2 Verify deletion
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<throwaway iid>)
```
- Returns an error message containing `404`
- Does not crash

---

## Section 17: Merge Requests — Merge

> The source branch (`mr-test-merge`) must have at least one commit ahead of `main` for GitLab to allow the merge. Add this commit via git or the GitLab UI after creating the branch in the seed step.

### 17.1 Merge an open MR
```
gitlab_mrs_merge(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-4>
)
```
- Returned `state == "merged"`
- `merged_at` is non-null
- `merge_commit_sha` is present

### 17.2 Attempt to merge an already-closed MR
```
gitlab_mrs_merge(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-3>
)
```
- GitLab returns 405 or 406 — MR is not in a mergeable state
- Tool surfaces this as an error message; no crash

> After completing Section 17, proceed to Section 11 (Branches — Delete Merged) to verify that `mr-test-merge` is now deleted by `gitlab_branches_delete_merged`.

---

## Cross-Tool Workflows — Issues

### Workflow A: Find and close a bug
1. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", labels="bug")` — find open bugs
2. Pick the `iid` of seed-1
3. `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid>)` — confirm it is `opened`
4. `gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid>, state_event="close")` — close it
5. `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid>)` — confirm `state == "closed"`
6. Reopen: `gitlab_issues_update(..., state_event="reopen")` to restore state

### Workflow B: Create, update, then delete
1. `gitlab_issues_create(project_id="3kirt1/gitlab-mcp-testing", title="Workflow B scratch")` — record `iid`
2. `gitlab_issues_update(..., issue_iid=<iid>, title="Workflow B updated", labels="test")` — verify title and label
3. `gitlab_issues_update(..., issue_iid=<iid>, state_event="close")` — verify closed
4. `gitlab_issues_delete(..., issue_iid=<iid>)` — verify success message
5. `gitlab_issues_get(..., issue_iid=<iid>)` — verify 404 error

### Workflow C: Label-based triage audit
1. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")` — list all bugs
2. For each result: confirm `labels` array/string contains `"bug"`
3. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="closed", labels="bug")` — list resolved bugs
4. Verify seed-3 appears; verify seed-1 does not (it is opened)

### Workflow D: Search and update
1. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="README")` — find docs issue
2. Confirm seed-4 is returned
3. `gitlab_issues_update(..., issue_iid=<iid of seed-4>, labels="documentation,needs-review")` — add label
4. `gitlab_issues_get(..., issue_iid=<iid of seed-4>)` — confirm both labels present

---

## Cross-Tool Workflows — Branches

### Workflow H: Create branch, open MR, close MR, delete branch
1. `gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-h", ref="main")` — verify `name == "workflow-h"`
2. `gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-h")` — confirm it exists and `merged == false`
3. `gitlab_mrs_create(project_id="3kirt1/gitlab-mcp-testing", source_branch="workflow-h", target_branch="main", title="Workflow H MR")` — record `iid`
4. `gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", source_branch="workflow-h")` — confirm the MR appears
5. `gitlab_mrs_update(..., merge_request_iid=<iid>, state_event="close")` — close the MR
6. `gitlab_mrs_get(..., merge_request_iid=<iid>)` — confirm `state == "closed"`
7. `gitlab_mrs_delete(..., merge_request_iid=<iid>)` — delete the MR
8. `gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-h")` — delete the branch
9. `gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-h")` — confirm 404 error

### Workflow I: List branches, find MR source, inspect MR
1. `gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", search="mr-test")` — list MR source branches
2. Pick `mr-test-open` from the results; confirm `merged == false`
3. `gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", source_branch="mr-test-open")` — find its MR
4. `gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-1>)` — confirm `source_branch == "mr-test-open"` and `state == "opened"`

---

## Cross-Tool Workflows — Merge Requests

### Workflow E: Find open MR and close it
1. `gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened")` — list open MRs
2. Pick mr-seed-1; note the `iid`
3. `gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid>)` — confirm `state == "opened"`
4. `gitlab_mrs_update(..., merge_request_iid=<iid>, state_event="close")` — close it
5. `gitlab_mrs_get(..., merge_request_iid=<iid>)` — confirm `state == "closed"`
6. `gitlab_mrs_update(..., merge_request_iid=<iid>, state_event="reopen")` — restore state

### Workflow F: Create branch, create MR, update, delete both
1. `gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-f", ref="main")` — create source branch
2. `gitlab_mrs_create(project_id="3kirt1/gitlab-mcp-testing", source_branch="workflow-f", target_branch="main", title="Workflow F scratch")` — record `iid`
3. `gitlab_mrs_update(..., merge_request_iid=<iid>, title="Workflow F updated", labels="test", draft=true)` — verify changes
4. `gitlab_mrs_update(..., merge_request_iid=<iid>, state_event="close")` — verify closed
5. `gitlab_mrs_delete(..., merge_request_iid=<iid>)` — verify success message
6. `gitlab_mrs_get(..., merge_request_iid=<iid>)` — verify 404 error
7. `gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-f")` — clean up branch
8. `gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-f")` — verify branch gone

### Workflow G: Draft promotion
1. `gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", draft=true)` — confirm mr-seed-2 is returned
2. `gitlab_mrs_update(..., merge_request_iid=<iid of mr-seed-2>, draft=false)` — mark as ready
3. `gitlab_mrs_get(..., merge_request_iid=<iid of mr-seed-2>)` — confirm `draft == false`
4. `gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", draft=true)` — confirm mr-seed-2 no longer appears
5. `gitlab_mrs_update(..., merge_request_iid=<iid of mr-seed-2>, draft=true)` — restore draft state

---

## Error Handling Checks

**Issues:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999)` | Tool returns an error message containing `404`; no crash |
| `gitlab_issues_list(project_id="nonexistent-group/nonexistent-repo")` | Tool returns an error message containing `404`; no crash |
| `gitlab_issues_delete(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999)` | Tool returns an error message; no crash |
| `gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999, title="x")` | Tool returns an error message containing `404`; no crash |
| `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="invalid_value")` | GitLab 400 surfaced as tool error; no crash |

**Branches:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="nonexistent-branch")` | Tool returns an error message containing `404`; no crash |
| `gitlab_branches_list(project_id="nonexistent-group/nonexistent-repo")` | Tool returns an error message containing `404`; no crash |
| `gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="nonexistent-branch")` | Tool returns an error message containing `404`; no crash |
| `gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="main")` | Tool returns an error message containing `403`; no crash |
| `gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="new", ref="nonexistent-ref")` | Tool returns an error message; no crash |

**Merge requests:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999)` | Tool returns an error message containing `404`; no crash |
| `gitlab_mrs_list(project_id="nonexistent-group/nonexistent-repo")` | Tool returns an error message containing `404`; no crash |
| `gitlab_mrs_delete(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999)` | Tool returns an error message; no crash |
| `gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999, title="x")` | Tool returns an error message containing `404`; no crash |
| `gitlab_mrs_merge(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999)` | Tool returns an error message containing `404`; no crash |

---

## Checklist Summary

Run through these in order for a complete regression pass:

**Setup:**
- [ ] Issue seed data created: 5 issues in `3kirt1/gitlab-mcp-testing`, seed-3 and seed-5 closed
- [ ] Branch seed data created using `gitlab_branches_create`: `mr-test-open`, `mr-test-draft`, `mr-test-close`, `mr-test-merge`, `mr-test-scratch`, `branch-test-1`
- [ ] Commit added to `mr-test-merge` ahead of main (via git or GitLab UI)
- [ ] MR seed data created: 4 MRs, mr-seed-3 closed, mr-seed-4 has a commit ahead of main

**Issues:**
- [ ] Universal invariants: `iid`, `id`, `project_id`, `state`, `title`, `web_url` present on every issue object
- [ ] Section 1: List — default state, all, closed, label filter, multi-label AND, search match, search no-match, order asc, order desc, numeric project ID
- [ ] Section 2: Get — by IID, closed issue, issue with due date, numeric project ID
- [ ] Section 3: Create — title only, all optional fields, Markdown description
- [ ] Section 4: Update — title, description, labels, close, reopen, due date, no-op
- [ ] Section 5: Delete — success message, subsequent get returns 404
- [ ] Section 6: Pagination — first page, second page, beyond-last page, large per_page
- [ ] Workflow A: Find bug → get → close → verify → reopen
- [ ] Workflow B: Create → update → close → delete → verify gone
- [ ] Workflow C: Label-based triage audit
- [ ] Workflow D: Search → update labels → verify
- [ ] Issues error handling: 404 on get, 404 on nonexistent project, 404 on delete, 404 on update, invalid state param

**Branches:**
- [ ] Universal invariants: `name`, `commit.id`, `merged`, `protected`, `web_url` present on every branch object
- [ ] Section 7: List — no filter, search, regex, regex no-match, search no-match, pagination, numeric project ID
- [ ] Section 8: Get — default branch (protected), seeded branch (unprotected), numeric project ID
- [ ] Section 9: Create — from main, from another branch, verify with get, numeric project ID
- [ ] Section 10: Delete — success message, subsequent get returns 404, protected branch returns 403, scratch cleanup
- [ ] Section 11: Delete Merged — run after Section 17; verify merged branch removed, unmerged branch untouched
- [ ] Workflow H: Create branch → open MR → close MR → delete MR → delete branch → verify gone
- [ ] Workflow I: List branches → find MR source → inspect MR
- [ ] Branches error handling: 404 on get nonexistent, 404 on list nonexistent project, 404 on delete nonexistent, 403 on delete protected, error on create with bad ref

**Merge Requests:**
- [ ] Universal invariants: `iid`, `id`, `project_id`, `state`, `title`, `web_url`, `source_branch`, `target_branch`, `author` present on every MR object
- [ ] Section 12: List — default (opened), all, closed, source_branch, target_branch, label, draft, search match, search no-match, numeric project ID, pagination
- [ ] Section 13: Get — open MR, draft MR, closed MR, numeric project ID
- [ ] Section 14: Create — required fields only, all optional fields, Markdown description
- [ ] Section 15: Update — title, description, labels, mark draft, un-draft, close, reopen
- [ ] Section 16: Delete — success message, subsequent get returns 404
- [ ] Section 17: Merge — successful merge, attempt on non-open MR
- [ ] Workflow E: Find open MR → get → close → verify → reopen
- [ ] Workflow F: Create branch → create MR → update → close → delete MR → delete branch → verify both gone
- [ ] Workflow G: Draft promotion → verify removed from draft list → restore
- [ ] MR error handling: 404 on get, 404 on nonexistent project, 404 on delete, 404 on update, 404 on merge
