# GitLab MCP Testing Protocol

This document describes how to use the MCP tools to verify that all Issues API functionality is working correctly against the test project `3kirt1/gitlab-mcp-testing` (numeric ID `82279422`) seeded with the issues defined below.

---

## Universal Invariants

Every response from every tool must satisfy these properties. Check them on every call.

| Property | What to verify |
|---|---|
| `iid` present | Every issue object has a project-scoped `iid` (the number shown in the GitLab UI) |
| `id` present | Every issue object has a global GitLab `id` |
| `project_id` present | Every issue object has a `project_id` matching the requested project |
| `state` value | `state` is always `"opened"` or `"closed"` — never absent or `null` |
| `title` present | Every issue object has a non-empty `title` |
| `web_url` present | Every issue has a `web_url` pointing to the GitLab UI |
| List is an array | List responses are JSON arrays, not objects |
| Delete confirmation | Delete returns a success text message, not a JSON object |

---

## Seed Data

Before running the test suite, create the following issues in `3kirt1/gitlab-mcp-testing`. Record the `iid` returned for each — the protocol uses these as ground truth for assertions.

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

## Cross-Tool Workflows

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

## Error Handling Checks

| Scenario | Expected behavior |
|---|---|
| `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999)` | Tool returns an error message containing `404`; no crash |
| `gitlab_issues_list(project_id="nonexistent-group/nonexistent-repo")` | Tool returns an error message containing `404`; no crash |
| `gitlab_issues_delete(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999)` | Tool returns an error message; no crash |
| `gitlab_issues_update(project_id="3kirt1/gitlab-mcp-testing", issue_iid=999999, title="x")` | Tool returns an error message containing `404`; no crash |
| `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="invalid_value")` | GitLab 400 surfaced as tool error; no crash |

---

## Checklist Summary

Run through these in order for a complete regression pass:

- [ ] Seed data created: 5 issues in `3kirt1/gitlab-mcp-testing`, seed-3 and seed-5 closed
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
- [ ] Error handling: 404 on get, 404 on nonexistent project, 404 on delete, 404 on update, invalid state param
