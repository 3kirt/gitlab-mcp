use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{BodyBuilder, QueryBuilder, encode_path_segment, project_path};

// --------------------------------------------------------------------------
// Get file (metadata + Base64 content)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileGetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Full path to the file (e.g. \"src/main.rs\"); slashes are encoded automatically"
    )]
    pub file_path: String,
    #[schemars(
        description = "Branch, tag, or commit SHA to read from; use \"HEAD\" for the default branch"
    )]
    pub ref_name: String,
}

pub async fn file_get(client: &GitlabClient, p: FileGetParams) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/files/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let params = QueryBuilder::new()
        .opt("ref", Some(p.ref_name))
        .into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Get raw file content
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileRawParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Full path to the file (e.g. \"src/main.rs\"); slashes are encoded automatically"
    )]
    pub file_path: String,
    #[schemars(description = "Branch, tag, or commit SHA (default: HEAD of default branch)")]
    pub ref_name: Option<String>,
    #[schemars(
        description = "Return Git LFS object contents instead of the pointer (default: false)"
    )]
    pub lfs: Option<bool>,
}

pub async fn file_raw(client: &GitlabClient, p: FileRawParams) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/files/{}/raw",
        project_path(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let params = QueryBuilder::new()
        .opt("ref", p.ref_name)
        .opt("lfs", p.lfs)
        .into_params();
    let content = client.get_text(&path, &params).await?;
    Ok(json!({"content": content}))
}

// --------------------------------------------------------------------------
// Get file blame
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileBlameParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Full path to the file (e.g. \"src/main.rs\"); slashes are encoded automatically"
    )]
    pub file_path: String,
    #[schemars(
        description = "Branch, tag, or commit SHA to read from; use \"HEAD\" for the default branch"
    )]
    pub ref_name: String,
    #[schemars(description = "First line number of the blame range (1-based, inclusive)")]
    pub range_start: Option<u64>,
    #[schemars(description = "Last line number of the blame range (1-based, inclusive)")]
    pub range_end: Option<u64>,
}

pub async fn file_blame(client: &GitlabClient, p: FileBlameParams) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/files/{}/blame",
        project_path(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let params = QueryBuilder::new()
        .opt("ref", Some(p.ref_name))
        .opt("range[start]", p.range_start)
        .opt("range[end]", p.range_end)
        .into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Create file
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileCreateParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Full path to the new file (e.g. \"src/main.rs\"); slashes are encoded automatically"
    )]
    pub file_path: String,
    #[schemars(description = "Branch to commit the new file to")]
    pub branch: String,
    #[schemars(description = "Commit message")]
    pub commit_message: String,
    #[schemars(description = "File content (plain text unless encoding is \"base64\")")]
    pub content: String,
    #[schemars(description = "Content encoding: \"text\" (default) or \"base64\"")]
    pub encoding: Option<String>,
    #[schemars(description = "Name of the commit author")]
    pub author_name: Option<String>,
    #[schemars(description = "Email of the commit author")]
    pub author_email: Option<String>,
    #[schemars(description = "Set the execute bit on the file (default: false)")]
    pub execute_filemode: Option<bool>,
    #[schemars(
        description = "Base branch to create the target branch from if it does not yet exist"
    )]
    pub start_branch: Option<String>,
}

pub async fn file_create(client: &GitlabClient, p: FileCreateParams) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/files/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let body = BodyBuilder::new()
        .req("branch", &p.branch)
        .req("commit_message", &p.commit_message)
        .req("content", &p.content)
        .opt("encoding", p.encoding)
        .opt("author_name", p.author_name)
        .opt("author_email", p.author_email)
        .opt("execute_filemode", p.execute_filemode)
        .opt("start_branch", p.start_branch)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Update file
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileUpdateParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Full path to the file (e.g. \"src/main.rs\"); slashes are encoded automatically"
    )]
    pub file_path: String,
    #[schemars(description = "Branch to commit the update to")]
    pub branch: String,
    #[schemars(description = "Commit message")]
    pub commit_message: String,
    #[schemars(description = "New file content (plain text unless encoding is \"base64\")")]
    pub content: String,
    #[schemars(description = "Content encoding: \"text\" (default) or \"base64\"")]
    pub encoding: Option<String>,
    #[schemars(description = "Name of the commit author")]
    pub author_name: Option<String>,
    #[schemars(description = "Email of the commit author")]
    pub author_email: Option<String>,
    #[schemars(description = "Set the execute bit on the file")]
    pub execute_filemode: Option<bool>,
    #[schemars(
        description = "Last known commit ID for the file; used to prevent overwriting concurrent changes"
    )]
    pub last_commit_id: Option<String>,
    #[schemars(
        description = "Base branch to create the target branch from if it does not yet exist"
    )]
    pub start_branch: Option<String>,
}

pub async fn file_update(client: &GitlabClient, p: FileUpdateParams) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/files/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let body = BodyBuilder::new()
        .req("branch", &p.branch)
        .req("commit_message", &p.commit_message)
        .req("content", &p.content)
        .opt("encoding", p.encoding)
        .opt("author_name", p.author_name)
        .opt("author_email", p.author_email)
        .opt("execute_filemode", p.execute_filemode)
        .opt("last_commit_id", p.last_commit_id)
        .opt("start_branch", p.start_branch)
        .build();
    client.put(&path, &body).await
}

// --------------------------------------------------------------------------
// Delete file
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileDeleteParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Full path to the file to delete (e.g. \"src/main.rs\"); slashes are encoded automatically"
    )]
    pub file_path: String,
    #[schemars(description = "Branch to commit the deletion to")]
    pub branch: String,
    #[schemars(description = "Commit message")]
    pub commit_message: String,
    #[schemars(description = "Name of the commit author")]
    pub author_name: Option<String>,
    #[schemars(description = "Email of the commit author")]
    pub author_email: Option<String>,
    #[schemars(
        description = "Last known commit ID for the file; used to prevent overwriting concurrent changes"
    )]
    pub last_commit_id: Option<String>,
    #[schemars(
        description = "Base branch to create the target branch from if it does not yet exist"
    )]
    pub start_branch: Option<String>,
}

pub async fn file_delete(client: &GitlabClient, p: FileDeleteParams) -> Result<(), GitlabError> {
    let path = format!(
        "{}/repository/files/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let body = BodyBuilder::new()
        .req("branch", &p.branch)
        .req("commit_message", &p.commit_message)
        .opt("author_name", p.author_name)
        .opt("author_email", p.author_email)
        .opt("last_commit_id", p.last_commit_id)
        .opt("start_branch", p.start_branch)
        .build();
    client.delete_with_body(&path, &body).await?;
    Ok(())
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_repository_files, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "Get a file from a GitLab repository. Returns metadata and Base64-encoded content. Required: project_id, file_path (e.g. \"src/main.rs\"), ref_name (branch/tag/SHA or \"HEAD\" for default branch).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_file_get(
        &self,
        Parameters(p): Parameters<FileGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, file_get, p, "file")
    }

    #[tool(
        description = "Get the raw text content of a file from a GitLab repository. Required: project_id, file_path. Optional: ref_name (default: HEAD), lfs (return LFS object instead of pointer).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_file_raw(
        &self,
        Parameters(p): Parameters<FileRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, file_raw, p, "raw file")
    }

    #[tool(
        description = "Get the blame history for a file in a GitLab repository, showing which commit last modified each line. Required: project_id, file_path, ref_name. Optional: range_start, range_end (1-based line numbers).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_file_blame(
        &self,
        Parameters(p): Parameters<FileBlameParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, file_blame, p, "file blame")
    }

    #[tool(
        description = "Create a new file in a GitLab repository. Required: project_id, file_path, branch, commit_message, content. Optional: encoding (\"base64\"), author_name, author_email, execute_filemode, start_branch.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_file_create(
        &self,
        Parameters(p): Parameters<FileCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, file_create, p, "file")
    }

    #[tool(
        description = "Update an existing file in a GitLab repository. Required: project_id, file_path, branch, commit_message, content. Optional: encoding (\"base64\"), author_name, author_email, execute_filemode, last_commit_id, start_branch.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_file_update(
        &self,
        Parameters(p): Parameters<FileUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, file_update, p, "file")
    }

    #[tool(
        description = "Delete a file from a GitLab repository by committing its removal. Required: project_id, file_path, branch, commit_message. Optional: author_name, author_email, last_commit_id, start_branch.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_file_delete(
        &self,
        Parameters(p): Parameters<FileDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, file_delete, p, "file")
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
        FileBlameParams, FileCreateParams, FileDeleteParams, FileGetParams, FileRawParams,
        FileUpdateParams, file_blame, file_create, file_delete, file_get, file_raw, file_update,
    };
    use crate::test_util::mock_client;

    fn captured_body(reqs: &[wiremock::Request], m: &wiremock::http::Method) -> serde_json::Value {
        reqs.iter()
            .find(|r| r.method == *m)
            .and_then(|r| r.body_json::<serde_json::Value>().ok())
            .expect("request not found")
    }

    #[tokio::test]
    async fn file_get_encodes_slashes_in_file_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/files/src%2Fmain.rs"))
            .and(query_param("ref", "main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "file_path": "src/main.rs",
                "ref": "main"
            })))
            .mount(&server)
            .await;

        let item = file_get(
            &mock_client(&server),
            FileGetParams {
                project_id: "42".into(),
                file_path: "src/main.rs".into(),
                ref_name: "main".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(item["file_path"], "src/main.rs");
    }

    #[tokio::test]
    async fn file_raw_wraps_text_response_in_content_field() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/files/README.md/raw"))
            .and(query_param("ref", "main"))
            .respond_with(ResponseTemplate::new(200).set_body_string("# hello"))
            .mount(&server)
            .await;

        let item = file_raw(
            &mock_client(&server),
            FileRawParams {
                project_id: "42".into(),
                file_path: "README.md".into(),
                ref_name: Some("main".into()),
                lfs: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["content"], "# hello");
    }

    #[tokio::test]
    async fn file_blame_uses_bracket_range_query_params() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v4/projects/42/repository/files/src%2Flib.rs/blame",
            ))
            .and(query_param("ref", "main"))
            .and(query_param("range[start]", "10"))
            .and(query_param("range[end]", "20"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        file_blame(
            &mock_client(&server),
            FileBlameParams {
                project_id: "42".into(),
                file_path: "src/lib.rs".into(),
                ref_name: "main".into(),
                range_start: Some(10),
                range_end: Some(20),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn file_create_posts_body_with_branch_and_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(
                "/api/v4/projects/42/repository/files/docs%2FREADME.md",
            ))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!({ "file_path": "docs/README.md" })),
            )
            .mount(&server)
            .await;

        file_create(
            &mock_client(&server),
            FileCreateParams {
                project_id: "42".into(),
                file_path: "docs/README.md".into(),
                branch: "main".into(),
                commit_message: "Add README".into(),
                content: "# Hello".into(),
                encoding: Some("text".into()),
                author_name: None,
                author_email: None,
                execute_filemode: None,
                start_branch: None,
            },
        )
        .await
        .unwrap();

        let body = captured_body(
            &server.received_requests().await.unwrap(),
            &wiremock::http::Method::POST,
        );
        assert_eq!(body["branch"], "main");
        assert_eq!(body["commit_message"], "Add README");
        assert_eq!(body["content"], "# Hello");
        assert_eq!(body["encoding"], "text");
    }

    #[tokio::test]
    async fn file_update_puts_body_with_last_commit_id() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v4/projects/42/repository/files/src%2Fa.rs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "file_path": "src/a.rs" })),
            )
            .mount(&server)
            .await;

        file_update(
            &mock_client(&server),
            FileUpdateParams {
                project_id: "42".into(),
                file_path: "src/a.rs".into(),
                branch: "main".into(),
                commit_message: "Update a.rs".into(),
                content: "fn main() {}".into(),
                encoding: None,
                author_name: None,
                author_email: None,
                execute_filemode: None,
                last_commit_id: Some("abc123".into()),
                start_branch: None,
            },
        )
        .await
        .unwrap();

        let body = captured_body(
            &server.received_requests().await.unwrap(),
            &wiremock::http::Method::PUT,
        );
        assert_eq!(body["last_commit_id"], "abc123");
        assert!(body.get("encoding").is_none());
    }

    #[tokio::test]
    async fn file_delete_sends_delete_with_body() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/42/repository/files/old%2Ffile.txt"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        file_delete(
            &mock_client(&server),
            FileDeleteParams {
                project_id: "42".into(),
                file_path: "old/file.txt".into(),
                branch: "main".into(),
                commit_message: "Drop file".into(),
                author_name: None,
                author_email: None,
                last_commit_id: None,
                start_branch: None,
            },
        )
        .await
        .unwrap();

        let body = captured_body(
            &server.received_requests().await.unwrap(),
            &wiremock::http::Method::DELETE,
        );
        assert_eq!(body["branch"], "main");
        assert_eq!(body["commit_message"], "Drop file");
    }

    #[tokio::test]
    async fn file_get_propagates_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/files/ghost.rs"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let err = file_get(
            &mock_client(&server),
            FileGetParams {
                project_id: "42".into(),
                file_path: "ghost.rs".into(),
                ref_name: "main".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }
}
