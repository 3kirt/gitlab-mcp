//! Live integration tests for the Issues domain — a deterministic, scriptable
//! replacement for the LLM-driven `docs/testing-protocol.md` §1–6.
//!
//! These run against a *real* GitLab instance and verify the one thing wiremock
//! unit tests cannot: fidelity to the actual API (param names, body shapes,
//! response shapes, the slimming/envelope the server emits). They are
//! self-seeding and self-cleaning — every test creates the issues it needs and
//! deletes them in a teardown — so they are idempotent and repeatable, with no
//! reliance on pre-seeded state.
//!
//! Run with:
//! ```sh
//! GITLAB_URL=https://gitlab.com \
//! GITLAB_TOKEN=glpat-xxx \
//! GITLAB_TEST_PROJECT=3kirt1/gitlab-mcp-testing \
//!   cargo test --features live-tests -- --test-threads=1
//! ```
//! Absent `GITLAB_URL`/`GITLAB_TOKEN`, each test prints a skip notice and
//! passes, so the feature is safe to enable in CI without credentials.

use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::slim;
use crate::tools::{
    PaginationParams, branches, issue_discussions, issue_notes, issues, merge_requests,
    repository_files,
};

// --------------------------------------------------------------------------
// Harness
// --------------------------------------------------------------------------

/// A live client plus the project under test, or `None` when credentials are
/// absent (so tests skip rather than fail). Every test begins with
/// `let Some(env) = live_env() else { return };`.
struct LiveEnv {
    client: GitlabClient,
    project: String,
}

fn live_env() -> Option<LiveEnv> {
    let url = std::env::var("GITLAB_URL").ok()?;
    let token = std::env::var("GITLAB_TOKEN").ok()?;
    if url.is_empty() || token.is_empty() {
        return None;
    }
    let project =
        std::env::var("GITLAB_TEST_PROJECT").unwrap_or_else(|_| "3kirt1/gitlab-mcp-testing".into());
    let client = GitlabClient::new(url, token).expect("build live client");
    Some(LiveEnv { client, project })
}

macro_rules! skip_unless_live {
    () => {
        match live_env() {
            Some(env) => env,
            None => {
                eprintln!("SKIP: set GITLAB_URL + GITLAB_TOKEN to run live tests");
                return;
            }
        }
    };
}

/// A short unique tag so concurrent/repeated runs never collide on titles or
/// labels, and so a crashed run's leftovers are identifiable.
fn run_tag() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("mcp-livetest-{nanos}")
}

/// Build the pagination triple without spelling out all three fields each time.
fn pg(page: Option<u64>, per_page: Option<u64>) -> PaginationParams {
    PaginationParams {
        page,
        per_page,
        fetch_all: None,
    }
}

/// `IssuesListParams` with everything defaulted; callers set the fields a given
/// case exercises. Saves repeating ~12 `None`s per test.
fn list_params(project: &str) -> issues::IssuesListParams {
    issues::IssuesListParams {
        project_id: project.to_string(),
        state: None,
        labels: None,
        search: None,
        scope: None,
        assignee_id: None,
        author_id: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
        order_by: None,
        sort: None,
        pagination: pg(None, None),
    }
}

/// Create an issue and return its `iid`. The created payload is also returned
/// for callers that want to assert on the create response directly.
async fn create_issue(env: &LiveEnv, p: issues::IssueCreateParams) -> (u64, Value) {
    let created = issues::issue_create(&env.client, p)
        .await
        .expect("issue_create");
    let iid = created["iid"].as_u64().expect("created issue has iid");
    (iid, created)
}

async fn delete_issue(env: &LiveEnv, iid: u64) {
    let _ = issues::issue_delete(
        &env.client,
        issues::IssueDeleteParams {
            project_id: env.project.clone(),
            issue_iid: iid,
        },
    )
    .await;
}

/// Fetch an issue through the same path the server uses: domain function +
/// `slim_get`. Asserts on the slimmed shape an MCP client would actually see.
async fn get_issue_slimmed(env: &LiveEnv, iid: u64) -> Value {
    let raw = issues::issue_get(
        &env.client,
        issues::IssueGetParams {
            project_id: env.project.clone(),
            issue_iid: iid,
        },
    )
    .await
    .expect("issue_get");
    slim::slim_get(raw)
}

/// Run a list request through the server's path: domain function + `slim_list`.
/// Returns the slimmed items array; pagination meta is asserted separately.
async fn list_issues_slimmed(env: &LiveEnv, p: issues::IssuesListParams) -> (Value, u64) {
    let (body, meta) = issues::issues_list(&env.client, p).await.expect("issues_list");
    (slim::slim_list(body), meta.per_page.unwrap_or(0))
}

// --------------------------------------------------------------------------
// Invariant assertions (the protocol's "Universal Invariants" as code)
// --------------------------------------------------------------------------

fn assert_no_stripped_keys(v: &Value) {
    let obj = v.as_object().expect("object");
    assert!(obj.get("_links").is_none(), "_links must be stripped");
    assert!(
        obj.get("references").is_none(),
        "references must be stripped"
    );
}

fn assert_nonempty_str(v: &Value, key: &str) {
    let s = v.get(key).and_then(Value::as_str).unwrap_or("");
    assert!(!s.is_empty(), "{key} must be a non-empty string");
}

/// Invariants every single-issue GET must satisfy.
fn assert_issue_get_invariants(item: &Value) {
    assert!(item.get("iid").and_then(Value::as_u64).is_some(), "iid");
    assert_nonempty_str(item, "title");
    assert_nonempty_str(item, "web_url");
    let state = item["state"].as_str().unwrap_or("");
    assert!(
        state == "opened" || state == "closed",
        "state must be opened|closed, got {state:?}"
    );
    assert_no_stripped_keys(item);
    // Enrichment from issue_get: both must always be present arrays.
    assert!(item["linked_issues"].is_array(), "linked_issues is array");
    assert!(item["closed_by"].is_array(), "closed_by is array");
}

/// Invariants every issue *list item* must satisfy (note: heavier slimming —
/// `description` is stripped from list responses).
fn assert_issue_list_item_invariants(item: &Value) {
    assert!(item.get("iid").and_then(Value::as_u64).is_some(), "iid");
    assert_nonempty_str(item, "web_url");
    assert_no_stripped_keys(item);
    assert!(
        item.get("description").is_none(),
        "description must be stripped from list items"
    );
}

// --------------------------------------------------------------------------
// §3 / §2 / §4 / §5 — Create, Get, Update, Delete lifecycle
// --------------------------------------------------------------------------

#[tokio::test]
async fn issues_create_get_update_delete_lifecycle() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // §3.1 Create with title only.
    let (iid, created) = create_issue(
        &env,
        issues::IssueCreateParams {
            project_id: env.project.clone(),
            title: format!("{tag} title-only"),
            description: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await;
    assert_eq!(created["state"], "opened");

    // §3.2 Create with optional fields (description, labels, due date).
    let (iid_full, created_full) = create_issue(
        &env,
        issues::IssueCreateParams {
            project_id: env.project.clone(),
            title: format!("{tag} full"),
            description: Some("**bold** body".into()),
            labels: Some(format!("{tag}-a,{tag}-b")),
            assignee_ids: None,
            milestone_id: None,
            due_date: Some("2030-01-15".into()),
            weight: None,
        },
    )
    .await;
    assert_eq!(created_full["due_date"], "2030-01-15");
    let labels = created_full["labels"].as_array().expect("labels array");
    assert_eq!(labels.len(), 2, "both labels applied on create");

    // §2 Get — single-get keeps description, embeds linked_issues/closed_by.
    let got = get_issue_slimmed(&env, iid_full).await;
    assert_issue_get_invariants(&got);
    assert_eq!(got["description"], "**bold** body");

    // §4.1 Update title.
    let updated = issues::issue_update(
        &env.client,
        issues::IssueUpdateParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            title: Some(format!("{tag} retitled")),
            description: None,
            state_event: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await
    .expect("update title");
    assert_eq!(updated["title"], format!("{tag} retitled"));

    // §4.3 Replace labels.
    let relabeled = issues::issue_update(
        &env.client,
        issues::IssueUpdateParams {
            project_id: env.project.clone(),
            issue_iid: iid_full,
            title: None,
            description: None,
            state_event: None,
            labels: Some(format!("{tag}-c")),
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await
    .expect("replace labels");
    let relabeled_labels = relabeled["labels"].as_array().expect("labels array");
    assert_eq!(relabeled_labels.len(), 1, "labels replaced, not appended");
    assert_eq!(relabeled_labels[0], format!("{tag}-c"));

    // §4.4 Close, §4.5 reopen via state_event.
    let closed = issues::issue_update(
        &env.client,
        issues::IssueUpdateParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            title: None,
            description: None,
            state_event: Some("close".into()),
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await
    .expect("close");
    assert_eq!(closed["state"], "closed");

    // §5 Delete both; a follow-up get must surface a 404.
    delete_issue(&env, iid).await;
    delete_issue(&env, iid_full).await;

    let err = issues::issue_get(
        &env.client,
        issues::IssueGetParams {
            project_id: env.project.clone(),
            issue_iid: iid,
        },
    )
    .await
    .expect_err("get after delete must 404");
    assert!(
        matches!(err, GitlabError::Api { status, .. } if status.as_u16() == 404),
        "expected 404 after delete, got {err:?}"
    );
}

// --------------------------------------------------------------------------
// §1 / §6 — List filters, search, sort, pagination
// --------------------------------------------------------------------------

#[tokio::test]
async fn issues_list_filters_search_sort_pagination() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let label = format!("{tag}-grp");

    // Seed three labelled issues so we have a deterministic working set.
    let mut iids = Vec::new();
    for n in 1..=3 {
        let (iid, _) = create_issue(
            &env,
            issues::IssueCreateParams {
                project_id: env.project.clone(),
                title: format!("{tag} item {n}"),
                description: None,
                labels: Some(label.clone()),
                assignee_ids: None,
                milestone_id: None,
                due_date: None,
                weight: None,
            },
        )
        .await;
        iids.push(iid);
    }

    // §1.4 Filter by label — every returned item satisfies list invariants and
    // the description is stripped (the key fidelity check vs single-get).
    let mut p = list_params(&env.project);
    p.labels = Some(label.clone());
    p.state = Some("all".into());
    let (items, _) = list_issues_slimmed(&env, p).await;
    let arr = items.as_array().expect("items array");
    assert_eq!(arr.len(), 3, "label filter returns exactly the seeded set");
    for item in arr {
        assert_issue_list_item_invariants(item);
    }

    // §1.6 Search by the unique tag keyword.
    let mut p = list_params(&env.project);
    p.search = Some(tag.clone());
    p.state = Some("all".into());
    let (found, _) = list_issues_slimmed(&env, p).await;
    assert_eq!(found.as_array().unwrap().len(), 3, "search by tag finds all");

    // §1.7 Sort ascending by created_at — IIDs must be monotonically increasing.
    let mut p = list_params(&env.project);
    p.labels = Some(label.clone());
    p.state = Some("all".into());
    p.order_by = Some("created_at".into());
    p.sort = Some("asc".into());
    let (sorted, _) = list_issues_slimmed(&env, p).await;
    let sorted_iids: Vec<u64> = sorted
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["iid"].as_u64().unwrap())
        .collect();
    let mut expect = sorted_iids.clone();
    expect.sort_unstable();
    assert_eq!(sorted_iids, expect, "ascending created_at => ascending iid");

    // §6 Pagination — per_page=1 returns a single item and echoes per_page.
    let mut p = list_params(&env.project);
    p.labels = Some(label.clone());
    p.state = Some("all".into());
    p.pagination = pg(Some(1), Some(1));
    let (page1, per_page) = list_issues_slimmed(&env, p).await;
    assert_eq!(page1.as_array().unwrap().len(), 1, "per_page=1 => one item");
    assert_eq!(per_page, 1, "X-Per-Page header echoed in meta");

    for iid in iids {
        delete_issue(&env, iid).await;
    }
}

// --------------------------------------------------------------------------
// §2.4–2.7 — Embedded linked_issues (positive and negative)
// --------------------------------------------------------------------------

#[tokio::test]
async fn issue_get_embeds_linked_issues() {
    let env = skip_unless_live!();
    let tag = run_tag();

    let (src, _) = create_issue(
        &env,
        issues::IssueCreateParams {
            project_id: env.project.clone(),
            title: format!("{tag} source"),
            description: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await;
    let (dst, _) = create_issue(
        &env,
        issues::IssueCreateParams {
            project_id: env.project.clone(),
            title: format!("{tag} target"),
            description: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await;

    // §2.5 Negative case: a fresh issue embeds an empty linked_issues array.
    let before = get_issue_slimmed(&env, src).await;
    assert_eq!(before["linked_issues"], serde_json::json!([]));

    // Link src -> dst. We use "relates_to" rather than "blocks"/"is_blocked_by"
    // because the latter are gated behind Premium/Ultimate — on a Free-tier
    // instance GitLab rejects them with 403 "Blocked issues not available for
    // current license". relates_to is available on every tier.
    issues::issue_link_create(
        &env.client,
        issues::IssueLinkCreateParams {
            project_id: env.project.clone(),
            issue_iid: src,
            target_project_id: env.project.clone(),
            target_issue_iid: dst,
            link_type: Some("relates_to".into()),
        },
    )
    .await
    .expect("create link");

    // §2.4 Positive case: linked_issues now carries the relationship.
    let after = get_issue_slimmed(&env, src).await;
    let links = after["linked_issues"].as_array().expect("linked_issues");
    assert_eq!(links.len(), 1, "one linked issue");
    assert_eq!(links[0]["iid"], dst);
    assert_eq!(links[0]["link_type"], "relates_to");

    delete_issue(&env, src).await;
    delete_issue(&env, dst).await;
}

// --------------------------------------------------------------------------
// Issue Notes — flat comments on an issue
// --------------------------------------------------------------------------

/// Invariants for a note object (single-get / create / list item).
fn assert_note_invariants(note: &Value) {
    assert!(note.get("id").and_then(Value::as_u64).is_some(), "note id");
    assert!(note.get("body").and_then(Value::as_str).is_some(), "body");
    assert_no_stripped_keys(note);
    assert_user_collapsed(&note["author"]);
}

#[tokio::test]
async fn issue_notes_crud() {
    let env = skip_unless_live!();
    let tag = run_tag();

    let (iid, _) = create_issue(
        &env,
        issues::IssueCreateParams {
            project_id: env.project.clone(),
            title: format!("{tag} notes"),
            description: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await;

    // Create — the create response is slimmed via slim_get on the server.
    let created = slim::slim_get(
        issue_notes::issue_note_create(
            &env.client,
            issue_notes::IssueNoteCreateParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                body: format!("{tag} first comment"),
                created_at: None,
            },
        )
        .await
        .expect("create note"),
    );
    assert_note_invariants(&created);
    assert_eq!(created["body"], format!("{tag} first comment"));
    let note_id = created["id"].as_u64().unwrap();

    // Get the note back.
    let got = slim::slim_get(
        issue_notes::issue_note_get(
            &env.client,
            issue_notes::IssueNoteGetParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                note_id,
            },
        )
        .await
        .expect("get note"),
    );
    assert_note_invariants(&got);
    assert_eq!(got["id"].as_u64().unwrap(), note_id);

    // List — our note must appear and every item satisfies note invariants.
    let (body, _) = issue_notes::issue_notes_list(
        &env.client,
        issue_notes::IssueNotesListParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            order_by: None,
            sort: None,
            pagination: pg(None, None),
        },
    )
    .await
    .expect("list notes");
    let items = slim::slim_list(body);
    let arr = items.as_array().expect("items array");
    for item in arr {
        assert_note_invariants(item);
    }
    assert!(
        arr.iter().any(|n| n["id"].as_u64() == Some(note_id)),
        "created note must appear in the list"
    );

    // Update the body.
    let updated = slim::slim_get(
        issue_notes::issue_note_update(
            &env.client,
            issue_notes::IssueNoteUpdateParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                note_id,
                body: format!("{tag} edited comment"),
            },
        )
        .await
        .expect("update note"),
    );
    assert_eq!(updated["body"], format!("{tag} edited comment"));

    // Delete; a follow-up get must 404.
    issue_notes::issue_note_delete(
        &env.client,
        issue_notes::IssueNoteDeleteParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            note_id,
        },
    )
    .await
    .expect("delete note");
    let err = issue_notes::issue_note_get(
        &env.client,
        issue_notes::IssueNoteGetParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            note_id,
        },
    )
    .await
    .expect_err("get after delete must 404");
    assert!(
        matches!(err, GitlabError::Api { status, .. } if status.as_u16() == 404),
        "expected 404 after delete, got {err:?}"
    );

    delete_issue(&env, iid).await;
}

// --------------------------------------------------------------------------
// Issue Discussions — threaded comments (a discussion wraps one or more notes)
// --------------------------------------------------------------------------

/// Count the notes inside a (slimmed) discussion object.
fn discussion_note_count(disc: &Value) -> usize {
    disc["notes"].as_array().map(Vec::len).unwrap_or(0)
}

#[tokio::test]
async fn issue_discussions_crud() {
    let env = skip_unless_live!();
    let tag = run_tag();

    let (iid, _) = create_issue(
        &env,
        issues::IssueCreateParams {
            project_id: env.project.clone(),
            title: format!("{tag} discussions"),
            description: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await;

    // Start a discussion thread. The id is a hex string, not an integer.
    let created = slim::slim_get(
        issue_discussions::issue_discussion_create(
            &env.client,
            issue_discussions::IssueDiscussionCreateParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                body: format!("{tag} thread root"),
                created_at: None,
            },
        )
        .await
        .expect("create discussion"),
    );
    let discussion_id = created["id"].as_str().expect("discussion id is a string").to_string();
    assert_eq!(discussion_note_count(&created), 1, "root note present");
    assert_eq!(created["notes"][0]["body"], format!("{tag} thread root"));

    // Reply — adds a second note to the same thread.
    let reply = slim::slim_get(
        issue_discussions::issue_discussion_note_create(
            &env.client,
            issue_discussions::IssueDiscussionNoteCreateParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                discussion_id: discussion_id.clone(),
                body: format!("{tag} reply"),
                created_at: None,
            },
        )
        .await
        .expect("reply to discussion"),
    );
    let reply_id = reply["id"].as_u64().expect("reply note id");
    assert_eq!(reply["body"], format!("{tag} reply"));

    // Get the thread — both notes present, author collapsed on each.
    let got = slim::slim_get(
        issue_discussions::issue_discussion_get(
            &env.client,
            issue_discussions::IssueDiscussionGetParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                discussion_id: discussion_id.clone(),
            },
        )
        .await
        .expect("get discussion"),
    );
    assert_eq!(got["id"].as_str(), Some(discussion_id.as_str()));
    assert_eq!(discussion_note_count(&got), 2, "root + reply");
    for note in got["notes"].as_array().unwrap() {
        assert_note_invariants(note);
    }

    // List — our discussion must appear.
    let (body, _) = issue_discussions::issue_discussions_list(
        &env.client,
        issue_discussions::IssueDiscussionsListParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            pagination: pg(None, None),
        },
    )
    .await
    .expect("list discussions");
    let discussions = slim::slim_list(body);
    assert!(
        discussions
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"].as_str() == Some(discussion_id.as_str())),
        "created discussion must appear in the list"
    );

    // Edit the reply note within the thread.
    let edited = slim::slim_get(
        issue_discussions::issue_discussion_note_update(
            &env.client,
            issue_discussions::IssueDiscussionNoteUpdateParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                discussion_id: discussion_id.clone(),
                note_id: reply_id,
                body: format!("{tag} reply edited"),
            },
        )
        .await
        .expect("edit discussion note"),
    );
    assert_eq!(edited["body"], format!("{tag} reply edited"));

    // Delete the reply — thread drops back to a single note.
    issue_discussions::issue_discussion_note_delete(
        &env.client,
        issue_discussions::IssueDiscussionNoteDeleteParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            discussion_id: discussion_id.clone(),
            note_id: reply_id,
        },
    )
    .await
    .expect("delete discussion note");
    let after = slim::slim_get(
        issue_discussions::issue_discussion_get(
            &env.client,
            issue_discussions::IssueDiscussionGetParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                discussion_id: discussion_id.clone(),
            },
        )
        .await
        .expect("get discussion after delete"),
    );
    assert_eq!(discussion_note_count(&after), 1, "reply removed, root remains");

    delete_issue(&env, iid).await;
}

// ==========================================================================
// Merge Requests
//
// MRs need a source branch that differs from the target. We use
// `file_create` with `start_branch` to create the branch *and* a
// differentiating commit in one call, then tear the branch down afterward.
// Merge tests target a throwaway base branch so `main` is never modified.
// ==========================================================================

// --------------------------------------------------------------------------
// MR harness
// --------------------------------------------------------------------------

/// Create a branch off `source_ref` carrying one new file, so an MR opened from
/// it against `source_ref` has a real diff. Returns the branch name.
async fn seed_branch_with_file(env: &LiveEnv, branch: &str, source_ref: &str) -> String {
    repository_files::file_create(
        &env.client,
        repository_files::FileCreateParams {
            project_id: env.project.clone(),
            file_path: format!("livetest/{branch}.txt"),
            branch: branch.to_string(),
            commit_message: format!("seed {branch}"),
            content: format!("content for {branch}\n"),
            encoding: None,
            author_name: None,
            author_email: None,
            execute_filemode: None,
            // Branch the new ref off an existing one in the same call.
            start_branch: Some(source_ref.to_string()),
        },
    )
    .await
    .expect("seed branch with file");
    branch.to_string()
}

async fn delete_branch(env: &LiveEnv, branch: &str) {
    let _ = branches::branch_delete(
        &env.client,
        branches::BranchDeleteParams {
            project_id: env.project.clone(),
            branch: branch.to_string(),
        },
    )
    .await;
}

async fn create_mr(env: &LiveEnv, p: merge_requests::MrCreateParams) -> (u64, Value) {
    let created = merge_requests::mr_create(&env.client, p)
        .await
        .expect("mr_create");
    let iid = created["iid"].as_u64().expect("created MR has iid");
    (iid, created)
}

async fn delete_mr(env: &LiveEnv, iid: u64) {
    let _ = merge_requests::mr_delete(
        &env.client,
        merge_requests::MrDeleteParams {
            project_id: env.project.clone(),
            merge_request_iid: iid,
        },
    )
    .await;
}

/// MR create params with sensible defaults; callers override the fields a case
/// exercises. `squash`/`remove_source_branch` mirror the tool's `default_true`.
fn mr_create_params(
    env: &LiveEnv,
    source_branch: &str,
    target_branch: &str,
    title: &str,
) -> merge_requests::MrCreateParams {
    merge_requests::MrCreateParams {
        project_id: env.project.clone(),
        source_branch: source_branch.to_string(),
        target_branch: target_branch.to_string(),
        title: title.to_string(),
        description: None,
        assignee_id: None,
        reviewer_ids: None,
        labels: None,
        milestone_id: None,
        squash: true,
        remove_source_branch: true,
        draft: None,
    }
}

/// `MrUpdateParams` with everything defaulted; callers set the fields under test.
fn mr_update_params(env: &LiveEnv, iid: u64) -> merge_requests::MrUpdateParams {
    merge_requests::MrUpdateParams {
        project_id: env.project.clone(),
        merge_request_iid: iid,
        title: None,
        description: None,
        state_event: None,
        target_branch: None,
        assignee_id: None,
        reviewer_ids: None,
        labels: None,
        milestone_id: None,
        squash: None,
        draft: None,
    }
}

/// `MrsListParams` with everything defaulted; callers set the fields under test.
fn mrs_list_params(project: &str) -> merge_requests::MrsListParams {
    merge_requests::MrsListParams {
        project_id: project.to_string(),
        state: None,
        source_branch: None,
        target_branch: None,
        author_id: None,
        assignee_id: None,
        reviewer_id: None,
        labels: None,
        search: None,
        draft: None,
        scope: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
        order_by: None,
        sort: None,
        pagination: pg(None, None),
    }
}

/// Wait until GitLab finishes its async mergeability check. A freshly created
/// MR starts as `unchecked`/`checking`; merging before it reaches
/// `can_be_merged` yields `405 Method Not Allowed`. Polls with a bounded budget.
async fn wait_until_mergeable(env: &LiveEnv, iid: u64) {
    for _ in 0..30 {
        let mr = merge_requests::mr_get(
            &env.client,
            merge_requests::MrGetParams {
                project_id: env.project.clone(),
                merge_request_iid: iid,
            },
        )
        .await
        .expect("mr_get while polling merge status");
        match mr["merge_status"].as_str().unwrap_or("") {
            "can_be_merged" => return,
            "cannot_be_merged" => panic!("MR {iid} reported cannot_be_merged"),
            _ => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
        }
    }
    panic!("MR {iid} never became mergeable within the polling budget");
}

async fn get_mr_slimmed(env: &LiveEnv, iid: u64) -> Value {
    let raw = merge_requests::mr_get(
        &env.client,
        merge_requests::MrGetParams {
            project_id: env.project.clone(),
            merge_request_iid: iid,
        },
    )
    .await
    .expect("mr_get");
    slim::slim_get(raw)
}

async fn list_mrs_slimmed(env: &LiveEnv, p: merge_requests::MrsListParams) -> (Value, u64) {
    let (body, meta) = merge_requests::mrs_list(&env.client, p)
        .await
        .expect("mrs_list");
    (slim::slim_list(body), meta.per_page.unwrap_or(0))
}

// --------------------------------------------------------------------------
// MR invariants
// --------------------------------------------------------------------------

/// A collapsed user object must carry only id/username/name (the slimmer drops
/// avatar_url, web_url, state, etc.).
fn assert_user_collapsed(user: &Value) {
    if let Some(obj) = user.as_object() {
        for key in obj.keys() {
            assert!(
                matches!(key.as_str(), "id" | "username" | "name"),
                "user object should be collapsed, unexpected key {key:?}"
            );
        }
    }
}

fn assert_mr_get_invariants(item: &Value) {
    assert!(item.get("iid").and_then(Value::as_u64).is_some(), "iid");
    assert_nonempty_str(item, "title");
    assert_nonempty_str(item, "web_url");
    assert_nonempty_str(item, "source_branch");
    assert_nonempty_str(item, "target_branch");
    let state = item["state"].as_str().unwrap_or("");
    assert!(
        matches!(state, "opened" | "closed" | "merged" | "locked"),
        "unexpected MR state {state:?}"
    );
    assert_no_stripped_keys(item);
    // Enrichment from mr_get: both always present as arrays.
    assert!(item["closes_issues"].is_array(), "closes_issues is array");
    assert!(item["related_issues"].is_array(), "related_issues is array");
}

fn assert_mr_list_item_invariants(item: &Value) {
    assert!(item.get("iid").and_then(Value::as_u64).is_some(), "iid");
    assert_nonempty_str(item, "source_branch");
    assert_nonempty_str(item, "target_branch");
    assert_nonempty_str(item, "web_url");
    assert_no_stripped_keys(item);
    // Heavier list slimming: these are stripped from list items.
    for stripped in ["description", "pipeline", "head_pipeline"] {
        assert!(
            item.get(stripped).is_none(),
            "{stripped} must be stripped from MR list items"
        );
    }
    assert_user_collapsed(&item["author"]);
}

// --------------------------------------------------------------------------
// Create / get / update (incl. draft toggle) / delete lifecycle
// --------------------------------------------------------------------------

#[tokio::test]
async fn mrs_create_get_update_delete_lifecycle() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let branch = format!("{tag}-feat");
    seed_branch_with_file(&env, &branch, "main").await;

    // Create an MR with a description so single-get can assert it survives.
    let mut create = mr_create_params(&env, &branch, "main", &format!("{tag} mr"));
    create.description = Some("**mr** body".into());
    let (iid, created) = create_mr(&env, create).await;
    assert_eq!(created["state"], "opened");
    assert_eq!(created["source_branch"], branch);
    assert_eq!(created["target_branch"], "main");

    // Get — single-get keeps description and embeds closes/related issues.
    let got = get_mr_slimmed(&env, iid).await;
    assert_mr_get_invariants(&got);
    assert_eq!(got["description"], "**mr** body");

    // Update title.
    let mut upd = mr_update_params(&env, iid);
    upd.title = Some(format!("{tag} retitled"));
    let updated = merge_requests::mr_update(&env.client, upd)
        .await
        .expect("update title");
    assert_eq!(updated["title"], format!("{tag} retitled"));

    // Replace labels.
    let mut upd = mr_update_params(&env, iid);
    upd.labels = Some(format!("{tag}-x"));
    let relabeled = merge_requests::mr_update(&env.client, upd)
        .await
        .expect("replace labels");
    let labels = relabeled["labels"].as_array().expect("labels array");
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0], format!("{tag}-x"));

    // Draft toggle on: the tool implements draft via a "Draft: " title prefix.
    let mut upd = mr_update_params(&env, iid);
    upd.draft = Some(true);
    let drafted = merge_requests::mr_update(&env.client, upd)
        .await
        .expect("set draft");
    let drafted_title = drafted["title"].as_str().unwrap_or("");
    assert!(
        drafted_title.starts_with("Draft:"),
        "draft=true must add the Draft: prefix, got {drafted_title:?}"
    );
    assert_eq!(drafted["draft"], true);

    // Draft toggle off: prefix removed.
    let mut upd = mr_update_params(&env, iid);
    upd.draft = Some(false);
    let undrafted = merge_requests::mr_update(&env.client, upd)
        .await
        .expect("clear draft");
    assert!(
        !undrafted["title"].as_str().unwrap_or("").starts_with("Draft:"),
        "draft=false must strip the Draft: prefix"
    );
    assert_eq!(undrafted["draft"], false);

    // Close then reopen via state_event.
    let mut upd = mr_update_params(&env, iid);
    upd.state_event = Some("close".into());
    let closed = merge_requests::mr_update(&env.client, upd)
        .await
        .expect("close");
    assert_eq!(closed["state"], "closed");

    let mut upd = mr_update_params(&env, iid);
    upd.state_event = Some("reopen".into());
    let reopened = merge_requests::mr_update(&env.client, upd)
        .await
        .expect("reopen");
    assert_eq!(reopened["state"], "opened");

    // Delete; a follow-up get must 404.
    delete_mr(&env, iid).await;
    let err = merge_requests::mr_get(
        &env.client,
        merge_requests::MrGetParams {
            project_id: env.project.clone(),
            merge_request_iid: iid,
        },
    )
    .await
    .expect_err("get after delete must 404");
    assert!(
        matches!(err, GitlabError::Api { status, .. } if status.as_u16() == 404),
        "expected 404 after delete, got {err:?}"
    );

    delete_branch(&env, &branch).await;
}

// --------------------------------------------------------------------------
// List filters, slimming, sort, pagination
// --------------------------------------------------------------------------

#[tokio::test]
async fn mrs_list_filters_slimming_sort_pagination() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let label = format!("{tag}-grp");

    // Two MRs against main, sharing a label, from distinct source branches.
    let mut branches_made = Vec::new();
    let mut iids = Vec::new();
    for n in 1..=2 {
        let branch = format!("{tag}-feat-{n}");
        seed_branch_with_file(&env, &branch, "main").await;
        branches_made.push(branch.clone());
        let mut create = mr_create_params(&env, &branch, "main", &format!("{tag} mr {n}"));
        create.labels = Some(label.clone());
        let (iid, _) = create_mr(&env, create).await;
        iids.push(iid);
    }

    // Filter by label — every item satisfies list invariants (author collapsed,
    // pipeline + description stripped — the fidelity checks vs single-get).
    let mut p = mrs_list_params(&env.project);
    p.labels = Some(label.clone());
    p.state = Some("all".into());
    let (items, _) = list_mrs_slimmed(&env, p).await;
    let arr = items.as_array().expect("items array");
    assert_eq!(arr.len(), 2, "label filter returns exactly the seeded set");
    for item in arr {
        assert_mr_list_item_invariants(item);
    }

    // Filter by source branch — returns just that one MR.
    let mut p = mrs_list_params(&env.project);
    p.source_branch = Some(branches_made[0].clone());
    p.state = Some("all".into());
    let (one, _) = list_mrs_slimmed(&env, p).await;
    assert_eq!(one.as_array().unwrap().len(), 1, "source_branch filter");

    // Sort ascending by created_at — IIDs monotonically increasing.
    let mut p = mrs_list_params(&env.project);
    p.labels = Some(label.clone());
    p.state = Some("all".into());
    p.order_by = Some("created_at".into());
    p.sort = Some("asc".into());
    let (sorted, _) = list_mrs_slimmed(&env, p).await;
    let sorted_iids: Vec<u64> = sorted
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["iid"].as_u64().unwrap())
        .collect();
    let mut expect = sorted_iids.clone();
    expect.sort_unstable();
    assert_eq!(sorted_iids, expect, "ascending created_at => ascending iid");

    // Pagination — per_page=1 returns a single item and echoes per_page.
    let mut p = mrs_list_params(&env.project);
    p.labels = Some(label.clone());
    p.state = Some("all".into());
    p.pagination = pg(Some(1), Some(1));
    let (page1, per_page) = list_mrs_slimmed(&env, p).await;
    assert_eq!(page1.as_array().unwrap().len(), 1, "per_page=1 => one item");
    assert_eq!(per_page, 1, "X-Per-Page header echoed in meta");

    for iid in iids {
        delete_mr(&env, iid).await;
    }
    for branch in branches_made {
        delete_branch(&env, &branch).await;
    }
}

// --------------------------------------------------------------------------
// Merge flow — targets a throwaway base branch so `main` is never modified.
// --------------------------------------------------------------------------

#[tokio::test]
async fn mr_merge_into_throwaway_base() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let base = format!("{tag}-base");
    let feat = format!("{tag}-merge-feat");

    // Throwaway base off main, then a feature branch off that base with a diff.
    seed_branch_with_file(&env, &base, "main").await;
    seed_branch_with_file(&env, &feat, &base).await;

    let (iid, _) = create_mr(&env, mr_create_params(&env, &feat, &base, &format!("{tag} merge"))).await;

    // GitLab computes mergeability asynchronously; merging too early returns 405.
    wait_until_mergeable(&env, iid).await;

    let merged = merge_requests::mr_merge(
        &env.client,
        merge_requests::MrMergeParams {
            project_id: env.project.clone(),
            merge_request_iid: iid,
            merge_commit_message: None,
            squash: None,
            should_remove_source_branch: Some(true),
            merge_when_pipeline_succeeds: None,
        },
    )
    .await
    .expect("merge");
    assert_eq!(merged["state"], "merged");

    // The get path still works post-merge and reports the merged state.
    let got = get_mr_slimmed(&env, iid).await;
    assert_mr_get_invariants(&got);
    assert_eq!(got["state"], "merged");

    // Cleanup: deleting the base branch discards the merged commit; main is
    // untouched. The feature branch was removed on merge, but delete defensively.
    delete_branch(&env, &feat).await;
    delete_branch(&env, &base).await;
}
