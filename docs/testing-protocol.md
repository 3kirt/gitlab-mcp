# GitLab MCP Testing Protocol

This document describes how to use the MCP tools to verify that all Issues and Merge Requests API functionality is working correctly against the test project `3kirt1/gitlab-mcp-testing` (numeric ID `82279422`) seeded with the data defined below.

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

### Merge Requests

MR tests require branches to exist in the repository. Create the following branches via git or the GitLab UI before running MR tests:

| Branch name | Base branch | Purpose |
|---|---|---|
| `mr-test-open` | `main` | Source branch for an open MR (add any trivial commit) |
| `mr-test-draft` | `main` | Source branch for a draft MR |
| `mr-test-close` | `main` | Source branch for an MR that will be closed during testing |
| `mr-test-merge` | `main` | Source branch for an MR that will be merged (add a real commit so it can merge) |

Then create the following MRs using `gitlab_mrs_create`. Record the `iid` of each:

| # | Title | Source branch | Labels | Draft | State after seed |
|---|---|---|---|---|---|
| mr-seed-1 | `Fix: correct off-by-one error` | `mr-test-open` | `bug` | false | opened |
| mr-seed-2 | `Draft: refactor auth module` | `mr-test-draft` | `enhancement` | true | opened (draft) |
| mr-seed-3 | `Chore: update CI config` | `mr-test-close` | — | false | opened → **closed** (close after creating) |
| mr-seed-4 | `Feature: add health check endpoint` | `mr-test-merge` | `enhancement` | false | opened (to be merged in section 12) |

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

## Section 7: Merge Requests — List

### 7.1 List all open MRs (default)
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns an array
- All results have `state == "opened"` (GitLab defaults to opened when no state param is sent)

### 7.2 List all MRs regardless of state
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all")
```
- Returns all 4 seeded MRs
- Mix of `state == "opened"` and `state == "closed"`

### 7.3 List closed MRs only
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="closed")
```
- Returns exactly mr-seed-3
- All results have `state == "closed"`

### 7.4 Filter by source branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", source_branch="mr-test-open")
```
- Returns exactly mr-seed-1

### 7.5 Filter by target branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", target_branch="main")
```
- Returns all 4 seeded MRs (all target `main`)

### 7.6 Filter by label
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")
```
- Returns mr-seed-1 only

### 7.7 Filter by draft
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", draft=true)
```
- Returns mr-seed-2 only
- `draft == true` on the result

### 7.8 Search by title keyword
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="health")
```
- Returns mr-seed-4 (`Feature: add health check endpoint`)

### 7.9 Search with no matches
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="xyzzy-nonexistent")
```
- Returns an empty array `[]`
- No error

### 7.10 Numeric project ID
```
gitlab_mrs_list(project_id="82279422", state="all")
```
- Same results as 7.2

### 7.11 Pagination
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=1)
```
- Returns exactly 2 MRs

```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=2)
```
- Returns the next 2 MRs; no overlap with page 1

---

## Section 8: Merge Requests — Get

### 8.1 Get open MR
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

### 8.2 Get draft MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-2>)
```
- `draft == true`
- Title begins with `"Draft:"` or the `draft` field is `true`

### 8.3 Get closed MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-3>)
```
- `state == "closed"`

### 8.4 Numeric project ID
```
gitlab_mrs_get(project_id="82279422", merge_request_iid=<iid of mr-seed-1>)
```
- Same result as 8.1

---

## Section 9: Merge Requests — Create

### 9.1 Create with required fields only
First create a branch `mr-test-scratch` from `main` (via git or GitLab UI), then:
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

### 9.2 Create with all optional fields
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

### 9.3 Create with Markdown description
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

## Section 10: Merge Requests — Update

For each test in this section, operate on a scratch MR created in Section 9. Verify the returned object reflects the change.

### 10.1 Update title
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  title="Updated MR title"
)
```
- Returned `title == "Updated MR title"`
- Other fields unchanged

### 10.2 Update description
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  description="New description."
)
```
- Returned `description == "New description."`

### 10.3 Add labels
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  labels="bug,needs-review"
)
```
- Returned `labels` contains `"bug"` and `"needs-review"`

### 10.4 Mark as draft
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  draft=true
)
```
- Returned `draft == true`

### 10.5 Mark as ready (un-draft)
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  draft=false
)
```
- Returned `draft == false`

### 10.6 Close via state_event
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  state_event="close"
)
```
- Returned `state == "closed"`

### 10.7 Reopen via state_event
```
gitlab_mrs_update(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<scratch iid>,
  state_event="reopen"
)
```
- Returned `state == "opened"`

---

## Section 11: Merge Requests — Delete

> Delete requires Maintainer or Owner role on the project.

### 11.1 Delete a scratch MR
Create a throwaway MR (source branch `mr-test-scratch`), record its `iid`, then:
```
gitlab_mrs_delete(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<throwaway iid>)
```
- Returns a success text message (not a JSON object)
- No error

### 11.2 Verify deletion
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<throwaway iid>)
```
- Returns an error message containing `404`
- Does not crash

---

## Section 12: Merge Requests — Merge

> The source branch (`mr-test-merge`) must have at least one commit ahead of `main` for GitLab to allow the merge.

### 12.1 Merge an open MR
```
gitlab_mrs_merge(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-4>
)
```
- Returned `state == "merged"`
- `merged_at` is non-null
- `merge_commit_sha` is present

### 12.2 Attempt to merge an already-closed MR
```
gitlab_mrs_merge(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-3>
)
```
- GitLab returns 405 or 406 — MR is not in a mergeable state
- Tool surfaces this as an error message; no crash

---

## Cross-Tool Workflows — Merge Requests

### Workflow E: Find open MR and close it
1. `gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened")` — list open MRs
2. Pick mr-seed-1; note the `iid`
3. `gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid>)` — confirm `state == "opened"`
4. `gitlab_mrs_update(..., merge_request_iid=<iid>, state_event="close")` — close it
5. `gitlab_mrs_get(..., merge_request_iid=<iid>)` — confirm `state == "closed"`
6. `gitlab_mrs_update(..., merge_request_iid=<iid>, state_event="reopen")` — restore state

### Workflow F: Create, update, then delete
1. `gitlab_mrs_create(project_id="3kirt1/gitlab-mcp-testing", source_branch="mr-test-scratch", target_branch="main", title="Workflow F scratch")` — record `iid`
2. `gitlab_mrs_update(..., merge_request_iid=<iid>, title="Workflow F updated", labels="test", draft=true)` — verify changes
3. `gitlab_mrs_update(..., merge_request_iid=<iid>, state_event="close")` — verify closed
4. `gitlab_mrs_delete(..., merge_request_iid=<iid>)` — verify success message
5. `gitlab_mrs_get(..., merge_request_iid=<iid>)` — verify 404 error

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
- [ ] MR seed branches created: `mr-test-open`, `mr-test-draft`, `mr-test-close`, `mr-test-merge`, `mr-test-scratch`
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

**Merge Requests:**
- [ ] Universal invariants: `iid`, `id`, `project_id`, `state`, `title`, `web_url`, `source_branch`, `target_branch`, `author` present on every MR object
- [ ] Section 7: List — default (opened), all, closed, source_branch, target_branch, label, draft, search match, search no-match, numeric project ID, pagination
- [ ] Section 8: Get — open MR, draft MR, closed MR, numeric project ID
- [ ] Section 9: Create — required fields only, all optional fields, Markdown description
- [ ] Section 10: Update — title, description, labels, mark draft, un-draft, close, reopen
- [ ] Section 11: Delete — success message, subsequent get returns 404
- [ ] Section 12: Merge — successful merge, attempt on non-open MR
- [ ] Workflow E: Find open MR → get → close → verify → reopen
- [ ] Workflow F: Create → update → close → delete → verify gone
- [ ] Workflow G: Draft promotion → verify removed from draft list → restore
- [ ] MR error handling: 404 on get, 404 on nonexistent project, 404 on delete, 404 on update, 404 on merge
