use std::time::Duration;

use reqwest::{Client, StatusCode, header};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, trace};

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

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Max times to retry a request GitLab rejects with 429 Too Many Requests.
const MAX_RETRIES: u32 = 4;
/// Upper bound on how long to wait between 429 retries (honors `Retry-After`
/// up to this, then caps).
const MAX_RETRY_WAIT: Duration = Duration::from_secs(60);

/// Upper bound on pages fetched in a single `fetch_all` request, guarding
/// against runaway loops when an endpoint never signals the last page.
/// At 100 items/page this caps a merged response at 20,000 items.
pub(crate) const MAX_PAGES: u64 = 200;

#[derive(Debug, Error)]
pub enum GitlabError {
    #[error("GitLab API error {status}: {body}")]
    Api { status: StatusCode, body: String },
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    /// Validation failure or malformed API response — anything that's not an HTTP
    /// error from GitLab but still prevents the operation from succeeding.
    #[error("{0}")]
    Other(String),
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
        let resp = self.send(self.http.get(&url)).await?;
        self.handle_response(resp).await
    }

    /// GET {base_url}{path}?{params} — returns the JSON response body.
    pub async fn get_with_params(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.send(self.http.get(&url).query(params)).await?;
        self.handle_response(resp).await
    }

    /// GET {base_url}{path}?{params} for list endpoints — returns the JSON body
    /// together with pagination metadata extracted from the `X-*` response headers.
    pub async fn list(&self, path: &str, params: &[(&str, String)]) -> ListResult {
        let url = self.url(path);
        let resp = self.send(self.http.get(&url).query(params)).await?;
        let resp = check_status(resp).await?;
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
        let resp = self.send(self.http.post(&url).json(body)).await?;
        self.handle_response(resp).await
    }

    /// POST {base_url}{path} with a JSON body — returns () on success (no response body expected).
    pub async fn post_void(&self, path: &str, body: &Value) -> Result<(), GitlabError> {
        let url = self.url(path);
        let resp = self.send(self.http.post(&url).json(body)).await?;
        check_status(resp).await?;
        Ok(())
    }

    /// PUT {base_url}{path} with a JSON body — returns the JSON response body.
    pub async fn put(&self, path: &str, body: &Value) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.send(self.http.put(&url).json(body)).await?;
        self.handle_response(resp).await
    }

    /// POST a GraphQL query to {base_url}/api/graphql — returns the `data` object.
    ///
    /// Unlike the REST endpoints, GitLab's GraphQL endpoint returns HTTP 200 even
    /// when a query fails, signalling failure via a top-level `errors` array. We
    /// surface a non-empty `errors` array as [`GitlabError::Api`] (with a 200
    /// status, since that is what GitLab actually returned) and otherwise unwrap
    /// and return the `data` object so callers see the same shape as REST tools.
    pub async fn graphql(&self, query: &str, variables: Value) -> Result<Value, GitlabError> {
        let url = self.url("/api/graphql");
        debug!(target: "gitlab_mcp", %query, variables = %variables, "graphql request");
        let body = serde_json::json!({ "query": query, "variables": variables });
        let resp = self.send(self.http.post(&url).json(&body)).await?;
        let resp = check_status(resp).await?;
        let mut json: Value = resp.json().await?;
        trace!(target: "gitlab_mcp", response = %json, "graphql response");

        let has_errors = json
            .get("errors")
            .and_then(Value::as_array)
            .is_some_and(|errs| !errs.is_empty());
        if has_errors {
            return Err(GitlabError::Api {
                status: StatusCode::OK,
                body: json["errors"].to_string(),
            });
        }

        Ok(json.get_mut("data").map(Value::take).unwrap_or(Value::Null))
    }

    /// GET {base_url}{path}?{params} — returns the raw text response body (for non-JSON endpoints).
    pub async fn get_text(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<String, GitlabError> {
        let url = self.url(path);
        let resp = self.send(self.http.get(&url).query(params)).await?;
        let resp = check_status(resp).await?;
        Ok(resp.text().await?)
    }

    /// DELETE {base_url}{path} with a JSON body — returns the JSON response body.
    pub async fn delete_with_body(&self, path: &str, body: &Value) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.send(self.http.delete(&url).json(body)).await?;
        self.handle_response(resp).await
    }

    /// DELETE {base_url}{path} — returns the JSON response body.
    pub async fn delete_json(&self, path: &str) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.send(self.http.delete(&url)).await?;
        self.handle_response(resp).await
    }

    /// DELETE {base_url}{path} — returns () on success (204 No Content expected).
    pub async fn delete(&self, path: &str) -> Result<(), GitlabError> {
        let url = self.url(path);
        let resp = self.send(self.http.delete(&url)).await?;
        check_status(resp).await?;
        Ok(())
    }

    /// Send a request, retrying on HTTP 429 (Too Many Requests). A 429 is a
    /// pre-processing rejection, so retrying is safe even for mutating methods
    /// (POST/PUT/DELETE) — the rejected request never reached the resource.
    /// Honors GitLab's `Retry-After` header (seconds), falling back to
    /// exponential backoff; both are capped at [`MAX_RETRY_WAIT`].
    async fn send(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, GitlabError> {
        let mut attempt: u32 = 0;
        loop {
            // Clone per attempt; our bodies are in-memory JSON, so this is `Some`.
            let attempt_builder = builder
                .try_clone()
                .ok_or_else(|| GitlabError::Other("request body is not retryable".into()))?;
            // Log the method + URL once (first attempt) from the built request, so a
            // single central point covers every REST and GraphQL call. Headers (and
            // thus the PRIVATE-TOKEN) are never logged. The `enabled!` guard keeps the
            // extra clone + build out of the hot path when debug logging is off.
            if attempt == 0
                && tracing::enabled!(target: "gitlab_mcp", tracing::Level::DEBUG)
                && let Some(req) = builder.try_clone().and_then(|b| b.build().ok())
            {
                debug!(target: "gitlab_mcp", method = %req.method(), url = %req.url(), "gitlab request");
            }
            let resp = attempt_builder.send().await?;
            if resp.status() == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
                let wait = retry_after(resp.headers())
                    .unwrap_or_else(|| backoff(attempt))
                    .min(MAX_RETRY_WAIT);
                attempt += 1;
                tokio::time::sleep(wait).await;
                continue;
            }
            return Ok(resp);
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    async fn handle_response(&self, resp: reqwest::Response) -> Result<Value, GitlabError> {
        let resp = check_status(resp).await?;
        if resp.status() == StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        Ok(resp.json().await?)
    }
}

/// Map a non-2xx response to [`GitlabError::Api`] (consuming the body as the
/// error message), passing successful responses through untouched.
async fn check_status(resp: reqwest::Response) -> Result<reqwest::Response, GitlabError> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        // Log the full, untruncated error body — `to_tool_message()` clips it to
        // 300 chars for the MCP client, but a debug trace wants the whole thing.
        debug!(target: "gitlab_mcp", %status, body = %body, "gitlab error response");
        return Err(GitlabError::Api { status, body });
    }
    trace!(target: "gitlab_mcp", %status, "gitlab response");
    Ok(resp)
}

/// Parse a `Retry-After` header (delay in whole seconds) into a `Duration`.
fn retry_after(headers: &header::HeaderMap) -> Option<Duration> {
    headers
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Exponential backoff for retry `attempt` (0-based): 0.5s, 1s, 2s, 4s, …
fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500u64.saturating_mul(2u64.saturating_pow(attempt)))
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
    use crate::test_util::mock_client;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
    async fn post_void_success_returns_unit() {
        let server = MockServer::start().await;
        let req_body = serde_json::json!({});
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/1/merge_requests/3/unapprove"))
            .and(body_json(req_body.clone()))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .post_void("/api/v4/projects/1/merge_requests/3/unapprove", &req_body)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn post_void_error_returns_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/1/merge_requests/99/unapprove"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .post_void(
                "/api/v4/projects/1/merge_requests/99/unapprove",
                &serde_json::json!({}),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GitlabError::Api { status, .. } if status.as_u16() == 404));
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
    async fn delete_json_returns_response_body() {
        let server = MockServer::start().await;
        let response_body = serde_json::json!({"issue_link_id": 7, "link_type": "relates_to"});
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/1/issues/1/links/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body.clone()))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .delete_json("/api/v4/projects/1/issues/1/links/7")
            .await
            .unwrap();
        assert_eq!(result, response_body);
    }

    #[tokio::test]
    async fn delete_json_error_returns_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/1/issues/1/links/99"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .delete_json("/api/v4/projects/1/issues/1/links/99")
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
    async fn graphql_sends_query_and_returns_data() {
        let server = MockServer::start().await;
        let req_body = serde_json::json!({
            "query": "query($id: ID!) { issue(id: $id) { title } }",
            "variables": { "id": "gid://gitlab/Issue/1" }
        });
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(body_json(req_body.clone()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "data": { "issue": { "title": "Hello" } } }),
                ),
            )
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .graphql(
                "query($id: ID!) { issue(id: $id) { title } }",
                serde_json::json!({ "id": "gid://gitlab/Issue/1" }),
            )
            .await
            .unwrap();
        // The `data` envelope is unwrapped.
        assert_eq!(result, serde_json::json!({ "issue": { "title": "Hello" } }));
    }

    #[tokio::test]
    async fn graphql_surfaces_errors_array_despite_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [{ "message": "Field 'bogus' doesn't exist on type 'Query'" }],
                "data": null
            })))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .graphql("query { bogus }", serde_json::json!({}))
            .await
            .unwrap_err();
        match err {
            GitlabError::Api { status, body } => {
                assert_eq!(status, StatusCode::OK);
                assert!(body.contains("doesn't exist"));
            }
            other => panic!("expected GitlabError::Api, got {other}"),
        }
    }

    #[tokio::test]
    async fn graphql_empty_errors_array_is_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "errors": [], "data": { "ok": true } })),
            )
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .graphql("query { ok }", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!({ "ok": true }));
    }

    #[tokio::test]
    async fn graphql_missing_data_returns_null() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let result = mock_client(&server)
            .graphql("query { whatever }", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn graphql_http_error_returns_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .graphql("query { ok }", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitlabError::Api { status, .. } if status == StatusCode::UNAUTHORIZED)
        );
    }

    // --- 429 retry behaviour ---

    #[test]
    fn retry_after_parses_seconds() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("3"),
        );
        assert_eq!(retry_after(&h), Some(Duration::from_secs(3)));
        assert_eq!(retry_after(&reqwest::header::HeaderMap::new()), None);
    }

    #[test]
    fn backoff_is_exponential() {
        assert_eq!(backoff(0), Duration::from_millis(500));
        assert_eq!(backoff(1), Duration::from_millis(1000));
        assert_eq!(backoff(3), Duration::from_millis(4000));
    }

    #[tokio::test]
    async fn send_retries_on_429_then_succeeds() {
        let server = MockServer::start().await;
        // First call gets 429 (Retry-After: 0 so the test doesn't actually sleep);
        // `up_to_n_times(1)` + higher priority makes it answer only the first call.
        Mock::given(method("GET"))
            .and(path("/api/v4/projects"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .with_priority(1)
            .expect(1)
            .mount(&server)
            .await;
        // The retry succeeds.
        Mock::given(method("GET"))
            .and(path("/api/v4/projects"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"id": 1}])))
            .with_priority(2)
            .mount(&server)
            .await;

        let result = mock_client(&server).get("/api/v4/projects").await.unwrap();
        assert_eq!(result[0]["id"], 1);
    }

    #[tokio::test]
    async fn send_gives_up_after_max_retries_returning_429() {
        let server = MockServer::start().await;
        // Always 429: after MAX_RETRIES the 429 is returned and mapped to Api.
        Mock::given(method("GET"))
            .and(path("/api/v4/projects"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .expect(MAX_RETRIES as u64 + 1) // initial attempt + MAX_RETRIES
            .mount(&server)
            .await;

        let err = mock_client(&server)
            .get("/api/v4/projects")
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitlabError::Api { status, .. } if status == StatusCode::TOO_MANY_REQUESTS)
        );
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
}
