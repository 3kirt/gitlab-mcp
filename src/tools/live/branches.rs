//! Live tests for the Branches domain. Self-seeding and self-cleaning: each
//! test creates branches off `main` and deletes them in teardown.

use serde_json::Value;

use crate::client::GitlabError;
use crate::tools::{branches, slim};

use super::harness::{
    LiveEnv, assert_no_stripped_keys, assert_nonempty_str, delete_branch, pg, run_tag,
    skip_unless_live,
};

// --------------------------------------------------------------------------
// Branch helpers
// --------------------------------------------------------------------------

async fn create_branch(env: &LiveEnv, name: &str, source_ref: &str) -> Value {
    branches::branch_create(
        &env.client,
        branches::BranchCreateParams {
            project_id: env.project.clone(),
            branch: name.to_string(),
            source_ref: source_ref.to_string(),
        },
    )
    .await
    .expect("branch_create")
}

/// Fetch a branch through the server's path: domain function + `slim_get`.
async fn get_branch_slimmed(env: &LiveEnv, name: &str) -> Value {
    let raw = branches::branch_get(
        &env.client,
        branches::BranchGetParams {
            project_id: env.project.clone(),
            branch: name.to_string(),
        },
    )
    .await
    .expect("branch_get");
    slim::slim_get(raw)
}

async fn list_branches_slimmed(env: &LiveEnv, p: branches::BranchesListParams) -> (Value, u64) {
    let (body, meta) = branches::branches_list(&env.client, p)
        .await
        .expect("branches_list");
    (slim::slim_list(body), meta.per_page.unwrap_or(0))
}

/// `BranchesListParams` with everything defaulted; callers set the fields under test.
fn branches_list_params(project: &str) -> branches::BranchesListParams {
    branches::BranchesListParams {
        project_id: project.to_string(),
        regex: None,
        search: None,
        pagination: pg(None, None),
    }
}

/// Invariants for a branch object (single-get or list item). `merged`/`protected`
/// must be real booleans (never null), and `commit.id` a non-empty SHA.
fn assert_branch_invariants(item: &Value) {
    assert_nonempty_str(item, "name");
    assert_nonempty_str(item, "web_url");
    let commit_id = item["commit"]["id"].as_str().unwrap_or("");
    assert!(!commit_id.is_empty(), "commit.id must be a non-empty SHA");
    assert!(item["merged"].is_boolean(), "merged must be a boolean");
    assert!(
        item["protected"].is_boolean(),
        "protected must be a boolean"
    );
    assert_no_stripped_keys(item);
}

// --------------------------------------------------------------------------
// Create / get / delete lifecycle
// --------------------------------------------------------------------------

#[tokio::test]
async fn branches_create_get_delete_lifecycle() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let name = format!("{tag}-br");

    // Create off main, then read it back through the slim path.
    let created = create_branch(&env, &name, "main").await;
    assert_eq!(created["name"], name);

    let got = get_branch_slimmed(&env, &name).await;
    assert_branch_invariants(&got);
    assert_eq!(got["name"], name);

    // Delete; a follow-up get must 404.
    branches::branch_delete(
        &env.client,
        branches::BranchDeleteParams {
            project_id: env.project.clone(),
            branch: name.clone(),
        },
    )
    .await
    .expect("delete branch");

    let err = branches::branch_get(
        &env.client,
        branches::BranchGetParams {
            project_id: env.project.clone(),
            branch: name.clone(),
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
// List — search, regex, pagination
// --------------------------------------------------------------------------

#[tokio::test]
async fn branches_list_search_regex_pagination() {
    let env = skip_unless_live!();
    let tag = run_tag();

    // Seed three branches off main sharing the unique tag prefix.
    let mut names = Vec::new();
    for n in 1..=3 {
        let name = format!("{tag}-br-{n}");
        create_branch(&env, &name, "main").await;
        names.push(name);
    }

    // Search by substring — returns exactly the seeded set, each item valid.
    let mut p = branches_list_params(&env.project);
    p.search = Some(tag.clone());
    let (items, _) = list_branches_slimmed(&env, p).await;
    let arr = items.as_array().expect("items array");
    assert_eq!(arr.len(), 3, "search returns exactly the seeded set");
    for item in arr {
        assert_branch_invariants(item);
    }

    // Regex anchored on the tag — same set (the tag has no regex metacharacters).
    let mut p = branches_list_params(&env.project);
    p.regex = Some(format!("^{tag}"));
    let (matched, _) = list_branches_slimmed(&env, p).await;
    assert_eq!(
        matched.as_array().unwrap().len(),
        3,
        "regex matches the seeded set"
    );

    // Pagination over the search subset — per_page=1 yields one item.
    let mut p = branches_list_params(&env.project);
    p.search = Some(tag.clone());
    p.pagination = pg(Some(1), Some(1));
    let (page1, per_page) = list_branches_slimmed(&env, p).await;
    assert_eq!(page1.as_array().unwrap().len(), 1, "per_page=1 => one item");
    assert_eq!(per_page, 1, "X-Per-Page header echoed in meta");

    for name in names {
        delete_branch(&env, &name).await;
    }
}
