//! Live tests for MCP resource reads (`gitlab://` URIs).
//!
//! Verifies the URI → domain-function dispatch end-to-end: the parsed URI must
//! reach the same GitLab object the tools see, with file content decoded and
//! JSON resources slimmed identically to the single-get tools.

use rmcp::model::ResourceContents;

use crate::tools::resources::{list_recent_projects, parse_uri, read};

use super::harness::{
    LiveEnv, delete_branch, delete_issue, run_tag, seed_branch_with_file, seed_issue,
    skip_unless_live,
};

/// The test project addressed as a resource-URI segment (namespace paths carry
/// a slash, which must be percent-encoded inside a URI).
fn project_segment(env: &LiveEnv) -> String {
    env.project.replace('/', "%2F")
}

/// Read a resource URI and unwrap its single text contents.
async fn read_text(env: &LiveEnv, uri: &str) -> (String, Option<String>) {
    let parsed = parse_uri(uri).expect("parse resource URI");
    let contents = read(&env.client, parsed, uri).await.expect("read resource");
    match contents.into_iter().next() {
        Some(ResourceContents::TextResourceContents {
            text, mime_type, ..
        }) => (text, mime_type),
        other => panic!("expected text contents, got {other:?}"),
    }
}

/// The `resources/list` surface and the project resource it points at: the
/// test project is a recently active membership project, so it must appear,
/// and its listed URI must read back as the project's JSON.
#[tokio::test]
async fn recent_project_listing_includes_readable_test_project() {
    let env = skip_unless_live!();

    let listed = list_recent_projects(&env.client)
        .await
        .expect("list recent projects");
    let expected_uri = format!("gitlab://{}", project_segment(&env));
    let entry = listed
        .iter()
        .find(|r| r.uri == expected_uri)
        .unwrap_or_else(|| panic!("test project {expected_uri} not in recent listing"));
    assert_eq!(entry.name, env.project);

    let (text, mime) = read_text(&env, &entry.uri).await;
    assert_eq!(mime.as_deref(), Some("application/json"));
    let v: serde_json::Value = serde_json::from_str(&text).expect("resource is JSON");
    assert_eq!(
        v["path_with_namespace"].as_str(),
        Some(env.project.as_str())
    );
    super::harness::assert_no_stripped_keys(&v);
}

#[tokio::test]
async fn file_resource_returns_decoded_seeded_content() {
    let env = skip_unless_live!();
    let branch = seed_branch_with_file(&env, &run_tag(), "main").await;

    // seed_branch_with_file commits livetest/{branch}.txt with known content.
    let uri = format!(
        "gitlab://{}/files/livetest%2F{branch}.txt?ref={branch}",
        project_segment(&env)
    );
    let (text, mime) = read_text(&env, &uri).await;

    delete_branch(&env, &branch).await;

    assert_eq!(text, format!("content for {branch}\n"));
    assert_eq!(mime.as_deref(), Some("text/plain"));
}

#[tokio::test]
async fn issue_resource_matches_the_issue_get_tool() {
    let env = skip_unless_live!();
    let title = format!("{} resource-read", run_tag());
    let iid = seed_issue(&env, &title).await;

    let uri = format!("gitlab://{}/issues/{iid}", project_segment(&env));
    let (text, mime) = read_text(&env, &uri).await;

    delete_issue(&env, iid).await;

    assert_eq!(mime.as_deref(), Some("application/json"));
    let v: serde_json::Value = serde_json::from_str(&text).expect("resource is JSON");
    assert_eq!(v["iid"].as_u64(), Some(iid));
    assert_eq!(v["title"].as_str(), Some(title.as_str()));
    // The slim_get shaping applies to resources too: enriched arrays present,
    // user objects collapsed by the shared invariants.
    assert!(v.get("linked_issues").is_some(), "issue_get enrichment");
    super::harness::assert_no_stripped_keys(&v);
}
