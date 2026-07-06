//! MCP Completions: argument autocompletion for the prompts (`prompts.rs`) and
//! resource templates (`resources.rs`), served from live GitLab data.
//!
//! Completion requests arrive with the argument being typed plus a `context`
//! of previously-resolved arguments. Dispatch is on the *argument name* — the
//! prompt and template arguments deliberately share names (`project_id`,
//! `issue_iid`, …), so one completer per name covers every reference:
//!
//! - `project_id` — projects the token can see, matched by search term
//! - `branch` / `target_branch` / `ref` — branch names in the context project
//! - `issue_iid` / `merge_request_iid` — recently updated open items in the
//!   context project, prefix-filtered on the IID being typed
//!
//! Arguments that need a project return no suggestions until the client
//! supplies `project_id` in the completion context. For resource-template
//! references the suggested `project_id` values are percent-encoded
//! (`mygroup%2Fmyproject`) since they will be substituted into a URI, and a
//! context `project_id` arriving encoded is decoded before use.

use std::collections::HashMap;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{QueryBuilder, encode_namespace_id, project_path, recent_member_projects};

/// Max suggestions per response; see [`crate::tools::SUGGESTION_LIMIT`].
const LIMIT: usize = crate::tools::SUGGESTION_LIMIT;

pub struct Completion {
    pub values: Vec<String>,
    pub has_more: bool,
}

impl Completion {
    const fn empty() -> Self {
        Self {
            values: Vec::new(),
            has_more: false,
        }
    }
}

/// Complete one argument value. `for_resource_uri` is true when the reference
/// is a resource template (values will be substituted into a `gitlab://` URI).
pub async fn complete_argument(
    client: &GitlabClient,
    for_resource_uri: bool,
    argument: &str,
    value: &str,
    context: Option<&HashMap<String, String>>,
) -> Result<Completion, GitlabError> {
    let project = context
        .and_then(|c| c.get("project_id"))
        .map(|p| decode_context_project(p));
    match argument {
        "project_id" => complete_project(client, value, for_resource_uri).await,
        "branch" | "target_branch" | "ref" => match project {
            Some(p) => complete_branch(client, &p, value).await,
            None => Ok(Completion::empty()),
        },
        "issue_iid" => match project {
            Some(p) => complete_iid(client, &p, "issues", value).await,
            None => Ok(Completion::empty()),
        },
        "merge_request_iid" => match project {
            Some(p) => complete_iid(client, &p, "merge_requests", value).await,
            None => Ok(Completion::empty()),
        },
        _ => Ok(Completion::empty()),
    }
}

/// A context `project_id` copied from a resource URI arrives percent-encoded;
/// decode it so `project_path` doesn't double-encode. Falls back to the raw
/// value on invalid escapes.
fn decode_context_project(p: &str) -> String {
    percent_encoding::percent_decode_str(p)
        .decode_utf8()
        .map_or_else(|_| p.to_string(), std::borrow::Cow::into_owned)
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

async fn complete_project(
    client: &GitlabClient,
    value: &str,
    for_resource_uri: bool,
) -> Result<Completion, GitlabError> {
    let projects = recent_member_projects(client, non_empty(value)).await?;
    let values: Vec<String> = projects
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| p["path_with_namespace"].as_str())
                .map(|path| {
                    if for_resource_uri {
                        encode_namespace_id(path)
                    } else {
                        path.to_string()
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(full_page(values))
}

async fn complete_branch(
    client: &GitlabClient,
    project: &str,
    value: &str,
) -> Result<Completion, GitlabError> {
    let params = QueryBuilder::new()
        .opt("per_page", Some(LIMIT))
        .opt("search", non_empty(value))
        .into_params();
    let branches = client
        .get_with_params(
            &format!("{}/repository/branches", project_path(project)),
            &params,
        )
        .await?;
    let values: Vec<String> = branches
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|b| b["name"].as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    Ok(full_page(values))
}

/// Suggest IIDs of recently updated open issues/MRs, prefix-filtered on the
/// digits typed so far (an empty value suggests the most recent ones).
async fn complete_iid(
    client: &GitlabClient,
    project: &str,
    kind: &str,
    value: &str,
) -> Result<Completion, GitlabError> {
    let params = QueryBuilder::new()
        .opt("state", Some("opened"))
        .opt("order_by", Some("updated_at"))
        .opt("per_page", Some(100))
        .into_params();
    let items = client
        .get_with_params(&format!("{}/{kind}", project_path(project)), &params)
        .await?;
    let prefix = value.trim();
    let matches: Vec<String> = items
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|i| i["iid"].as_u64())
                .map(|iid| iid.to_string())
                .filter(|iid| iid.starts_with(prefix))
                .collect()
        })
        .unwrap_or_default();
    let has_more = matches.len() > LIMIT;
    Ok(Completion {
        values: matches.into_iter().take(LIMIT).collect(),
        has_more,
    })
}

/// A full page implies GitLab likely has more matches.
const fn full_page(values: Vec<String>) -> Completion {
    let has_more = values.len() >= LIMIT;
    Completion { values, has_more }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::complete_argument;
    use crate::test_util::mock_client;

    #[tokio::test]
    async fn project_id_searches_member_projects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects"))
            .and(query_param("membership", "true"))
            .and(query_param("search", "gitlab"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "path_with_namespace": "3kirt1/gitlab-mcp-testing"},
                {"id": 2, "path_with_namespace": "3kirt1/gitlab-mcp"}
            ])))
            .mount(&server)
            .await;

        let c = complete_argument(&mock_client(&server), false, "project_id", "gitlab", None)
            .await
            .unwrap();
        assert_eq!(
            c.values,
            vec!["3kirt1/gitlab-mcp-testing", "3kirt1/gitlab-mcp"]
        );
        assert!(!c.has_more);

        // For a resource-template reference the same values are URI-encoded.
        let c = complete_argument(&mock_client(&server), true, "project_id", "gitlab", None)
            .await
            .unwrap();
        assert_eq!(c.values[0], "3kirt1%2Fgitlab-mcp-testing");
    }

    #[tokio::test]
    async fn branch_requires_project_context() {
        let server = MockServer::start().await;
        // No mocks mounted: without context this must not call the API.
        let c = complete_argument(&mock_client(&server), false, "branch", "fe", None)
            .await
            .unwrap();
        assert!(c.values.is_empty());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn branch_searches_context_project_decoding_encoded_paths() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            // An encoded context project_id must not be double-encoded.
            .and(path("/api/v4/projects/mygroup%2Fproj/repository/branches"))
            .and(query_param("search", "fe"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"name": "feat/resources"},
                {"name": "feat/prompts"}
            ])))
            .mount(&server)
            .await;

        let ctx = std::collections::HashMap::from([(
            "project_id".to_string(),
            "mygroup%2Fproj".to_string(),
        )]);
        let c = complete_argument(&mock_client(&server), false, "branch", "fe", Some(&ctx))
            .await
            .unwrap();
        assert_eq!(c.values, vec!["feat/resources", "feat/prompts"]);
    }

    #[tokio::test]
    async fn issue_iid_prefix_filters_open_issues() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/issues"))
            .and(query_param("state", "opened"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"iid": 794}, {"iid": 71}, {"iid": 12}
            ])))
            .mount(&server)
            .await;

        let ctx = std::collections::HashMap::from([("project_id".to_string(), "42".to_string())]);
        let c = complete_argument(&mock_client(&server), false, "issue_iid", "7", Some(&ctx))
            .await
            .unwrap();
        assert_eq!(c.values, vec!["794", "71"]);
    }

    #[tokio::test]
    async fn full_page_sets_has_more() {
        let server = MockServer::start().await;
        let branches: Vec<_> = (0..20).map(|i| json!({"name": format!("b{i}")})).collect();
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(branches)))
            .mount(&server)
            .await;

        let ctx = std::collections::HashMap::from([("project_id".to_string(), "42".to_string())]);
        let c = complete_argument(&mock_client(&server), false, "branch", "", Some(&ctx))
            .await
            .unwrap();
        assert_eq!(c.values.len(), 20);
        assert!(c.has_more);
    }

    #[tokio::test]
    async fn unknown_argument_returns_no_suggestions() {
        let server = MockServer::start().await;
        let c = complete_argument(&mock_client(&server), false, "file_path", "src", None)
            .await
            .unwrap();
        assert!(c.values.is_empty());
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
