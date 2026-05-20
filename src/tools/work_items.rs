use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, GraphqlListResult, GraphqlPageInfo};

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

/// Convert a short work item type name (e.g. "TASK") to its GitLab global type ID.
/// Pass-through for strings that already start with "gid://".
fn type_name_to_gid(s: &str) -> String {
    if s.starts_with("gid://") {
        return s.to_string();
    }
    let id: u32 = match s.to_uppercase().as_str() {
        "ISSUE" => 1,
        "INCIDENT" => 2,
        "TEST_CASE" => 3,
        "REQUIREMENT" => 4,
        "TASK" => 5,
        "OBJECTIVE" => 6,
        "KEY_RESULT" => 7,
        "EPIC" => 8,
        "TICKET" => 9,
        _ => return s.to_string(),
    };
    format!("gid://gitlab/WorkItems::Type/{id}")
}

/// Extract mutation-level errors from a GraphQL response payload and return
/// `Err(GitlabError::Graphql)` if any are present.
fn check_mutation_errors(payload: &Value, field: &str) -> Result<(), GitlabError> {
    if let Some(errors) = payload[field]["errors"].as_array()
        && !errors.is_empty()
    {
        let msg = errors
            .iter()
            .filter_map(|e| e.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(GitlabError::Graphql(msg));
    }
    Ok(())
}

// --------------------------------------------------------------------------
// List work items
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemsListParams {
    #[schemars(
        description = "Full project path (e.g. \"mygroup/myproject\"). Numeric IDs are not supported by the GraphQL API."
    )]
    pub project_path: String,
    #[schemars(
        description = "Filter by work item type(s): ISSUE, TASK, EPIC, TICKET, INCIDENT, TEST_CASE, REQUIREMENT, OBJECTIVE, KEY_RESULT"
    )]
    pub types: Option<Vec<String>>,
    #[schemars(description = "Filter by state: \"opened\" or \"closed\"")]
    pub state: Option<String>,
    #[schemars(description = "Search in title and description")]
    pub search: Option<String>,
    #[schemars(description = "Filter by assignee usernames")]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(description = "Filter by author username")]
    pub author_username: Option<String>,
    #[schemars(description = "Filter by label names")]
    pub label_name: Option<Vec<String>>,
    #[schemars(description = "Filter by project-relative IIDs")]
    pub iids: Option<Vec<String>>,
    #[schemars(
        description = "Sort order (e.g. CREATED_DESC, CREATED_ASC, UPDATED_DESC, UPDATED_ASC, TITLE_ASC, TITLE_DESC)"
    )]
    pub sort: Option<String>,
    #[schemars(
        description = "Number of items to return for cursor-based pagination (default 20, max 100)"
    )]
    pub first: Option<i64>,
    #[schemars(
        description = "Cursor for forward pagination — pass end_cursor from a previous response"
    )]
    pub after: Option<String>,
}

const LIST_QUERY: &str = r#"
query WorkItemsList(
  $fullPath: ID!
  $types: [IssueType!]
  $state: IssuableState
  $search: String
  $assigneeUsernames: [String!]
  $authorUsername: String
  $labelName: [String!]
  $iids: [String!]
  $sort: WorkItemSort
  $first: Int
  $after: String
) {
  project(fullPath: $fullPath) {
    workItems(
      types: $types
      state: $state
      search: $search
      assigneeUsernames: $assigneeUsernames
      authorUsername: $authorUsername
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

pub async fn work_items_list(client: &GitlabClient, p: WorkItemsListParams) -> GraphqlListResult {
    let vars = json!({
        "fullPath": p.project_path,
        "types": p.types,
        "state": p.state,
        "search": p.search,
        "assigneeUsernames": p.assignee_usernames,
        "authorUsername": p.author_username,
        "labelName": p.label_name,
        "iids": p.iids,
        "sort": p.sort,
        "first": p.first,
        "after": p.after,
    });

    let mut data = client.graphql(LIST_QUERY, vars).await?;
    if data["project"].is_null() {
        return Err(GitlabError::Graphql(
            "project not found or not accessible".into(),
        ));
    }
    let wi = &mut data["project"]["workItems"];
    let has_next_page = wi["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
    let end_cursor = wi["pageInfo"]["endCursor"].as_str().map(String::from);
    let nodes = wi["nodes"].take();
    Ok((nodes, GraphqlPageInfo { has_next_page, end_cursor }))
}

// --------------------------------------------------------------------------
// Get single work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemGetParams {
    #[schemars(
        description = "Work item global ID (e.g. \"gid://gitlab/WorkItem/123\"). Returned by list and create operations."
    )]
    pub id: String,
}

const GET_QUERY: &str = r#"
query WorkItemGet($id: WorkItemID!) {
  workItem(id: $id) {
    id
    iid
    title
    state
    createdAt
    updatedAt
    closedAt
    webUrl
    author { name username }
    workItemType { name id }
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
"#;

pub async fn work_item_get(client: &GitlabClient, p: WorkItemGetParams) -> Result<Value, GitlabError> {
    let vars = json!({ "id": p.id });
    let mut data = client.graphql(GET_QUERY, vars).await?;
    let item = data["workItem"].take();
    if item.is_null() {
        return Err(GitlabError::Graphql("work item not found".into()));
    }
    Ok(item)
}

// --------------------------------------------------------------------------
// Create work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemCreateParams {
    #[schemars(
        description = "Full project path (e.g. \"mygroup/myproject\"). Numeric IDs are not supported by the GraphQL API."
    )]
    pub project_path: String,
    #[schemars(
        description = "Work item type: ISSUE, TASK, EPIC, TICKET, INCIDENT, TEST_CASE, REQUIREMENT, OBJECTIVE, KEY_RESULT, or a full \"gid://gitlab/WorkItems::Type/<id>\" string"
    )]
    pub work_item_type: String,
    #[schemars(description = "Work item title")]
    pub title: String,
    #[schemars(description = "Work item description (Markdown)")]
    pub description: Option<String>,
    #[schemars(description = "Assignee usernames")]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(
        description = "Parent work item global ID (e.g. \"gid://gitlab/WorkItem/123\") to set a hierarchy parent"
    )]
    pub parent_id: Option<String>,
    #[schemars(description = "Start date (ISO 8601, e.g. \"2024-01-01\")")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601, e.g. \"2024-12-31\")")]
    pub due_date: Option<String>,
}

const CREATE_MUTATION: &str = r#"
mutation WorkItemCreate($input: WorkItemCreateInput!) {
  workItemCreate(input: $input) {
    workItem {
      id
      iid
      title
      state
      webUrl
      createdAt
      workItemType { name }
    }
    errors
  }
}
"#;

pub async fn work_item_create(
    client: &GitlabClient,
    p: WorkItemCreateParams,
) -> Result<Value, GitlabError> {
    let mut input = serde_json::Map::new();
    input.insert("projectPath".into(), p.project_path.into());
    input.insert(
        "workItemTypeId".into(),
        type_name_to_gid(&p.work_item_type).into(),
    );
    input.insert("title".into(), p.title.into());
    if let Some(desc) = p.description {
        input.insert("descriptionWidget".into(), json!({ "description": desc }));
    }
    if let Some(usernames) = p.assignee_usernames {
        input.insert(
            "assigneesWidget".into(),
            json!({ "assigneeUsernames": usernames }),
        );
    }
    if let Some(parent_id) = p.parent_id {
        input.insert("hierarchyWidget".into(), json!({ "parentId": parent_id }));
    }
    if p.start_date.is_some() || p.due_date.is_some() {
        input.insert(
            "startAndDueDateWidget".into(),
            json!({ "startDate": p.start_date, "dueDate": p.due_date }),
        );
    }

    let vars = json!({ "input": Value::Object(input) });
    let mut data = client.graphql(CREATE_MUTATION, vars).await?;
    check_mutation_errors(&data, "workItemCreate")?;
    Ok(data["workItemCreate"]["workItem"].take())
}

// --------------------------------------------------------------------------
// Update work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemUpdateParams {
    #[schemars(
        description = "Work item global ID (e.g. \"gid://gitlab/WorkItem/123\")"
    )]
    pub id: String,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New description (Markdown)")]
    pub description: Option<String>,
    #[schemars(description = "State change: \"CLOSE\" or \"REOPEN\"")]
    pub state_event: Option<String>,
    #[schemars(
        description = "Replace the full assignee list with these usernames. Pass an empty list to clear all assignees."
    )]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(
        description = "Set or change the hierarchy parent by providing its global ID (e.g. \"gid://gitlab/WorkItem/123\")"
    )]
    pub parent_id: Option<String>,
    #[schemars(description = "Start date (ISO 8601)")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601)")]
    pub due_date: Option<String>,
}

const UPDATE_MUTATION: &str = r#"
mutation WorkItemUpdate($input: WorkItemUpdateInput!) {
  workItemUpdate(input: $input) {
    workItem {
      id
      iid
      title
      state
      webUrl
      updatedAt
      workItemType { name }
    }
    errors
  }
}
"#;

pub async fn work_item_update(
    client: &GitlabClient,
    p: WorkItemUpdateParams,
) -> Result<Value, GitlabError> {
    let mut input = serde_json::Map::new();
    input.insert("id".into(), p.id.into());
    if let Some(title) = p.title {
        input.insert("title".into(), title.into());
    }
    if let Some(desc) = p.description {
        input.insert("descriptionWidget".into(), json!({ "description": desc }));
    }
    if let Some(state_event) = p.state_event {
        input.insert("stateEvent".into(), state_event.into());
    }
    if let Some(usernames) = p.assignee_usernames {
        input.insert(
            "assigneesWidget".into(),
            json!({ "assigneeUsernames": usernames }),
        );
    }
    if let Some(parent_id) = p.parent_id {
        input.insert("hierarchyWidget".into(), json!({ "parentId": parent_id }));
    }
    if p.start_date.is_some() || p.due_date.is_some() {
        input.insert(
            "startAndDueDateWidget".into(),
            json!({ "startDate": p.start_date, "dueDate": p.due_date }),
        );
    }

    let vars = json!({ "input": Value::Object(input) });
    let mut data = client.graphql(UPDATE_MUTATION, vars).await?;
    check_mutation_errors(&data, "workItemUpdate")?;
    Ok(data["workItemUpdate"]["workItem"].take())
}

// --------------------------------------------------------------------------
// Delete work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemDeleteParams {
    #[schemars(
        description = "Work item global ID (e.g. \"gid://gitlab/WorkItem/123\")"
    )]
    pub id: String,
}

const DELETE_MUTATION: &str = r#"
mutation WorkItemDelete($input: WorkItemDeleteInput!) {
  workItemDelete(input: $input) {
    errors
  }
}
"#;

pub async fn work_item_delete(
    client: &GitlabClient,
    p: WorkItemDeleteParams,
) -> Result<(), GitlabError> {
    let vars = json!({ "input": { "id": p.id } });
    let data = client.graphql(DELETE_MUTATION, vars).await?;
    check_mutation_errors(&data, "workItemDelete")?;
    Ok(())
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        WorkItemCreateParams, WorkItemDeleteParams, WorkItemGetParams, WorkItemUpdateParams,
        WorkItemsListParams, check_mutation_errors, type_name_to_gid, work_item_create,
        work_item_delete, work_item_get, work_item_update, work_items_list,
    };
    use crate::client::{GitlabClient, GitlabError};

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn graphql_ok(data: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": data }))
    }

    // ------------------------------------------------------------------
    // type_name_to_gid
    // ------------------------------------------------------------------

    #[test]
    fn type_name_to_gid_maps_all_known_types() {
        let cases = [
            ("ISSUE", 1),
            ("INCIDENT", 2),
            ("TEST_CASE", 3),
            ("REQUIREMENT", 4),
            ("TASK", 5),
            ("OBJECTIVE", 6),
            ("KEY_RESULT", 7),
            ("EPIC", 8),
            ("TICKET", 9),
        ];
        for (name, id) in cases {
            assert_eq!(
                type_name_to_gid(name),
                format!("gid://gitlab/WorkItems::Type/{id}"),
                "failed for {name}"
            );
        }
    }

    #[test]
    fn type_name_to_gid_is_case_insensitive() {
        assert_eq!(type_name_to_gid("task"), "gid://gitlab/WorkItems::Type/5");
        assert_eq!(type_name_to_gid("Task"), "gid://gitlab/WorkItems::Type/5");
        assert_eq!(type_name_to_gid("ePiC"), "gid://gitlab/WorkItems::Type/8");
    }

    #[test]
    fn type_name_to_gid_passes_through_existing_gid() {
        let gid = "gid://gitlab/WorkItems::Type/5";
        assert_eq!(type_name_to_gid(gid), gid);
    }

    #[test]
    fn type_name_to_gid_passes_through_unknown_names() {
        assert_eq!(type_name_to_gid("CUSTOM"), "CUSTOM");
        assert_eq!(type_name_to_gid(""), "");
    }

    // ------------------------------------------------------------------
    // check_mutation_errors
    // ------------------------------------------------------------------

    #[test]
    fn check_mutation_errors_ok_on_empty_array() {
        let payload = serde_json::json!({ "workItemCreate": { "errors": [] } });
        assert!(check_mutation_errors(&payload, "workItemCreate").is_ok());
    }

    #[test]
    fn check_mutation_errors_ok_when_field_absent() {
        let payload = serde_json::json!({ "workItemCreate": {} });
        assert!(check_mutation_errors(&payload, "workItemCreate").is_ok());
    }

    #[test]
    fn check_mutation_errors_err_joins_messages() {
        let payload = serde_json::json!({
            "workItemCreate": { "errors": ["Title can't be blank", "Type is invalid"] }
        });
        let err = check_mutation_errors(&payload, "workItemCreate").unwrap_err();
        match err {
            GitlabError::Graphql(msg) => {
                assert!(msg.contains("Title can't be blank"), "msg: {msg}");
                assert!(msg.contains("Type is invalid"), "msg: {msg}");
            }
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    // ------------------------------------------------------------------
    // work_items_list
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_items_list_returns_nodes_and_page_info() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "project": {
                    "workItems": {
                        "nodes": [
                            { "id": "gid://gitlab/WorkItem/1", "iid": "1", "title": "Fix bug", "state": "OPEN" }
                        ],
                        "pageInfo": { "hasNextPage": true, "endCursor": "cursor42" }
                    }
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemsListParams {
            project_path: "mygroup/myrepo".into(),
            types: None, state: None, search: None, assignee_usernames: None,
            author_username: None, label_name: None, iids: None, sort: None,
            first: None, after: None,
        };
        let (nodes, page_info) = work_items_list(&mock_client(&server), p).await.unwrap();
        assert_eq!(nodes[0]["title"], "Fix bug");
        assert!(page_info.has_next_page);
        assert_eq!(page_info.end_cursor.as_deref(), Some("cursor42"));
    }

    #[tokio::test]
    async fn work_items_list_last_page_has_no_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "project": {
                    "workItems": {
                        "nodes": [],
                        "pageInfo": { "hasNextPage": false, "endCursor": null }
                    }
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemsListParams {
            project_path: "mygroup/myrepo".into(),
            types: None, state: None, search: None, assignee_usernames: None,
            author_username: None, label_name: None, iids: None, sort: None,
            first: None, after: None,
        };
        let (nodes, page_info) = work_items_list(&mock_client(&server), p).await.unwrap();
        assert_eq!(nodes, serde_json::json!([]));
        assert!(!page_info.has_next_page);
        assert!(page_info.end_cursor.is_none());
    }

    #[tokio::test]
    async fn work_items_list_errors_when_project_null() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({ "project": null })))
            .mount(&server)
            .await;

        let p = WorkItemsListParams {
            project_path: "nonexistent/project".into(),
            types: None, state: None, search: None, assignee_usernames: None,
            author_username: None, label_name: None, iids: None, sort: None,
            first: None, after: None,
        };
        let err = work_items_list(&mock_client(&server), p).await.unwrap_err();
        assert!(matches!(err, GitlabError::Graphql(_)));
    }

    // ------------------------------------------------------------------
    // work_item_get
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_get_returns_item() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "workItem": {
                    "id": "gid://gitlab/WorkItem/42",
                    "iid": "5",
                    "title": "Fix the bug",
                    "state": "OPEN"
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemGetParams { id: "gid://gitlab/WorkItem/42".into() };
        let item = work_item_get(&mock_client(&server), p).await.unwrap();
        assert_eq!(item["id"], "gid://gitlab/WorkItem/42");
        assert_eq!(item["title"], "Fix the bug");
    }

    #[tokio::test]
    async fn work_item_get_errors_when_null() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({ "workItem": null })))
            .mount(&server)
            .await;

        let p = WorkItemGetParams { id: "gid://gitlab/WorkItem/999".into() };
        let err = work_item_get(&mock_client(&server), p).await.unwrap_err();
        assert!(matches!(err, GitlabError::Graphql(_)));
    }

    // ------------------------------------------------------------------
    // work_item_create
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_create_returns_item_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemCreate": {
                    "workItem": {
                        "id": "gid://gitlab/WorkItem/99",
                        "iid": "10",
                        "title": "New task",
                        "state": "OPEN"
                    },
                    "errors": []
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemCreateParams {
            project_path: "mygroup/myrepo".into(),
            work_item_type: "TASK".into(),
            title: "New task".into(),
            description: Some("Do the thing".into()),
            assignee_usernames: None, parent_id: None,
            start_date: None, due_date: None,
        };
        let item = work_item_create(&mock_client(&server), p).await.unwrap();
        assert_eq!(item["id"], "gid://gitlab/WorkItem/99");
        assert_eq!(item["title"], "New task");
    }

    #[tokio::test]
    async fn work_item_create_errors_on_mutation_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemCreate": {
                    "workItem": null,
                    "errors": ["Title can't be blank"]
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemCreateParams {
            project_path: "mygroup/myrepo".into(),
            work_item_type: "TASK".into(),
            title: "".into(),
            description: None, assignee_usernames: None,
            parent_id: None, start_date: None, due_date: None,
        };
        let err = work_item_create(&mock_client(&server), p).await.unwrap_err();
        match err {
            GitlabError::Graphql(msg) => assert!(msg.contains("Title can't be blank")),
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    // ------------------------------------------------------------------
    // work_item_update
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_update_returns_item_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemUpdate": {
                    "workItem": {
                        "id": "gid://gitlab/WorkItem/1",
                        "iid": "1",
                        "title": "Updated title",
                        "state": "CLOSED"
                    },
                    "errors": []
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemUpdateParams {
            id: "gid://gitlab/WorkItem/1".into(),
            title: Some("Updated title".into()),
            state_event: Some("CLOSE".into()),
            description: None, assignee_usernames: None,
            parent_id: None, start_date: None, due_date: None,
        };
        let item = work_item_update(&mock_client(&server), p).await.unwrap();
        assert_eq!(item["title"], "Updated title");
        assert_eq!(item["state"], "CLOSED");
    }

    #[tokio::test]
    async fn work_item_update_errors_on_mutation_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemUpdate": {
                    "workItem": null,
                    "errors": ["Work item not found"]
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemUpdateParams {
            id: "gid://gitlab/WorkItem/999".into(),
            title: None, description: None, state_event: None,
            assignee_usernames: None, parent_id: None,
            start_date: None, due_date: None,
        };
        let err = work_item_update(&mock_client(&server), p).await.unwrap_err();
        assert!(matches!(err, GitlabError::Graphql(_)));
    }

    // ------------------------------------------------------------------
    // work_item_delete
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_delete_returns_ok_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemDelete": { "errors": [] }
            })))
            .mount(&server)
            .await;

        let p = WorkItemDeleteParams { id: "gid://gitlab/WorkItem/1".into() };
        assert!(work_item_delete(&mock_client(&server), p).await.is_ok());
    }

    #[tokio::test]
    async fn work_item_delete_errors_on_mutation_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "workItemDelete": {
                    "errors": ["You don't have permission to delete this work item"]
                }
            })))
            .mount(&server)
            .await;

        let p = WorkItemDeleteParams { id: "gid://gitlab/WorkItem/1".into() };
        let err = work_item_delete(&mock_client(&server), p).await.unwrap_err();
        assert!(matches!(err, GitlabError::Graphql(_)));
    }
}
