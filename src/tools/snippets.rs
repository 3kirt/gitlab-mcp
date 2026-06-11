use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_path_segment, list_paginated,
};

// --------------------------------------------------------------------------
// Shared list filters
//
// All three list endpoints share the same created_after/before + pagination
// filters. Only `snippets/all` adds repository_storage (admin only).
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetsListFilters {
    #[schemars(description = "Return snippets created after this time (ISO 8601)")]
    pub created_after: Option<String>,
    #[schemars(description = "Return snippets created before this time (ISO 8601)")]
    pub created_before: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Build the shared snippet list query, returning the pagination fields
/// separately so the caller can drive [`list_paginated`].
fn snippets_query(f: SnippetsListFilters) -> (QueryBuilder, PaginationParams) {
    let qb = QueryBuilder::new()
        .opt("created_after", f.created_after)
        .opt("created_before", f.created_before);
    (qb, f.pagination)
}

// --------------------------------------------------------------------------
// List current user's snippets
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetsListParams {
    #[serde(flatten)]
    pub filters: SnippetsListFilters,
}

pub async fn snippets_list(client: &GitlabClient, p: SnippetsListParams) -> ListResult {
    let (qb, pagination) = snippets_query(p.filters);
    list_paginated(client, "/api/v4/snippets", qb, pagination).await
}

// --------------------------------------------------------------------------
// List public snippets
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetsPublicListParams {
    #[serde(flatten)]
    pub filters: SnippetsListFilters,
}

pub async fn snippets_public_list(
    client: &GitlabClient,
    p: SnippetsPublicListParams,
) -> ListResult {
    let (qb, pagination) = snippets_query(p.filters);
    list_paginated(client, "/api/v4/snippets/public", qb, pagination).await
}

// --------------------------------------------------------------------------
// List all snippets (admin)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetsAllListParams {
    #[schemars(description = "Filter by repository storage used by snippet (administrators only)")]
    pub repository_storage: Option<String>,
    #[serde(flatten)]
    pub filters: SnippetsListFilters,
}

pub async fn snippets_all_list(client: &GitlabClient, p: SnippetsAllListParams) -> ListResult {
    let (qb, pagination) = snippets_query(p.filters);
    let qb = qb.opt("repository_storage", p.repository_storage);
    list_paginated(client, "/api/v4/snippets/all", qb, pagination).await
}

// --------------------------------------------------------------------------
// Get single snippet
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetGetParams {
    #[schemars(description = "ID of the snippet")]
    pub id: u64,
}

pub async fn snippet_get(client: &GitlabClient, p: SnippetGetParams) -> Result<Value, GitlabError> {
    client.get(&format!("/api/v4/snippets/{}", p.id)).await
}

// --------------------------------------------------------------------------
// Get snippet raw content
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetRawParams {
    #[schemars(description = "ID of the snippet")]
    pub id: u64,
}

pub async fn snippet_raw(client: &GitlabClient, p: SnippetRawParams) -> Result<Value, GitlabError> {
    let content = client
        .get_text(&format!("/api/v4/snippets/{}/raw", p.id), &[])
        .await?;
    Ok(json!({"content": content}))
}

// --------------------------------------------------------------------------
// Get snippet repository file raw content
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetFileRawParams {
    #[schemars(description = "ID of the snippet")]
    pub id: u64,
    #[schemars(description = "Branch, tag, or commit reference")]
    pub ref_name: String,
    #[schemars(description = "URL-encoded path to the file within the snippet repository")]
    pub file_path: String,
}

pub async fn snippet_file_raw(
    client: &GitlabClient,
    p: SnippetFileRawParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/snippets/{}/files/{}/{}/raw",
        p.id,
        p.ref_name,
        encode_path_segment(&p.file_path),
    );
    let content = client.get_text(&path, &[]).await?;
    Ok(json!({"content": content}))
}

// --------------------------------------------------------------------------
// Create snippet
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetFileInput {
    #[schemars(description = "Content of the snippet file")]
    pub content: String,
    #[schemars(description = "File path within the snippet")]
    pub file_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetCreateParams {
    #[schemars(description = "Title of the snippet")]
    pub title: String,
    #[schemars(description = "Array of snippet files, each with content and file_path (required)")]
    pub files: Vec<SnippetFileInput>,
    #[schemars(description = "Description of the snippet")]
    pub description: Option<String>,
    #[schemars(
        description = "Visibility level: \"public\", \"internal\", or \"private\" (default: \"private\")"
    )]
    pub visibility: Option<String>,
}

pub async fn snippet_create(
    client: &GitlabClient,
    p: SnippetCreateParams,
) -> Result<Value, GitlabError> {
    let files: Vec<Value> = p
        .files
        .into_iter()
        .map(|f| json!({"content": f.content, "file_path": f.file_path}))
        .collect();
    let visibility = p.visibility.as_deref().unwrap_or("private");
    let body = BodyBuilder::new()
        .req("title", &p.title)
        .req("files", &files)
        .opt("description", p.description)
        .req("visibility", visibility)
        .build();
    client.post("/api/v4/snippets", &body).await
}

// --------------------------------------------------------------------------
// Update snippet
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetFileUpdateInput {
    #[schemars(description = "Action to perform: \"create\", \"update\", \"delete\", or \"move\"")]
    pub action: String,
    #[schemars(description = "File path of the snippet file")]
    pub file_path: Option<String>,
    #[schemars(description = "Previous file path (required for \"move\" action)")]
    pub previous_path: Option<String>,
    #[schemars(description = "New content of the file (for \"create\" or \"update\" actions)")]
    pub content: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetUpdateParams {
    #[schemars(description = "ID of the snippet to update")]
    pub id: u64,
    #[schemars(description = "New title for the snippet")]
    pub title: Option<String>,
    #[schemars(description = "New description for the snippet")]
    pub description: Option<String>,
    #[schemars(description = "New visibility level: \"public\", \"internal\", or \"private\"")]
    pub visibility: Option<String>,
    #[schemars(
        description = "Array of file changes; each entry requires action (\"create\", \"update\", \"delete\", \"move\") and optional file_path, previous_path, content"
    )]
    pub files: Option<Vec<SnippetFileUpdateInput>>,
}

pub async fn snippet_update(
    client: &GitlabClient,
    p: SnippetUpdateParams,
) -> Result<Value, GitlabError> {
    let files: Option<Vec<Value>> = p.files.map(|fs| {
        fs.into_iter()
            .map(|f| {
                let mut obj = serde_json::Map::new();
                obj.insert("action".into(), json!(f.action));
                if let Some(fp) = f.file_path {
                    obj.insert("file_path".into(), json!(fp));
                }
                if let Some(pp) = f.previous_path {
                    obj.insert("previous_path".into(), json!(pp));
                }
                if let Some(c) = f.content {
                    obj.insert("content".into(), json!(c));
                }
                Value::Object(obj)
            })
            .collect()
    });
    let body = BodyBuilder::new()
        .opt("title", p.title)
        .opt("description", p.description)
        .opt("visibility", p.visibility)
        .opt("files", files)
        .build();
    client
        .put(&format!("/api/v4/snippets/{}", p.id), &body)
        .await
}

// --------------------------------------------------------------------------
// Delete snippet
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetDeleteParams {
    #[schemars(description = "ID of the snippet to delete")]
    pub id: u64,
}

pub async fn snippet_delete(
    client: &GitlabClient,
    p: SnippetDeleteParams,
) -> Result<(), GitlabError> {
    client.delete(&format!("/api/v4/snippets/{}", p.id)).await
}

// --------------------------------------------------------------------------
// Get user agent detail (admin only)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetUserAgentDetailParams {
    #[schemars(description = "ID of the snippet")]
    pub id: u64,
}

pub async fn snippet_user_agent_detail(
    client: &GitlabClient,
    p: SnippetUserAgentDetailParams,
) -> Result<Value, GitlabError> {
    client
        .get(&format!("/api/v4/snippets/{}/user_agent_detail", p.id))
        .await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_snippets, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List snippets for the current authenticated user. Optional: created_after, created_before (ISO 8601). Paginate with page and per_page."
    )]
    async fn gitlab_snippets_list(
        &self,
        Parameters(p): Parameters<SnippetsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, snippets_list, p, "snippets")
    }

    #[tool(
        description = "List all public snippets. Optional: created_after, created_before (ISO 8601). Paginate with page and per_page."
    )]
    async fn gitlab_snippets_public_list(
        &self,
        Parameters(p): Parameters<SnippetsPublicListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, snippets_public_list, p, "public snippets")
    }

    #[tool(
        description = "List all snippets the current user has access to (administrators and auditors see all snippets). Optional: created_after, created_before, repository_storage (admin only). Paginate with page and per_page."
    )]
    async fn gitlab_snippets_all_list(
        &self,
        Parameters(p): Parameters<SnippetsAllListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, snippets_all_list, p, "all snippets")
    }

    #[tool(description = "Get a single GitLab snippet by ID.")]
    async fn gitlab_snippets_get(
        &self,
        Parameters(p): Parameters<SnippetGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, snippet_get, p, "snippet")
    }

    #[tool(description = "Get the raw content of a GitLab snippet. Returns {\"content\": \"...\"}")]
    async fn gitlab_snippets_raw(
        &self,
        Parameters(p): Parameters<SnippetRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, snippet_raw, p, "snippet raw content")
    }

    #[tool(
        description = "Get the raw content of a specific file in a GitLab snippet repository. Required: id, ref_name (branch/tag/commit), file_path (URL-encoded). Returns {\"content\": \"...\"}."
    )]
    async fn gitlab_snippets_file_raw(
        &self,
        Parameters(p): Parameters<SnippetFileRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, snippet_file_raw, p, "snippet file content")
    }

    #[tool(
        description = "Create a new GitLab snippet. Required: title, files (array of {content, file_path}). Optional: description, visibility (\"public\", \"internal\", or \"private\")."
    )]
    async fn gitlab_snippets_create(
        &self,
        Parameters(p): Parameters<SnippetCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, snippet_create, p, "snippet")
    }

    #[tool(
        description = "Update an existing GitLab snippet. Required: id. Optional: title, description, visibility, files (array of {action, file_path, previous_path, content}; action must be \"create\", \"update\", \"delete\", or \"move\")."
    )]
    async fn gitlab_snippets_update(
        &self,
        Parameters(p): Parameters<SnippetUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, snippet_update, p, "snippet")
    }

    #[tool(
        description = "Delete a GitLab snippet by ID. This action is permanent and cannot be undone."
    )]
    async fn gitlab_snippets_delete(
        &self,
        Parameters(p): Parameters<SnippetDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, snippet_delete, p, "snippet")
    }

    #[tool(
        description = "Get user agent details for a GitLab snippet (administrators only). Returns ip_address, user_agent, and akismet_submitted."
    )]
    async fn gitlab_snippets_user_agent_detail(
        &self,
        Parameters(p): Parameters<SnippetUserAgentDetailParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            snippet_user_agent_detail,
            p,
            "snippet user agent detail"
        )
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{SnippetFileRawParams, snippet_file_raw};
    use crate::test_util::mock_client;

    #[tokio::test]
    async fn snippet_file_raw_encodes_slashes_in_file_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/snippets/5/files/main/src%2Fmain.rs/raw"))
            .respond_with(ResponseTemplate::new(200).set_body_string("fn main() {}"))
            .mount(&server)
            .await;

        let result = snippet_file_raw(
            &mock_client(&server),
            SnippetFileRawParams {
                id: 5,
                ref_name: "main".into(),
                file_path: "src/main.rs".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(result["content"], "fn main() {}");
    }

    #[tokio::test]
    async fn snippet_file_raw_plain_path_passes_through_unchanged() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/snippets/3/files/main/README.md/raw"))
            .respond_with(ResponseTemplate::new(200).set_body_string("# hello"))
            .mount(&server)
            .await;

        let result = snippet_file_raw(
            &mock_client(&server),
            SnippetFileRawParams {
                id: 3,
                ref_name: "main".into(),
                file_path: "README.md".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(result["content"], "# hello");
    }
}
