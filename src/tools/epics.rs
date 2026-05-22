//! Group-level GitLab epics.
//!
//! REST-style MCP tool surface (`group_id`, `epic_iid`) layered on top of the
//! GraphQL plumbing in [`crate::tools::work_items`]. Users never see
//! `gid://gitlab/WorkItem/N` strings, `WorkItemsType` enums, or cursor
//! mechanics for IID-based lookups.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, GraphqlListResult, GraphqlPageInfo};
use crate::tools::work_items::{
    WorkItemCreateParams, WorkItemDeleteParams, WorkItemUpdateParams, work_item_create,
    work_item_delete, work_item_update,
};

// --------------------------------------------------------------------------
// Resolver helpers
// --------------------------------------------------------------------------

/// Resolve a numeric or path-style `group_id` to a GitLab namespace full path.
/// Returns the input unchanged if already path-style.
async fn resolve_group_path(client: &GitlabClient, group_id: &str) -> Result<String, GitlabError> {
    let trimmed = group_id.trim();
    if trimmed.is_empty() {
        return Err(GitlabError::Graphql("group_id must not be empty".into()));
    }
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        let data = client.get(&format!("/api/v4/groups/{trimmed}")).await?;
        return data["full_path"].as_str().map(String::from).ok_or_else(|| {
            GitlabError::Graphql(format!("group {trimmed} response missing full_path"))
        });
    }
    Ok(trimmed.to_string())
}

const RESOLVE_EPIC_QUERY: &str = r#"
query EpicIidToGid($fullPath: ID!, $iid: String!) {
  group(fullPath: $fullPath) {
    workItems(iid: $iid, first: 1) {
      nodes { id }
    }
  }
}
"#;

/// Resolve a `(group_path, epic_iid)` pair to the global WorkItem gid.
async fn resolve_epic_gid(
    client: &GitlabClient,
    group_path: &str,
    epic_iid: u64,
) -> Result<String, GitlabError> {
    let vars = json!({
        "fullPath": group_path,
        "iid": epic_iid.to_string(),
    });
    let data = client.graphql(RESOLVE_EPIC_QUERY, vars).await?;
    if data["group"].is_null() {
        return Err(GitlabError::Graphql(
            "group not found or not accessible".into(),
        ));
    }
    data["group"]["workItems"]["nodes"][0]["id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| {
            GitlabError::Graphql(format!("epic !{epic_iid} not found in group {group_path}"))
        })
}

/// Convenience: resolve_group_path then resolve_epic_gid.
async fn resolve_group_and_epic(
    client: &GitlabClient,
    group_id: &str,
    epic_iid: u64,
) -> Result<(String, String), GitlabError> {
    let group_path = resolve_group_path(client, group_id).await?;
    let gid = resolve_epic_gid(client, &group_path, epic_iid).await?;
    Ok((group_path, gid))
}

// --------------------------------------------------------------------------
// List epics
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EpicsListParams {
    #[schemars(
        description = "Group ID (numeric) or full namespace path (e.g. \"mygroup\" or \"mygroup/subgroup\")"
    )]
    pub group_id: String,
    #[schemars(description = "Filter by state: \"opened\" or \"closed\"")]
    pub state: Option<String>,
    #[schemars(description = "Search in title and description")]
    pub search: Option<String>,
    #[schemars(description = "Filter by author username")]
    pub author_username: Option<String>,
    #[schemars(description = "Filter by assignee usernames")]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(description = "Filter by label names")]
    pub label_name: Option<Vec<String>>,
    #[schemars(description = "Filter by group-relative epic IIDs (the number from the URL)")]
    pub iids: Option<Vec<String>>,
    #[schemars(
        description = "Sort order: CREATED_DESC, CREATED_ASC, UPDATED_DESC, UPDATED_ASC, TITLE_ASC, TITLE_DESC"
    )]
    pub sort: Option<String>,
    #[schemars(description = "Page size for cursor pagination (default 20, max 100)")]
    pub first: Option<i64>,
    #[schemars(
        description = "Cursor for forward pagination — pass end_cursor from a previous response"
    )]
    pub after: Option<String>,
}

const EPICS_LIST_QUERY: &str = r#"
query EpicsList(
  $fullPath: ID!
  $state: IssuableState
  $search: String
  $authorUsername: String
  $assigneeUsernames: [String!]
  $labelName: [String!]
  $iids: [String!]
  $sort: WorkItemSort
  $first: Int
  $after: String
) {
  group(fullPath: $fullPath) {
    workItems(
      types: [EPIC]
      state: $state
      search: $search
      authorUsername: $authorUsername
      assigneeUsernames: $assigneeUsernames
      labelName: $labelName
      iids: $iids
      sort: $sort
      first: $first
      after: $after
    ) {
      nodes {
        id
        iid
        title
        state
        createdAt
        updatedAt
        webUrl
        workItemType { name }
        widgets {
          type
          ... on WorkItemWidgetDescription { description }
          ... on WorkItemWidgetAssignees {
            assignees { nodes { name username } }
          }
          ... on WorkItemWidgetLabels {
            labels { nodes { title } }
          }
          ... on WorkItemWidgetHierarchy {
            parent { id iid title }
            hasChildren
          }
          ... on WorkItemWidgetStartAndDueDate {
            startDate
            dueDate
          }
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
}
"#;

pub async fn epics_list(client: &GitlabClient, p: EpicsListParams) -> GraphqlListResult {
    let group_path = resolve_group_path(client, &p.group_id).await?;
    let vars = json!({
        "fullPath": group_path,
        "state": p.state,
        "search": p.search,
        "authorUsername": p.author_username,
        "assigneeUsernames": p.assignee_usernames,
        "labelName": p.label_name,
        "iids": p.iids,
        "sort": p.sort,
        "first": p.first,
        "after": p.after,
    });
    let mut data = client.graphql(EPICS_LIST_QUERY, vars).await?;
    if data["group"].is_null() {
        return Err(GitlabError::Graphql(
            "group not found or not accessible".into(),
        ));
    }
    let wi = &mut data["group"]["workItems"];
    let has_next_page = wi["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
    let end_cursor = wi["pageInfo"]["endCursor"].as_str().map(String::from);
    let nodes = wi["nodes"].take();
    Ok((
        nodes,
        GraphqlPageInfo {
            has_next_page,
            end_cursor,
        },
    ))
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

const EPIC_GET_QUERY: &str = r#"
query EpicGet($fullPath: ID!, $iid: String!) {
  group(fullPath: $fullPath) {
    workItems(iid: $iid, first: 1) {
      nodes {
        id
        iid
        title
        state
        createdAt
        updatedAt
        closedAt
        webUrl
        author { name username }
        workItemType { name }
        namespace { fullPath }
        widgets {
          type
          ... on WorkItemWidgetDescription { description }
          ... on WorkItemWidgetAssignees {
            assignees { nodes { name username } }
          }
          ... on WorkItemWidgetLabels {
            labels { nodes { title color } }
          }
          ... on WorkItemWidgetMilestone {
            milestone { title id }
          }
          ... on WorkItemWidgetHierarchy {
            parent { id iid title }
            children { nodes { id iid title state } }
            hasChildren
          }
          ... on WorkItemWidgetStartAndDueDate {
            startDate
            dueDate
          }
          ... on WorkItemWidgetTimeTracking {
            timeEstimate
            totalTimeSpent
          }
          ... on WorkItemWidgetWeight {
            weight
          }
        }
      }
    }
  }
}
"#;

pub async fn epic_get(client: &GitlabClient, p: EpicGetParams) -> Result<Value, GitlabError> {
    let group_path = resolve_group_path(client, &p.group_id).await?;
    let vars = json!({
        "fullPath": group_path,
        "iid": p.epic_iid.to_string(),
    });
    let mut data = client.graphql(EPIC_GET_QUERY, vars).await?;
    if data["group"].is_null() {
        return Err(GitlabError::Graphql(
            "group not found or not accessible".into(),
        ));
    }
    let node = match data["group"]["workItems"]["nodes"].take() {
        Value::Array(arr) => arr.into_iter().next().unwrap_or(Value::Null),
        _ => Value::Null,
    };
    if node.is_null() {
        return Err(GitlabError::Graphql(format!(
            "epic !{} not found in group {}",
            p.epic_iid, group_path
        )));
    }
    Ok(node)
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
    #[schemars(
        description = "Assignee usernames. Every supplied username must resolve to a real GitLab user; the call fails with \"unknown username(s): …\" if any do not."
    )]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(description = "Parent epic IID (in the same group) to set as the hierarchy parent")]
    pub parent_epic_iid: Option<u64>,
    #[schemars(description = "Start date (ISO 8601, e.g. \"2024-01-01\")")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601, e.g. \"2024-12-31\")")]
    pub due_date: Option<String>,
}

pub async fn epic_create(client: &GitlabClient, p: EpicCreateParams) -> Result<Value, GitlabError> {
    let group_path = resolve_group_path(client, &p.group_id).await?;
    let parent_gid = if let Some(parent_iid) = p.parent_epic_iid {
        if parent_iid == 0 {
            return Err(GitlabError::Graphql(
                "parent_epic_iid=0 is only valid on update (to clear an existing parent)".into(),
            ));
        }
        Some(resolve_epic_gid(client, &group_path, parent_iid).await?)
    } else {
        None
    };

    let inner = WorkItemCreateParams {
        namespace_path: group_path,
        work_item_type: "EPIC".into(),
        title: p.title,
        description: p.description,
        assignee_usernames: p.assignee_usernames,
        parent_id: parent_gid,
        start_date: p.start_date,
        due_date: p.due_date,
    };
    work_item_create(client, inner).await
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
    #[schemars(description = "State change: \"CLOSE\" or \"REOPEN\"")]
    pub state_event: Option<String>,
    #[schemars(
        description = "Replace the full assignee list with these usernames. Pass an empty list to clear all assignees. Every supplied username must resolve to a real GitLab user."
    )]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(
        description = "New parent epic IID (in the same group). Pass 0 to remove the existing parent."
    )]
    pub parent_epic_iid: Option<u64>,
    #[schemars(description = "Start date (ISO 8601)")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601)")]
    pub due_date: Option<String>,
}

pub async fn epic_update(client: &GitlabClient, p: EpicUpdateParams) -> Result<Value, GitlabError> {
    let (group_path, epic_gid) = resolve_group_and_epic(client, &p.group_id, p.epic_iid).await?;

    let parent_id: Option<Value> = match p.parent_epic_iid {
        None => None,
        Some(0) => Some(Value::Null),
        Some(iid) => Some(Value::String(
            resolve_epic_gid(client, &group_path, iid).await?,
        )),
    };

    let inner = WorkItemUpdateParams {
        id: epic_gid,
        title: p.title,
        description: p.description,
        state_event: p.state_event,
        assignee_usernames: p.assignee_usernames,
        parent_id,
        start_date: p.start_date,
        due_date: p.due_date,
    };
    work_item_update(client, inner).await
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
    let (_, epic_gid) = resolve_group_and_epic(client, &p.group_id, p.epic_iid).await?;
    work_item_delete(client, WorkItemDeleteParams { id: epic_gid }).await
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        EpicCreateParams, EpicDeleteParams, EpicGetParams, EpicUpdateParams, EpicsListParams,
        epic_create, epic_delete, epic_get, epic_update, epics_list, resolve_epic_gid,
        resolve_group_path,
    };
    use crate::client::{GitlabClient, GitlabError};

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn graphql_ok(data: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": data }))
    }

    // ------------------------------------------------------------------
    // resolve_group_path
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn resolve_group_path_passes_through_path() {
        let server = MockServer::start().await;
        // No mock — should not issue a request.
        let path = resolve_group_path(&mock_client(&server), "mygroup/subgroup")
            .await
            .unwrap();
        assert_eq!(path, "mygroup/subgroup");
    }

    #[tokio::test]
    async fn resolve_group_path_resolves_numeric_via_rest() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "full_path": "mygroup/subgroup",
            })))
            .mount(&server)
            .await;

        let path = resolve_group_path(&mock_client(&server), "42")
            .await
            .unwrap();
        assert_eq!(path, "mygroup/subgroup");
    }

    #[tokio::test]
    async fn resolve_group_path_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/9999"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = resolve_group_path(&mock_client(&server), "9999")
            .await
            .unwrap_err();
        assert!(matches!(err, GitlabError::Api { .. }));
    }

    #[tokio::test]
    async fn resolve_group_path_empty_input_errors() {
        let server = MockServer::start().await;
        let err = resolve_group_path(&mock_client(&server), "  ")
            .await
            .unwrap_err();
        match err {
            GitlabError::Graphql(msg) => assert!(msg.contains("must not be empty")),
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    // ------------------------------------------------------------------
    // resolve_epic_gid
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn resolve_epic_gid_returns_id_on_match() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "group": {
                    "workItems": {
                        "nodes": [{ "id": "gid://gitlab/WorkItem/77" }]
                    }
                }
            })))
            .mount(&server)
            .await;

        let gid = resolve_epic_gid(&mock_client(&server), "mygroup", 5)
            .await
            .unwrap();
        assert_eq!(gid, "gid://gitlab/WorkItem/77");
    }

    #[tokio::test]
    async fn resolve_epic_gid_errors_when_group_null() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({ "group": null })))
            .mount(&server)
            .await;

        let err = resolve_epic_gid(&mock_client(&server), "ghost", 1)
            .await
            .unwrap_err();
        match err {
            GitlabError::Graphql(msg) => assert!(msg.contains("group not found")),
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    #[tokio::test]
    async fn resolve_epic_gid_errors_when_iid_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "group": { "workItems": { "nodes": [] } }
            })))
            .mount(&server)
            .await;

        let err = resolve_epic_gid(&mock_client(&server), "mygroup", 999)
            .await
            .unwrap_err();
        match err {
            GitlabError::Graphql(msg) => {
                assert!(msg.contains("epic !999"));
                assert!(msg.contains("mygroup"));
            }
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    // ------------------------------------------------------------------
    // epics_list
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epics_list_returns_nodes_and_page_info() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "group": {
                    "workItems": {
                        "nodes": [
                            { "id": "gid://gitlab/WorkItem/1", "iid": "1", "title": "Q1", "state": "OPEN" }
                        ],
                        "pageInfo": { "hasNextPage": true, "endCursor": "cursor1" }
                    }
                }
            })))
            .mount(&server)
            .await;

        let p = EpicsListParams {
            group_id: "mygroup".into(),
            state: None,
            search: None,
            author_username: None,
            assignee_usernames: None,
            label_name: None,
            iids: None,
            sort: None,
            first: None,
            after: None,
        };
        let (nodes, page_info) = epics_list(&mock_client(&server), p).await.unwrap();
        assert_eq!(nodes[0]["title"], "Q1");
        assert!(page_info.has_next_page);
        assert_eq!(page_info.end_cursor.as_deref(), Some("cursor1"));
    }

    #[tokio::test]
    async fn epics_list_errors_when_group_null() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({ "group": null })))
            .mount(&server)
            .await;

        let p = EpicsListParams {
            group_id: "ghost".into(),
            state: None,
            search: None,
            author_username: None,
            assignee_usernames: None,
            label_name: None,
            iids: None,
            sort: None,
            first: None,
            after: None,
        };
        let err = epics_list(&mock_client(&server), p).await.unwrap_err();
        assert!(matches!(err, GitlabError::Graphql(_)));
    }

    // ------------------------------------------------------------------
    // epic_get
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_get_returns_first_node() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "group": {
                    "workItems": {
                        "nodes": [
                            { "id": "gid://gitlab/WorkItem/42", "iid": "5", "title": "Epic" }
                        ]
                    }
                }
            })))
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
        assert_eq!(item["title"], "Epic");
        assert_eq!(item["iid"], "5");
    }

    #[tokio::test]
    async fn epic_get_errors_when_iid_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "group": { "workItems": { "nodes": [] } }
            })))
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
        match err {
            GitlabError::Graphql(msg) => {
                assert!(msg.contains("epic !999"));
                assert!(msg.contains("mygroup"));
            }
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    #[tokio::test]
    async fn epic_get_resolves_numeric_group_id_via_rest() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "full_path": "resolved/group",
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "group": {
                    "workItems": {
                        "nodes": [
                            { "id": "gid://gitlab/WorkItem/7", "iid": "3", "title": "X" }
                        ]
                    }
                }
            })))
            .mount(&server)
            .await;

        let item = epic_get(
            &mock_client(&server),
            EpicGetParams {
                group_id: "42".into(),
                epic_iid: 3,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "X");
    }

    // ------------------------------------------------------------------
    // epic_create
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_create_passes_epic_type_and_namespace() {
        let server = MockServer::start().await;
        // Only one GraphQL call expected (no assignees, no parent).
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": {
                    "input": {
                        "namespacePath": "mygroup",
                        "workItemTypeId": "gid://gitlab/WorkItems::Type/8",
                        "title": "Roadmap",
                    }
                }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemCreate": {
                    "workItem": {
                        "id": "gid://gitlab/WorkItem/100",
                        "iid": "10",
                        "title": "Roadmap",
                        "state": "OPEN"
                    },
                    "errors": []
                }
            })))
            .mount(&server)
            .await;

        let item = epic_create(
            &mock_client(&server),
            EpicCreateParams {
                group_id: "mygroup".into(),
                title: "Roadmap".into(),
                description: None,
                assignee_usernames: None,
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
    async fn epic_create_resolves_parent_then_creates() {
        let server = MockServer::start().await;
        // Resolve-id call: matched by the `iid` variable, which only the resolve
        // query carries (mutations send `variables.input` instead).
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "fullPath": "mygroup", "iid": "7" }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "group": { "workItems": { "nodes": [{ "id": "gid://gitlab/WorkItem/55" }] } }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": {
                    "input": {
                        "namespacePath": "mygroup",
                        "workItemTypeId": "gid://gitlab/WorkItems::Type/8",
                        "hierarchyWidget": { "parentId": "gid://gitlab/WorkItem/55" }
                    }
                }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemCreate": {
                    "workItem": {
                        "id": "gid://gitlab/WorkItem/101",
                        "iid": "11",
                        "title": "Child",
                        "state": "OPEN"
                    },
                    "errors": []
                }
            })))
            .mount(&server)
            .await;

        let item = epic_create(
            &mock_client(&server),
            EpicCreateParams {
                group_id: "mygroup".into(),
                title: "Child".into(),
                description: None,
                assignee_usernames: None,
                parent_epic_iid: Some(7),
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["iid"], "11");
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
                assignee_usernames: None,
                parent_epic_iid: Some(0),
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap_err();
        match err {
            GitlabError::Graphql(msg) => assert!(msg.contains("parent_epic_iid=0")),
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    // ------------------------------------------------------------------
    // epic_update
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_update_resolves_then_updates() {
        let server = MockServer::start().await;
        // Resolve epic gid (matched by `iid` variable — unique to the resolve query).
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "fullPath": "mygroup", "iid": "5" }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "group": { "workItems": { "nodes": [{ "id": "gid://gitlab/WorkItem/200" }] } }
            })))
            .mount(&server)
            .await;

        // Update mutation.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": {
                    "input": {
                        "id": "gid://gitlab/WorkItem/200",
                        "stateEvent": "CLOSE"
                    }
                }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemUpdate": {
                    "workItem": {
                        "id": "gid://gitlab/WorkItem/200",
                        "iid": "5",
                        "title": "Closed Epic",
                        "state": "CLOSED"
                    },
                    "errors": []
                }
            })))
            .mount(&server)
            .await;

        let item = epic_update(
            &mock_client(&server),
            EpicUpdateParams {
                group_id: "mygroup".into(),
                epic_iid: 5,
                title: None,
                description: None,
                state_event: Some("CLOSE".into()),
                assignee_usernames: None,
                parent_epic_iid: None,
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["state"], "CLOSED");
    }

    #[tokio::test]
    async fn epic_update_omits_hierarchy_widget_when_parent_unset() {
        let server = MockServer::start().await;
        // Resolve epic gid.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "fullPath": "mygroup", "iid": "5" }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "group": { "workItems": { "nodes": [{ "id": "gid://gitlab/WorkItem/500" }] } }
            })))
            .mount(&server)
            .await;

        // Update mutation responds OK; we'll assert hierarchyWidget absence below.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "id": "gid://gitlab/WorkItem/500", "title": "Renamed" } }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemUpdate": {
                    "workItem": {
                        "id": "gid://gitlab/WorkItem/500",
                        "iid": "5",
                        "title": "Renamed",
                        "state": "OPEN"
                    },
                    "errors": []
                }
            })))
            .mount(&server)
            .await;

        epic_update(
            &mock_client(&server),
            EpicUpdateParams {
                group_id: "mygroup".into(),
                epic_iid: 5,
                title: Some("Renamed".into()),
                description: None,
                state_event: None,
                assignee_usernames: None,
                parent_epic_iid: None,
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        let mutation_body = requests
            .iter()
            .map(|r| r.body_json::<serde_json::Value>().unwrap())
            .find(|body| body["variables"]["input"]["id"] == "gid://gitlab/WorkItem/500")
            .expect("update mutation request not found");
        let input = mutation_body["variables"]["input"]
            .as_object()
            .expect("input should be an object");
        assert!(
            !input.contains_key("hierarchyWidget"),
            "expected hierarchyWidget to be absent when parent_epic_iid is None, got: {mutation_body}"
        );
    }

    #[tokio::test]
    async fn epic_update_parent_iid_zero_sends_null() {
        let server = MockServer::start().await;
        // Resolve epic gid (matched by `iid` variable — unique to the resolve query).
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "fullPath": "mygroup", "iid": "9" }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "group": { "workItems": { "nodes": [{ "id": "gid://gitlab/WorkItem/300" }] } }
            })))
            .mount(&server)
            .await;

        // Expect hierarchyWidget with explicit null parentId.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": {
                    "input": {
                        "id": "gid://gitlab/WorkItem/300",
                        "hierarchyWidget": { "parentId": null }
                    }
                }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemUpdate": {
                    "workItem": {
                        "id": "gid://gitlab/WorkItem/300",
                        "iid": "9",
                        "title": "Orphan",
                        "state": "OPEN"
                    },
                    "errors": []
                }
            })))
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
                assignee_usernames: None,
                parent_epic_iid: Some(0),
                start_date: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Orphan");
    }

    // ------------------------------------------------------------------
    // epic_delete
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn epic_delete_resolves_then_deletes() {
        let server = MockServer::start().await;
        // Resolve epic gid (matched by `iid` variable — unique to the resolve query).
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "fullPath": "mygroup", "iid": "12" }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "group": { "workItems": { "nodes": [{ "id": "gid://gitlab/WorkItem/400" }] } }
            })))
            .mount(&server)
            .await;
        // Delete mutation.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "id": "gid://gitlab/WorkItem/400" } }
            })))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemDelete": { "errors": [] }
            })))
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
}
