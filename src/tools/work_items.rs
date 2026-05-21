//! Internal GraphQL primitives for GitLab work items.
//!
//! No public MCP tools live here — the user-facing surface for epics is in
//! [`crate::tools::epics`], which composes the create/update/delete primitives
//! and shared helpers (`type_name_to_gid`, `usernames_to_ids`,
//! `check_mutation_errors`, `add_shared_widgets`) defined here.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError};
use crate::tools::BodyBuilder;

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

/// Convert a short work item type name (e.g. "TASK") to its GitLab global type ID.
/// Pass-through for strings that already start with "gid://".
//
// The numeric IDs below are seeded by GitLab migrations and are stable on gitlab.com,
// but a self-hosted instance with custom types could see different IDs. Callers can
// always bypass the mapping by passing a full "gid://gitlab/WorkItems::Type/<id>".
pub(crate) fn type_name_to_gid(s: &str) -> String {
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

/// Look up user IDs by username(s) via GraphQL. Returns numeric user IDs (order unspecified).
/// Returns an error if any input username does not resolve to a GitLab user — this prevents
/// a typo from silently dropping an assignee.
pub(crate) async fn usernames_to_ids(
    client: &GitlabClient,
    usernames: Vec<String>,
) -> Result<Vec<i64>, GitlabError> {
    if usernames.is_empty() {
        return Ok(vec![]);
    }

    const USER_LOOKUP_QUERY: &str = r#"
    query UsersLookup($usernames: [String!]!) {
      users(usernames: $usernames) {
        nodes {
          id
          username
        }
      }
    }
    "#;

    let vars = json!({ "usernames": &usernames });
    let data = client.graphql(USER_LOOKUP_QUERY, vars).await?;
    let nodes = data["users"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut ids = Vec::with_capacity(nodes.len());
    let mut found: std::collections::HashSet<String> = std::collections::HashSet::new();
    for node in &nodes {
        if let Some(u) = node["username"].as_str() {
            found.insert(u.to_lowercase());
        }
        if let Some(id) = node["id"]
            .as_str()
            .and_then(|gid| gid.rsplit('/').next())
            .and_then(|s| s.parse().ok())
        {
            ids.push(id);
        }
    }

    let missing: Vec<&str> = usernames
        .iter()
        .filter(|u| !found.contains(&u.to_lowercase()))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(GitlabError::Graphql(format!(
            "unknown username(s): {}",
            missing.join(", ")
        )));
    }

    Ok(ids)
}

/// Extract mutation-level errors from a GraphQL response payload and return
/// `Err(GitlabError::Graphql)` if any are present.
pub(crate) fn check_mutation_errors(payload: &Value, field: &str) -> Result<(), GitlabError> {
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

/// Append the work item widget fields shared by create and update mutations.
/// Each widget is omitted entirely when its corresponding parameter is `None`.
pub(crate) fn add_shared_widgets(
    builder: BodyBuilder,
    description: Option<String>,
    assignee_ids: Option<Vec<i64>>,
    parent_id: Option<Value>,
    start_date: Option<String>,
    due_date: Option<String>,
) -> BodyBuilder {
    let dates_widget = (start_date.is_some() || due_date.is_some())
        .then(|| json!({ "startDate": start_date, "dueDate": due_date }));
    builder
        .opt(
            "descriptionWidget",
            description.map(|d| json!({ "description": d })),
        )
        .opt(
            "assigneesWidget",
            assignee_ids.map(|ids| json!({ "assigneeIds": ids })),
        )
        .opt(
            "hierarchyWidget",
            parent_id.map(|id| json!({ "parentId": id })),
        )
        .opt("startAndDueDateWidget", dates_widget)
}

// --------------------------------------------------------------------------
// Create work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct WorkItemCreateParams {
    #[schemars(
        description = "Full namespace path (project like \"mygroup/myproject\" or group like \"mygroup\"). Numeric IDs are not supported by the GraphQL API."
    )]
    pub namespace_path: String,
    #[schemars(
        description = "Work item type: ISSUE, TASK, EPIC, TICKET, INCIDENT, TEST_CASE, REQUIREMENT, OBJECTIVE, KEY_RESULT, or a full \"gid://gitlab/WorkItems::Type/<id>\" string"
    )]
    pub work_item_type: String,
    #[schemars(description = "Work item title")]
    pub title: String,
    #[schemars(description = "Work item description (Markdown)")]
    pub description: Option<String>,
    #[schemars(
        description = "Assignee usernames. Every username must resolve to a real GitLab user; the call fails with \"unknown username(s): …\" if any do not."
    )]
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

pub(crate) async fn work_item_create(
    client: &GitlabClient,
    p: WorkItemCreateParams,
) -> Result<Value, GitlabError> {
    let assignee_ids = if let Some(usernames) = p.assignee_usernames {
        Some(usernames_to_ids(client, usernames).await?)
    } else {
        None
    };

    let input = add_shared_widgets(
        BodyBuilder::new()
            .req("namespacePath", p.namespace_path)
            .req("workItemTypeId", type_name_to_gid(&p.work_item_type))
            .req("title", p.title),
        p.description,
        assignee_ids,
        p.parent_id.map(Value::String),
        p.start_date,
        p.due_date,
    )
    .build();

    let vars = json!({ "input": input });
    let mut data = client.graphql(CREATE_MUTATION, vars).await?;
    check_mutation_errors(&data, "workItemCreate")?;
    Ok(data["workItemCreate"]["workItem"].take())
}

// --------------------------------------------------------------------------
// Update work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct WorkItemUpdateParams {
    #[schemars(description = "Work item global ID (e.g. \"gid://gitlab/WorkItem/123\")")]
    pub id: String,
    #[schemars(description = "New title")]
    pub title: Option<String>,
    #[schemars(description = "New description (Markdown)")]
    pub description: Option<String>,
    #[schemars(description = "State change: \"CLOSE\" or \"REOPEN\"")]
    pub state_event: Option<String>,
    #[schemars(
        description = "Replace the full assignee list with these usernames. Pass an empty list to clear all assignees. Every supplied username must resolve to a real GitLab user; the call fails with \"unknown username(s): …\" if any do not."
    )]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(
        description = "Set or change the hierarchy parent. Pass a global ID string (e.g. \"gid://gitlab/WorkItem/123\") to set, or `null` to clear the existing parent."
    )]
    pub parent_id: Option<Value>,
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

pub(crate) async fn work_item_update(
    client: &GitlabClient,
    p: WorkItemUpdateParams,
) -> Result<Value, GitlabError> {
    let assignee_ids = if let Some(usernames) = p.assignee_usernames {
        Some(usernames_to_ids(client, usernames).await?)
    } else {
        None
    };

    let input = add_shared_widgets(
        BodyBuilder::new()
            .req("id", p.id)
            .opt("title", p.title)
            .opt("stateEvent", p.state_event),
        p.description,
        assignee_ids,
        p.parent_id,
        p.start_date,
        p.due_date,
    )
    .build();

    let vars = json!({ "input": input });
    let mut data = client.graphql(UPDATE_MUTATION, vars).await?;
    check_mutation_errors(&data, "workItemUpdate")?;
    Ok(data["workItemUpdate"]["workItem"].take())
}

// --------------------------------------------------------------------------
// Delete work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct WorkItemDeleteParams {
    #[schemars(description = "Work item global ID (e.g. \"gid://gitlab/WorkItem/123\")")]
    pub id: String,
}

const DELETE_MUTATION: &str = r#"
mutation WorkItemDelete($input: WorkItemDeleteInput!) {
  workItemDelete(input: $input) {
    errors
  }
}
"#;

pub(crate) async fn work_item_delete(
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
        WorkItemCreateParams, WorkItemDeleteParams, WorkItemUpdateParams, check_mutation_errors,
        type_name_to_gid, usernames_to_ids, work_item_create, work_item_delete, work_item_update,
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
    // usernames_to_ids
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn usernames_to_ids_empty_input_skips_request() {
        // No mock is mounted — if the function sent a request, wiremock would 404 and the
        // call would error. Returning Ok(vec![]) proves the short-circuit fired.
        let server = MockServer::start().await;
        let ids = usernames_to_ids(&mock_client(&server), vec![])
            .await
            .unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn usernames_to_ids_extracts_numeric_ids_from_gids() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "users": {
                    "nodes": [
                        { "id": "gid://gitlab/User/5", "username": "alice" },
                        { "id": "gid://gitlab/User/42", "username": "bob" }
                    ]
                }
            })))
            .mount(&server)
            .await;

        let mut ids = usernames_to_ids(&mock_client(&server), vec!["alice".into(), "bob".into()])
            .await
            .unwrap();
        ids.sort();
        assert_eq!(ids, vec![5, 42]);
    }

    #[tokio::test]
    async fn usernames_to_ids_matches_case_insensitively() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "users": {
                    "nodes": [
                        { "id": "gid://gitlab/User/5", "username": "alice" }
                    ]
                }
            })))
            .mount(&server)
            .await;

        let ids = usernames_to_ids(&mock_client(&server), vec!["Alice".into()])
            .await
            .unwrap();
        assert_eq!(ids, vec![5]);
    }

    #[tokio::test]
    async fn usernames_to_ids_errors_on_unknown_username() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "users": { "nodes": [] }
            })))
            .mount(&server)
            .await;

        let err = usernames_to_ids(&mock_client(&server), vec!["ghost".into()])
            .await
            .unwrap_err();
        match err {
            GitlabError::Graphql(msg) => {
                assert!(msg.contains("unknown username"), "msg: {msg}");
                assert!(msg.contains("ghost"), "msg: {msg}");
            }
            other => panic!("expected Graphql error, got {other}"),
        }
    }

    #[tokio::test]
    async fn usernames_to_ids_partial_mismatch_names_only_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(graphql_ok(serde_json::json!({
                "users": {
                    "nodes": [
                        { "id": "gid://gitlab/User/5", "username": "alice" }
                    ]
                }
            })))
            .mount(&server)
            .await;

        let err = usernames_to_ids(
            &mock_client(&server),
            vec!["alice".into(), "ghost".into(), "phantom".into()],
        )
        .await
        .unwrap_err();
        match err {
            GitlabError::Graphql(msg) => {
                assert!(msg.contains("ghost"), "msg: {msg}");
                assert!(msg.contains("phantom"), "msg: {msg}");
                assert!(
                    !msg.contains("alice"),
                    "should not mention resolved username: {msg}"
                );
            }
            other => panic!("expected Graphql error, got {other}"),
        }
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
            namespace_path: "mygroup/myrepo".into(),
            work_item_type: "TASK".into(),
            title: "New task".into(),
            description: Some("Do the thing".into()),
            assignee_usernames: None,
            parent_id: None,
            start_date: None,
            due_date: None,
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
            namespace_path: "mygroup/myrepo".into(),
            work_item_type: "TASK".into(),
            title: "".into(),
            description: None,
            assignee_usernames: None,
            parent_id: None,
            start_date: None,
            due_date: None,
        };
        let err = work_item_create(&mock_client(&server), p)
            .await
            .unwrap_err();
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
            description: None,
            assignee_usernames: None,
            parent_id: None,
            start_date: None,
            due_date: None,
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
            title: None,
            description: None,
            state_event: None,
            assignee_usernames: None,
            parent_id: None,
            start_date: None,
            due_date: None,
        };
        let err = work_item_update(&mock_client(&server), p)
            .await
            .unwrap_err();
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

        let p = WorkItemDeleteParams {
            id: "gid://gitlab/WorkItem/1".into(),
        };
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

        let p = WorkItemDeleteParams {
            id: "gid://gitlab/WorkItem/1".into(),
        };
        let err = work_item_delete(&mock_client(&server), p)
            .await
            .unwrap_err();
        assert!(matches!(err, GitlabError::Graphql(_)));
    }
}
