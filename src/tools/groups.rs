use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, PaginationMeta};
use crate::tools::{QueryBuilder, encode_namespace_id};

// --------------------------------------------------------------------------
// List groups
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupsListParams {
    #[schemars(description = "Search by group name or path")]
    pub search: Option<String>,
    #[schemars(
        description = "Return all groups the current user has access to (true) or only groups the user is a member of (false, default)"
    )]
    pub all_available: Option<bool>,
    #[schemars(description = "Limit to groups owned by the current user")]
    pub owned: Option<bool>,
    #[schemars(
        description = "Minimum access level required: 10=Guest, 20=Reporter, 30=Developer, 40=Maintainer, 50=Owner"
    )]
    pub min_access_level: Option<u32>,
    #[schemars(
        description = "Sort field: \"name\", \"path\", \"id\", or \"similarity\" (similarity only valid with search)"
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\"")]
    pub sort: Option<String>,
    #[schemars(description = "Return only top-level groups (exclude subgroups)")]
    pub top_level_only: Option<bool>,
    #[schemars(description = "Page number (default: 1)")]
    pub page: Option<u64>,
    #[schemars(description = "Number of results per page (default: 20, max: 100)")]
    pub per_page: Option<u64>,
}

pub async fn groups_list(
    client: &GitlabClient,
    p: GroupsListParams,
) -> Result<(Value, PaginationMeta), GitlabError> {
    let params = QueryBuilder::new()
        .opt("search", p.search)
        .opt("all_available", p.all_available)
        .opt("owned", p.owned)
        .opt("min_access_level", p.min_access_level)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort)
        .opt("top_level_only", p.top_level_only)
        .opt("page", p.page)
        .opt("per_page", p.per_page)
        .into_params();
    client.list("/api/v4/groups", &params).await
}

// --------------------------------------------------------------------------
// Get single group
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupGetParams {
    #[schemars(
        description = "Group ID (numeric) or full namespace path (e.g. \"mygroup\" or \"mygroup/subgroup\")"
    )]
    pub group_id: String,
    #[schemars(
        description = "Include the group's projects in the response (max 100). Defaults to false to keep the response compact."
    )]
    pub with_projects: Option<bool>,
}

pub async fn group_get(client: &GitlabClient, p: GroupGetParams) -> Result<Value, GitlabError> {
    let gid = encode_namespace_id(&p.group_id);
    // GitLab's upstream default for `with_projects` is true (deprecated but still functional),
    // which would embed up to 100 projects on every group fetch. Send it explicitly so the
    // tool default stays compact and matches the schemars description.
    let params = QueryBuilder::new()
        .opt("with_projects", Some(p.with_projects.unwrap_or(false)))
        .into_params();
    client
        .get_with_params(&format!("/api/v4/groups/{gid}"), &params)
        .await
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{GroupGetParams, GroupsListParams, group_get, groups_list};
    use crate::client::GitlabClient;

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn group_json(id: u64, name: &str, path: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": name,
            "path": path,
            "full_path": path,
            "description": null,
            "visibility": "private",
            "web_url": format!("https://gitlab.example.com/groups/{path}"),
            "parent_id": null,
            "created_at": "2024-01-01T00:00:00.000Z"
        })
    }

    // ------------------------------------------------------------------
    // groups_list
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn groups_list_returns_items_and_pagination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([
                        group_json(1, "Alpha", "alpha"),
                        group_json(2, "Beta", "beta"),
                    ]))
                    .insert_header("x-page", "1")
                    .insert_header("x-per-page", "20")
                    .insert_header("x-total", "2")
                    .insert_header("x-total-pages", "1")
                    .insert_header("x-next-page", ""),
            )
            .mount(&server)
            .await;

        let (items, meta) = groups_list(
            &mock_client(&server),
            GroupsListParams {
                search: None,
                all_available: None,
                owned: None,
                min_access_level: None,
                order_by: None,
                sort: None,
                top_level_only: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(items.as_array().unwrap().len(), 2);
        assert_eq!(items[0]["name"], "Alpha");
        assert_eq!(meta.total, Some(2));
    }

    #[tokio::test]
    async fn groups_list_passes_search_param() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups"))
            .and(query_param("search", "my"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([group_json(1, "mygroup", "mygroup")]))
                    .insert_header("x-page", "1")
                    .insert_header("x-per-page", "20")
                    .insert_header("x-total", "1")
                    .insert_header("x-total-pages", "1")
                    .insert_header("x-next-page", ""),
            )
            .mount(&server)
            .await;

        let (items, _) = groups_list(
            &mock_client(&server),
            GroupsListParams {
                search: Some("my".into()),
                all_available: None,
                owned: None,
                min_access_level: None,
                order_by: None,
                sort: None,
                top_level_only: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(items[0]["name"], "mygroup");
    }

    #[tokio::test]
    async fn groups_list_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = groups_list(
            &mock_client(&server),
            GroupsListParams {
                search: None,
                all_available: None,
                owned: None,
                min_access_level: None,
                order_by: None,
                sort: None,
                top_level_only: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }

    // ------------------------------------------------------------------
    // group_get
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn group_get_returns_group() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(group_json(1, "My Group", "mygroup")),
            )
            .mount(&server)
            .await;

        let item = group_get(
            &mock_client(&server),
            GroupGetParams {
                group_id: "mygroup".into(),
                with_projects: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["name"], "My Group");
        assert_eq!(item["path"], "mygroup");
    }

    #[tokio::test]
    async fn group_get_encodes_numeric_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/42"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(group_json(42, "Numeric", "numeric")),
            )
            .mount(&server)
            .await;

        let item = group_get(
            &mock_client(&server),
            GroupGetParams {
                group_id: "42".into(),
                with_projects: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["id"], 42);
    }

    #[tokio::test]
    async fn group_get_defaults_with_projects_to_false() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup"))
            .and(query_param("with_projects", "false"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(group_json(1, "My Group", "mygroup")),
            )
            .mount(&server)
            .await;

        let item = group_get(
            &mock_client(&server),
            GroupGetParams {
                group_id: "mygroup".into(),
                with_projects: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["name"], "My Group");
    }

    #[tokio::test]
    async fn group_get_forwards_with_projects_true() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup"))
            .and(query_param("with_projects", "true"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(group_json(1, "My Group", "mygroup")),
            )
            .mount(&server)
            .await;

        let item = group_get(
            &mock_client(&server),
            GroupGetParams {
                group_id: "mygroup".into(),
                with_projects: Some(true),
            },
        )
        .await
        .unwrap();

        assert_eq!(item["name"], "My Group");
    }

    #[tokio::test]
    async fn group_get_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/ghost"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = group_get(
            &mock_client(&server),
            GroupGetParams {
                group_id: "ghost".into(),
                with_projects: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }
}
