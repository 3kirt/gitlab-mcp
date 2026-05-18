use std::time::Duration;

use reqwest::{Client, StatusCode, header};
use serde_json::Value;
use thiserror::Error;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum GitlabError {
    #[error("GitLab API error {status}: {body}")]
    Api { status: StatusCode, body: String },
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
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
    pub async fn list(&self, path: &str, params: &[(&str, String)]) -> Result<Value, GitlabError> {
        let url = self.url(path);
        let resp = self.http.get(&url).query(params).send().await?;
        self.handle_response(resp).await
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

    /// GET {base_url}{path} — returns the raw text response body (for non-JSON endpoints).
    pub async fn get_text(&self, path: &str) -> Result<String, GitlabError> {
        let url = self.url(path);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GitlabError::Api { status, body });
        }
        Ok(resp.text().await?)
    }

    /// GET {base_url}{path}?{params} — returns the raw text response body (for non-JSON endpoints).
    pub async fn get_text_with_params(
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
