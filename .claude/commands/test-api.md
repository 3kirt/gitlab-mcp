---
model: claude-haiku-4-5-20251001
---
Run the testing protocol for the API area given in $ARGUMENTS against the live test project.

The GitLab MCP tools are already loaded and available as tool calls in your context. Use them directly — do not run any shell commands, do not check whether a binary is built, and do not use curl or the Bash tool. Every interaction with GitLab must go through an MCP tool call (e.g., `gitlab_issues_list`, `gitlab_emoji_reactions_issues_create`).

**Test project:** `3kirt1/gitlab-mcp-testing` (numeric ID `82279422`). **Group (epics):** `3kirt1`.

## Step 1 — Identify the target area

Map `$ARGUMENTS` (case-insensitive, hyphens or underscores) to the section range below. If the argument doesn't match any row, print the table and stop.

| Argument | Sections in testing-protocol.md |
|---|---|
| `issues` | 1–6 |
| `branches` | 7–11 |
| `mrs`, `merge_requests` | 12–17 |
| `mr_approvals` | 17B |
| `repository` | 18–24 |
| `repository_files`, `files` | 25–30 |
| `discussions`, `mr_discussions` | 31–37 |
| `issue_notes` | 38–42 |
| `epics` | 43–47 |
| `epic_issues` | 47B |
| `issue_links` | 48–51 |
| `metadata` | 52 |
| `pipeline_schedules` | 53–59 |
| `search` | 60–62 |
| `snippets` | 63–70 |
| `emoji_reactions` | 71–76 + Workflow M |
| `groups` | 77–78 |
| `projects` | 79 |
| `runners` | 80–86 + Workflow N |
| `issue_discussions` | 87–92 + Workflow O |

## Step 2 — Read the protocol

Read `docs/testing-protocol.md`. Extract:
- The "Universal Invariants" block for the target resource type(s)
- Every numbered section and workflow in the target range

Read the full sections — you will need the exact tool names, parameters, and assertions.

## Step 3 — Discover seed data

Call MCP tools to resolve placeholder IDs before running tests. Only fetch what the target sections actually need. Common lookups:

| Placeholder | How to resolve |
|---|---|
| `<iid of seed-1>` | `gitlab_issues_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="Bug: login page crashes")` → first result's `iid` |
| `<iid of mr-seed-1>` | `gitlab_mrs_list(project_id="3kirt1/gitlab-mcp-testing", state="all", search="Fix: correct off-by-one")` → first result's `iid` |
| `<id of note-issue-seed-1>` | `gitlab_issues_notes_list(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>)` → find note whose body starts with "Seeded note" → its `id` |
| `<id of note-seed-1>` (MR discussion note) | `gitlab_mrs_discussions_list(project_id="3kirt1/gitlab-mcp-testing", merge_request_iid=<iid of mr-seed-1>)` → find `disc-seed-1` whose first note body starts with "Seeded review comment" → `notes[0].id` |
| `<issue-disc-seed-1>` / `<issue-disc-note-seed-1>` | `gitlab_issues_discussions_list(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>)` → find discussion whose `notes[0].body` starts with "Seeded thread for issue discussion testing" → its `id` (and `notes[0].id` for the note) |
| `<runner-id>` | `gitlab_runners_list()` → first item's `id`. If empty, mark Sections 82–84 SKIP |
| `<seed-4-global-id>` (epic-issue assign) | `gitlab_issues_get(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-4>)` → top-level `id` |
| `<award-seed-issue>` | `gitlab_emoji_reactions_issues_list(project_id="3kirt1/gitlab-mcp-testing", issue_iid=<iid of seed-1>)` → find reaction with `name=="thumbsup"` → its `id` |
| `<award-seed-mr>` | `gitlab_emoji_reactions_mrs_list(...)` on `mr-seed-1` → thumbsup reaction id |
| `<award-seed-issue-note>` | `gitlab_emoji_reactions_issue_notes_list(...)` on `seed-1` note → thumbsup reaction id |
| `<proj-snippet-seed-1>` | `gitlab_snippets_list(...)` or `gitlab_snippets_all_list(...)` → find project snippet in the test project |
| Epic IIDs | `gitlab_epics_list(group_id="3kirt1")` → match by title |
| Pipeline schedule IDs | `gitlab_pipeline_schedules_list(project_id="3kirt1/gitlab-mcp-testing")` |

If a seed item is not found, note it as MISSING and skip the test cases that depend on it. Continue with remaining tests.

## Step 4 — Execute each test case

Work through sections sequentially. For each numbered subsection (e.g., 71.1, 71.2):

1. Make an MCP tool call with real IDs substituted for all `<placeholders>` — never use Bash or curl
2. Check the response against the stated assertions (field presence, field values, response shape, error codes)
3. Record intermediate IDs returned by create calls (e.g., `award-issue-tada`) for use in subsequent steps
4. Mark the case PASS or FAIL with a one-line reason

Keep going even if individual cases fail. If a step returns an error that makes subsequent steps impossible (e.g., create failed so delete has no ID), mark the dependent steps as SKIP with reason "depends on failed step X".

**Assertion shortcuts:**
- "Universal invariants hold" → check `id`, `name`, `user`, `created_at`, `awardable_id`, `awardable_type` are all present and non-null
- "Returns success text message" → response is a string like `"X deleted"`, not a JSON object
- "Returns at least one item" → `items` array is non-empty

## Step 5 — Report results

After all cases, print:

```
## Test Results: <area> (<date>)

| Section | Description | Result | Notes |
|---|---|---|---|
| 71.1 | List issue emoji | PASS | 2 items |
| 71.2 | Get issue emoji | PASS | name=thumbsup, awardable_type=Issue |
| 71.3 | Create issue emoji | PASS | id=<award-issue-tada> |
| 71.4 | Delete issue emoji | PASS | deletion confirmed, follow-up list clean |
...

**Summary: X/Y passed** (Z skipped)
```

If any cases failed, append a "Failures" section with the error detail for each.
