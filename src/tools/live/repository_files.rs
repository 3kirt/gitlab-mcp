//! Live tests for the Repository Files domain.
//!
//! Every file operation commits to a throwaway branch created off `main` (via
//! `file_create` with `start_branch`); deleting the branch in teardown discards
//! all those commits, so `main` is never modified.

use serde_json::Value;

use crate::client::GitlabError;
use crate::tools::{repository_files, slim};

use super::harness::{
    LiveEnv, assert_no_stripped_keys, assert_nonempty_str, delete_branch, run_tag, skip_unless_live,
};

// --------------------------------------------------------------------------
// File helpers
// --------------------------------------------------------------------------

/// Fetch file metadata through the server's path: domain function + `slim_get`.
async fn get_file_slimmed(env: &LiveEnv, file_path: &str, ref_name: &str) -> Value {
    let raw = repository_files::file_get(
        &env.client,
        repository_files::FileGetParams {
            project_id: env.project.clone(),
            file_path: file_path.to_string(),
            ref_name: ref_name.to_string(),
        },
    )
    .await
    .expect("file_get");
    slim::slim_get(raw)
}

/// Fetch raw file content (the tool wraps it as `{ "content": <text> }`).
async fn raw_file(env: &LiveEnv, file_path: &str, ref_name: &str) -> Value {
    let raw = repository_files::file_raw(
        &env.client,
        repository_files::FileRawParams {
            project_id: env.project.clone(),
            file_path: file_path.to_string(),
            ref_name: Some(ref_name.to_string()),
            lfs: None,
        },
    )
    .await
    .expect("file_raw");
    slim::slim_get(raw)
}

/// Invariants for a `file_get` response: identifying metadata present and the
/// content delivered as Base64.
fn assert_file_get_invariants(item: &Value, file_path: &str) {
    assert_eq!(item["file_path"], file_path);
    assert_nonempty_str(item, "file_name");
    assert_nonempty_str(item, "blob_id");
    assert_nonempty_str(item, "commit_id");
    assert_eq!(item["encoding"], "base64", "file_get delivers Base64 content");
    assert_nonempty_str(item, "content");
    assert_no_stripped_keys(item);
}

// --------------------------------------------------------------------------
// Create / get / raw / update / blame / delete lifecycle
// --------------------------------------------------------------------------

#[tokio::test]
async fn files_crud_raw_blame_lifecycle() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let branch = format!("{tag}-files");
    let file_path = format!("livetest/{tag}.txt");

    // Create the file and the branch together (start_branch=main).
    let created = repository_files::file_create(
        &env.client,
        repository_files::FileCreateParams {
            project_id: env.project.clone(),
            file_path: file_path.clone(),
            branch: branch.clone(),
            commit_message: format!("{tag} create"),
            content: "v1\n".into(),
            encoding: None,
            author_name: None,
            author_email: None,
            execute_filemode: None,
            start_branch: Some("main".into()),
        },
    )
    .await
    .expect("file_create");
    assert_eq!(created["file_path"], file_path);
    assert_eq!(created["branch"], branch);

    // Get — metadata + Base64 content.
    let got = get_file_slimmed(&env, &file_path, &branch).await;
    assert_file_get_invariants(&got, &file_path);

    // Raw — plaintext content round-trips exactly.
    let raw = raw_file(&env, &file_path, &branch).await;
    assert_eq!(raw["content"], "v1\n");

    // Update — content changes on the same path.
    repository_files::file_update(
        &env.client,
        repository_files::FileUpdateParams {
            project_id: env.project.clone(),
            file_path: file_path.clone(),
            branch: branch.clone(),
            commit_message: format!("{tag} update"),
            content: "v2\n".into(),
            encoding: None,
            author_name: None,
            author_email: None,
            execute_filemode: None,
            last_commit_id: None,
            start_branch: None,
        },
    )
    .await
    .expect("file_update");
    let raw2 = raw_file(&env, &file_path, &branch).await;
    assert_eq!(raw2["content"], "v2\n");

    // Blame — non-empty, each entry carries a commit and a lines array.
    let blame = slim::slim_get(
        repository_files::file_blame(
            &env.client,
            repository_files::FileBlameParams {
                project_id: env.project.clone(),
                file_path: file_path.clone(),
                ref_name: branch.clone(),
                range_start: None,
                range_end: None,
            },
        )
        .await
        .expect("file_blame"),
    );
    let entries = blame.as_array().expect("blame is an array");
    assert!(!entries.is_empty(), "blame has at least one entry");
    let commit_id = entries[0]["commit"]["id"].as_str().unwrap_or("");
    assert!(!commit_id.is_empty(), "blame entry has commit.id");
    assert!(entries[0]["lines"].is_array(), "blame entry has a lines array");

    // Delete — then a get must 404.
    repository_files::file_delete(
        &env.client,
        repository_files::FileDeleteParams {
            project_id: env.project.clone(),
            file_path: file_path.clone(),
            branch: branch.clone(),
            commit_message: format!("{tag} delete"),
            author_name: None,
            author_email: None,
            last_commit_id: None,
            start_branch: None,
        },
    )
    .await
    .expect("file_delete");

    let err = repository_files::file_get(
        &env.client,
        repository_files::FileGetParams {
            project_id: env.project.clone(),
            file_path: file_path.clone(),
            ref_name: branch.clone(),
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
