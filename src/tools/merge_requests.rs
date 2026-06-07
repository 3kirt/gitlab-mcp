use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id, list_paginated,
    unwrap_404_as_empty_array, unwrap_404_or_403_as_empty_array,
};

fn default_true() -> bool {
    true
}

// --------------------------------------------------------------------------
// List merge requests
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrsListParams {
    #[schemars(
        description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")"
    )]
    pub project_id: String,
    #[schemars(
        description = "Filter by state: \"opened\", \"closed\", \"merged\", \"locked\", or \"all\" (default: \"all\")"
    )]
    pub state: Option<String>,
    #[schemars(description = "Filter by source branch name")]
    pub source_branch: Option<String>,
    #[schemars(description = "Filter by target branch name")]
    pub target_branch: Option<String>,
    #[schemars(description = "Filter by author user ID")]
    pub author_id: Option<u64>,
    #[schemars(description = "Filter by assignee user ID")]
    pub assignee_id: Option<u64>,
    #[schemars(description = "Filter by reviewer user ID")]
    pub reviewer_id: Option<u64>,
    #[schemars(description = "Comma-separated label names to filter by")]
    pub labels: Option<String>,
    #[schemars(description = "Search in title and description")]
    pub search: Option<String>,
    #[schemars(description = "Filter by draft status (true/false)")]
    pub draft: Option<bool>,
    #[schemars(
        description = "Scope: \"created_by_me\", \"assigned_to_me\", \"reviews_for_me\", or \"all\" (default: \"all\")"
    )]
    pub scope: Option<String>,
    #[schemars(
        description = "Return only MRs created after this datetime (ISO 8601, e.g. \"2024-01-01T00:00:00Z\")"
    )]
    pub created_after: Option<String>,
    #[schemars(description = "Return only MRs created before this datetime (ISO 8601)")]
    pub created_before: Option<String>,
    #[schemars(description = "Return only MRs updated after this datetime (ISO 8601)")]
    pub updated_after: Option<String>,
    #[schemars(description = "Return only MRs updated before this datetime (ISO 8601)")]
    pub updated_before: Option<String>,
    #[schemars(
        description = "Order by: \"created_at\", \"updated_at\", \"merged_at\", \"title\" (default: \"created_at\")"
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"desc\")")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mrs_list(client: &GitlabClient, p: MrsListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/merge_requests",
        encode_namespace_id(&p.project_id)
    );
    let qb = QueryBuilder::new()
        .opt("state", p.state)
        .opt("source_branch", p.source_branch)
        .opt("target_branch", p.target_branch)
        .opt("author_id", p.author_id)
        .opt("assignee_id", p.assignee_id)
        .opt("reviewer_id", p.reviewer_id)
        .opt("labels", p.labels)
        .opt("search", p.search)
        .opt("draft", p.draft)
        .opt("scope", p.scope)
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before)
        .opt("updated_after", p.updated_after)
        .opt("updated_before", p.updated_before)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get single merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(
        description = "Merge request internal ID (IID) — the number shown in the GitLab UI"
    )]
    pub merge_request_iid: u64,
}

pub async fn mr_get(client: &GitlabClient, p: MrGetParams) -> Result<Value, GitlabError> {
    let pid = encode_namespace_id(&p.project_id);
    let iid = p.merge_request_iid;
    let mr_path = format!("/api/v4/projects/{pid}/merge_requests/{iid}");
    let closes_path = format!("/api/v4/projects/{pid}/merge_requests/{iid}/closes_issues");
    let related_path = format!("/api/v4/projects/{pid}/merge_requests/{iid}/related_issues");
    let (mut mr, closes_issues, related_issues) = tokio::try_join!(
        client.get(&mr_path),
        async { unwrap_404_as_empty_array(client.get(&closes_path).await) },
        async { unwrap_404_or_403_as_empty_array(client.get(&related_path).await) },
    )?;
    mr["closes_issues"] = closes_issues;
    mr["related_issues"] = related_issues;
    Ok(mr)
}

// --------------------------------------------------------------------------
// Create merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Name of the source branch")]
    pub source_branch: String,
    #[schemars(description = "Name of the target branch")]
    pub target_branch: String,
    #[schemars(description = "Merge request title")]
    pub title: String,
    #[schemars(description = "Merge request description (Markdown supported)")]
    pub description: Option<String>,
    #[schemars(description = "User ID to assign the MR to")]
    pub assignee_id: Option<u64>,
    #[schemars(description = "User IDs to request reviews from")]
    pub reviewer_ids: Option<Vec<u64>>,
    #[schemars(description = "Comma-separated label names")]
    pub labels: Option<String>,
    #[schemars(description = "Milestone ID to associate with the MR")]
    pub milestone_id: Option<u64>,
    #[serde(default = "default_true")]
    #[schemars(description = "Squash commits on merge (default: true)")]
    pub squash: bool,
    #[serde(default = "default_true")]
    #[schemars(description = "Remove source branch after merge (default: true)")]
    pub remove_source_branch: bool,
    #[schemars(
        description = "Mark as draft (true/false); implemented via the \"Draft: \" title prefix"
    )]
    pub draft: Option<bool>,
}

pub async fn mr_create(client: &GitlabClient, p: MrCreateParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests",
        encode_namespace_id(&p.project_id)
    );
    // GitLab ignores the `draft` body field; the title prefix is the reliable mechanism.
    let title = match p.draft {
        Some(true) if !p.title.starts_with("Draft:") => format!("Draft: {}", p.title),
        _ => p.title,
    };
    let body = BodyBuilder::new()
        .req("source_branch", &p.source_branch)
        .req("target_branch", &p.target_branch)
        .req("title", &title)
        .req("squash", p.squash)
        .req("remove_source_branch", p.remove_source_branch)
        .opt("description", p.description)
        .opt("assignee_id", p.assignee_id)
        .opt("reviewer_ids", p.reviewer_ids)
        .opt("labels", p.labels)
        .opt("milestone_id", p.milestone_id)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Update merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrUpdateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New description (Markdown supported)")]
    pub description: Option<String>,
    #[schemars(description = "State transition: \"close\" or \"reopen\"")]
    pub state_event: Option<String>,
    #[schemars(description = "New target branch")]
    pub target_branch: Option<String>,
    #[schemars(description = "User ID to assign the MR to (0 to unassign)")]
    pub assignee_id: Option<u64>,
    #[schemars(description = "User IDs to request reviews from (replaces existing reviewers)")]
    pub reviewer_ids: Option<Vec<u64>>,
    #[schemars(description = "Comma-separated label names (replaces existing labels)")]
    pub labels: Option<String>,
    #[schemars(description = "Milestone ID (0 to remove the milestone)")]
    pub milestone_id: Option<u64>,
    #[schemars(description = "Squash commits on merge (true/false)")]
    pub squash: Option<bool>,
    #[schemars(
        description = "Mark as draft (true) or ready to merge (false); adds or removes the \"Draft: \" title prefix"
    )]
    pub draft: Option<bool>,
}

pub async fn mr_update(client: &GitlabClient, p: MrUpdateParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    // GitLab's update endpoint doesn't accept `draft` as a body field; draft status is
    // controlled by the "Draft: " title prefix. Fetch the current title when needed.
    let effective_title = match p.draft {
        Some(make_draft) => {
            let base = match p.title {
                Some(ref t) => t.clone(),
                None => {
                    let current = client.get(&path).await?;
                    current["title"].as_str().unwrap_or("").to_string()
                }
            };
            Some(if make_draft {
                if base.starts_with("Draft:") {
                    base
                } else {
                    format!("Draft: {}", base)
                }
            } else {
                base.strip_prefix("Draft: ")
                    .or_else(|| base.strip_prefix("Draft:"))
                    .unwrap_or(&base)
                    .to_string()
            })
        }
        None => p.title,
    };
    let body = BodyBuilder::new()
        .opt("title", effective_title)
        .opt("description", p.description)
        .opt("state_event", p.state_event)
        .opt("target_branch", p.target_branch)
        .opt("assignee_id", p.assignee_id)
        .opt("reviewer_ids", p.reviewer_ids)
        .opt("labels", p.labels)
        .opt("milestone_id", p.milestone_id)
        .opt("squash", p.squash)
        .build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
}

pub async fn mr_delete(client: &GitlabClient, p: MrDeleteParams) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Merge (accept) merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrMergeParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(description = "Custom merge commit message")]
    pub merge_commit_message: Option<String>,
    #[schemars(description = "Squash commits on merge (true/false)")]
    pub squash: Option<bool>,
    #[schemars(description = "Remove source branch after merge (true/false)")]
    pub should_remove_source_branch: Option<bool>,
    #[schemars(description = "Merge only once the pipeline succeeds (true/false)")]
    pub merge_when_pipeline_succeeds: Option<bool>,
}

pub async fn mr_merge(client: &GitlabClient, p: MrMergeParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/merge",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    let body = BodyBuilder::new()
        .opt("merge_commit_message", p.merge_commit_message)
        .opt("squash", p.squash)
        .opt("should_remove_source_branch", p.should_remove_source_branch)
        .opt(
            "merge_when_pipeline_succeeds",
            p.merge_when_pipeline_succeeds,
        )
        .build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Approve merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrApproveParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
    #[schemars(
        description = "The HEAD commit SHA of the merge request. If specified, GitLab rejects the approval if the MR has since been updated."
    )]
    pub sha: Option<String>,
    #[schemars(
        description = "Current user's password. Required only if re-authentication is enabled on the instance."
    )]
    pub approval_password: Option<String>,
}

pub async fn mr_approve(client: &GitlabClient, p: MrApproveParams) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/approve",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    let body = BodyBuilder::new()
        .opt("sha", p.sha)
        .opt("approval_password", p.approval_password)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Unapprove merge request
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrUnapproveParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID)")]
    pub merge_request_iid: u64,
}

pub async fn mr_unapprove(client: &GitlabClient, p: MrUnapproveParams) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/unapprove",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    client.post_void(&path, &serde_json::json!({})).await
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        MrApproveParams, MrGetParams, MrUnapproveParams, mr_approve, mr_get, mr_unapprove,
    };
    use crate::client::GitlabClient;

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn mr_json(iid: u64) -> serde_json::Value {
        serde_json::json!({
            "id": iid * 100,
            "iid": iid,
            "project_id": 1,
            "title": format!("MR {iid}"),
            "state": "opened",
            "source_branch": "feature",
            "target_branch": "main",
            "web_url": format!("https://gitlab.example.com/p/-/mrs/{iid}"),
        })
    }

    #[tokio::test]
    async fn mr_get_embeds_closes_and_related_issues() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyrepo/merge_requests/12"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mr_json(12)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v4/projects/mygroup%2Fmyrepo/merge_requests/12/closes_issues",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 11, "iid": 4, "title": "Bug", "state": "opened", "project_id": 1 }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v4/projects/mygroup%2Fmyrepo/merge_requests/12/related_issues",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 22, "iid": 5, "title": "Linked", "state": "opened", "project_id": 1 }
            ])))
            .mount(&server)
            .await;

        let item = mr_get(
            &mock_client(&server),
            MrGetParams {
                project_id: "mygroup/myrepo".into(),
                merge_request_iid: 12,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["iid"], 12);
        assert_eq!(item["closes_issues"][0]["iid"], 4);
        assert_eq!(item["related_issues"][0]["iid"], 5);
    }

    #[tokio::test]
    async fn mr_get_degrades_related_issues_on_403() {
        // related_issues is Premium/Ultimate; lower tiers return 403. Must surface as [].
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mr_json(3)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/3/closes_issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/3/related_issues"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;

        let item = mr_get(
            &mock_client(&server),
            MrGetParams {
                project_id: "p".into(),
                merge_request_iid: 3,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["closes_issues"], serde_json::json!([]));
        assert_eq!(item["related_issues"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn mr_get_does_not_swallow_403_on_closes_issues() {
        // closes_issues is not tier-gated; a 403 there is a real failure, not licensing.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mr_json(3)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/3/closes_issues"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/3/related_issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let err = mr_get(
            &mock_client(&server),
            MrGetParams {
                project_id: "p".into(),
                merge_request_iid: 3,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, crate::client::GitlabError::Api { status, .. } if status.as_u16() == 403)
        );
    }

    #[tokio::test]
    async fn mr_get_propagates_404_for_mr_itself() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/999"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/999/closes_issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/p/merge_requests/999/related_issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let err = mr_get(
            &mock_client(&server),
            MrGetParams {
                project_id: "p".into(),
                merge_request_iid: 999,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, crate::client::GitlabError::Api { status, .. } if status.as_u16() == 404)
        );
    }

    // ------------------------------------------------------------------
    // mr_approve
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mr_approve_posts_and_returns_approval_state() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(
                "/api/v4/projects/mygroup%2Fmyrepo/merge_requests/3/approve",
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 3,
                "iid": 3,
                "approvals_left": 0,
                "approved_by": [{ "user": { "id": 1, "username": "alice" } }]
            })))
            .mount(&server)
            .await;

        let item = mr_approve(
            &mock_client(&server),
            MrApproveParams {
                project_id: "mygroup/myrepo".into(),
                merge_request_iid: 3,
                sha: None,
                approval_password: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["approvals_left"], 0);
        assert_eq!(item["approved_by"][0]["user"]["username"], "alice");
    }

    #[tokio::test]
    async fn mr_approve_propagates_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/p/merge_requests/99/approve"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = mr_approve(
            &mock_client(&server),
            MrApproveParams {
                project_id: "p".into(),
                merge_request_iid: 99,
                sha: None,
                approval_password: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }

    // ------------------------------------------------------------------
    // mr_unapprove
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn mr_unapprove_posts_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/p/merge_requests/3/unapprove"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let result = mr_unapprove(
            &mock_client(&server),
            MrUnapproveParams {
                project_id: "p".into(),
                merge_request_iid: 3,
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn mr_unapprove_propagates_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/p/merge_requests/99/unapprove"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = mr_unapprove(
            &mock_client(&server),
            MrUnapproveParams {
                project_id: "p".into(),
                merge_request_iid: 99,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }
}
