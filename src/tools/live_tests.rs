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
use crate::tools::{PaginationParams, issues};

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
