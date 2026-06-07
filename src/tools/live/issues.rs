//! Live tests for the Issues domain (protocol §1–6) plus issue notes and issue
//! discussions. Self-seeding and self-cleaning: every test creates the issues it
//! needs and deletes them in teardown.

use serde_json::Value;

use crate::client::GitlabError;
use crate::tools::{issue_discussions, issue_notes, issues, slim};

use super::harness::{
    LiveEnv, assert_no_stripped_keys, assert_nonempty_str, assert_note_invariants,
    delete_issue, discussion_note_count, pg, run_tag, skip_unless_live,
};

// --------------------------------------------------------------------------
// Issue helpers
// --------------------------------------------------------------------------

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
// Issue Links — the dedicated list/get/create/delete tools
//
// The embed test above covers `issue_get`'s `linked_issues`; this drives the
// relationship-id flow: create returns source/target issues, the list surfaces
// the `issue_link_id`, and get/delete key off it.
// --------------------------------------------------------------------------

#[tokio::test]
async fn issue_links_crud() {
    let env = skip_unless_live!();
    let tag = run_tag();

    let (src, _) = create_issue(
        &env,
        issues::IssueCreateParams {
            project_id: env.project.clone(),
            title: format!("{tag} link source"),
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
            title: format!("{tag} link target"),
            description: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await;

    // Create a relates_to link (blocks/is_blocked_by are Premium-gated). The
    // response carries the source and target issue objects.
    let created = slim::slim_get(
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
        .expect("create link"),
    );
    assert_eq!(created["source_issue"]["iid"], src);
    assert_eq!(created["target_issue"]["iid"], dst);
    assert_eq!(created["link_type"], "relates_to");

    // List links on the source — one entry pointing at dst, carrying the
    // relationship id used by get/delete.
    let (body, _) = issues::issue_links_list(
        &env.client,
        issues::IssueLinksListParams {
            project_id: env.project.clone(),
            issue_iid: src,
        },
    )
    .await
    .expect("list links");
    let items = slim::slim_list(body);
    let arr = items.as_array().expect("items array");
    assert_eq!(arr.len(), 1, "exactly one link");
    assert_eq!(arr[0]["iid"], dst);
    assert_eq!(arr[0]["link_type"], "relates_to");
    let issue_link_id = arr[0]["issue_link_id"].as_u64().expect("issue_link_id");

    // Get the link by its relationship id.
    let got = slim::slim_get(
        issues::issue_link_get(
            &env.client,
            issues::IssueLinkGetParams {
                project_id: env.project.clone(),
                issue_iid: src,
                issue_link_id,
            },
        )
        .await
        .expect("get link"),
    );
    assert_eq!(got["source_issue"]["iid"], src);
    assert_eq!(got["target_issue"]["iid"], dst);
    assert_eq!(got["link_type"], "relates_to");

    // Delete the link — returns the removed relationship; the list is then empty.
    let deleted = slim::slim_get(
        issues::issue_link_delete(
            &env.client,
            issues::IssueLinkDeleteParams {
                project_id: env.project.clone(),
                issue_iid: src,
                issue_link_id,
            },
        )
        .await
        .expect("delete link"),
    );
    assert_eq!(deleted["link_type"], "relates_to");

    let (body, _) = issues::issue_links_list(
        &env.client,
        issues::IssueLinksListParams {
            project_id: env.project.clone(),
            issue_iid: src,
        },
    )
    .await
    .expect("list after delete");
    assert!(
        slim::slim_list(body).as_array().unwrap().is_empty(),
        "link removed"
    );

    delete_issue(&env, src).await;
    delete_issue(&env, dst).await;
}

// --------------------------------------------------------------------------
// Issue Notes — flat comments on an issue
// --------------------------------------------------------------------------

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
    let discussion_id = created["id"]
        .as_str()
        .expect("discussion id is a string")
        .to_string();
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
    assert_eq!(
        discussion_note_count(&after),
        1,
        "reply removed, root remains"
    );

    delete_issue(&env, iid).await;
}
