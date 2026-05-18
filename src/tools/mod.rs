use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::*,
    prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

use crate::client::{GitlabClient, GitlabError};

pub mod branches;
pub mod issues;
pub mod merge_requests;
pub mod repositories;
pub mod repository_files;

// --------------------------------------------------------------------------
// Shared helpers
// --------------------------------------------------------------------------

/// Pagination fields shared by every list-params struct.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct PaginationParams {
    #[schemars(description = "Page number (default: 1)")]
    pub page: Option<u64>,
    #[schemars(description = "Number of results per page (default: 20, max: 100)")]
    pub per_page: Option<u64>,
}

pub fn json_result(v: Value) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(&v)
        .map_err(|e| McpError::internal_error(format!("marshalling response: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

pub fn tool_error(msg: &str) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(msg)]))
}

// --------------------------------------------------------------------------
// Query construction
// --------------------------------------------------------------------------

/// URL-encode a project ID for use in REST API paths.
/// Numeric IDs pass through unchanged; path-style IDs like
/// "mygroup/myrepo" have slashes replaced with %2F.
pub(crate) fn encode_project_id(id: &str) -> String {
    if id.chars().all(|c| c.is_ascii_digit()) {
        id.to_string()
    } else {
        id.replace('/', "%2F")
    }
}

pub struct QueryBuilder {
    params: Vec<(&'static str, String)>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self { params: vec![] }
    }

    pub fn opt<T: ToString>(mut self, key: &'static str, v: Option<T>) -> Self {
        if let Some(v) = v {
            self.params.push((key, v.to_string()));
        }
        self
    }

    pub fn into_params(self) -> Vec<(&'static str, String)> {
        self.params
    }
}

// --------------------------------------------------------------------------
// Delegation macros
// --------------------------------------------------------------------------

macro_rules! delegate_list {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(v) => json_result(v),
            Err(e) => tool_error(&format!("listing {}: {}", $noun, e.to_tool_message())),
        }
    }};
}

macro_rules! delegate_get {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(v) => json_result(v),
            Err(e) => tool_error(&format!("getting {}: {}", $noun, e.to_tool_message())),
        }
    }};
}

macro_rules! delegate_create {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(v) => json_result(v),
            Err(e) => tool_error(&format!("creating {}: {}", $noun, e.to_tool_message())),
        }
    }};
}

macro_rules! delegate_update {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(v) => json_result(v),
            Err(e) => tool_error(&format!("updating {}: {}", $noun, e.to_tool_message())),
        }
    }};
}

macro_rules! delegate_delete {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "{} deleted",
                $noun
            ))])),
            Err(e) => tool_error(&format!("deleting {}: {}", $noun, e.to_tool_message())),
        }
    }};
}

// --------------------------------------------------------------------------
// Server struct
// --------------------------------------------------------------------------

#[derive(Clone)]
pub struct GitlabMcpServer {
    /// Shared GitLab client. In HTTP mode this is populated in `initialize()`
    /// once the per-session bearer token has been extracted from request headers.
    client: Arc<OnceLock<GitlabClient>>,
    base_url: String,
    #[allow(dead_code)]
    tool_router: ToolRouter<GitlabMcpServer>,
    #[allow(dead_code)]
    prompt_router: PromptRouter<GitlabMcpServer>,
}

impl GitlabMcpServer {
    /// Create a server with an already-known token (stdio mode).
    pub fn new_stdio(base_url: String, token: String) -> anyhow::Result<Self> {
        let cell = OnceLock::new();
        let _ = cell.set(GitlabClient::new(base_url.clone(), token)?);
        Ok(Self {
            client: Arc::new(cell),
            base_url,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        })
    }

    /// Create a server without a token (HTTP mode — token injected in `initialize()`).
    pub fn new_http(base_url: String) -> Self {
        Self {
            client: Arc::new(OnceLock::new()),
            base_url,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    fn get_client(&self) -> Result<&GitlabClient, McpError> {
        self.client
            .get()
            .ok_or_else(|| McpError::internal_error("GitLab client not initialized", None))
    }
}

// --------------------------------------------------------------------------
// Tool shims
// --------------------------------------------------------------------------

#[tool_router]
impl GitlabMcpServer {
    #[tool(
        description = "List issues for a GitLab project. Filters: state (opened/closed/all), labels, search, scope, assignee_id, author_id, order_by, sort. Paginate with page and per_page."
    )]
    async fn gitlab_issues_list(
        &self,
        Parameters(p): Parameters<issues::IssuesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issues::issues_list, p, "issues")
    }

    #[tool(
        description = "Get a single GitLab issue by project ID and issue IID (the issue number shown in the GitLab UI)."
    )]
    async fn gitlab_issues_get(
        &self,
        Parameters(p): Parameters<issues::IssueGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, issues::issue_get, p, "issue")
    }

    #[tool(
        description = "Create a new issue in a GitLab project. Required: project_id, title. Optional: description, labels, assignee_ids, milestone_id, due_date, weight."
    )]
    async fn gitlab_issues_create(
        &self,
        Parameters(p): Parameters<issues::IssueCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, issues::issue_create, p, "issue")
    }

    #[tool(
        description = "Update an existing GitLab issue. Use state_event=\"close\" to close it or \"reopen\" to reopen it. All fields except project_id and issue_iid are optional."
    )]
    async fn gitlab_issues_update(
        &self,
        Parameters(p): Parameters<issues::IssueUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, issues::issue_update, p, "issue")
    }

    #[tool(
        description = "Delete a GitLab issue. Requires at least Maintainer role on the project. This action is permanent and cannot be undone."
    )]
    async fn gitlab_issues_delete(
        &self,
        Parameters(p): Parameters<issues::IssueDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, issues::issue_delete, p, "issue")
    }

    #[tool(
        description = "List merge requests for a GitLab project. Filters: state (opened/closed/merged/all), source_branch, target_branch, author_id, assignee_id, reviewer_id, labels, search, draft, scope, order_by, sort. Paginate with page and per_page."
    )]
    async fn gitlab_mrs_list(
        &self,
        Parameters(p): Parameters<merge_requests::MrsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, merge_requests::mrs_list, p, "merge requests")
    }

    #[tool(
        description = "Get a single GitLab merge request by project ID and merge request IID (the number shown in the GitLab UI)."
    )]
    async fn gitlab_mrs_get(
        &self,
        Parameters(p): Parameters<merge_requests::MrGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, merge_requests::mr_get, p, "merge request")
    }

    #[tool(
        description = "Create a new merge request in a GitLab project. Required: project_id, source_branch, target_branch, title. Optional: description, assignee_id, reviewer_ids, labels, milestone_id, squash, remove_source_branch, draft."
    )]
    async fn gitlab_mrs_create(
        &self,
        Parameters(p): Parameters<merge_requests::MrCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, merge_requests::mr_create, p, "merge request")
    }

    #[tool(
        description = "Update an existing GitLab merge request. Use state_event=\"close\" to close or \"reopen\" to reopen. All fields except project_id and merge_request_iid are optional."
    )]
    async fn gitlab_mrs_update(
        &self,
        Parameters(p): Parameters<merge_requests::MrUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, merge_requests::mr_update, p, "merge request")
    }

    #[tool(
        description = "Delete a GitLab merge request. Requires at least Maintainer role. This action is permanent and cannot be undone."
    )]
    async fn gitlab_mrs_delete(
        &self,
        Parameters(p): Parameters<merge_requests::MrDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, merge_requests::mr_delete, p, "merge request")
    }

    #[tool(
        description = "Accept and merge a GitLab merge request. Optional: merge_commit_message, squash, should_remove_source_branch, merge_when_pipeline_succeeds."
    )]
    async fn gitlab_mrs_merge(
        &self,
        Parameters(p): Parameters<merge_requests::MrMergeParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, merge_requests::mr_merge, p, "merge request")
    }

    #[tool(
        description = "List branches for a GitLab project, sorted alphabetically. Optional filters: search (substring match) and regex (re2 regular expression). Paginate with page and per_page."
    )]
    async fn gitlab_branches_list(
        &self,
        Parameters(p): Parameters<branches::BranchesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, branches::branches_list, p, "branches")
    }

    #[tool(
        description = "Get a single GitLab branch by project and branch name. Returns commit details and protection status."
    )]
    async fn gitlab_branches_get(
        &self,
        Parameters(p): Parameters<branches::BranchGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, branches::branch_get, p, "branch")
    }

    #[tool(
        description = "Create a new branch in a GitLab project. Required: project_id, branch (new branch name), ref (source branch name or commit SHA)."
    )]
    async fn gitlab_branches_create(
        &self,
        Parameters(p): Parameters<branches::BranchCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, branches::branch_create, p, "branch")
    }

    #[tool(
        description = "Delete a GitLab branch by name. Cannot delete default or protected branches."
    )]
    async fn gitlab_branches_delete(
        &self,
        Parameters(p): Parameters<branches::BranchDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, branches::branch_delete, p, "branch")
    }

    #[tool(
        description = "Delete all branches in a GitLab project that have been merged into the default branch. Protected branches are excluded."
    )]
    async fn gitlab_branches_delete_merged(
        &self,
        Parameters(p): Parameters<branches::BranchesDeleteMergedParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, branches::branches_delete_merged, p, "merged branches")
    }

    #[tool(
        description = "List files and directories in a GitLab repository tree. Optional: path (subdirectory), ref (branch/tag/SHA), recursive, pagination mode (keyset), page_token, page, per_page."
    )]
    async fn gitlab_repo_tree(
        &self,
        Parameters(p): Parameters<repositories::RepoTreeListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, repositories::repo_tree_list, p, "repository tree")
    }

    #[tool(
        description = "Get metadata for a GitLab repository blob (file) by its SHA. Returns content (Base64 encoded), encoding, sha, and size in bytes."
    )]
    async fn gitlab_repo_blob_get(
        &self,
        Parameters(p): Parameters<repositories::RepoBlobGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repositories::repo_blob_get, p, "blob")
    }

    #[tool(
        description = "Get the raw text content of a GitLab repository blob by its SHA. Best suited for text files; binary files may not decode cleanly."
    )]
    async fn gitlab_repo_blob_raw(
        &self,
        Parameters(p): Parameters<repositories::RepoBlobRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repositories::repo_blob_raw, p, "raw blob")
    }

    #[tool(
        description = "Compare two refs (branches, tags, or commit SHAs) in a GitLab repository. Returns commit list, diffs, and comparison metadata. Optional: from_project_id, straight (direct diff), unidiff (unified format)."
    )]
    async fn gitlab_repo_compare(
        &self,
        Parameters(p): Parameters<repositories::RepoCompareParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repositories::repo_compare, p, "repository comparison")
    }

    #[tool(
        description = "List contributors for a GitLab repository with commit counts, additions, and deletions. Optional: order_by (name/email/commits), sort (asc/desc), ref (branch/tag/SHA), page, per_page."
    )]
    async fn gitlab_repo_contributors(
        &self,
        Parameters(p): Parameters<repositories::RepoContributorsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            repositories::repo_contributors_list,
            p,
            "contributors"
        )
    }

    #[tool(
        description = "Find the common ancestor (merge base) of two or more refs (commit SHAs, branch names, or tag names) in a GitLab repository."
    )]
    async fn gitlab_repo_merge_base(
        &self,
        Parameters(p): Parameters<repositories::RepoMergeBaseParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repositories::repo_merge_base, p, "merge base")
    }

    #[tool(
        description = "Generate changelog markdown for a semantic version without committing it. Required: project_id, version. Optional: config_file, config_file_ref, from, to, trailer, date."
    )]
    async fn gitlab_repo_changelog_get(
        &self,
        Parameters(p): Parameters<repositories::RepoChangelogGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repositories::repo_changelog_get, p, "changelog")
    }

    #[tool(
        description = "Generate changelog for a semantic version and commit it to the repository. Required: project_id, version. Optional: branch, config_file, config_file_ref, file, from, to, message, trailer, date."
    )]
    async fn gitlab_repo_changelog_add(
        &self,
        Parameters(p): Parameters<repositories::RepoChangelogAddParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, repositories::repo_changelog_add, p, "changelog")
    }

    #[tool(
        description = "Get repository health statistics for a GitLab project, including size, references, objects, commit graph, and bitmap information. Optional: generate (create a report if none exists)."
    )]
    async fn gitlab_repo_health(
        &self,
        Parameters(p): Parameters<repositories::RepoHealthParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repositories::repo_health, p, "repository health")
    }

    #[tool(
        description = "Get a file from a GitLab repository. Returns metadata and Base64-encoded content. Required: project_id, file_path (e.g. \"src/main.rs\"), ref_name (branch/tag/SHA or \"HEAD\" for default branch)."
    )]
    async fn gitlab_file_get(
        &self,
        Parameters(p): Parameters<repository_files::FileGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repository_files::file_get, p, "file")
    }

    #[tool(
        description = "Get the raw text content of a file from a GitLab repository. Required: project_id, file_path. Optional: ref_name (default: HEAD), lfs (return LFS object instead of pointer)."
    )]
    async fn gitlab_file_raw(
        &self,
        Parameters(p): Parameters<repository_files::FileRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repository_files::file_raw, p, "raw file")
    }

    #[tool(
        description = "Get the blame history for a file in a GitLab repository, showing which commit last modified each line. Required: project_id, file_path, ref_name. Optional: range_start, range_end (1-based line numbers)."
    )]
    async fn gitlab_file_blame(
        &self,
        Parameters(p): Parameters<repository_files::FileBlameParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, repository_files::file_blame, p, "file blame")
    }

    #[tool(
        description = "Create a new file in a GitLab repository. Required: project_id, file_path, branch, commit_message, content. Optional: encoding (\"base64\"), author_name, author_email, execute_filemode, start_branch."
    )]
    async fn gitlab_file_create(
        &self,
        Parameters(p): Parameters<repository_files::FileCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, repository_files::file_create, p, "file")
    }

    #[tool(
        description = "Update an existing file in a GitLab repository. Required: project_id, file_path, branch, commit_message, content. Optional: encoding (\"base64\"), author_name, author_email, execute_filemode, last_commit_id, start_branch."
    )]
    async fn gitlab_file_update(
        &self,
        Parameters(p): Parameters<repository_files::FileUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, repository_files::file_update, p, "file")
    }

    #[tool(
        description = "Delete a file from a GitLab repository by committing its removal. Required: project_id, file_path, branch, commit_message. Optional: author_name, author_email, last_commit_id, start_branch."
    )]
    async fn gitlab_file_delete(
        &self,
        Parameters(p): Parameters<repository_files::FileDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, repository_files::file_delete, p, "file")
    }
}

// --------------------------------------------------------------------------
// Prompts (empty — no project-specific prompts for initial implementation)
// --------------------------------------------------------------------------

#[prompt_router]
impl GitlabMcpServer {}

// --------------------------------------------------------------------------
// ServerHandler
// --------------------------------------------------------------------------

#[tool_handler]
#[prompt_handler]
impl ServerHandler for GitlabMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("gitlab-mcp", env!("CARGO_PKG_VERSION")))
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        // In HTTP mode, extract the bearer token from the request headers and
        // create the per-session GitlabClient. OnceLock::set is idempotent so
        // a re-initialize call is a silent no-op rather than a panic.
        if let Some(parts) = context.extensions.get::<axum::http::request::Parts>()
            && let Some(token) = parts
                .headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        {
            match GitlabClient::new(self.base_url.clone(), token) {
                Ok(client) => {
                    let _ = self.client.set(client);
                }
                Err(e) => {
                    return Err(McpError::internal_error(
                        format!("invalid token: {e}"),
                        None,
                    ));
                }
            }
        }
        Ok(self.get_info())
    }
}

// Suppress the unused import warning — GitlabError is referenced only in macro expansions.
#[allow(unused_imports)]
use GitlabError as _;
