//! Live tests for the Merge Requests domain.
//!
//! MRs need a source branch that differs from the target. We use `file_create`
//! with `start_branch` to create the branch *and* a differentiating commit in
//! one call, then tear the branch down afterward. The merge test targets a
//! throwaway base branch so `main` is never modified.

use serde_json::Value;

use crate::client::GitlabError;
use crate::tools::{branches, merge_requests, repository_files, slim};

use super::harness::{
    LiveEnv, assert_no_stripped_keys, assert_nonempty_str, assert_user_collapsed, pg, run_tag,
    skip_unless_live,
};

// --------------------------------------------------------------------------
// MR helpers
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

/// Merge an MR once GitLab is actually ready to accept it, returning the merge
/// response. GitLab computes mergeability asynchronously, so two races have to
/// be tolerated: the status fields lag reality, and even when they report ready
/// the merge ref can be a beat behind — yielding a transient `405 Method Not
/// Allowed`. So we poll `detailed_merge_status` (falling back to the legacy
/// `merge_status`) *and* retry the merge itself on a 405, within one budget.
async fn merge_when_ready(env: &LiveEnv, iid: u64) -> Value {
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
        let detailed = mr["detailed_merge_status"].as_str().unwrap_or("");
        let legacy = mr["merge_status"].as_str().unwrap_or("");
        if detailed == "conflict" || legacy == "cannot_be_merged" {
            panic!("MR {iid} cannot be merged (detailed_merge_status={detailed:?})");
        }
        let ready = detailed == "mergeable" || (detailed.is_empty() && legacy == "can_be_merged");
        if ready {
            match merge_requests::mr_merge(
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
            {
                Ok(merged) => return merged,
                // 405 = "not mergeable yet" despite the status; back off and retry.
                Err(GitlabError::Api { status, .. }) if status.as_u16() == 405 => {}
                Err(e) => panic!("merge: {e:?}"),
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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

    let (iid, _) = create_mr(
        &env,
        mr_create_params(&env, &feat, &base, &format!("{tag} merge")),
    )
    .await;

    // GitLab computes mergeability asynchronously; this polls + retries the 405.
    let merged = merge_when_ready(&env, iid).await;
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
