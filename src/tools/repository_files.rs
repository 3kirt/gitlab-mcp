use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{BodyBuilder, QueryBuilder, encode_path_segment, encode_project_id};

// --------------------------------------------------------------------------
// Get file (metadata + Base64 content)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileGetParams {
    #[schemars(
        description = "Project ID or URL-encoded path (e.g. 42 or \"mygroup%2Fmyproject\")"
    )]
    pub project_id: String,
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
        "/api/v4/projects/{}/repository/files/{}",
        encode_project_id(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let params = QueryBuilder::new()
        .opt("ref", Some(p.ref_name))
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Get raw file content
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileRawParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/repository/files/{}/raw",
        encode_project_id(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let params = QueryBuilder::new()
        .opt("ref", p.ref_name)
        .opt("lfs", p.lfs)
        .into_params();
    let content = client.get_text_with_params(&path, &params).await?;
    Ok(json!({"content": content}))
}

// --------------------------------------------------------------------------
// Get file blame
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileBlameParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/repository/files/{}/blame",
        encode_project_id(&p.project_id),
        encode_path_segment(&p.file_path)
    );
    let params = QueryBuilder::new()
        .opt("ref", Some(p.ref_name))
        .opt("range[start]", p.range_start)
        .opt("range[end]", p.range_end)
        .into_params();
    client.list(&path, &params).await
}

// --------------------------------------------------------------------------
// Create file
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/repository/files/{}",
        encode_project_id(&p.project_id),
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
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/repository/files/{}",
        encode_project_id(&p.project_id),
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
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
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
        "/api/v4/projects/{}/repository/files/{}",
        encode_project_id(&p.project_id),
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
