//! Group-level GitLab epics via the REST API.
//!
//! Uses `GET/POST/PUT/DELETE /api/v4/groups/:id/epics[/:iid]`.
//! The REST Epics API is deprecated since GitLab 17.0 (planned removal in API
//! v5) but remains fully functional on GitLab EE 18.x, where epics have not
//! been migrated to the work-items architecture and the work-items GraphQL API
//! rejects Epic GIDs. Revisit when API v5 ships.

use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, PaginationMeta};
use crate::tools::{BodyBuilder, QueryBuilder, encode_project_id};

// --------------------------------------------------------------------------
// List epics
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EpicsListParams {
    #[schemars(
        description = "Group ID (numeric) or full namespace path (e.g. \"mygroup\" or \"mygroup/subgroup\")"
    )]
    pub group_id: String,
    #[schemars(description = "Filter by state: \"opened\", \"closed\", or \"all\"")]
    pub state: Option<String>,
    #[schemars(description = "Search in title and description")]
    pub search: Option<String>,
    #[schemars(description = "Filter by author username")]
    pub author_username: Option<String>,
    #[schemars(description = "Filter by label names")]
    pub label_name: Option<Vec<String>>,
    #[schemars(description = "Filter by group-relative epic IIDs (the number from the URL)")]
    pub iids: Option<Vec<String>>,
    #[schemars(description = "Sort field: \"created_at\", \"updated_at\", or \"title\"")]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\"")]
    pub sort: Option<String>,
    #[schemars(description = "Page number (default: 1)")]
    pub page: Option<u64>,
    #[schemars(description = "Number of results per page (default: 20, max: 100)")]
    pub per_page: Option<u64>,
}

pub async fn epics_list(
    client: &GitlabClient,
    p: EpicsListParams,
) -> Result<(Value, PaginationMeta), GitlabError> {
    let gid = encode_project_id(&p.group_id);
    let labels = p.label_name.map(|v| v.join(","));
    let params = QueryBuilder::new()
        .opt("state", p.state)
        .opt("search", p.search)
        .opt("author_username", p.author_username)
        .opt("labels", labels)
        .multi("iids[]", p.iids)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("page", p.page)
        .opt("per_page", p.per_page)
        .into_params();
    client
        .list(&format!("/api/v4/groups/{gid}/epics"), &params)
        .await
}

// --------------------------------------------------------------------------
// Get single epic
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EpicGetParams {
    #[schemars(description = "Group ID (numeric) or full namespace path")]
    pub group_id: String,
    #[schemars(description = "Epic IID (the number from the URL `/groups/<g>/-/epics/<iid>`)")]
    pub epic_iid: u64,
}

pub async fn epic_get(client: &GitlabClient, p: EpicGetParams) -> Result<Value, GitlabError> {
    let gid = encode_project_id(&p.group_id);
    let iid = p.epic_iid;
    let mut epic = client
        .get(&format!("/api/v4/groups/{gid}/epics/{iid}"))
        .await?;
    // Supplement with linked issues — the REST hierarchy only surfaces child
    // epics, not classic epic→issue associations.
    let issues = client
        .get(&format!("/api/v4/groups/{gid}/epics/{iid}/issues"))
        .await
        .unwrap_or(Value::Array(vec![]));
    epic["linked_issues"] = issues;
    Ok(epic)
}

// --------------------------------------------------------------------------
// Create epic
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EpicCreateParams {
    #[schemars(description = "Group ID (numeric) or full namespace path")]
    pub group_id: String,
    #[schemars(description = "Epic title")]
    pub title: String,
    #[schemars(description = "Epic description (Markdown)")]
    pub description: Option<String>,
    #[schemars(description = "Comma-separated label names to apply")]
    pub labels: Option<String>,
    #[schemars(description = "Parent epic IID (in the same group) to set as the hierarchy parent")]
    pub parent_epic_iid: Option<u64>,
    #[schemars(description = "Start date (ISO 8601, e.g. \"2024-01-01\")")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601, e.g. \"2024-12-31\")")]
    pub due_date: Option<String>,
}

pub async fn epic_create(
    client: &GitlabClient,
    p: EpicCreateParams,
) -> Result<Value, GitlabError> {
    let gid = encode_project_id(&p.group_id);

    let parent_id: Option<u64> = if let Some(parent_iid) = p.parent_epic_iid {
        if parent_iid == 0 {
            return Err(GitlabError::Graphql(
                "parent_epic_iid=0 is only valid on update (to clear an existing parent)".into(),
            ));
        }
        let parent = client
            .get(&format!("/api/v4/groups/{gid}/epics/{parent_iid}"))
            .await?;
        let id = parent["id"]
            .as_u64()
            .ok_or_else(|| GitlabError::Graphql("parent epic response missing id field".into()))?;
        Some(id)
    } else {
        None
    };

    let mut body = BodyBuilder::new().req("title", &p.title);
    body = body.opt("description", p.description);
    body = body.opt("labels", p.labels);
    body = body.opt("parent_id", parent_id);
    if let Some(date) = p.start_date {
        body = body
            .req("start_date_is_fixed", true)
            .req("start_date_fixed", date);
    }
    if let Some(date) = p.due_date {
        body = body
            .req("due_date_is_fixed", true)
            .req("due_date_fixed", date);
    }

    client
        .post(&format!("/api/v4/groups/{gid}/epics"), &body.build())
        .await
}

// --------------------------------------------------------------------------
// Update epic
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EpicUpdateParams {
    #[schemars(description = "Group ID (numeric) or full namespace path")]
    pub group_id: String,
    #[schemars(description = "Epic IID (the number from the URL)")]
    pub epic_iid: u64,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New description (Markdown)")]
    pub description: Option<String>,
    #[schemars(description = "State change: \"close\" or \"reopen\"")]
    pub state_event: Option<String>,
    #[schemars(description = "Comma-separated label names (replaces current labels)")]
    pub labels: Option<String>,
    #[schemars(description = "Comma-separated label names to add")]
    pub add_labels: Option<String>,
    #[schemars(description = "Comma-separated label names to remove")]
    pub remove_labels: Option<String>,
    #[schemars(
        description = "New parent epic IID (in the same group). Pass 0 to remove the existing parent."
    )]
    pub parent_epic_iid: Option<u64>,
    #[schemars(description = "Start date (ISO 8601)")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601)")]
    pub due_date: Option<String>,
}

pub async fn epic_update(
    client: &GitlabClient,
    p: EpicUpdateParams,
) -> Result<Value, GitlabError> {
    let gid = encode_project_id(&p.group_id);
    let iid = p.epic_iid;

    let parent_id: Option<u64> = match p.parent_epic_iid {
        None => None,
        Some(0) => Some(0),
        Some(parent_iid) => {
            let parent = client
                .get(&format!("/api/v4/groups/{gid}/epics/{parent_iid}"))
                .await?;
            let id = parent["id"].as_u64().ok_or_else(|| {
                GitlabError::Graphql("parent epic response missing id field".into())
            })?;
            Some(id)
        }
    };

    let mut body = BodyBuilder::new();
    body = body.opt("title", p.title);
    body = body.opt("description", p.description);
    body = body.opt("state_event", p.state_event);
    body = body.opt("labels", p.labels);
    body = body.opt("add_labels", p.add_labels);
    body = body.opt("remove_labels", p.remove_labels);
    body = body.opt("parent_id", parent_id);
    if let Some(date) = p.start_date {
        body = body
            .req("start_date_is_fixed", true)
            .req("start_date_fixed", date);
    }
    if let Some(date) = p.due_date {
        body = body
            .req("due_date_is_fixed", true)
            .req("due_date_fixed", date);
    }

    client
        .put(&format!("/api/v4/groups/{gid}/epics/{iid}"), &body.build())
        .await
}

// --------------------------------------------------------------------------
// Delete epic
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EpicDeleteParams {
    #[schemars(description = "Group ID (numeric) or full namespace path")]
    pub group_id: String,
    #[schemars(description = "Epic IID (the number from the URL)")]
    pub epic_iid: u64,
}

pub async fn epic_delete(client: &GitlabClient, p: EpicDeleteParams) -> Result<(), GitlabError> {
    let gid = encode_project_id(&p.group_id);
    let iid = p.epic_iid;
    client
        .delete(&format!("/api/v4/groups/{gid}/epics/{iid}"))
        .await
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        EpicCreateParams, EpicDeleteParams, EpicGetParams, EpicUpdateParams, EpicsListParams,
        epic_create, epic_delete, epic_get, epic_update, epics_list,
    };
    use crate::client::GitlabClient;

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn epic_json(iid: u64, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": iid * 10,
            "iid": iid,
            "group_id": 1,
            "title": title,
            "state": "opened",
            "web_url": format!("https://gitlab.example.com/groups/mygroup/-/epics/{iid}"),
            "created_at": "2024-01-01T00:00:00.000Z",
            "updated_at": "2024-01-01T00:00:00.000Z"
        })
    }

    // ------------------------------------------------------------------
    // epics_list
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epics_list_returns_items_and_pagination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([
                        epic_json(1, "Alpha"),
                        epic_json(2, "Beta"),
                    ]))
                    .insert_header("x-page", "1")
                    .insert_header("x-per-page", "20")
                    .insert_header("x-total", "2")
                    .insert_header("x-total-pages", "1")
                    .insert_header("x-next-page", ""),
            )
            .mount(&server)
            .await;

        let (items, meta) = epics_list(
            &mock_client(&server),
            EpicsListParams {
                group_id: "mygroup".into(),
                state: None,
                search: None,
                author_username: None,
                label_name: None,
                iids: None,
                order_by: None,
                sort: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(items.as_array().unwrap().len(), 2);
        assert_eq!(items[0]["title"], "Alpha");
        assert_eq!(meta.total, Some(2));
    }

    #[tokio::test]
    async fn epics_list_encodes_numeric_group_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/42/epics"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([]))
                    .insert_header("x-page", "1")
                    .insert_header("x-per-page", "20")
                    .insert_header("x-total", "0")
                    .insert_header("x-total-pages", "1")
                    .insert_header("x-next-page", ""),
            )
            .mount(&server)
            .await;

        let (items, _) = epics_list(
            &mock_client(&server),
            EpicsListParams {
                group_id: "42".into(),
                state: None,
                search: None,
                author_username: None,
                label_name: None,
                iids: None,
                order_by: None,
                sort: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .unwrap();
        assert!(items.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn epics_list_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/ghost/epics"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = epics_list(
            &mock_client(&server),
            EpicsListParams {
                group_id: "ghost".into(),
                state: None,
                search: None,
                author_username: None,
                label_name: None,
                iids: None,
                order_by: None,
                sort: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }

    // ------------------------------------------------------------------
    // epic_get
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_get_returns_epic_with_linked_issues() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(epic_json(5, "Big Feature")))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics/5/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 101, "iid": 1, "title": "Sub-issue" }
            ])))
            .mount(&server)
            .await;

        let item = epic_get(
            &mock_client(&server),
            EpicGetParams {
                group_id: "mygroup".into(),
                epic_iid: 5,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["title"], "Big Feature");
        assert_eq!(item["linked_issues"][0]["title"], "Sub-issue");
    }

    #[tokio::test]
    async fn epic_get_tolerates_missing_issues_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics/3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(epic_json(3, "Solo Epic")))
            .mount(&server)
            .await;
        // No mock for /issues — returns 404, should be swallowed.
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics/3/issues"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let item = epic_get(
            &mock_client(&server),
            EpicGetParams {
                group_id: "mygroup".into(),
                epic_iid: 3,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["title"], "Solo Epic");
        assert_eq!(item["linked_issues"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn epic_get_propagates_404_for_epic_itself() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics/999"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = epic_get(
            &mock_client(&server),
            EpicGetParams {
                group_id: "mygroup".into(),
                epic_iid: 999,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }

    // ------------------------------------------------------------------
    // epic_create
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_create_posts_title_and_returns_epic() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/groups/mygroup/epics"))
            .respond_with(ResponseTemplate::new(201).set_body_json(epic_json(10, "Roadmap")))
            .mount(&server)
            .await;

        let item = epic_create(
            &mock_client(&server),
            EpicCreateParams {
                group_id: "mygroup".into(),
                title: "Roadmap".into(),
                description: None,
                labels: None,
                parent_epic_iid: None,
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Roadmap");
    }

    #[tokio::test]
    async fn epic_create_resolves_parent_iid_to_numeric_id() {
        let server = MockServer::start().await;
        // First call: resolve parent epic (GET parent IID=7 → id=70).
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(epic_json(7, "Parent")))
            .mount(&server)
            .await;
        // Second call: create the epic.
        Mock::given(method("POST"))
            .and(path("/api/v4/groups/mygroup/epics"))
            .respond_with(ResponseTemplate::new(201).set_body_json(epic_json(11, "Child")))
            .mount(&server)
            .await;

        let item = epic_create(
            &mock_client(&server),
            EpicCreateParams {
                group_id: "mygroup".into(),
                title: "Child".into(),
                description: None,
                labels: None,
                parent_epic_iid: Some(7),
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Child");
    }

    #[tokio::test]
    async fn epic_create_rejects_parent_iid_zero() {
        let server = MockServer::start().await;
        let err = epic_create(
            &mock_client(&server),
            EpicCreateParams {
                group_id: "mygroup".into(),
                title: "X".into(),
                description: None,
                labels: None,
                parent_epic_iid: Some(0),
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Graphql(msg) => {
                assert!(msg.contains("parent_epic_iid=0"))
            }
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    // ------------------------------------------------------------------
    // epic_update
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_update_sends_state_event_and_returns_epic() {
        let server = MockServer::start().await;
        let mut closed = epic_json(5, "Closed Epic");
        closed["state"] = serde_json::json!("closed");
        Mock::given(method("PUT"))
            .and(path("/api/v4/groups/mygroup/epics/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(closed))
            .mount(&server)
            .await;

        let item = epic_update(
            &mock_client(&server),
            EpicUpdateParams {
                group_id: "mygroup".into(),
                epic_iid: 5,
                title: None,
                description: None,
                state_event: Some("close".into()),
                labels: None,
                add_labels: None,
                remove_labels: None,
                parent_epic_iid: None,
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["state"], "closed");
    }

    #[tokio::test]
    async fn epic_update_resolves_parent_iid_to_numeric_id() {
        let server = MockServer::start().await;
        // Resolve parent IID=3 → id=30.
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup/epics/3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(epic_json(3, "New Parent")))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v4/groups/mygroup/epics/9"))
            .respond_with(ResponseTemplate::new(200).set_body_json(epic_json(9, "Re-parented")))
            .mount(&server)
            .await;

        let item = epic_update(
            &mock_client(&server),
            EpicUpdateParams {
                group_id: "mygroup".into(),
                epic_iid: 9,
                title: None,
                description: None,
                state_event: None,
                labels: None,
                add_labels: None,
                remove_labels: None,
                parent_epic_iid: Some(3),
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Re-parented");
    }

    #[tokio::test]
    async fn epic_update_parent_iid_zero_sends_zero_parent_id() {
        let server = MockServer::start().await;
        // No GET — parent_id=0 goes straight to PUT.
        let mut orphan = epic_json(9, "Orphan");
        orphan["parent_id"] = serde_json::json!(null);
        Mock::given(method("PUT"))
            .and(path("/api/v4/groups/mygroup/epics/9"))
            .respond_with(ResponseTemplate::new(200).set_body_json(orphan))
            .mount(&server)
            .await;

        let item = epic_update(
            &mock_client(&server),
            EpicUpdateParams {
                group_id: "mygroup".into(),
                epic_iid: 9,
                title: None,
                description: None,
                state_event: None,
                labels: None,
                add_labels: None,
                remove_labels: None,
                parent_epic_iid: Some(0),
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Orphan");
        // Verify the PUT body contained parent_id=0.
        let reqs = server.received_requests().await.unwrap();
        let put_body = reqs
            .iter()
            .find(|r| r.method == wiremock::http::Method::PUT)
            .and_then(|r| r.body_json::<serde_json::Value>().ok())
            .expect("PUT request not found");
        assert_eq!(put_body["parent_id"], 0);
    }

    // ------------------------------------------------------------------
    // epic_delete
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_delete_sends_delete_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/groups/mygroup/epics/12"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let result = epic_delete(
            &mock_client(&server),
            EpicDeleteParams {
                group_id: "mygroup".into(),
                epic_iid: 12,
            },
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn epic_delete_propagates_403() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/groups/mygroup/epics/1"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;

        let err = epic_delete(
            &mock_client(&server),
            EpicDeleteParams {
                group_id: "mygroup".into(),
                epic_iid: 1,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }
}
