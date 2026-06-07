//! Live tests for the Snippets domain (personal snippets — `/api/v4/snippets`).
//! Fully self-contained: the test creates its own snippet and deletes it, with
//! no project or branch seeding.

use serde_json::Value;

use crate::client::GitlabError;
use crate::tools::{PaginationParams, slim, snippets};

use super::harness::{
    LiveEnv, assert_no_stripped_keys, assert_nonempty_str, run_tag, skip_unless_live,
};

fn assert_snippet_invariants(snip: &Value) {
    assert!(
        snip.get("id").and_then(Value::as_u64).is_some(),
        "snippet id"
    );
    assert_nonempty_str(snip, "title");
    assert_nonempty_str(snip, "web_url");
    assert_no_stripped_keys(snip);
}

async fn get_snippet_slimmed(env: &LiveEnv, id: u64) -> Value {
    slim::slim_get(
        snippets::snippet_get(&env.client, snippets::SnippetGetParams { id })
            .await
            .expect("snippet_get"),
    )
}

/// Raw content of a single-file snippet (the tool wraps it as `{ "content": … }`).
async fn snippet_raw_content(env: &LiveEnv, id: u64) -> String {
    let v = snippets::snippet_raw(&env.client, snippets::SnippetRawParams { id })
        .await
        .expect("snippet_raw");
    v["content"].as_str().unwrap_or_default().to_string()
}

// --------------------------------------------------------------------------
// Create / get / raw / file-raw / list / update / delete lifecycle
// --------------------------------------------------------------------------

#[tokio::test]
async fn snippet_crud_raw_and_list() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let file_path = format!("livetest-{tag}.txt");

    // Create a single-file private snippet.
    let created = slim::slim_get(
        snippets::snippet_create(
            &env.client,
            snippets::SnippetCreateParams {
                title: format!("{tag} snippet"),
                files: vec![snippets::SnippetFileInput {
                    content: "v1 content\n".into(),
                    file_path: file_path.clone(),
                }],
                description: Some("live test snippet".into()),
                visibility: Some("private".into()),
            },
        )
        .await
        .expect("snippet_create"),
    );
    assert_snippet_invariants(&created);
    assert_eq!(created["title"], format!("{tag} snippet"));
    assert_eq!(created["visibility"], "private");
    let id = created["id"].as_u64().unwrap();

    // Get back through the slim path.
    let got = get_snippet_slimmed(&env, id).await;
    assert_snippet_invariants(&got);
    assert_eq!(got["id"].as_u64().unwrap(), id);

    // Raw content of the (single-file) snippet round-trips exactly.
    assert_eq!(snippet_raw_content(&env, id).await, "v1 content\n");

    // Raw content of the specific file in the snippet repo. Snippet repos default
    // to "main" on modern GitLab; on an instance that still defaults to "master"
    // the ref 404s — treat that as a skip rather than a failure, since it's an
    // instance default, not a tool defect.
    match snippets::snippet_file_raw(
        &env.client,
        snippets::SnippetFileRawParams {
            id,
            ref_name: "main".into(),
            file_path: file_path.clone(),
        },
    )
    .await
    {
        Ok(file) => assert_eq!(file["content"], "v1 content\n"),
        Err(GitlabError::Api { status, .. }) if status.as_u16() == 404 => {
            eprintln!("SKIP snippet_file_raw: snippet default branch is not 'main'")
        }
        Err(e) => panic!("snippet_file_raw: {e:?}"),
    }

    // The current user's snippet list includes ours. Walk every page
    // (fetch_all) so a backlog of other personal snippets can't push ours off
    // the first page and confound the membership check.
    let (body, _) = snippets::snippets_list(
        &env.client,
        snippets::SnippetsListParams {
            filters: snippets::SnippetsListFilters {
                created_after: None,
                created_before: None,
                pagination: PaginationParams {
                    page: None,
                    per_page: None,
                    fetch_all: Some(true),
                },
            },
        },
    )
    .await
    .expect("snippets_list");
    assert!(
        slim::slim_list(body)
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"].as_u64() == Some(id)),
        "snippet present in the current user's list"
    );

    // Update the title and the file content (exercises the files-action shape).
    let updated = slim::slim_get(
        snippets::snippet_update(
            &env.client,
            snippets::SnippetUpdateParams {
                id,
                title: Some(format!("{tag} updated")),
                description: None,
                visibility: None,
                files: Some(vec![snippets::SnippetFileUpdateInput {
                    action: "update".into(),
                    file_path: Some(file_path.clone()),
                    previous_path: None,
                    content: Some("v2 content\n".into()),
                }]),
            },
        )
        .await
        .expect("snippet_update"),
    );
    assert_eq!(updated["title"], format!("{tag} updated"));
    assert_eq!(snippet_raw_content(&env, id).await, "v2 content\n");

    // Delete; a follow-up get must 404.
    snippets::snippet_delete(&env.client, snippets::SnippetDeleteParams { id })
        .await
        .expect("snippet_delete");
    let err = snippets::snippet_get(&env.client, snippets::SnippetGetParams { id })
        .await
        .expect_err("get after delete must 404");
    assert!(
        matches!(err, GitlabError::Api { status, .. } if status.as_u16() == 404),
        "expected 404 after delete, got {err:?}"
    );
}
