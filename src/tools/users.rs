use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{PaginationParams, QueryBuilder, list_paginated};

// --------------------------------------------------------------------------
// List users
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UsersListParams {
    #[schemars(description = "Get a single user with this exact username (case-insensitive)")]
    pub username: Option<String>,
    #[schemars(description = "Search for users by name, username, or public email (fuzzy)")]
    pub search: Option<String>,
    #[schemars(description = "Filter to only active users")]
    pub active: Option<bool>,
    #[schemars(description = "Filter to only blocked users")]
    pub blocked: Option<bool>,
    #[schemars(description = "Filter to only external users")]
    pub external: Option<bool>,
    #[schemars(description = "Filter to only regular users (exclude bot and internal users)")]
    pub humans: Option<bool>,
    #[schemars(description = "Return users created after this time (ISO 8601)")]
    pub created_after: Option<String>,
    #[schemars(description = "Return users created before this time (ISO 8601)")]
    pub created_before: Option<String>,
    #[schemars(
        description = "Sort field (admin only): \"id\", \"name\", \"username\", \"created_at\", or \"updated_at\""
    )]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction (admin only): \"asc\" or \"desc\"")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn users_list(client: &GitlabClient, p: UsersListParams) -> ListResult {
    let qb = QueryBuilder::new()
        .opt("username", p.username)
        .opt("search", p.search)
        .opt("active", p.active)
        .opt("blocked", p.blocked)
        .opt("external", p.external)
        .opt("humans", p.humans)
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort);
    list_paginated(client, "/api/v4/users", qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get a single user
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UserGetParams {
    #[schemars(description = "Numeric user ID or username of the user to fetch")]
    pub user_id: String,
}

pub async fn user_get(client: &GitlabClient, p: UserGetParams) -> Result<Value, GitlabError> {
    let id = resolve_user_id(client, &p.user_id).await?;
    client.get(&format!("/api/v4/users/{id}")).await
}

// --------------------------------------------------------------------------
// List a user's SSH keys
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UsersKeysListParams {
    #[schemars(description = "Numeric user ID or username of the user whose SSH keys to list")]
    pub user_id: String,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Resolve a `user_id` parameter (numeric ID or username) to the numeric user
/// ID that the `/users/:id/keys` endpoint requires. Numeric values pass through
/// unchanged; a username is looked up via `GET /users?username=` (an extra
/// request), mirroring the resolve-then-call pattern used by the work-items and
/// epics modules.
async fn resolve_user_id(client: &GitlabClient, user_id: &str) -> Result<String, GitlabError> {
    if user_id.chars().all(|c| c.is_ascii_digit()) {
        return Ok(user_id.to_string());
    }
    let matches = client
        .get_with_params("/api/v4/users", &[("username", user_id.to_string())])
        .await?;
    matches
        .as_array()
        .and_then(|users| users.first())
        .and_then(|user| user.get("id"))
        .and_then(serde_json::Value::as_u64)
        .map(|id| id.to_string())
        .ok_or_else(|| GitlabError::Other(format!("no user found with username '{user_id}'")))
}

pub async fn users_keys_list(client: &GitlabClient, p: UsersKeysListParams) -> ListResult {
    // `resolve_user_id` always yields a numeric ID, so the path needs no encoding.
    let id = resolve_user_id(client, &p.user_id).await?;
    let path = format!("/api/v4/users/{id}/keys");
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_users, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List GitLab users (GET /users). Filter with username (exact, case-insensitive lookup), search (fuzzy match on name/username/public email), active, blocked, external, humans (exclude bots/internal), and created_after/created_before (ISO 8601). order_by and sort are admin-only. Paginate with page and per_page. To fetch one user's full details by ID or username, use gitlab_users_get instead.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_users_list(
        &self,
        Parameters(p): Parameters<UsersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, users_list, p, "users")
    }

    #[tool(
        description = "Get a single GitLab user's details (GET /users/:id). Pass user_id as either a numeric user ID or a username. Returns the user's profile: id, username, name, state, web_url, created_at, bio, public_email, and more (extra fields are visible to administrators or for your own account).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_users_get(
        &self,
        Parameters(p): Parameters<UserGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, user_get, p, "user")
    }

    #[tool(
        description = "List the public SSH keys for a GitLab user (GET /users/:id/keys). Useful for looking up a user's SSH public keys to add to server authorized_keys (e.g. when provisioning infrastructure with Ansible). Pass user_id as either a numeric user ID or a username. Returns key objects with id, title, key, created_at, and expires_at. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_users_keys_list(
        &self,
        Parameters(p): Parameters<UsersKeysListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, users_keys_list, p, "user SSH keys")
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        UserGetParams, UsersKeysListParams, UsersListParams, user_get, users_keys_list, users_list,
    };
    use crate::test_util::mock_client;
    use crate::tools::PaginationParams;

    fn key_json(id: u64, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "title": title,
            "key": "ssh-rsa AAAAB3Nza...",
            "created_at": "2024-01-01T00:00:00.000Z",
            "expires_at": null
        })
    }

    fn no_pagination() -> PaginationParams {
        PaginationParams {
            page: None,
            per_page: None,
            fetch_all: None,
        }
    }

    fn list_headers(t: ResponseTemplate) -> ResponseTemplate {
        t.insert_header("x-page", "1")
            .insert_header("x-per-page", "20")
            .insert_header("x-total", "1")
            .insert_header("x-total-pages", "1")
            .insert_header("x-next-page", "")
    }

    fn user_json(id: u64, username: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "username": username,
            "name": username,
            "state": "active",
            "web_url": format!("https://gitlab.example.com/{username}")
        })
    }

    // ------------------------------------------------------------------
    // users_list
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn users_list_returns_items() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .respond_with(list_headers(ResponseTemplate::new(200).set_body_json(
                serde_json::json!([user_json(1, "alice"), user_json(2, "bob")]),
            )))
            .mount(&server)
            .await;

        let (items, _) = users_list(
            &mock_client(&server),
            UsersListParams {
                username: None,
                search: None,
                active: None,
                blocked: None,
                external: None,
                humans: None,
                created_after: None,
                created_before: None,
                order_by: None,
                sort: None,
                pagination: no_pagination(),
            },
        )
        .await
        .unwrap();

        assert_eq!(items.as_array().unwrap().len(), 2);
        assert_eq!(items[0]["username"], "alice");
    }

    #[tokio::test]
    async fn users_list_passes_search_param() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .and(query_param("search", "ali"))
            .respond_with(list_headers(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([user_json(1, "alice")])),
            ))
            .mount(&server)
            .await;

        let (items, _) = users_list(
            &mock_client(&server),
            UsersListParams {
                username: None,
                search: Some("ali".into()),
                active: None,
                blocked: None,
                external: None,
                humans: None,
                created_after: None,
                created_before: None,
                order_by: None,
                sort: None,
                pagination: no_pagination(),
            },
        )
        .await
        .unwrap();

        assert_eq!(items[0]["username"], "alice");
    }

    // ------------------------------------------------------------------
    // user_get
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn user_get_numeric_id_hits_endpoint_directly() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(user_json(42, "alice")))
            .mount(&server)
            .await;

        let item = user_get(
            &mock_client(&server),
            UserGetParams {
                user_id: "42".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(item["id"], 42);
        assert_eq!(item["username"], "alice");
    }

    #[tokio::test]
    async fn user_get_username_resolves_then_fetches() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .and(query_param("username", "bob"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([user_json(9, "bob")])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users/9"))
            .respond_with(ResponseTemplate::new(200).set_body_json(user_json(9, "bob")))
            .mount(&server)
            .await;

        let item = user_get(
            &mock_client(&server),
            UserGetParams {
                user_id: "bob".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(item["id"], 9);
    }

    // ------------------------------------------------------------------
    // users_keys_list
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn keys_list_numeric_id_hits_endpoint_directly() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users/42/keys"))
            .respond_with(list_headers(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([key_json(1, "laptop")])),
            ))
            .mount(&server)
            .await;

        let (items, _) = users_keys_list(
            &mock_client(&server),
            UsersKeysListParams {
                user_id: "42".into(),
                pagination: no_pagination(),
            },
        )
        .await
        .unwrap();

        assert_eq!(items.as_array().unwrap().len(), 1);
        assert_eq!(items[0]["title"], "laptop");
    }

    #[tokio::test]
    async fn keys_list_username_resolves_then_lists() {
        let server = MockServer::start().await;
        // First: username lookup.
        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .and(query_param("username", "john_smith"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{ "id": 7, "username": "john_smith" }])),
            )
            .mount(&server)
            .await;
        // Then: keys for the resolved numeric ID.
        Mock::given(method("GET"))
            .and(path("/api/v4/users/7/keys"))
            .respond_with(list_headers(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([key_json(2, "desktop")])),
            ))
            .mount(&server)
            .await;

        let (items, _) = users_keys_list(
            &mock_client(&server),
            UsersKeysListParams {
                user_id: "john_smith".into(),
                pagination: no_pagination(),
            },
        )
        .await
        .unwrap();

        assert_eq!(items[0]["title"], "desktop");
    }

    #[tokio::test]
    async fn keys_list_unknown_username_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users"))
            .and(query_param("username", "ghost"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let err = users_keys_list(
            &mock_client(&server),
            UsersKeysListParams {
                user_id: "ghost".into(),
                pagination: no_pagination(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Other(_)));
    }

    #[tokio::test]
    async fn keys_list_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/users/99/keys"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = users_keys_list(
            &mock_client(&server),
            UsersKeysListParams {
                user_id: "99".into(),
                pagination: no_pagination(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }
}
