use std::time::Duration;

use reqwest::{Client, StatusCode, header};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

/// Pagination headers extracted from a GitLab list response.
///
/// GitLab omits `X-Total` and `X-Total-Pages` on large endpoints, and omits
/// `X-Next-Page` on the last page, so all fields are optional.
#[derive(Debug, Default, Serialize)]
pub struct PaginationMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<u64>,
}

/// Result type for list endpoints — JSON body plus pagination metadata.
pub type ListResult = Result<(Value, PaginationMeta), GitlabError>;

/// Cursor-based pagination metadata extracted from a GraphQL `pageInfo` object.
#[derive(Debug, Default, Serialize)]
pub struct GraphqlPageInfo {
    pub has_next_page: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_cursor: Option<String>,
}

/// Result type for GraphQL list operations — JSON array of nodes plus cursor pagination.
pub type GraphqlListResult = Result<(Value, GraphqlPageInfo), GitlabError>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum GitlabError {
    #[error("GitLab API error {status}: {body}")]
    Api { status: StatusCode, body: String },
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("GraphQL error: {0}")]
    Graphql(String),
}

impl GitlabError {
    /// Returns a message safe to forward to the MCP client.
    /// Truncates the API error body to 300 chars to avoid blowing up context windows.
    pub fn to_tool_message(&self) -> String {
        match self {
            GitlabError::Api { status, body } => {
                let cut = body
                    .char_indices()
                    .nth(300)
                    .map(|(i, _)| i)
                    .unwrap_or(body.len());
                if cut < body.len() {
                    format!("GitLab API error {status}: {}… (truncated)", &body[..cut])
                } else {
                    format!("GitLab API error {status}: {body}")
                }
            }
            other => other.to_string(),
        }
    }
}

/// Thin HTTP client for the GitLab REST API.
///
/// All responses are returned as `serde_json::Value` — tools serialize them
/// directly to text, so typed structs provide no benefit.
#[derive(Clone)]
pub struct GitlabClient {
    http: Client,
    base_url: String,
}

impl GitlabClient {
    pub fn new(base_url: impl Into<String>, token: impl AsRef<str>) -> anyhow::Result<Self> {
        let token_value = header::HeaderValue::from_str(token.as_ref()).map_err(|_| {
            anyhow::anyhow!(
                "GitLab token contains characters not valid in HTTP headers (must be visible ASCII)"
            )
        })?;

        let mut headers = header::HeaderMap::new();
        // GitLab uses the PRIVATE-TOKEN header for personal access tokens.
        headers.insert("PRIVATE-TOKEN", token_value);

        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .default_headers(headers)
            .build()?;

        Ok(GitlabClient {
            http,
            base_url: base_url.into(),
        })
    }

    /// GET {base_url}{path} — returns the JSON response body.
    pub async fn get(&self, path: &str) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.http.get(&url).send().await?;
        self.handle_response(resp).await
    }

    /// GET {base_url}{path}?{params} — returns the JSON response body.
    pub async fn get_with_params(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.http.get(&url).query(params).send().await?;
        self.handle_response(resp).await
    }

    /// GET {base_url}{path}?{params} for list endpoints — returns the JSON body
    /// together with pagination metadata extracted from the `X-*` response headers.
    pub async fn list(&self, path: &str, params: &[(&str, String)]) -> ListResult {
        let url = self.url(path);
        let resp = self.http.get(&url).query(params).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitlabError::Api { status, body });
        }
        let meta = PaginationMeta {
            page: parse_pagination_header(resp.headers(), "x-page"),
            per_page: parse_pagination_header(resp.headers(), "x-per-page"),
            total: parse_pagination_header(resp.headers(), "x-total"),
            total_pages: parse_pagination_header(resp.headers(), "x-total-pages"),
            next_page: parse_pagination_header(resp.headers(), "x-next-page"),
        };
        let body: Value = resp.json().await?;
        Ok((body, meta))
    }

    /// POST {base_url}{path} with a JSON body — returns the JSON response body.
    pub async fn post(&self, path: &str, body: &Value) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.http.post(&url).json(body).send().await?;
        self.handle_response(resp).await
    }

    /// PUT {base_url}{path} with a JSON body — returns the JSON response body.
    pub async fn put(&self, path: &str, body: &Value) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.http.put(&url).json(body).send().await?;
        self.handle_response(resp).await
    }

    /// GET {base_url}{path}?{params} — returns the raw text response body (for non-JSON endpoints).
    pub async fn get_text(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<String, GitlabError> {
        let url = self.url(path);
        let resp = self.http.get(&url).query(params).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitlabError::Api { status, body });
        }
        Ok(resp.text().await?)
    }

    /// DELETE {base_url}{path} with a JSON body — returns the JSON response body.
    pub async fn delete_with_body(&self, path: &str, body: &Value) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.http.delete(&url).json(body).send().await?;
        self.handle_response(resp).await
    }

    /// DELETE {base_url}{path} — returns () on success (204 No Content expected).
    pub async fn delete(&self, path: &str) -> Result<(), GitlabError> {
        let url = self.url(path);
        let resp = self.http.delete(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitlabError::Api { status, body });
        }
        Ok(())
    }

    /// POST /api/graphql — executes a GraphQL query or mutation.
    ///
    /// Returns the `data` field of the response. Top-level GraphQL `errors` are mapped
    /// to `GitlabError::Graphql`; HTTP errors map to `GitlabError::Api`.
    /// Mutation-level errors (inside `data.mutationName.errors`) must be checked
    /// by the caller.
    pub async fn graphql(&self, query: &str, variables: Value) -> Result<Value, GitlabError> {
        let body = serde_json::json!({ "query": query, "variables": variables });
        let url = self.url("/api/graphql");
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitlabError::Api { status, body });
        }
        let mut val: Value = resp.json().await?;
        if let Some(errors) = val.get("errors") {
            let msg = errors
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| errors.to_string());
            return Err(GitlabError::Graphql(msg));
        }
        Ok(val["data"].take())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    async fn handle_response(&self, resp: reqwest::Response) -> Result<Value, GitlabError> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitlabError::Api { status, body });
        }
        if status == StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        Ok(resp.json().await?)
    }
}

fn parse_pagination_header(headers: &header::HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    fn api_err(status: StatusCode, body: &str) -> GitlabError {
        GitlabError::Api {
            status,
            body: body.to_string(),
        }
    }

    #[test]
    fn to_tool_message_short_body_not_truncated() {
        let msg = api_err(StatusCode::NOT_FOUND, "Not found").to_tool_message();
        assert_eq!(msg, "GitLab API error 404 Not Found: Not found");
        assert!(!msg.contains("truncated"));
    }

    #[test]
    fn to_tool_message_exactly_300_chars_not_truncated() {
        let body = "x".repeat(300);
        let msg = api_err(StatusCode::BAD_REQUEST, &body).to_tool_message();
        assert!(msg.ends_with(&body));
        assert!(!msg.contains("truncated"));
    }

    #[test]
    fn to_tool_message_over_300_chars_truncated() {
        let body = "y".repeat(400);
        let msg = api_err(StatusCode::INTERNAL_SERVER_ERROR, &body).to_tool_message();
        assert!(msg.contains("(truncated)"));
        assert!(msg.contains("GitLab API error 500"));
        // Body portion should be exactly 300 chars of 'y'
        assert!(msg.contains(&"y".repeat(300)));
        assert!(!msg.contains(&"y".repeat(301)));
    }

    #[test]
    fn to_tool_message_multibyte_truncation_at_char_boundary() {
        // 300 three-byte chars (é) + extra: must not split a char
        let body = "é".repeat(305);
        let msg = api_err(StatusCode::FORBIDDEN, &body).to_tool_message();
        assert!(msg.contains("(truncated)"));
        // Result must be valid UTF-8 (would panic on display if not)
        let _ = msg.len();
    }

    // --- HTTP behaviour tests (wiremock) ---

    #[tokio::test]
    async fn get_sends_private_token_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let result = mock_client(&server).get("/api/v4/projects").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_returns_parsed_json() {
        let server = MockServer::start().await;
        let body = serde_json::json!({"id": 1, "title": "Test"});
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/1/issues/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body.clone()))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .get("/api/v4/projects/1/issues/1")
            .await
            .unwrap();
        assert_eq!(result, body);
    }

    #[tokio::test]
    async fn get_maps_error_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/99/issues/1"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .get("/api/v4/projects/99/issues/1")
            .await
            .unwrap_err();
        match err {
            GitlabError::Api { status, body } => {
                assert_eq!(status, StatusCode::NOT_FOUND);
                assert_eq!(body, "Not found");
            }
            other => panic!("expected GitlabError::Api, got {other}"),
        }
    }

    #[tokio::test]
    async fn get_with_params_sends_query_params() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/1/issues"))
            .and(query_param("state", "opened"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let params = &[("state", "opened".to_string()), ("page", "2".to_string())];
        let result = mock_client(&server)
            .get_with_params("/api/v4/projects/1/issues", params)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn post_sends_json_body_and_returns_response() {
        let server = MockServer::start().await;
        let req_body = serde_json::json!({"title": "New Issue"});
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/1/issues"))
            .and(body_json(req_body.clone()))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"iid": 5})))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .post("/api/v4/projects/1/issues", &req_body)
            .await
            .unwrap();
        assert_eq!(result["iid"], 5);
    }

    #[tokio::test]
    async fn put_sends_json_body_and_returns_response() {
        let server = MockServer::start().await;
        let req_body = serde_json::json!({"title": "Updated"});
        Mock::given(method("PUT"))
            .and(path("/api/v4/projects/1/issues/1"))
            .and(body_json(req_body.clone()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"title": "Updated"})),
            )
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .put("/api/v4/projects/1/issues/1", &req_body)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handle_response_204_no_content_returns_null() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v4/projects/1/issues/1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .put("/api/v4/projects/1/issues/1", &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn delete_success_returns_unit() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/1/issues/1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .delete("/api/v4/projects/1/issues/1")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn delete_error_returns_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/1/issues/99"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .delete("/api/v4/projects/1/issues/99")
            .await
            .unwrap_err();
        match err {
            GitlabError::Api { status, body } => {
                assert_eq!(status, StatusCode::FORBIDDEN);
                assert_eq!(body, "Forbidden");
            }
            other => panic!("expected GitlabError::Api, got {other}"),
        }
    }

    #[tokio::test]
    async fn delete_with_body_sends_json_body() {
        let server = MockServer::start().await;
        let req_body = serde_json::json!({"branch": "old-feature"});
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/1/repository/merged_branches"))
            .and(body_json(req_body.clone()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .delete_with_body("/api/v4/projects/1/repository/merged_branches", &req_body)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_text_returns_raw_text() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/1/repository/files/README/raw"))
            .respond_with(ResponseTemplate::new(200).set_body_string("# Hello"))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .get_text("/api/v4/projects/1/repository/files/README/raw", &[])
            .await
            .unwrap();
        assert_eq!(result, "# Hello");
    }

    #[tokio::test]
    async fn get_text_error_returns_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/1/repository/files/MISSING/raw"))
            .respond_with(ResponseTemplate::new(404).set_body_string("File not found"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .get_text("/api/v4/projects/1/repository/files/MISSING/raw", &[])
            .await
            .unwrap_err();
        match err {
            GitlabError::Api { status, body } => {
                assert_eq!(status, StatusCode::NOT_FOUND);
                assert_eq!(body, "File not found");
            }
            other => panic!("expected GitlabError::Api, got {other}"),
        }
    }

    #[tokio::test]
    async fn list_extracts_pagination_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/1/issues"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-page", "2")
                    .insert_header("x-per-page", "20")
                    .insert_header("x-total", "49")
                    .insert_header("x-total-pages", "3")
                    .insert_header("x-next-page", "3")
                    .set_body_json(serde_json::json!([])),
            )
            .mount(&server)
            .await;

        let (body, meta) = mock_client(&server)
            .list("/api/v4/projects/1/issues", &[])
            .await
            .unwrap();
        assert_eq!(body, serde_json::json!([]));
        assert_eq!(meta.page, Some(2));
        assert_eq!(meta.per_page, Some(20));
        assert_eq!(meta.total, Some(49));
        assert_eq!(meta.total_pages, Some(3));
        assert_eq!(meta.next_page, Some(3));
    }

    #[tokio::test]
    async fn list_handles_absent_total_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/1/issues"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-page", "1")
                    .insert_header("x-per-page", "100")
                    .set_body_json(serde_json::json!([])),
            )
            .mount(&server)
            .await;

        let (_, meta) = mock_client(&server)
            .list("/api/v4/projects/1/issues", &[])
            .await
            .unwrap();
        assert_eq!(meta.page, Some(1));
        assert_eq!(meta.per_page, Some(100));
        assert_eq!(meta.total, None);
        assert_eq!(meta.total_pages, None);
        assert_eq!(meta.next_page, None);
    }

    #[tokio::test]
    async fn list_error_returns_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/99/issues"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .list("/api/v4/projects/99/issues", &[])
            .await
            .unwrap_err();
        match err {
            GitlabError::Api { status, body } => {
                assert_eq!(status, StatusCode::NOT_FOUND);
                assert_eq!(body, "Not found");
            }
            other => panic!("expected GitlabError::Api, got {other}"),
        }
    }

    // --- graphql() tests ---

    #[tokio::test]
    async fn graphql_returns_data_field_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItem": { "id": "gid://gitlab/WorkItem/1", "title": "Test" } }
            })))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .graphql(
                "{ workItem(id: \"1\") { id title } }",
                serde_json::json!({}),
            )
            .await
            .unwrap();
        assert_eq!(result["workItem"]["title"], "Test");
    }

    #[tokio::test]
    async fn graphql_returns_error_on_top_level_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [
                    { "message": "Field 'foo' doesn't exist" },
                    { "message": "Argument 'bar' is required" }
                ]
            })))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .graphql("{ foo }", serde_json::json!({}))
            .await
            .unwrap_err();
        match err {
            GitlabError::Graphql(msg) => {
                assert!(msg.contains("Field 'foo' doesn't exist"));
                assert!(msg.contains("Argument 'bar' is required"));
            }
            other => panic!("expected GitlabError::Graphql, got {other}"),
        }
    }

    #[tokio::test]
    async fn graphql_http_error_returns_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .graphql("{ workItem(id: \"1\") { id } }", serde_json::json!({}))
            .await
            .unwrap_err();
        match err {
            GitlabError::Api { status, body } => {
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert_eq!(body, "Unauthorized");
            }
            other => panic!("expected GitlabError::Api, got {other}"),
        }
    }

    #[tokio::test]
    async fn graphql_sends_private_token_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {}
            })))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .graphql("{ __typename }", serde_json::json!({}))
            .await;
        assert!(result.is_ok());
    }
}
