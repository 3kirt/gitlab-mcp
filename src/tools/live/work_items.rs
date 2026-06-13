//! Live tests for the Work Items (GraphQL) domain.
//!
//! These exist to answer the one question wiremock can't: does the GraphQL
//! `work_items.rs` module actually agree with the battle-tested REST issues
//! module when both look at the *same* record on a real GitLab instance? An
//! issue in GitLab *is* a work item (same project-scoped IID), so we seed an
//! issue over REST and then read it back through both APIs and assert the
//! overlapping fields match (modulo the known REST↔GraphQL representation
//! differences: snake_case vs camelCase keys, "opened"/"closed" vs
//! "OPEN"/"CLOSED" state, numeric vs `gid://` ids, u64 vs string iid).
//!
//! Self-seeding and self-cleaning, like the rest of the suite. Skips without
//! credentials.
//!
//! NB: `work_items.rs` reaches namespaces via the GraphQL `namespace(fullPath:)`
//! field with the project's full path. If a future GitLab version stops
//! resolving project paths through `namespace`, these tests are where it surfaces
//! (the fix would be to branch to `project(fullPath:)` / `group(fullPath:)`).

use std::time::Duration;

use serde_json::Value;

use crate::tools::{issue_notes, issues, slim, work_items};

use super::harness::{LiveEnv, delete_issue, pg, run_tag, skip_unless_live};

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

/// Seed an issue over REST and return its iid. `labels` is a comma-separated
/// string (GitLab's REST shape) or `None`.
async fn seed_issue_full(
    env: &LiveEnv,
    title: String,
    description: Option<String>,
    labels: Option<String>,
) -> u64 {
    let created = issues::issue_create(
        &env.client,
        issues::IssueCreateParams {
            project_id: env.project.clone().into(),
            title,
            description,
            labels,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await
    .expect("seed issue");
    created["iid"].as_u64().expect("created issue has iid")
}

/// `IssuesListParams` defaulted except for `search` — mirrors the REST list the
/// server would run.
fn issues_list_params(project: &str, search: &str) -> issues::IssuesListParams {
    issues::IssuesListParams {
        project_id: project.to_string().into(),
        state: None,
        labels: None,
        search: Some(search.to_string()),
        scope: None,
        assignee_id: None,
        author_id: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
        order_by: None,
        sort: None,
        pagination: pg(None, Some(100)),
    }
}

/// Fetch an issue through the REST path the server uses (domain fn + `slim_get`).
async fn rest_issue_get(env: &LiveEnv, iid: u64) -> Value {
    let raw = issues::issue_get(
        &env.client,
        issues::IssueGetParams {
            project_id: env.project.clone().into(),
            issue_iid: iid,
        },
    )
    .await
    .expect("issue_get");
    slim::slim_get(raw)
}

/// Fetch a work item through the GraphQL path the server uses (domain fn +
/// `slim_get`, matching `json_result`).
async fn gql_work_item_get(env: &LiveEnv, iid: u64) -> Value {
    let raw = work_items::work_item_get(
        &env.client,
        work_items::WorkItemGetParams {
            namespace_path: env.project.clone(),
            work_item_iid: iid,
        },
    )
    .await
    .expect("work_item_get");
    slim::slim_get(raw)
}

/// Map a GraphQL `WorkItemState` to the REST `state` spelling.
fn graphql_state_to_rest(state: &str) -> &str {
    match state {
        "OPEN" => "opened",
        "CLOSED" => "closed",
        other => other,
    }
}

/// Sorted label title strings from either an issue's or work item's `labels`.
fn sorted_labels(v: &Value) -> Vec<String> {
    let mut labels: Vec<String> = v["labels"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    labels.sort();
    labels
}

/// iids from a slimmed REST issues list (numeric).
fn rest_list_iids(items: &Value) -> Vec<u64> {
    items
        .as_array()
        .map(|a| a.iter().filter_map(|i| i["iid"].as_u64()).collect())
        .unwrap_or_default()
}

/// iids from a GraphQL work-items list (`nodes[].iid` is a string).
fn gql_list_iids(nodes: &Value) -> Vec<u64> {
    nodes
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|n| n["iid"].as_str().and_then(|s| s.parse().ok()))
                .collect()
        })
        .unwrap_or_default()
}

// --------------------------------------------------------------------------
// GET comparison: same record, both APIs, fields agree
// --------------------------------------------------------------------------

#[tokio::test]
async fn work_item_get_matches_rest_issue_get() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // Seed an issue with description + a label so there is content to compare.
    let iid = seed_issue_full(
        &env,
        format!("{tag} compare"),
        Some(format!("body for {tag}")),
        Some(format!("{tag}-label")),
    )
    .await;

    let rest = rest_issue_get(&env, iid).await;
    let gql = gql_work_item_get(&env, iid).await;

    // --- overlapping fields must agree (accounting for representation diffs) ---

    assert_eq!(gql["title"], rest["title"], "title");

    assert_eq!(
        gql["iid"].as_str().expect("graphql iid is a string"),
        rest["iid"]
            .as_u64()
            .expect("rest iid is numeric")
            .to_string(),
        "iid"
    );

    assert_eq!(gql["description"], rest["description"], "description");

    assert_eq!(
        gql["webUrl"], rest["web_url"],
        "web url (webUrl vs web_url)"
    );

    assert_eq!(
        graphql_state_to_rest(gql["state"].as_str().expect("graphql state")),
        rest["state"].as_str().expect("rest state"),
        "state"
    );

    assert_eq!(sorted_labels(&gql), sorted_labels(&rest), "labels");

    // Author: ids differ (numeric vs gid://), but username + name must match.
    assert_eq!(
        gql["author"]["username"], rest["author"]["username"],
        "author username"
    );
    assert_eq!(gql["author"]["name"], rest["author"]["name"], "author name");

    // GraphQL-only enrichment: an issue is a work item of type "Issue".
    assert_eq!(gql["workItemType"], "Issue", "workItemType");

    delete_issue(&env, iid).await;
}

// --------------------------------------------------------------------------
// LIST comparison: both backends return the same set for the same filter
// --------------------------------------------------------------------------

#[tokio::test]
async fn work_items_list_matches_rest_issues_list() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // Seed two issues sharing a unique search token (the run tag).
    let mut seeded = vec![
        seed_issue_full(&env, format!("{tag} alpha"), None, None).await,
        seed_issue_full(&env, format!("{tag} beta"), None, None).await,
    ];
    seeded.sort();

    // Search indexing can lag for freshly created issues, so poll each backend
    // until the seeded issues surface (or attempts run out). Both sides retry
    // independently; the assertions below catch a genuine divergence.
    let rest_iids = poll_for_iids(&seeded, || async {
        let (body, _) = issues::issues_list(&env.client, issues_list_params(&env.project, &tag))
            .await
            .expect("issues_list");
        rest_list_iids(&slim::slim_list(body))
    })
    .await;

    let gql_iids = poll_for_iids(&seeded, || async {
        let result = work_items::work_items_list(
            &env.client,
            work_items::WorkItemsListParams {
                namespace_path: env.project.clone(),
                types: Some(vec!["ISSUE".into()]),
                state: None,
                search: Some(tag.clone()),
                first: Some(100),
                after: None,
            },
        )
        .await
        .expect("work_items_list");
        gql_list_iids(&result["nodes"])
    })
    .await;

    // The tag is unique to this run, so each backend should return exactly the
    // two seeded issues — and crucially, the *same* set.
    let mut rest_sorted = rest_iids.clone();
    rest_sorted.sort();
    let mut gql_sorted = gql_iids.clone();
    gql_sorted.sort();

    assert_eq!(
        rest_sorted, seeded,
        "REST issues list did not match seeded set"
    );
    assert_eq!(
        gql_sorted, seeded,
        "GraphQL work items list did not match seeded set"
    );
    assert_eq!(
        gql_sorted, rest_sorted,
        "GraphQL and REST disagree on the result set"
    );

    for iid in seeded {
        delete_issue(&env, iid).await;
    }
}

// --------------------------------------------------------------------------
// MUTATION lifecycle: create/update/delete over GraphQL, verified via REST
// --------------------------------------------------------------------------

#[tokio::test]
async fn work_item_mutation_lifecycle_visible_via_rest() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // CREATE an Issue-type work item over GraphQL (resolves the type id first).
    let created = work_items::work_item_create(
        &env.client,
        work_items::WorkItemCreateParams {
            namespace_path: env.project.clone(),
            work_item_type: "ISSUE".into(),
            title: format!("{tag} wi-create"),
            description: Some(format!("created via graphql {tag}")),
            confidential: None,
            labels: None,
            assignees: None,
            parent_work_item_iid: None,
        },
    )
    .await
    .expect("work_item_create");

    assert_eq!(created["workItemType"], "Issue", "created type");
    let iid = created["iid"]
        .as_str()
        .expect("created iid is a string")
        .parse::<u64>()
        .expect("iid parses to u64");

    // An Issue work item *is* a REST issue — confirm the create landed by reading
    // it back through the REST API, including the top-level `description` field
    // we set on create (this is what verifies `description` maps to the widget).
    let rest = rest_issue_get(&env, iid).await;
    assert_eq!(rest["title"], created["title"], "REST sees created title");
    assert_eq!(
        rest["description"],
        format!("created via graphql {tag}"),
        "REST sees created description"
    );
    assert_eq!(rest["state"], "opened", "REST state after create");

    // UPDATE: rename + close over GraphQL.
    let updated = work_items::work_item_update(
        &env.client,
        work_items::WorkItemUpdateParams {
            namespace_path: env.project.clone(),
            work_item_iid: iid,
            title: Some(format!("{tag} wi-renamed")),
            description: None,
            state_event: Some("close".into()),
            confidential: None,
            add_labels: None,
            remove_labels: None,
            assignees: None,
            parent_work_item_iid: None,
        },
    )
    .await
    .expect("work_item_update");
    assert_eq!(
        updated["title"],
        format!("{tag} wi-renamed"),
        "updated title"
    );
    assert_eq!(
        updated["state"], "CLOSED",
        "updated state (GraphQL spelling)"
    );

    // Verify the update is visible via REST.
    let rest2 = rest_issue_get(&env, iid).await;
    assert_eq!(
        rest2["title"],
        format!("{tag} wi-renamed"),
        "REST sees rename"
    );
    assert_eq!(rest2["state"], "closed", "REST state after close");

    // DELETE over GraphQL.
    work_items::work_item_delete(
        &env.client,
        work_items::WorkItemDeleteParams {
            namespace_path: env.project.clone(),
            work_item_iid: iid,
        },
    )
    .await
    .expect("work_item_delete");

    // Verify deletion via REST: the issue now 404s.
    let err = issues::issue_get(
        &env.client,
        issues::IssueGetParams {
            project_id: env.project.clone().into(),
            issue_iid: iid,
        },
    )
    .await
    .expect_err("deleted issue should error via REST");
    assert!(
        matches!(err, crate::client::GitlabError::Api { status, .. } if status.as_u16() == 404),
        "deleted issue should 404 via REST, got {err:?}"
    );
}

// --------------------------------------------------------------------------
// Label / assignee resolution, verified via REST
// --------------------------------------------------------------------------

/// True if the REST issue's `labels` array contains `name`.
fn labels_contain(rest: &Value, name: &str) -> bool {
    rest["labels"]
        .as_array()
        .is_some_and(|a| a.iter().any(|l| l.as_str() == Some(name)))
}

#[tokio::test]
async fn work_item_create_with_labels_and_assignee_visible_via_rest() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // The token's own user, as the assignee (avoids hardcoding an account).
    let me = env.client.get("/api/v4/user").await.expect("GET /user");
    let username = me["username"]
        .as_str()
        .expect("current username")
        .to_string();

    // Relies on the test project's default labels ("bug", "enhancement"); a
    // missing label would surface as a clear "label(s) not found" error.
    let created = work_items::work_item_create(
        &env.client,
        work_items::WorkItemCreateParams {
            namespace_path: env.project.clone(),
            work_item_type: "ISSUE".into(),
            title: format!("{tag} wi-labels"),
            description: None,
            confidential: None,
            labels: Some(vec!["bug".into()]),
            assignees: Some(vec![username.clone()]),
            parent_work_item_iid: None,
        },
    )
    .await
    .expect("work_item_create with labels+assignee");
    let iid = created["iid"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .expect("iid");

    // REST sees the resolved label and assignee.
    let rest = rest_issue_get(&env, iid).await;
    assert!(labels_contain(&rest, "bug"), "REST sees label 'bug'");
    assert_eq!(
        rest["assignee"]["username"], username,
        "REST sees the assignee"
    );

    // Swap labels via update (add enhancement, remove bug).
    work_items::work_item_update(
        &env.client,
        work_items::WorkItemUpdateParams {
            namespace_path: env.project.clone(),
            work_item_iid: iid,
            title: None,
            description: None,
            state_event: None,
            confidential: None,
            add_labels: Some(vec!["enhancement".into()]),
            remove_labels: Some(vec!["bug".into()]),
            assignees: None,
            parent_work_item_iid: None,
        },
    )
    .await
    .expect("work_item_update labels");

    let rest2 = rest_issue_get(&env, iid).await;
    assert!(labels_contain(&rest2, "enhancement"), "label added");
    assert!(!labels_contain(&rest2, "bug"), "label removed");

    delete_issue(&env, iid).await;
}

#[tokio::test]
async fn work_item_create_with_parent_sets_hierarchy() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // Parent and child as Issue work items.
    let parent = work_items::work_item_create(
        &env.client,
        work_items::WorkItemCreateParams {
            namespace_path: env.project.clone(),
            work_item_type: "ISSUE".into(),
            title: format!("{tag} wi-parent"),
            description: None,
            confidential: None,
            labels: None,
            assignees: None,
            parent_work_item_iid: None,
        },
    )
    .await
    .expect("create parent");
    let parent_iid = parent["iid"].as_str().unwrap().to_string();
    let parent_iid_u64 = parent_iid.parse::<u64>().unwrap();

    // The child must be a Task: GitLab's hierarchy rules forbid Issue→Issue, but
    // allow Issue→Task (and Task isn't a REST issue, so it must be deleted over
    // GraphQL — exercising work_item_delete on a non-issue type).
    let child = work_items::work_item_create(
        &env.client,
        work_items::WorkItemCreateParams {
            namespace_path: env.project.clone(),
            work_item_type: "TASK".into(),
            title: format!("{tag} wi-child"),
            description: None,
            confidential: None,
            labels: None,
            assignees: None,
            parent_work_item_iid: Some(parent_iid_u64),
        },
    )
    .await
    .expect("create child task with parent issue");
    let child_iid = child["iid"].as_str().unwrap().parse::<u64>().unwrap();

    // Read the child back over GraphQL: the hierarchy widget flattened to `parent`.
    let fetched = gql_work_item_get(&env, child_iid).await;
    assert_eq!(
        fetched["parent"]["iid"], parent_iid,
        "child's parent IID matches"
    );

    // Delete both over GraphQL (work_item_delete handles any type, incl. Task).
    for iid in [child_iid, parent_iid_u64] {
        work_items::work_item_delete(
            &env.client,
            work_items::WorkItemDeleteParams {
                namespace_path: env.project.clone(),
                work_item_iid: iid,
            },
        )
        .await
        .expect("work_item_delete teardown");
    }
}

// --------------------------------------------------------------------------
// Notes (comments): GraphQL note ops cross-verified against REST issue notes
// --------------------------------------------------------------------------

#[tokio::test]
async fn work_item_notes_lifecycle_visible_via_rest() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // A work item to comment on (an Issue, so REST issue-notes can see the same
    // note — a work-item note *is* an issue note).
    let iid = seed_issue_full(&env, format!("{tag} wi-notes"), None, None).await;
    let body = format!("comment via graphql {tag}");

    // CREATE a note over GraphQL.
    let created = work_items::work_item_note_create(
        &env.client,
        work_items::WorkItemNoteCreateParams {
            namespace_path: env.project.clone(),
            work_item_iid: iid,
            body: body.clone(),
            internal: None,
        },
    )
    .await
    .expect("note create");
    let note_id = created["id"].as_str().expect("note gid").to_string();
    assert_eq!(created["body"], body);

    // LIST over GraphQL: the note is present (ignoring system notes). Reads after
    // a write can lag, so poll rather than asserting once. A transient error on
    // the read counts as "not yet".
    let in_graphql = poll_until(|| async {
        work_items::work_item_notes_list(
            &env.client,
            work_items::WorkItemNotesListParams {
                namespace_path: env.project.clone(),
                work_item_iid: iid,
                first: None,
                after: None,
            },
        )
        .await
        .ok()
        .and_then(|r| {
            r["nodes"]
                .as_array()
                .map(|a| a.iter().any(|n| n["body"] == serde_json::json!(body)))
        })
        .unwrap_or(false)
    })
    .await;
    assert!(in_graphql, "GraphQL notes list contains the comment");

    // Cross-check via REST issue notes: the same comment shows up there (a
    // work-item note *is* an issue note).
    let in_rest = poll_until(|| async {
        issue_notes::issue_notes_list(
            &env.client,
            issue_notes::IssueNotesListParams {
                project_id: env.project.clone().into(),
                issue_iid: iid,
                order_by: None,
                sort: None,
                pagination: pg(None, Some(100)),
            },
        )
        .await
        .ok()
        .and_then(|(notes, _)| {
            notes
                .as_array()
                .map(|a| a.iter().any(|n| n["body"] == serde_json::json!(body)))
        })
        .unwrap_or(false)
    })
    .await;
    assert!(
        in_rest,
        "REST issue notes also see the GraphQL-created comment"
    );

    // UPDATE the note over GraphQL.
    let edited = format!("edited {tag}");
    let updated = work_items::work_item_note_update(
        &env.client,
        work_items::WorkItemNoteUpdateParams {
            note_id: note_id.clone(),
            body: edited.clone(),
        },
    )
    .await
    .expect("note update");
    assert_eq!(updated["body"], edited);

    // DELETE the note over GraphQL, then confirm it's gone from the list.
    work_items::work_item_note_delete(
        &env.client,
        work_items::WorkItemNoteDeleteParams {
            note_id: note_id.clone(),
        },
    )
    .await
    .expect("note delete");

    let gone = poll_until(|| async {
        work_items::work_item_notes_list(
            &env.client,
            work_items::WorkItemNotesListParams {
                namespace_path: env.project.clone(),
                work_item_iid: iid,
                first: None,
                after: None,
            },
        )
        .await
        .ok()
        .and_then(|r| {
            r["nodes"]
                .as_array()
                .map(|a| a.iter().all(|n| n["id"] != serde_json::json!(note_id)))
        })
        .unwrap_or(false)
    })
    .await;
    assert!(gone, "deleted note no longer in the list");

    delete_issue(&env, iid).await;
}

/// Re-run `check` (up to ~10s) until it returns true, tolerating read-after-write
/// lag and transient read errors. Returns false if it never does.
async fn poll_until<F, Fut>(mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..20 {
        if check().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// Re-run `fetch` (up to ~10s) until its returned iids contain every `expected`
/// id, tolerating asynchronous search indexing. Returns the last result either
/// way so the caller's assertions report the real divergence on failure.
async fn poll_for_iids<F, Fut>(expected: &[u64], mut fetch: F) -> Vec<u64>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Vec<u64>>,
{
    let mut last = Vec::new();
    for _ in 0..20 {
        last = fetch().await;
        if expected.iter().all(|e| last.contains(e)) {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    last
}
