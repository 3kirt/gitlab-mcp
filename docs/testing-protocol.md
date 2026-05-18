# GitLab MCP Testing Protocol

This document describes how to use the MCP tools to verify that all Issues, Branches, Merge Requests, Repositories, and Repository Files API functionality is working correctly against the test project `3kirt1/gitlab-mcp-testing` (numeric ID `82279422`) seeded with the data defined below.

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

**Repository tree entries:**

| Property | What to verify |
|---|---|
| `id` present | Each entry has a non-empty SHA (`id`) |
| `name` present | Each entry has a non-empty `name` |
| `type` present | `type` is `"blob"` or `"tree"` |
| `path` present | Each entry has a non-empty `path` |
| `mode` present | Each entry has a `mode` string |
| List is an array | Tree list responses are JSON arrays |

**Repository files (GET):**

| Property | What to verify |
|---|---|
| `file_name` present | Filename without directory path |
| `file_path` present | Full path within repository |
| `size` present | Non-negative integer |
| `encoding` present | Usually `"base64"` |
| `content` present | Non-empty Base64-encoded string |
| `content_sha256` present | 64-character hex string |
| `ref` present | Matches the ref that was requested |
| `blob_id` present | Non-empty blob SHA |
| `commit_id` present | Non-empty commit SHA |
| `last_commit_id` present | Non-empty commit SHA |

---

## Seed Data

Perform all seed steps **in the order listed below**. Later steps depend on earlier ones.

### Step 1: Create Test Files on `main`

Use `gitlab_file_create` to add test files that all subsequent branches will inherit.

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
- Returns `{"file_path": "testing/sample.txt", "branch": "main"}`
- Record the `commit_id` from a subsequent `gitlab_file_get` call — used in blame assertions

**1b.** Update it to create a second commit (needed for blame history):
```
gitlab_file_update(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  branch="main",
  commit_message="Add fourth line to sample.txt",
  content="line one\nline two\nline three\nline four"
)
```
- After this, `testing/sample.txt` has two commits in its history: lines 1–3 from commit A, line 4 from commit B

After Step 1, `main` has `testing/sample.txt` with 4 lines and 2 blame entries.

### Step 2: Create Branches

Create all branches from `main` (after Step 1, so all branches start with `testing/sample.txt`).

| Branch name | Purpose |
|---|---|
| `mr-test-open` | Source branch for an open MR |
| `mr-test-draft` | Source branch for a draft MR |
| `mr-test-close` | Source branch for an MR that will be closed during testing |
| `mr-test-merge` | Source branch for an MR that will be merged |
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

After Step 2, the project has at least 7 branches: `main` plus the 6 above.

### Step 3: Advance `mr-test-merge` Ahead of `main`

Add a file unique to the feature branch so GitLab allows the merge and the compare diff is non-empty:
```
gitlab_file_create(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/feature.txt",
  branch="mr-test-merge",
  commit_message="Add feature file",
  content="This file was added in the feature branch."
)
```
- `mr-test-merge` is now exactly one commit ahead of `main`
- This replaces the previous requirement to add a commit via the git CLI or GitLab UI

### Step 4: Create Issues

Create the following issues. Record the `iid` returned for each.

| # | Title | Labels | Due Date | State after seed |
|---|---|---|---|---|
| seed-1 | `Bug: login page crashes on submit` | `bug` | — | opened |
| seed-2 | `Feature: add dark mode support` | `enhancement` | `2026-12-31` | opened |
| seed-3 | `Fix: memory leak in issues API` | `bug,performance` | — | opened → **closed** |
| seed-4 | `Docs: update README with auth instructions` | `documentation` | — | opened |
| seed-5 | `Chore: bump Rust dependencies` | — | — | opened → **closed** |

After seeding:
- 5 issues total; 3 opened (seed-1, seed-2, seed-4); 2 closed (seed-3, seed-5)
- Labels in use: `bug`, `enhancement`, `performance`, `documentation`
- 1 issue with a due date: seed-2

> **Note:** Issue `#1` ("Test issue") was created before the seed data. Adjust expected counts accordingly.

### Step 5: Create Merge Requests

Create the following MRs. Record the `iid` of each.

| # | Title | Source branch | Labels | Draft | State after seed |
|---|---|---|---|---|---|
| mr-seed-1 | `Fix: correct off-by-one error` | `mr-test-open` | `bug` | false | opened |
| mr-seed-2 | `Draft: refactor auth module` | `mr-test-draft` | `enhancement` | true | opened (draft) |
| mr-seed-3 | `Chore: update CI config` | `mr-test-close` | — | false | opened → **closed** |
| mr-seed-4 | `Feature: add health check endpoint` | `mr-test-merge` | `enhancement` | false | opened (to be merged in Section 17) |

After seeding:
- 4 MRs total; 3 opened; 1 closed (mr-seed-3)

---

## Section 1: Issues — List

### 1.1 List all issues (no state filter)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns an array
- Returns all issues regardless of state — GitLab omits the state filter when no `state` param is sent

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
- Returns seed-1 and seed-3; does not include seed-2, seed-4, seed-5

### 1.5 Filter by label (multiple labels — AND logic)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug,performance")
```
- Returns only seed-3 (the only issue with both `bug` and `performance`)

### 1.6 Search by title keyword
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="memory")
```
- Returns seed-3; does not return unrelated issues

### 1.7 Search with no matches
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="xyzzy-nonexistent")
```
- Returns an empty array `[]`; no error

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

### 1.10 Numeric project ID
```
gitlab_issues_list(project_id="82279422", state="all")
```
- Same results as 1.2

---

## Section 2: Issues — Get

### 2.1 Get by IID
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>)
```
- `title == "Bug: login page crashes on submit"`, `state == "opened"`, `labels` contains `"bug"`

### 2.2 Get a closed issue
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-3>)
```
- `state == "closed"`, `closed_at` is non-null

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
- `title == "Minimal issue"`, `state == "opened"`, `description` is null or absent
- Record the `iid` — delete after testing

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
- All provided fields are reflected in the response
- Record the `iid` — delete after testing

### 3.3 Create with Markdown description
```
gitlab_issues_create(
  project_id="3kirt1/gitlab-mcp-testing",
  title="Markdown description test",
  description="## Summary\n\n- item one\n- item two\n\n**Bold** and _italic_."
)
```
- `description` contains the raw Markdown string without escaping
- Record the `iid` — delete after testing

---

## Section 4: Issues — Update

For each test operate on a scratch issue from Section 3. Verify the returned object reflects the change.

### 4.1 Update title
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, title="Updated title")
```
- Returned `title == "Updated title"`

### 4.2 Update description
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, description="New description.")
```
- Returned `description == "New description."`

### 4.3 Add labels
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, labels="bug,needs-review")
```
- Returned `labels` contains `"bug"` and `"needs-review"` (previous labels replaced)

### 4.4 Close issue via state_event
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, state_event="close")
```
- Returned `state == "closed"`, `closed_at` is non-null

### 4.5 Reopen issue via state_event
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, state_event="reopen")
```
- Returned `state == "opened"`, `closed_at` is null or absent

### 4.6 Set due date
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>, due_date="2027-01-01")
```
- Returned `due_date == "2027-01-01"`

### 4.7 No-op update (empty body)
```
gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<scratch iid>)
```
- **Expected:** GitLab returns `400 Bad Request` — the API requires at least one field
- Tool surfaces this as an error message containing `400`; no crash

---

## Section 5: Issues — Delete

### 5.1 Delete a scratch issue
Create a throwaway issue, record its `iid`, then:
```
gitlab_issues_delete(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<throwaway iid>)
```
- Returns a success text message (not a JSON object); no error

### 5.2 Verify deletion
```
gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<throwaway iid>)
```
- Returns an error message containing `404`; does not crash

---

## Section 6: Pagination

### 6.1 First page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=1)
```
- Returns exactly 2 issues

### 6.2 Second page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=2)
```
- Returns the next 2 issues; no overlap with page 1

### 6.3 Last page (beyond available results)
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=2, page=99)
```
- Returns an empty array `[]`; no error

### 6.4 Large per_page
```
gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", per_page=100)
```
- Returns all issues in a single call; array length equals total issue count

---

## Section 7: Branches — List

### 7.1 List all branches (no filter)
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns an array of at least 7 branches (main + 6 seeded)
- Each result satisfies all branch universal invariants

### 7.2 Filter by search string
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", search="mr-test")
```
- Returns exactly the 5 `mr-test-*` branches; does not return `main` or `branch-test-1`

### 7.3 Filter by regex
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", regex="^mr-test-(open|draft)$")
```
- Returns exactly `mr-test-open` and `mr-test-draft`

### 7.4 Regex with no matches
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", regex="^xyzzy-nonexistent$")
```
- Returns an empty array `[]`; no error

### 7.5 Search with no matches
```
gitlab_branches_list(project_id="3kirt1/gitlab-mcp-testing", search="xyzzy-nonexistent")
```
- Returns an empty array `[]`; no error

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

### 8.1 Get the default branch
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="main")
```
- `name == "main"`, `protected == true`, `default == true`
- `commit` object present with non-empty `id`

### 8.2 Get a seeded branch
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-open")
```
- `name == "mr-test-open"`, `merged == false`, `protected == false`

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
- `name == "branch-test-scratch"`, `commit.id` matches HEAD of `main`
- Record the branch name — delete after testing

### 9.2 Create from another branch
```
gitlab_branches_create(
  project_id="3kirt1/gitlab-mcp-testing",
  branch="branch-test-from-seed",
  ref="mr-test-open"
)
```
- `commit.id` matches HEAD of `mr-test-open`
- Delete after testing

### 9.3 Verify creation with get
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-scratch")
```
- `commit.id` matches the value returned in 9.1

### 9.4 Numeric project ID
```
gitlab_branches_create(project_id="82279422", branch="branch-test-numeric-id", ref="main")
```
- Succeeds; delete after testing

---

## Section 10: Branches — Delete

### 10.1 Delete a branch
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-1")
```
- Returns a success text message; no error

### 10.2 Verify deletion
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-1")
```
- Returns an error message containing `404`; does not crash

### 10.3 Attempt to delete a protected branch
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="main")
```
- Tool surfaces a `403` error; no crash

### 10.4 Delete scratch branches from Section 9
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-scratch")
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-from-seed")
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="branch-test-numeric-id")
```
- Each returns a success text message

---

## Section 11: Branches — Delete Merged

> Run this section **after Section 17** (MR merge). After merging mr-seed-4, `mr-test-merge` will be the first merged non-protected branch.

### 11.1 Confirm a merged branch exists
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-merge")
```
- `merged == true` (after the merge in Section 17)

### 11.2 Delete all merged branches
```
gitlab_branches_delete_merged(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns a success text message; no error

### 11.3 Verify merged branch was removed
```
gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="mr-test-merge")
```
- Returns an error message containing `404`; does not crash

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
- All results have `state == "opened"`

### 12.2 List all MRs regardless of state
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all")
```
- Returns all 4 seeded MRs; mix of opened and closed

### 12.3 List closed MRs only
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="closed")
```
- Returns exactly mr-seed-3; all results have `state == "closed"`

### 12.4 Filter by source branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", source_branch="mr-test-open")
```
- Returns exactly mr-seed-1

### 12.5 Filter by target branch
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", target_branch="main")
```
- Returns all 4 seeded MRs

### 12.6 Filter by label
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", labels="bug")
```
- Returns mr-seed-1 only

### 12.7 Filter by draft
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", draft=true)
```
- Returns mr-seed-2 only; `draft == true` on the result

### 12.8 Search by title keyword
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="health")
```
- Returns mr-seed-4

### 12.9 Search with no matches
```
gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="xyzzy-nonexistent")
```
- Returns an empty array `[]`; no error

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
- `title == "Fix: correct off-by-one error"`, `state == "opened"`, `source_branch == "mr-test-open"`, `draft == false`

### 13.2 Get draft MR
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-2>)
```
- `draft == true`

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
- `state == "opened"`, `draft == false`, `description` is null or absent
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
- `squash == true`, `draft == true`; labels and description reflected in response
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
- `description` contains the raw Markdown string; record the `iid` — delete after testing

---

## Section 15: Merge Requests — Update

Operate on a scratch MR from Section 14.

### 15.1 Update title
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, title="Updated MR title")
```
- Returned `title == "Updated MR title"`

### 15.2 Update description
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, description="New description.")
```
- Returned `description == "New description."`

### 15.3 Add labels
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, labels="bug,needs-review")
```
- Returned `labels` contains `"bug"` and `"needs-review"`

### 15.4 Mark as draft
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, draft=true)
```
- Returned `draft == true`

### 15.5 Mark as ready (un-draft)
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, draft=false)
```
- Returned `draft == false`

### 15.6 Close via state_event
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, state_event="close")
```
- Returned `state == "closed"`

### 15.7 Reopen via state_event
```
gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<scratch iid>, state_event="reopen")
```
- Returned `state == "opened"`

---

## Section 16: Merge Requests — Delete

### 16.1 Delete a scratch MR
Create a throwaway MR (source branch `mr-test-scratch`), record its `iid`, then:
```
gitlab_mrs_delete(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<throwaway iid>)
```
- Returns a success text message; no error

### 16.2 Verify deletion
```
gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<throwaway iid>)
```
- Returns an error message containing `404`; does not crash

---

## Section 17: Merge Requests — Merge

### 17.1 Merge an open MR
```
gitlab_mrs_merge(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-4>
)
```
- Returned `state == "merged"`, `merged_at` is non-null, `merge_commit_sha` is present

### 17.2 Attempt to merge an already-closed MR
```
gitlab_mrs_merge(
  project_id="3kirt1/gitlab-mcp-testing",
  merge_request_iid=<iid of mr-seed-3>
)
```
- GitLab returns 405 or 406; tool surfaces an error; no crash

> After completing Section 17, proceed to Section 11 (Branches — Delete Merged).

---

## Section 18: Repository — Tree

### 18.1 List the root of the default branch
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main")
```
- Returns an array of tree entries
- At least one entry has `name == "testing"` and `type == "tree"`
- Each entry satisfies all repository tree universal invariants

### 18.2 List a subdirectory
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", path="testing")
```
- Returns at least one entry: `name == "sample.txt"`, `type == "blob"`
- `path` field is `"testing/sample.txt"` (full path, not just filename)
- Record the `id` (blob SHA) of `sample.txt` — used in Section 19

### 18.3 List recursively
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", recursive=true)
```
- Returns a flat array covering both root entries and all nested files
- `testing/sample.txt` appears directly in the array

### 18.4 List from a feature branch
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="mr-test-merge", path="testing")
```
- Returns both `sample.txt` and `feature.txt` (the file added to this branch in seed Step 3)
- `mr-test-merge` has one more entry than `main` in the `testing/` directory

### 18.5 Non-existent path returns empty
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", path="nonexistent-dir")
```
- Returns an empty array `[]` or a 404 error; no crash

### 18.6 Pagination
```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", per_page=1, page=1)
```
- Returns exactly 1 entry

```
gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", per_page=1, page=2)
```
- Returns a different single entry; no overlap with page 1

---

## Section 19: Repository — Blob Get and Raw

Use the blob SHA recorded in 18.2 (`sample.txt` blob id from the `testing/` tree listing).

### 19.1 Get blob metadata
```
gitlab_repo_blob_get(project_id="3kirt1/gitlab-mcp-testing", sha=<blob SHA of sample.txt>)
```
- Response contains `content` (non-empty Base64 string), `encoding == "base64"`, `sha`, `size` (positive integer)
- Decoding the Base64 content produces `"line one\nline two\nline three\nline four"`

### 19.2 Get raw blob content
```
gitlab_repo_blob_raw(project_id="3kirt1/gitlab-mcp-testing", sha=<blob SHA of sample.txt>)
```
- Response is `{"content": "line one\nline two\nline three\nline four"}`
- `content` key is present; no JSON array wrapping

### 19.3 Invalid SHA returns error
```
gitlab_repo_blob_get(project_id="3kirt1/gitlab-mcp-testing", sha="0000000000000000000000000000000000000000")
```
- Tool surfaces a `404` error; no crash

---

## Section 20: Repository — Compare

### 20.1 Compare main to feature branch (non-empty diff)
```
gitlab_repo_compare(
  project_id="3kirt1/gitlab-mcp-testing",
  from="main",
  to="mr-test-merge"
)
```
- `commits` is a non-empty array (at least the "Add feature file" commit)
- `diffs` is a non-empty array containing `testing/feature.txt`
- `compare_same_ref == false`
- `web_url` is present

### 20.2 Compare identical refs (empty diff)
```
gitlab_repo_compare(
  project_id="3kirt1/gitlab-mcp-testing",
  from="main",
  to="mr-test-open"
)
```
- `commits` is an empty array (mr-test-open has no extra commits)
- `diffs` is an empty array
- `compare_same_ref == false` (different branch names) or `true` (same underlying SHA)

### 20.3 Straight diff option
```
gitlab_repo_compare(
  project_id="3kirt1/gitlab-mcp-testing",
  from="main",
  to="mr-test-merge",
  straight=true
)
```
- Returns same structure as 20.1; straight diff does not error

### 20.4 Unified diff format
```
gitlab_repo_compare(
  project_id="3kirt1/gitlab-mcp-testing",
  from="main",
  to="mr-test-merge",
  unidiff=true
)
```
- `diffs` entries contain unified diff format output

---

## Section 21: Repository — Contributors

### 21.1 List contributors
```
gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing")
```
- Returns an array of at least 1 contributor
- Each entry has `name`, `email`, `commits` (positive integer), `additions`, `deletions`

### 21.2 Order by commits descending
```
gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing", order_by="commits", sort="desc")
```
- First contributor has the highest `commits` count
- Each subsequent entry has `commits <= previous`

### 21.3 Order by name ascending
```
gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing", order_by="name", sort="asc")
```
- Results are sorted alphabetically by `name`

### 21.4 Scope to a ref
```
gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing", ref_name="main")
```
- Returns contributors whose commits exist on `main`
- Should match or be a subset of 21.1

---

## Section 22: Repository — Merge Base

### 22.1 Find the common ancestor of two branches
```
gitlab_repo_merge_base(
  project_id="3kirt1/gitlab-mcp-testing",
  refs=["main", "mr-test-merge"]
)
```
- Returns a single commit object (not an array)
- Commit has `id`, `short_id`, `title`, `author_name`, `author_email`, `committed_date`
- The returned `id` is a valid commit SHA reachable from both `main` and `mr-test-merge`

### 22.2 Find the common ancestor of three refs
```
gitlab_repo_merge_base(
  project_id="3kirt1/gitlab-mcp-testing",
  refs=["main", "mr-test-open", "mr-test-draft"]
)
```
- Returns a commit object
- Since all three branches were created from the same `main` commit, the merge base is that commit

### 22.3 Invalid ref returns error
```
gitlab_repo_merge_base(
  project_id="3kirt1/gitlab-mcp-testing",
  refs=["main", "nonexistent-branch-xyz"]
)
```
- Tool surfaces an error (GitLab 400 or 404); no crash

---

## Section 23: Repository — Changelog

> The changelog API identifies commits using a Git trailer (default: `Changelog`). The seed commits were created via API without this trailer, so the generated `notes` will be empty. This is expected and tests that the tool calls succeed, not that the changelog is populated.

### 23.1 Generate changelog markdown (GET — does not commit)
```
gitlab_repo_changelog_get(
  project_id="3kirt1/gitlab-mcp-testing",
  version="0.1.0"
)
```
- Returns `{"notes": "..."}` where `notes` is a string (may be empty)
- No error; does not modify the repository

### 23.2 Generate with explicit range
```
gitlab_repo_changelog_get(
  project_id="3kirt1/gitlab-mcp-testing",
  version="0.1.0",
  to="main"
)
```
- Returns same structure as 23.1; no error

### 23.3 Commit changelog to repository (POST — write operation)

> **Optional**: this commits a file to the repository. Run on a scratch branch to keep `main` clean.

```
gitlab_branches_create(
  project_id="3kirt1/gitlab-mcp-testing",
  branch="changelog-test-scratch",
  ref="main"
)

gitlab_repo_changelog_add(
  project_id="3kirt1/gitlab-mcp-testing",
  version="0.1.0",
  branch="changelog-test-scratch",
  file="CHANGELOG-test.md"
)
```
- Returns a response without error
- Verify the file was created:
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="CHANGELOG-test.md",
  ref_name="changelog-test-scratch"
)
```
- `file_name == "CHANGELOG-test.md"` is returned
- Clean up:
```
gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="changelog-test-scratch")
```

---

## Section 24: Repository — Health

> Repository health requires admin-level access and may return `403 Forbidden` for project-scoped tokens. Both outcomes below are acceptable.

### 24.1 Get health statistics
```
gitlab_repo_health(project_id="3kirt1/gitlab-mcp-testing")
```
- **If token has admin access:** returns a health object with statistics (size, references, objects, etc.)
- **If token lacks access:** tool surfaces a `403` error; no crash

### 24.2 Get with generate flag
```
gitlab_repo_health(project_id="3kirt1/gitlab-mcp-testing", generate=true)
```
- Same as 24.1 — either succeeds with data or returns a `403`; no crash

---

## Section 25: Repository Files — Get

### 25.1 Get a seeded file
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name="main"
)
```
- All repository file universal invariants satisfied
- `file_name == "sample.txt"`
- `file_path == "testing/sample.txt"`
- `ref == "main"`
- `encoding == "base64"`
- Decoding `content` produces `"line one\nline two\nline three\nline four"`
- `size == 38` (or the byte length of the 4-line content)
- `blob_id` is a non-empty SHA — record it for Section 19 blob tests if not already done

### 25.2 Get using HEAD ref
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name="HEAD"
)
```
- Same content as 25.1 (HEAD points to tip of default branch)

### 25.3 Get from a feature branch
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/feature.txt",
  ref_name="mr-test-merge"
)
```
- `file_name == "feature.txt"`, content decodes to `"This file was added in the feature branch."`

### 25.4 Get from a specific commit SHA
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name=<commit_id from seed Step 1a>
)
```
- Content decodes to `"line one\nline two\nline three"` (3 lines — before the 4th was added)

### 25.5 File not found returns error
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="nonexistent/file.txt",
  ref_name="main"
)
```
- Tool surfaces a `404` error; no crash

### 25.6 Numeric project ID
```
gitlab_file_get(
  project_id="82279422",
  file_path="testing/sample.txt",
  ref_name="main"
)
```
- Same result as 25.1

---

## Section 26: Repository Files — Raw

### 26.1 Get raw text content
```
gitlab_file_raw(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt"
)
```
- Returns `{"content": "line one\nline two\nline three\nline four"}`
- `content` is a plain string, not Base64-encoded

### 26.2 Specify a ref
```
gitlab_file_raw(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name="main"
)
```
- Same result as 26.1

### 26.3 Get content at an earlier commit
```
gitlab_file_raw(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name=<commit_id from seed Step 1a>
)
```
- `content == "line one\nline two\nline three"` (3-line version)

### 26.4 Get file from feature branch
```
gitlab_file_raw(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/feature.txt",
  ref_name="mr-test-merge"
)
```
- `content == "This file was added in the feature branch."`

---

## Section 27: Repository Files — Blame

### 27.1 Get full blame history
```
gitlab_file_blame(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name="main"
)
```
- Returns an array of blame ranges
- Exactly 2 blame entries (one for lines 1–3, one for line 4)
- Each entry has a `commit` object with `id`, `author_name`, `committed_date`
- Each entry has a `lines` array of strings
- First entry's `lines` contains `"line one"`, `"line two"`, `"line three"`
- Second entry's `lines` contains `"line four"`
- The `commit.id` in the second entry is more recent than the first (it is the update commit)

### 27.2 Get blame for a specific line range
```
gitlab_file_blame(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  ref_name="main",
  range_start=4,
  range_end=4
)
```
- Returns exactly 1 blame entry
- Entry's `lines` contains only `"line four"`
- `commit.id` matches the commit from seed Step 1b

### 27.3 Blame a single-commit file
```
gitlab_file_blame(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/feature.txt",
  ref_name="mr-test-merge"
)
```
- Returns exactly 1 blame entry (only one commit added this file)
- All lines attributed to the seed Step 3 commit

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
- Returns `{"file_path": "testing/scratch.txt", "branch": "main"}`
- Verify with get:
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  ref_name="main"
)
```
- Content decodes to `"Hello from the test suite."`

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
- Verify with raw:
```
gitlab_file_raw(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch-b64.txt",
  ref_name="main"
)
```
- `content == "Hello from Base64."`

### 28.3 Attempt to create a file that already exists
```
gitlab_file_create(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/sample.txt",
  branch="main",
  commit_message="Should fail",
  content="duplicate"
)
```
- Tool surfaces a `400` error (file already exists); no crash

---

## Section 29: Repository Files — Update

Operate on `testing/scratch.txt` created in Section 28.

### 29.1 Update file content
```
gitlab_file_update(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  branch="main",
  commit_message="Update scratch file",
  content="Updated content."
)
```
- Returns `{"file_path": "testing/scratch.txt", "branch": "main"}`
- Verify with raw: `content == "Updated content."`

### 29.2 Update with last_commit_id guard
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  ref_name="main"
)
```
- Record `last_commit_id` from the response, then:
```
gitlab_file_update(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  branch="main",
  commit_message="Update with commit guard",
  content="Guarded update.",
  last_commit_id=<recorded last_commit_id>
)
```
- Succeeds; `last_commit_id` prevents overwriting concurrent changes

### 29.3 Update a non-existent file returns error
```
gitlab_file_update(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/nonexistent.txt",
  branch="main",
  commit_message="Should fail",
  content="x"
)
```
- Tool surfaces a `400` or `404` error; no crash

---

## Section 30: Repository Files — Delete

### 30.1 Delete the scratch file
```
gitlab_file_delete(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  branch="main",
  commit_message="Remove scratch file"
)
```
- Returns a JSON response with `file_path` and `branch` (or similar confirmation)
- No error

### 30.2 Verify deletion
```
gitlab_file_get(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch.txt",
  ref_name="main"
)
```
- Tool surfaces a `404` error; no crash

### 30.3 Delete the Base64 scratch file
```
gitlab_file_delete(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/scratch-b64.txt",
  branch="main",
  commit_message="Remove base64 scratch file"
)
```
- Returns success; no error

### 30.4 Delete a non-existent file returns error
```
gitlab_file_delete(
  project_id="3kirt1/gitlab-mcp-testing",
  file_path="testing/nonexistent.txt",
  branch="main",
  commit_message="Should fail"
)
```
- Tool surfaces a `400` or `404` error; no crash

---

## Cross-Tool Workflows — Issues

### Workflow A: Find and close a bug
1. `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="opened", labels="bug")` — find open bugs
2. Pick the `iid` of seed-1
3. `gitlab_issues_get(...)` — confirm `state == "opened"`
4. `gitlab_issues_update(..., state_event="close")` — close it
5. `gitlab_issues_get(...)` — confirm `state == "closed"`
6. `gitlab_issues_update(..., state_event="reopen")` — restore state

### Workflow B: Create, update, then delete
1. `gitlab_issues_create(project_id="3kirt1/gitlab-mcp-testing", title="Workflow B scratch")` — record `iid`
2. `gitlab_issues_update(..., title="Workflow B updated", labels="test")`
3. `gitlab_issues_update(..., state_event="close")` — verify closed
4. `gitlab_issues_delete(...)` — verify success message
5. `gitlab_issues_get(...)` — verify 404 error

### Workflow C: Label-based triage audit
1. `gitlab_issues_list(..., state="all", labels="bug")` — list all bugs
2. For each result: confirm `labels` contains `"bug"`
3. `gitlab_issues_list(..., state="closed", labels="bug")` — list resolved bugs
4. Verify seed-3 appears; verify seed-1 does not

### Workflow D: Search and update
1. `gitlab_issues_list(..., state="all", search="README")` — find docs issue
2. Confirm seed-4 is returned
3. `gitlab_issues_update(..., issue_iid=<iid of seed-4>, labels="documentation,needs-review")`
4. `gitlab_issues_get(...)` — confirm both labels present

---

## Cross-Tool Workflows — Branches and Merge Requests

### Workflow E: Find open MR and close it
1. `gitlab_mrs_list(..., state="opened")` — list open MRs; pick mr-seed-1
2. `gitlab_mrs_get(...)` — confirm `state == "opened"`
3. `gitlab_mrs_update(..., state_event="close")` — close it
4. `gitlab_mrs_get(...)` — confirm `state == "closed"`
5. `gitlab_mrs_update(..., state_event="reopen")` — restore state

### Workflow F: Create branch, create MR, update, delete both
1. `gitlab_branches_create(..., branch="workflow-f", ref="main")`
2. `gitlab_mrs_create(..., source_branch="workflow-f", target_branch="main", title="Workflow F scratch")` — record `iid`
3. `gitlab_mrs_update(..., title="Workflow F updated", labels="test", draft=true)`
4. `gitlab_mrs_update(..., state_event="close")`
5. `gitlab_mrs_delete(...)` — verify success message
6. `gitlab_mrs_get(...)` — verify 404 error
7. `gitlab_branches_delete(..., branch="workflow-f")`
8. `gitlab_branches_get(..., branch="workflow-f")` — verify gone

### Workflow G: Draft promotion
1. `gitlab_mrs_list(..., state="opened", draft=true)` — confirm mr-seed-2 appears
2. `gitlab_mrs_update(..., iid of mr-seed-2, draft=false)` — mark as ready
3. `gitlab_mrs_get(...)` — confirm `draft == false`
4. `gitlab_mrs_list(..., state="opened", draft=true)` — confirm mr-seed-2 no longer appears
5. `gitlab_mrs_update(..., draft=true)` — restore draft state

### Workflow H: Create branch, open MR, close MR, delete branch
1. `gitlab_branches_create(..., branch="workflow-h", ref="main")`
2. `gitlab_branches_get(..., branch="workflow-h")` — confirm exists, `merged == false`
3. `gitlab_mrs_create(..., source_branch="workflow-h", target_branch="main", title="Workflow H MR")` — record `iid`
4. `gitlab_mrs_list(..., source_branch="workflow-h")` — confirm MR appears
5. `gitlab_mrs_update(..., state_event="close")` — close it
6. `gitlab_mrs_get(...)` — confirm `state == "closed"`
7. `gitlab_mrs_delete(...)` — delete the MR
8. `gitlab_branches_delete(..., branch="workflow-h")`
9. `gitlab_branches_get(..., branch="workflow-h")` — confirm 404 error

### Workflow I: List branches, find MR source, inspect MR
1. `gitlab_branches_list(..., search="mr-test")` — list MR source branches
2. Pick `mr-test-open`; confirm `merged == false`
3. `gitlab_mrs_list(..., source_branch="mr-test-open")` — find its MR
4. `gitlab_mrs_get(..., iid of mr-seed-1)` — confirm `source_branch == "mr-test-open"`, `state == "opened"`

---

## Cross-Tool Workflows — Repository and Files

### Workflow J: Browse tree → read blob → read raw
1. `gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="main", path="testing")` — list the testing directory; find `sample.txt` and record its `id` (blob SHA)
2. `gitlab_repo_blob_get(project_id="3kirt1/gitlab-mcp-testing", sha=<blob SHA>)` — confirm `encoding == "base64"` and decode `content` to get the 4-line text
3. `gitlab_repo_blob_raw(project_id="3kirt1/gitlab-mcp-testing", sha=<blob SHA>)` — confirm `content` is the plain text without Base64 encoding
4. `gitlab_file_get(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")` — confirm `blob_id` matches the SHA from step 1
5. Verify: blob SHA from tree listing == `blob_id` from file get

### Workflow K: Create file on branch → compare → read → delete
1. `gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-k", ref="main")`
2. `gitlab_file_create(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/wk.txt", branch="workflow-k", commit_message="Add wk.txt", content="workflow k content")`
3. `gitlab_repo_compare(project_id="3kirt1/gitlab-mcp-testing", from="main", to="workflow-k")` — confirm `diffs` contains `testing/wk.txt`
4. `gitlab_file_raw(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/wk.txt", ref_name="workflow-k")` — confirm `content == "workflow k content"`
5. `gitlab_file_blame(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/wk.txt", ref_name="workflow-k")` — confirm 1 blame entry covering all lines
6. `gitlab_file_delete(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/wk.txt", branch="workflow-k", commit_message="Remove wk.txt")`
7. `gitlab_file_get(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/wk.txt", ref_name="workflow-k")` — confirm 404 error
8. `gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="workflow-k")`

### Workflow L: Read, update, blame — full file lifecycle on main
1. `gitlab_file_raw(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")` — record the current content
2. `gitlab_file_blame(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")` — confirm 2 blame entries
3. `gitlab_file_get(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")` — record `last_commit_id`
4. `gitlab_file_update(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", branch="main", commit_message="Add line five", content="line one\nline two\nline three\nline four\nline five", last_commit_id=<recorded>)` — add a fifth line
5. `gitlab_file_blame(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", ref_name="main")` — confirm 3 blame entries now
6. `gitlab_file_update(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", branch="main", commit_message="Restore to four lines", content="line one\nline two\nline three\nline four")` — restore original state

### Workflow M: Compare branches → find merge base → inspect contributors
1. `gitlab_repo_compare(project_id="3kirt1/gitlab-mcp-testing", from="main", to="mr-test-merge")` — record the commit `id` from the first entry in `commits`
2. `gitlab_repo_merge_base(project_id="3kirt1/gitlab-mcp-testing", refs=["main", "mr-test-merge"])` — record the merge base commit `id`
3. Verify: the merge base `id` is NOT the same as the latest commit on `mr-test-merge` (because `mr-test-merge` has commits ahead of the merge base)
4. `gitlab_repo_contributors(project_id="3kirt1/gitlab-mcp-testing", order_by="commits", sort="desc")` — list top contributors
5. Confirm the contributor who created the seed files appears with a `commits` count of at least 3 (seed Steps 1a, 1b, and 3)

---

## Error Handling Checks

**Issues:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999)` | Error message containing `404`; no crash |
| `gitlab_issues_list(project_id="nonexistent-group/nonexistent-repo")` | Error message containing `404`; no crash |
| `gitlab_issues_delete(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999)` | Error message; no crash |
| `gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999, title="x")` | Error message containing `404`; no crash |
| `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="invalid_value")` | GitLab 400 surfaced as tool error; no crash |

**Branches:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_branches_get(project_id="3kirt1/gitlab-mcp-testing", branch="nonexistent-branch")` | Error message containing `404`; no crash |
| `gitlab_branches_list(project_id="nonexistent-group/nonexistent-repo")` | Error message containing `404`; no crash |
| `gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="nonexistent-branch")` | Error message containing `404`; no crash |
| `gitlab_branches_delete(project_id="3kirt1/gitlab-mcp-testing", branch="main")` | Error message containing `403`; no crash |
| `gitlab_branches_create(project_id="3kirt1/gitlab-mcp-testing", branch="new", ref="nonexistent-ref")` | Error message; no crash |

**Merge requests:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_mrs_get(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999)` | Error message containing `404`; no crash |
| `gitlab_mrs_list(project_id="nonexistent-group/nonexistent-repo")` | Error message containing `404`; no crash |
| `gitlab_mrs_delete(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999)` | Error message; no crash |
| `gitlab_mrs_update(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999, title="x")` | Error message containing `404`; no crash |
| `gitlab_mrs_merge(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=999999)` | Error message containing `404`; no crash |

**Repository:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_repo_tree(project_id="3kirt1/gitlab-mcp-testing", ref="nonexistent-branch")` | Error message; no crash |
| `gitlab_repo_blob_get(project_id="3kirt1/gitlab-mcp-testing", sha="0000000000000000000000000000000000000000")` | Error message containing `404`; no crash |
| `gitlab_repo_compare(project_id="3kirt1/gitlab-mcp-testing", from="main", to="nonexistent-branch")` | Error message; no crash |
| `gitlab_repo_merge_base(project_id="3kirt1/gitlab-mcp-testing", refs=["main", "nonexistent-branch"])` | Error message; no crash |

**Repository files:**

| Scenario | Expected behavior |
|---|---|
| `gitlab_file_get(project_id="3kirt1/gitlab-mcp-testing", file_path="nonexistent.txt", ref_name="main")` | Error message containing `404`; no crash |
| `gitlab_file_raw(project_id="3kirt1/gitlab-mcp-testing", file_path="nonexistent.txt")` | Error message containing `404`; no crash |
| `gitlab_file_blame(project_id="3kirt1/gitlab-mcp-testing", file_path="nonexistent.txt", ref_name="main")` | Error message containing `404`; no crash |
| `gitlab_file_create(project_id="3kirt1/gitlab-mcp-testing", file_path="testing/sample.txt", branch="main", commit_message="x", content="y")` | Error message containing `400` (file already exists); no crash |
| `gitlab_file_update(project_id="3kirt1/gitlab-mcp-testing", file_path="nonexistent.txt", branch="main", commit_message="x", content="y")` | Error message; no crash |
| `gitlab_file_delete(project_id="3kirt1/gitlab-mcp-testing", file_path="nonexistent.txt", branch="main", commit_message="x")` | Error message; no crash |

---

## Checklist Summary

Run through these in order for a complete regression pass.

**Setup (all via MCP tools — no git CLI required):**
- [ ] Step 1a: `gitlab_file_create` — `testing/sample.txt` on `main` (3-line version); record commit id
- [ ] Step 1b: `gitlab_file_update` — `testing/sample.txt` on `main` (4-line version)
- [ ] Step 2: `gitlab_branches_create` × 6 — `mr-test-open`, `mr-test-draft`, `mr-test-close`, `mr-test-merge`, `mr-test-scratch`, `branch-test-1`
- [ ] Step 3: `gitlab_file_create` — `testing/feature.txt` on `mr-test-merge`
- [ ] Step 4: Issue seed data — 5 issues; seed-3 and seed-5 closed
- [ ] Step 5: MR seed data — 4 MRs; mr-seed-3 closed

**Issues:**
- [ ] Universal invariants: `iid`, `id`, `project_id`, `state`, `title`, `web_url` on every object
- [ ] Section 1: List — default, all, closed, label, multi-label AND, search match/miss, order asc/desc, numeric ID
- [ ] Section 2: Get — by IID, closed, due date, numeric ID
- [ ] Section 3: Create — title only, all optional fields, Markdown description
- [ ] Section 4: Update — title, description, labels, close, reopen, due date, no-op (400)
- [ ] Section 5: Delete — success message; subsequent get returns 404
- [ ] Section 6: Pagination — page 1, page 2, beyond-last, large per_page
- [ ] Workflows A–D

**Branches:**
- [ ] Universal invariants: `name`, `commit.id`, `merged`, `protected`, `web_url`
- [ ] Section 7: List — no filter, search, regex, regex miss, search miss, pagination, numeric ID
- [ ] Section 8: Get — default (protected), seeded (unprotected), numeric ID
- [ ] Section 9: Create — from main, from another branch, verify with get, numeric ID
- [ ] Section 10: Delete — success message, subsequent get 404, protected 403, scratch cleanup
- [ ] Section 11: Delete Merged — run after Section 17

**Merge Requests:**
- [ ] Universal invariants: `iid`, `id`, `project_id`, `state`, `title`, `web_url`, `source_branch`, `target_branch`, `author`
- [ ] Section 12: List — default, all, closed, source_branch, target_branch, label, draft, search, numeric ID, pagination
- [ ] Section 13: Get — open, draft, closed, numeric ID
- [ ] Section 14: Create — required only, all optional, Markdown description
- [ ] Section 15: Update — title, description, labels, draft, un-draft, close, reopen
- [ ] Section 16: Delete — success message; subsequent get 404
- [ ] Section 17: Merge — successful; attempt on closed MR errors
- [ ] Workflows E–I

**Repository:**
- [ ] Universal invariants: `id`, `name`, `type`, `path`, `mode` on every tree entry
- [ ] Section 18: Tree — root, subdirectory, recursive, feature branch, non-existent path, pagination
- [ ] Section 19: Blob get (metadata + Base64) and raw (plain text JSON); invalid SHA errors
- [ ] Section 20: Compare — non-empty diff, empty diff, straight, unidiff
- [ ] Section 21: Contributors — list, order by commits desc, order by name asc, scoped to ref
- [ ] Section 22: Merge base — two branches, three refs, invalid ref errors
- [ ] Section 23: Changelog get (empty notes accepted); optional changelog add to scratch branch
- [ ] Section 24: Health — success or 403; no crash in either case

**Repository Files:**
- [ ] Universal invariants: `file_name`, `file_path`, `size`, `encoding`, `content`, `content_sha256`, `ref`, `blob_id`, `commit_id`, `last_commit_id`
- [ ] Section 25: Get — seeded file, HEAD ref, feature branch, historic commit SHA, 404 miss, numeric ID
- [ ] Section 26: Raw — default ref, explicit ref, historic commit, feature branch file
- [ ] Section 27: Blame — full history (2 entries), line range (line 4 only), single-commit file
- [ ] Section 28: Create — plain text, Base64 encoding, duplicate errors 400
- [ ] Section 29: Update — new content, with last_commit_id guard, non-existent file errors
- [ ] Section 30: Delete — success, subsequent get 404, non-existent file errors
- [ ] Workflows J–M
