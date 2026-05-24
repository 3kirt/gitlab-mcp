use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder};

// --------------------------------------------------------------------------
// List current user's snippets
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetsListParams {
    #[schemars(description = "Return snippets created after this time (ISO 8601)")]
    pub created_after: Option<String>,
    #[schemars(description = "Return snippets created before this time (ISO 8601)")]
    pub created_before: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn snippets_list(client: &GitlabClient, p: SnippetsListParams) -> ListResult {
    let params = QueryBuilder::new()
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list("/api/v4/snippets", &params).await
}

// --------------------------------------------------------------------------
// List public snippets
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetsPublicListParams {
    #[schemars(description = "Return snippets created after this time (ISO 8601)")]
    pub created_after: Option<String>,
    #[schemars(description = "Return snippets created before this time (ISO 8601)")]
    pub created_before: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn snippets_public_list(
    client: &GitlabClient,
    p: SnippetsPublicListParams,
) -> ListResult {
    let params = QueryBuilder::new()
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list("/api/v4/snippets/public", &params).await
}

// --------------------------------------------------------------------------
// List all snippets (admin)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetsAllListParams {
    #[schemars(description = "Return snippets created after this time (ISO 8601)")]
    pub created_after: Option<String>,
    #[schemars(description = "Return snippets created before this time (ISO 8601)")]
    pub created_before: Option<String>,
    #[schemars(description = "Filter by repository storage used by snippet (administrators only)")]
    pub repository_storage: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn snippets_all_list(client: &GitlabClient, p: SnippetsAllListParams) -> ListResult {
    let params = QueryBuilder::new()
        .opt("created_after", p.created_after)
        .opt("created_before", p.created_before)
        .opt("repository_storage", p.repository_storage)
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list("/api/v4/snippets/all", &params).await
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
        p.file_path.replace('/', "%2F"),
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
    let body = BodyBuilder::new()
        .req("title", &p.title)
        .req("files", &files)
        .opt("description", p.description)
        .opt("visibility", p.visibility)
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

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{SnippetFileRawParams, snippet_file_raw};
    use crate::client::GitlabClient;

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

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
