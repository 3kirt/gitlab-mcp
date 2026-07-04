use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{
    BodyBuilder, PaginationParams, QueryBuilder, encode_path_segment, list_paginated, project_path,
};

// --------------------------------------------------------------------------
// List commits
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitsListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Branch name, tag, or revision range to list commits from")]
    pub ref_name: Option<String>,
    #[schemars(
        description = "Only commits after or on this date (ISO 8601: YYYY-MM-DDTHH:MM:SSZ)"
    )]
    pub since: Option<String>,
    #[schemars(
        description = "Only commits before or on this date (ISO 8601: YYYY-MM-DDTHH:MM:SSZ)"
    )]
    pub until: Option<String>,
    #[schemars(description = "File path to filter commits to those touching that path")]
    pub path: Option<String>,
    #[schemars(description = "Filter commits by author name")]
    pub author: Option<String>,
    #[schemars(description = "If true, retrieve every commit from the repository")]
    pub all: Option<bool>,
    #[schemars(description = "If true, follow only the first parent on merge commits")]
    pub first_parent: Option<bool>,
    #[schemars(description = "Commit ordering: \"default\" or \"topo\"")]
    pub order: Option<String>,
    #[schemars(description = "If true, include per-commit statistics")]
    pub with_stats: Option<bool>,
    #[schemars(description = "If true, parse and include Git trailers in the response")]
    pub trailers: Option<bool>,
    #[schemars(
        description = "If true, follow file renames when filtering by path (default: true)"
    )]
    pub follow: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn commits_list(client: &GitlabClient, p: CommitsListParams) -> ListResult {
    let path = format!("{}/repository/commits", project_path(&p.project_id));
    let qb = QueryBuilder::new()
        .opt("ref_name", p.ref_name)
        .opt("since", p.since)
        .opt("until", p.until)
        .opt("path", p.path)
        .opt("author", p.author)
        .opt("all", p.all)
        .opt("first_parent", p.first_parent)
        .opt("order", p.order)
        .opt("with_stats", p.with_stats)
        .opt("trailers", p.trailers)
        .opt("follow", p.follow);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Create a commit
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitAction {
    #[schemars(
        description = "Action type: \"create\", \"delete\", \"move\", \"update\", or \"chmod\""
    )]
    pub action: String,
    #[schemars(description = "Full file path in the repository")]
    pub file_path: String,
    #[schemars(
        description = "File content (required for create/update; omit for delete/chmod or move when preserving content)"
    )]
    pub content: Option<String>,
    #[schemars(description = "Content encoding: \"text\" (default) or \"base64\"")]
    pub encoding: Option<String>,
    #[schemars(description = "Original file path (required for move operations)")]
    pub previous_path: Option<String>,
    #[schemars(
        description = "Last known commit SHA for the file (for update/move/delete to prevent conflicts)"
    )]
    pub last_commit_id: Option<String>,
    #[schemars(description = "Enable or disable the execute flag on the file (chmod only)")]
    pub execute_filemode: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitCreateParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Target branch name for the commit")]
    pub branch: String,
    #[schemars(description = "Commit message")]
    pub commit_message: String,
    #[schemars(description = "Array of file actions to perform in this commit")]
    pub actions: Vec<CommitAction>,
    #[schemars(description = "Source branch to start from (mutually exclusive with start_sha)")]
    pub start_branch: Option<String>,
    #[schemars(
        description = "Source commit SHA to start from (mutually exclusive with start_branch)"
    )]
    pub start_sha: Option<String>,
    #[schemars(description = "Source project ID or path (for cross-project branching)")]
    pub start_project: Option<String>,
    #[schemars(description = "Commit author name")]
    pub author_name: Option<String>,
    #[schemars(description = "Commit author email")]
    pub author_email: Option<String>,
    #[schemars(description = "If true, overwrite branch history (force push)")]
    pub force: Option<bool>,
    #[schemars(description = "If true, allow creating an empty commit (default: false)")]
    pub allow_empty: Option<bool>,
    #[schemars(description = "If true, include commit statistics in the response (default: true)")]
    pub stats: Option<bool>,
}

pub async fn commit_create(
    client: &GitlabClient,
    p: CommitCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!("{}/repository/commits", project_path(&p.project_id));
    let actions_arr: Vec<Value> = p
        .actions
        .into_iter()
        .map(|a| {
            BodyBuilder::new()
                .req("action", &a.action)
                .req("file_path", &a.file_path)
                .opt("content", a.content)
                .opt("encoding", a.encoding)
                .opt("previous_path", a.previous_path)
                .opt("last_commit_id", a.last_commit_id)
                .opt("execute_filemode", a.execute_filemode)
                .build()
        })
        .collect();
    let body = BodyBuilder::new()
        .req("branch", &p.branch)
        .req("commit_message", &p.commit_message)
        .req("actions", actions_arr)
        .opt("start_branch", p.start_branch)
        .opt("start_sha", p.start_sha)
        .opt("start_project", p.start_project)
        .opt("author_name", p.author_name)
        .opt("author_email", p.author_email)
        .opt("force", p.force)
        .opt("allow_empty", p.allow_empty)
        .opt("stats", p.stats)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Get a single commit
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitGetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA, branch name, or tag name")]
    pub sha: String,
    #[schemars(description = "If true, include commit statistics (default: true)")]
    pub stats: Option<bool>,
}

pub async fn commit_get(client: &GitlabClient, p: CommitGetParams) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/commits/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let params = QueryBuilder::new().opt("stats", p.stats).into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// List refs for a commit
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitRefsParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA")]
    pub sha: String,
    #[schemars(description = "Filter by type: \"branch\", \"tag\", or \"all\" (default: \"all\")")]
    pub r#type: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn commit_refs(client: &GitlabClient, p: CommitRefsParams) -> ListResult {
    let path = format!(
        "{}/repository/commits/{}/refs",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let qb = QueryBuilder::new().opt("type", p.r#type);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get commit sequence number
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitSequenceParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA")]
    pub sha: String,
    #[schemars(description = "If true, follow only the first parent on merge commits")]
    pub first_parent: Option<bool>,
}

pub async fn commit_sequence(
    client: &GitlabClient,
    p: CommitSequenceParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/commits/{}/sequence",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let params = QueryBuilder::new()
        .opt("first_parent", p.first_parent)
        .into_params();
    client.get_with_params(&path, &params).await
}

// --------------------------------------------------------------------------
// Cherry-pick a commit
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitCherryPickParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA to cherry-pick")]
    pub sha: String,
    #[schemars(description = "Target branch to cherry-pick into")]
    pub branch: String,
    #[schemars(description = "If true, simulate without committing (default: false)")]
    pub dry_run: Option<bool>,
    #[schemars(description = "Custom commit message for the cherry-pick")]
    pub message: Option<String>,
}

pub async fn commit_cherry_pick(
    client: &GitlabClient,
    p: CommitCherryPickParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/commits/{}/cherry_pick",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let body = BodyBuilder::new()
        .req("branch", &p.branch)
        .opt("dry_run", p.dry_run)
        .opt("message", p.message)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Revert a commit
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitRevertParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA to revert")]
    pub sha: String,
    #[schemars(description = "Target branch to apply the revert to")]
    pub branch: String,
    #[schemars(description = "If true, simulate without committing (default: false)")]
    pub dry_run: Option<bool>,
}

pub async fn commit_revert(
    client: &GitlabClient,
    p: CommitRevertParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/commits/{}/revert",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let body = BodyBuilder::new()
        .req("branch", &p.branch)
        .opt("dry_run", p.dry_run)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// Get commit diff
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitDiffParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA, branch name, or tag name")]
    pub sha: String,
    #[schemars(description = "If true, use unified diff format (default: false)")]
    pub unidiff: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn commit_diff(client: &GitlabClient, p: CommitDiffParams) -> ListResult {
    let path = format!(
        "{}/repository/commits/{}/diff",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let qb = QueryBuilder::new().opt("unidiff", p.unidiff);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// List commit comments
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitCommentsListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA, branch name, or tag name")]
    pub sha: String,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn commit_comments_list(
    client: &GitlabClient,
    p: CommitCommentsListParams,
) -> ListResult {
    let path = format!(
        "{}/repository/commits/{}/comments",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// Post comment to commit
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitCommentCreateParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA, branch name, or tag name")]
    pub sha: String,
    #[schemars(description = "Comment text")]
    pub note: String,
    #[schemars(description = "File path relative to the repository root (for inline comments)")]
    pub path: Option<String>,
    #[schemars(description = "Line number for the comment (for inline comments)")]
    pub line: Option<u64>,
    #[schemars(description = "Line type context: \"new\" or \"old\"")]
    pub line_type: Option<String>,
}

pub async fn commit_comment_create(
    client: &GitlabClient,
    p: CommitCommentCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/commits/{}/comments",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let body = BodyBuilder::new()
        .req("note", &p.note)
        .opt("path", p.path)
        .opt("line", p.line)
        .opt("line_type", p.line_type)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// List commit discussions
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitDiscussionsListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA, branch name, or tag name")]
    pub sha: String,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn commit_discussions_list(
    client: &GitlabClient,
    p: CommitDiscussionsListParams,
) -> ListResult {
    let path = format!(
        "{}/repository/commits/{}/discussions",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    list_paginated(client, &path, QueryBuilder::new(), p.pagination).await
}

// --------------------------------------------------------------------------
// List commit statuses
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitStatusesListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA")]
    pub sha: String,
    #[schemars(
        description = "Branch or tag name to scope the statuses (default: project default branch)"
    )]
    pub r#ref: Option<String>,
    #[schemars(description = "Filter by job name")]
    pub name: Option<String>,
    #[schemars(description = "Filter by build stage")]
    pub stage: Option<String>,
    #[schemars(
        description = "If true, include all statuses, not just the latest per job (default: false)"
    )]
    pub all: Option<bool>,
    #[schemars(description = "Filter by pipeline ID")]
    pub pipeline_id: Option<u64>,
    #[schemars(description = "Sort field: \"id\" or \"pipeline_id\" (default: \"id\")")]
    pub order_by: Option<String>,
    #[schemars(description = "Sort direction: \"asc\" or \"desc\" (default: \"asc\")")]
    pub sort: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn commit_statuses_list(
    client: &GitlabClient,
    p: CommitStatusesListParams,
) -> ListResult {
    let path = format!(
        "{}/repository/commits/{}/statuses",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let qb = QueryBuilder::new()
        .opt("ref", p.r#ref)
        .opt("name", p.name)
        .opt("stage", p.stage)
        .opt("all", p.all)
        .opt("pipeline_id", p.pipeline_id)
        .opt("order_by", p.order_by)
        .opt("sort", p.sort);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Set commit pipeline status
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitStatusSetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA")]
    pub sha: String,
    #[schemars(
        description = "Status value: \"pending\", \"running\", \"success\", \"failed\", \"canceled\", or \"skipped\""
    )]
    pub state: String,
    #[schemars(description = "Status label / context identifier (default: \"default\")")]
    pub name: Option<String>,
    #[schemars(description = "Branch or tag name to scope this status (max 255 characters)")]
    pub r#ref: Option<String>,
    #[schemars(description = "Short status description (max 255 characters)")]
    pub description: Option<String>,
    #[schemars(description = "URL to associate with this status (max 255 characters)")]
    pub target_url: Option<String>,
    #[schemars(description = "Code coverage percentage")]
    pub coverage: Option<f64>,
    #[schemars(description = "Specific pipeline ID to associate this status with")]
    pub pipeline_id: Option<u64>,
}

pub async fn commit_status_set(
    client: &GitlabClient,
    p: CommitStatusSetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/statuses/{}",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let body = BodyBuilder::new()
        .req("state", &p.state)
        .opt("name", p.name)
        .opt("ref", p.r#ref)
        .opt("description", p.description)
        .opt("target_url", p.target_url)
        .opt("coverage", p.coverage)
        .opt("pipeline_id", p.pipeline_id)
        .build();
    client.post(&path, &body).await
}

// --------------------------------------------------------------------------
// List merge requests associated with a commit
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitMergeRequestsParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA")]
    pub sha: String,
    #[schemars(description = "Filter by state: \"opened\", \"closed\", \"locked\", or \"merged\"")]
    pub state: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn commit_merge_requests(
    client: &GitlabClient,
    p: CommitMergeRequestsParams,
) -> ListResult {
    let path = format!(
        "{}/repository/commits/{}/merge_requests",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    let qb = QueryBuilder::new().opt("state", p.state);
    list_paginated(client, &path, qb, p.pagination).await
}

// --------------------------------------------------------------------------
// Get commit signature
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommitSignatureParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Commit SHA, branch name, or tag name")]
    pub sha: String,
}

pub async fn commit_signature(
    client: &GitlabClient,
    p: CommitSignatureParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "{}/repository/commits/{}/signature",
        project_path(&p.project_id),
        encode_path_segment(&p.sha)
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_commits, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List commits for a GitLab project. Optional filters: ref_name (branch/tag/range), since/until (ISO 8601), path (file filter), author, all, first_parent, order (default/topo), with_stats, trailers, follow. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_list(
        &self,
        Parameters(p): Parameters<CommitsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commits_list, p, "commits")
    }

    #[tool(
        description = "Get a single GitLab commit by SHA, branch name, or tag name. Optional: stats (include commit statistics, default true).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_get(
        &self,
        Parameters(p): Parameters<CommitGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, commit_get, p, "commit")
    }

    #[tool(
        description = "Create a commit in a GitLab project with one or more file actions (create, update, delete, move, chmod). Required: project_id, branch, commit_message, actions[]. Optional: start_branch, start_sha, start_project, author_name, author_email, force, allow_empty, stats.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_commits_create(
        &self,
        Parameters(p): Parameters<CommitCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commit_create, p, "commit")
    }

    #[tool(
        description = "List all branches and tags that contain a specific commit. Optional: type (\"branch\", \"tag\", or \"all\"), page, per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_refs(
        &self,
        Parameters(p): Parameters<CommitRefsParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commit_refs, p, "commit refs")
    }

    #[tool(
        description = "Get the sequence number of a commit (number of ancestors by following parent links). Optional: first_parent.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_sequence(
        &self,
        Parameters(p): Parameters<CommitSequenceParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, commit_sequence, p, "commit sequence")
    }

    #[tool(
        description = "Cherry-pick a commit into a target branch. Required: project_id, sha, branch. Optional: dry_run (simulate without committing), message (custom commit message).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_commits_cherry_pick(
        &self,
        Parameters(p): Parameters<CommitCherryPickParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commit_cherry_pick, p, "cherry-pick")
    }

    #[tool(
        description = "Revert a commit by creating a new revert commit on the target branch. Required: project_id, sha, branch. Optional: dry_run (simulate without committing).",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_commits_revert(
        &self,
        Parameters(p): Parameters<CommitRevertParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commit_revert, p, "revert")
    }

    #[tool(
        description = "Get the diff introduced by a specific commit. Optional: unidiff (use unified diff format, default false), page, per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_diff(
        &self,
        Parameters(p): Parameters<CommitDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commit_diff, p, "commit diff")
    }

    #[tool(
        description = "List all comments on a commit. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_comments_list(
        &self,
        Parameters(p): Parameters<CommitCommentsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commit_comments_list, p, "commit comments")
    }

    #[tool(
        description = "Post a comment on a commit. Required: project_id, sha, note. Optional: path (file path for inline comment), line (line number), line_type (\"new\" or \"old\").",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_commits_comment_create(
        &self,
        Parameters(p): Parameters<CommitCommentCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commit_comment_create, p, "commit comment")
    }

    #[tool(
        description = "List comment threads (discussions) on a commit. Paginate with page and per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_discussions_list(
        &self,
        Parameters(p): Parameters<CommitDiscussionsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commit_discussions_list, p, "commit discussions")
    }

    #[tool(
        description = "List CI/CD pipeline statuses for a commit. Optional: ref (branch/tag), name (job name filter), stage, all (include non-latest), pipeline_id, order_by (id/pipeline_id), sort (asc/desc), page, per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_statuses_list(
        &self,
        Parameters(p): Parameters<CommitStatusesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commit_statuses_list, p, "commit statuses")
    }

    #[tool(
        description = "Set a pipeline status on a commit (for external CI systems). Required: project_id, sha, state (pending/running/success/failed/canceled/skipped). Optional: name/context, ref, description, target_url, coverage, pipeline_id.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_commits_status_set(
        &self,
        Parameters(p): Parameters<CommitStatusSetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commit_status_set, p, "commit status")
    }

    #[tool(
        description = "List merge requests that introduced a specific commit. Optional: state (opened/closed/locked/merged), page, per_page.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_merge_requests(
        &self,
        Parameters(p): Parameters<CommitMergeRequestsParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commit_merge_requests, p, "commit merge requests")
    }

    #[tool(
        description = "Get the GPG, SSH, or X.509 signature for a signed commit. Returns 404 for unsigned commits.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_commits_signature(
        &self,
        Parameters(p): Parameters<CommitSignatureParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, commit_signature, p, "commit signature")
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{CommitAction, CommitCreateParams, commit_create};
    use crate::test_util::mock_client;

    fn captured_post_body(reqs: &[wiremock::Request]) -> serde_json::Value {
        reqs.iter()
            .find(|r| r.method == wiremock::http::Method::POST)
            .and_then(|r| r.body_json::<serde_json::Value>().ok())
            .expect("POST request not found")
    }

    fn action(action: &str, file_path: &str) -> CommitAction {
        CommitAction {
            action: action.into(),
            file_path: file_path.into(),
            content: None,
            encoding: None,
            previous_path: None,
            last_commit_id: None,
            execute_filemode: None,
        }
    }

    fn base_params(actions: Vec<CommitAction>) -> CommitCreateParams {
        CommitCreateParams {
            project_id: "42".into(),
            branch: "main".into(),
            commit_message: "test commit".into(),
            actions,
            start_branch: None,
            start_sha: None,
            start_project: None,
            author_name: None,
            author_email: None,
            force: None,
            allow_empty: None,
            stats: None,
        }
    }

    #[tokio::test]
    async fn commit_create_builds_nested_actions_array_with_single_create() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/repository/commits"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": "deadbeef" })),
            )
            .mount(&server)
            .await;

        let mut act = action("create", "src/new.rs");
        act.content = Some("fn x() {}".into());
        act.encoding = Some("text".into());
        commit_create(&mock_client(&server), base_params(vec![act]))
            .await
            .unwrap();

        let body = captured_post_body(&server.received_requests().await.unwrap());
        assert_eq!(body["branch"], "main");
        assert_eq!(body["commit_message"], "test commit");
        assert!(body["actions"].is_array());
        assert_eq!(body["actions"].as_array().unwrap().len(), 1);
        assert_eq!(body["actions"][0]["action"], "create");
        assert_eq!(body["actions"][0]["file_path"], "src/new.rs");
        assert_eq!(body["actions"][0]["content"], "fn x() {}");
        assert_eq!(body["actions"][0]["encoding"], "text");
        assert!(body["actions"][0].get("previous_path").is_none());
    }

    #[tokio::test]
    async fn commit_create_emits_multiple_actions_in_order() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/repository/commits"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": "deadbeef" })),
            )
            .mount(&server)
            .await;

        let mut create = action("create", "a.rs");
        create.content = Some("a".into());
        let mut update = action("update", "b.rs");
        update.content = Some("b".into());
        let delete = action("delete", "c.rs");

        commit_create(
            &mock_client(&server),
            base_params(vec![create, update, delete]),
        )
        .await
        .unwrap();

        let body = captured_post_body(&server.received_requests().await.unwrap());
        let actions = body["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0]["action"], "create");
        assert_eq!(actions[0]["file_path"], "a.rs");
        assert_eq!(actions[1]["action"], "update");
        assert_eq!(actions[1]["file_path"], "b.rs");
        assert_eq!(actions[2]["action"], "delete");
        assert_eq!(actions[2]["file_path"], "c.rs");
        // delete had no content set, so it must not appear in the action object
        assert!(actions[2].get("content").is_none());
    }

    #[tokio::test]
    async fn commit_create_move_action_includes_previous_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/repository/commits"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": "deadbeef" })),
            )
            .mount(&server)
            .await;

        let mut mv = action("move", "src/new_name.rs");
        mv.previous_path = Some("src/old_name.rs".into());
        mv.last_commit_id = Some("abc123".into());
        commit_create(&mock_client(&server), base_params(vec![mv]))
            .await
            .unwrap();

        let body = captured_post_body(&server.received_requests().await.unwrap());
        assert_eq!(body["actions"][0]["action"], "move");
        assert_eq!(body["actions"][0]["file_path"], "src/new_name.rs");
        assert_eq!(body["actions"][0]["previous_path"], "src/old_name.rs");
        assert_eq!(body["actions"][0]["last_commit_id"], "abc123");
    }

    #[tokio::test]
    async fn commit_create_propagates_400() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/repository/commits"))
            .respond_with(ResponseTemplate::new(400).set_body_string("Bad request"))
            .mount(&server)
            .await;

        let err = commit_create(
            &mock_client(&server),
            base_params(vec![action("create", "x.rs")]),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }
}
