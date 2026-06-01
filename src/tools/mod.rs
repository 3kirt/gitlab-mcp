use reqwest::StatusCode;
use rmcp::{
    ErrorData as McpError, Peer, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::*,
    prompt_handler, prompt_router,
    service::{NotificationContext, RequestContext},
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex, OnceLock};

use crate::client::{GitlabClient, GitlabError, ListResult, PaginationMeta};

pub mod branches;
pub mod commits;
pub mod discussions;
pub mod emoji_reactions;
pub mod epics;
pub mod groups;
pub mod issue_discussions;
pub mod issue_notes;
pub mod issues;
pub mod jobs;
pub mod merge_requests;
pub mod metadata;
pub mod pipeline_schedules;
pub mod pipelines;
pub mod projects;
pub mod repositories;
pub mod repository_files;
pub mod runners;
pub mod search;
mod slim;
pub mod snippets;

// --------------------------------------------------------------------------
// Progress notifications
// --------------------------------------------------------------------------

/// The client's `progressToken` plus a handle to notify it back.
///
/// Stashed in a task-local so [`paginate`] can emit per-page progress without
/// threading request context through every domain function.
#[derive(Clone)]
struct ProgressCtx {
    peer: Peer<RoleServer>,
    token: ProgressToken,
}

tokio::task_local! {
    /// Set by the `call_tool` override for the duration of one tool call.
    /// `None` when the client didn't request progress on this call.
    static PROGRESS_CTX: Option<ProgressCtx>;
}

/// Emit a `notifications/progress` update for the in-flight tool call, if the
/// client supplied a `progressToken`. A no-op otherwise (including in unit
/// tests, where the task-local is never set). `progress`/`total` share the
/// same unit — absolute item count — per the MCP spec.
async fn emit_page_progress(progress: u64, total: Option<u64>) {
    let ctx = PROGRESS_CTX.try_with(|c| c.clone()).ok().flatten();
    if let Some(ctx) = ctx {
        let _ = ctx
            .peer
            .notify_progress(ProgressNotificationParam {
                progress_token: ctx.token,
                progress: progress as f64,
                total: total.map(|t| t as f64),
                message: Some(format!("{progress} items")),
            })
            .await;
    }
}

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
    #[schemars(
        description = "Fetch every page and merge the results into one array, ignoring `page`/`per_page`. Use sparingly: large endpoints can require many sequential requests."
    )]
    pub fetch_all: Option<bool>,
}

pub fn json_result(v: Value) -> Result<CallToolResult, McpError> {
    let v = slim::slim_get(v);
    let text = serde_json::to_string_pretty(&v)
        .map_err(|e| McpError::internal_error(format!("marshalling response: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[derive(Serialize)]
struct ListEnvelope {
    items: Value,
    #[serde(flatten)]
    meta: PaginationMeta,
}

pub fn json_list_result(v: Value, meta: PaginationMeta) -> Result<CallToolResult, McpError> {
    let envelope = ListEnvelope {
        items: slim::slim_list(v),
        meta,
    };
    let text = serde_json::to_string_pretty(&envelope)
        .map_err(|e| McpError::internal_error(format!("marshalling response: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

pub fn tool_error(msg: &str) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(msg)]))
}

/// Page size used when walking every page for a `fetch_all` request.
/// GitLab caps `per_page` at 100.
const FETCH_ALL_PER_PAGE: u64 = 100;

/// Issue a list request, optionally walking every page.
///
/// With `fetch_all == false` this is a thin pass-through to
/// [`GitlabClient::list`]: one page, with the real `X-*` pagination headers.
///
/// With `fetch_all == true` the caller's `page`/`per_page` are dropped and the
/// helper walks pages at [`FETCH_ALL_PER_PAGE`] each, concatenating the JSON
/// arrays into one. Termination is belt-and-suspenders — it stops on a short
/// page or a missing `X-Next-Page`, since GitLab omits total/next headers on
/// some large endpoints. A [`crate::client::MAX_PAGES`] guard bounds runaway
/// loops. The returned `PaginationMeta` describes the merged result as a single
/// complete page (`total` = items collected).
pub async fn paginate(
    client: &GitlabClient,
    path: &str,
    params: &[(&str, String)],
    fetch_all: bool,
) -> ListResult {
    if !fetch_all {
        return client.list(path, params).await;
    }

    // Strip any caller-supplied paging; we drive it ourselves.
    let base: Vec<(&str, String)> = params
        .iter()
        .filter(|(k, _)| *k != "page" && *k != "per_page")
        .cloned()
        .collect();

    let mut all: Vec<Value> = Vec::new();
    let mut page: u64 = 1;

    loop {
        if page > crate::client::MAX_PAGES {
            return Err(GitlabError::Other(format!(
                "fetch_all exceeded the {}-page limit; narrow the query or page manually",
                crate::client::MAX_PAGES
            )));
        }

        let mut page_params = base.clone();
        page_params.push(("per_page", FETCH_ALL_PER_PAGE.to_string()));
        page_params.push(("page", page.to_string()));

        let (body, meta) = client.list(path, &page_params).await?;
        let batch = match body {
            Value::Array(items) => items,
            // Non-array list body: surface it verbatim rather than guessing.
            other => return Ok((other, meta)),
        };
        let n = batch.len() as u64;
        let next_page = meta.next_page;
        all.extend(batch);

        // Emit after extending so `progress` reflects items collected so far;
        // `total` is the X-Total header when GitLab supplied it, else unknown.
        emit_page_progress(all.len() as u64, meta.total).await;

        if n < FETCH_ALL_PER_PAGE || next_page.is_none() {
            break;
        }
        page = next_page.unwrap();
    }

    let count = all.len() as u64;
    let meta = PaginationMeta {
        page: Some(1),
        per_page: Some(count),
        total: Some(count),
        total_pages: Some(1),
        next_page: None,
    };
    Ok((Value::Array(all), meta))
}

// --------------------------------------------------------------------------
// Query construction
// --------------------------------------------------------------------------

/// URL-encode a namespace ID (project or group) for use in REST API paths.
/// Numeric IDs pass through unchanged; path-style IDs like
/// "mygroup/myrepo" have slashes replaced with %2F.
pub(crate) fn encode_namespace_id(id: &str) -> String {
    if id.chars().all(|c| c.is_ascii_digit()) {
        id.to_string()
    } else {
        id.replace('/', "%2F")
    }
}

pub(crate) fn encode_path_segment(s: &str) -> String {
    s.replace('/', "%2F")
}

/// For supplemental fetches embedded inside a primary response: pass through
/// success and 404 (as an empty array) but propagate every other error.
/// A 404 here means the sub-resource genuinely doesn't exist; 4xx/5xx
/// otherwise would silently mask real failures, so we surface them.
pub(crate) fn unwrap_404_as_empty_array(
    result: Result<Value, GitlabError>,
) -> Result<Value, GitlabError> {
    match result {
        Err(GitlabError::Api { status, .. }) if status == StatusCode::NOT_FOUND => {
            Ok(Value::Array(vec![]))
        }
        other => other,
    }
}

/// Like `unwrap_404_as_empty_array`, but also swallows 403 — for embedding
/// tier-gated endpoints (e.g. Premium/Ultimate-only sub-resources) where a
/// 403 means "your tier doesn't expose this" rather than a real failure.
/// The caller has already authenticated to the parent resource, so a 403 on
/// the supplemental fetch is licensing, not permission.
pub(crate) fn unwrap_404_or_403_as_empty_array(
    result: Result<Value, GitlabError>,
) -> Result<Value, GitlabError> {
    match result {
        Err(GitlabError::Api { status, .. })
            if status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN =>
        {
            Ok(Value::Array(vec![]))
        }
        other => other,
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

    pub fn multi(mut self, key: &'static str, values: Option<Vec<String>>) -> Self {
        if let Some(vs) = values {
            for v in vs {
                self.params.push((key, v));
            }
        }
        self
    }

    pub fn into_params(self) -> Vec<(&'static str, String)> {
        self.params
    }
}

pub struct BodyBuilder {
    map: serde_json::Map<String, Value>,
}

impl BodyBuilder {
    pub fn new() -> Self {
        Self {
            map: serde_json::Map::new(),
        }
    }

    pub fn req<T: serde::Serialize>(mut self, key: &'static str, v: T) -> Self {
        self.map
            .insert(key.to_string(), serde_json::to_value(v).unwrap());
        self
    }

    pub fn opt<T: serde::Serialize>(mut self, key: &'static str, v: Option<T>) -> Self {
        if let Some(v) = v {
            self.map
                .insert(key.to_string(), serde_json::to_value(v).unwrap());
        }
        self
    }

    pub fn build(self) -> Value {
        Value::Object(self.map)
    }
}

// --------------------------------------------------------------------------
// Delegation macros
// --------------------------------------------------------------------------

macro_rules! delegate_json {
    ($self:expr, $domain_fn:path, $p:expr, $verb:literal, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(v) => json_result(v),
            Err(e) => {
                let msg = format!("{} {}: {}", $verb, $noun, e.to_tool_message());
                $self.send_log(LoggingLevel::Error, &msg).await;
                tool_error(&msg)
            }
        }
    }};
}

macro_rules! delegate_list {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok((v, meta)) => json_list_result(v, meta),
            Err(e) => {
                let msg = format!("listing {}: {}", $noun, e.to_tool_message());
                $self.send_log(LoggingLevel::Error, &msg).await;
                tool_error(&msg)
            }
        }
    }};
}

macro_rules! delegate_get {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {
        delegate_json!($self, $domain_fn, $p, "getting", $noun)
    };
}

macro_rules! delegate_create {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {
        delegate_json!($self, $domain_fn, $p, "creating", $noun)
    };
}

macro_rules! delegate_update {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {
        delegate_json!($self, $domain_fn, $p, "updating", $noun)
    };
}

macro_rules! delegate_delete {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "{} deleted",
                $noun
            ))])),
            Err(e) => {
                let msg = format!("deleting {}: {}", $noun, e.to_tool_message());
                $self.send_log(LoggingLevel::Error, &msg).await;
                tool_error(&msg)
            }
        }
    }};
}

// --------------------------------------------------------------------------
// Server struct
// --------------------------------------------------------------------------

fn level_severity(level: LoggingLevel) -> u8 {
    match level {
        LoggingLevel::Debug => 0,
        LoggingLevel::Info => 1,
        LoggingLevel::Notice => 2,
        LoggingLevel::Warning => 3,
        LoggingLevel::Error => 4,
        LoggingLevel::Critical => 5,
        LoggingLevel::Alert => 6,
        LoggingLevel::Emergency => 7,
    }
}

#[derive(Clone)]
pub struct GitlabMcpServer {
    client: Arc<OnceLock<GitlabClient>>,
    // Used by the `call_tool` override below; reused per call rather than
    // rebuilt via `Self::tool_router()` each time.
    tool_router: ToolRouter<GitlabMcpServer>,
    #[allow(dead_code)]
    prompt_router: PromptRouter<GitlabMcpServer>,
    peer: Arc<OnceLock<Peer<RoleServer>>>,
    log_level: Arc<Mutex<LoggingLevel>>,
}

impl GitlabMcpServer {
    pub fn new_stdio(base_url: String, token: String) -> anyhow::Result<Self> {
        let cell = OnceLock::new();
        let _ = cell.set(GitlabClient::new(base_url, token)?);
        Ok(Self {
            client: Arc::new(cell),
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
            peer: Arc::new(OnceLock::new()),
            log_level: Arc::new(Mutex::new(LoggingLevel::Warning)),
        })
    }

    async fn send_log(&self, level: LoggingLevel, message: &str) {
        let current = *self.log_level.lock().unwrap();
        if level_severity(level) >= level_severity(current)
            && let Some(peer) = self.peer.get()
        {
            let _ = peer
                .notify_logging_message(LoggingMessageNotificationParam {
                    level,
                    logger: Some("gitlab-mcp".to_string()),
                    data: serde_json::json!({ "message": message }),
                })
                .await;
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
        description = "Get metadata about the GitLab instance, including version, revision, enterprise status, and Kubernetes agent server (KAS) information."
    )]
    async fn gitlab_metadata_get(
        &self,
        Parameters(p): Parameters<metadata::MetadataParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, metadata::metadata_get, p, "metadata")
    }

    #[tool(
        description = "Search across the entire GitLab instance. Required: scope (projects, issues, merge_requests, milestones, snippet_titles, users, wiki_blobs, commits, blobs, notes), search. Optional: search_type, order_by, sort, confidential, state, fields, page, per_page."
    )]
    async fn gitlab_search_global(
        &self,
        Parameters(p): Parameters<search::GlobalSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, search::global_search, p, "search results")
    }

    #[tool(
        description = "Search within a group. Required: group_id, scope, search. Optional: search_type, order_by, sort, confidential, state, fields, page, per_page."
    )]
    async fn gitlab_search_group(
        &self,
        Parameters(p): Parameters<search::GroupSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, search::group_search, p, "search results")
    }

    #[tool(
        description = "Search within a project. Required: project_id, scope, search. Optional: search_type, order_by, sort, confidential, state, fields, page, per_page."
    )]
    async fn gitlab_search_project(
        &self,
        Parameters(p): Parameters<search::ProjectSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, search::project_search, p, "search results")
    }

    #[tool(
        description = "List issues for a GitLab project. Filters: state (opened/closed/all), labels, search, scope, assignee_id, author_id, created_after/created_before, updated_after/updated_before (ISO 8601), order_by, sort. Paginate with page and per_page."
    )]
    async fn gitlab_issues_list(
        &self,
        Parameters(p): Parameters<issues::IssuesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issues::issues_list, p, "issues")
    }

    #[tool(
        description = "Get a single GitLab issue by project ID and issue IID (the issue number shown in the GitLab UI). The response includes a linked_issues array (linked issues with link type and issue_link_id) and a closed_by array (merge requests that will close this issue when merged)."
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
        description = "List all links for a GitLab issue. Returns linked issues with their link type (relates_to, blocks, is_blocked_by) and issue_link_id."
    )]
    async fn gitlab_issues_links_list(
        &self,
        Parameters(p): Parameters<issues::IssueLinksListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issues::issue_links_list, p, "issue links")
    }

    #[tool(
        description = "Get a single issue link by its relationship ID (issue_link_id). Returns source_issue, target_issue, and link_type."
    )]
    async fn gitlab_issues_links_get(
        &self,
        Parameters(p): Parameters<issues::IssueLinkGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, issues::issue_link_get, p, "issue link")
    }

    #[tool(
        description = "Create a link between two GitLab issues. Required: project_id, issue_iid (source), target_project_id, target_issue_iid. Optional: link_type (\"relates_to\" (default), \"blocks\", or \"is_blocked_by\")."
    )]
    async fn gitlab_issues_links_create(
        &self,
        Parameters(p): Parameters<issues::IssueLinkCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, issues::issue_link_create, p, "issue link")
    }

    #[tool(
        description = "Delete a link between two GitLab issues by its relationship ID (issue_link_id from the list response). Returns the deleted link object."
    )]
    async fn gitlab_issues_links_delete(
        &self,
        Parameters(p): Parameters<issues::IssueLinkDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, issues::issue_link_delete, p, "deleting", "issue link")
    }

    #[tool(
        description = "List notes (comments) on a GitLab issue. Optional: order_by (\"created_at\" or \"updated_at\"), sort (\"asc\" or \"desc\"). Paginate with page and per_page."
    )]
    async fn gitlab_issues_notes_list(
        &self,
        Parameters(p): Parameters<issue_notes::IssueNotesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, issue_notes::issue_notes_list, p, "issue notes")
    }

    #[tool(description = "Get a single note on a GitLab issue by note ID.")]
    async fn gitlab_issues_notes_get(
        &self,
        Parameters(p): Parameters<issue_notes::IssueNoteGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, issue_notes::issue_note_get, p, "issue note")
    }

    #[tool(
        description = "Create a new note (comment) on a GitLab issue. Required: project_id, issue_iid, body. Optional: created_at (ISO 8601; requires administrator or Owner role)."
    )]
    async fn gitlab_issues_notes_create(
        &self,
        Parameters(p): Parameters<issue_notes::IssueNoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, issue_notes::issue_note_create, p, "issue note")
    }

    #[tool(
        description = "Update the body of a note on a GitLab issue. Required: project_id, issue_iid, note_id, body."
    )]
    async fn gitlab_issues_notes_update(
        &self,
        Parameters(p): Parameters<issue_notes::IssueNoteUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, issue_notes::issue_note_update, p, "issue note")
    }

    #[tool(
        description = "Delete a note from a GitLab issue. Required: project_id, issue_iid, note_id. This action is permanent."
    )]
    async fn gitlab_issues_notes_delete(
        &self,
        Parameters(p): Parameters<issue_notes::IssueNoteDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, issue_notes::issue_note_delete, p, "issue note")
    }

    #[tool(
        description = "List all discussion threads on a GitLab issue. Each thread contains an individual_note flag and a notes[] array. Paginate with page and per_page."
    )]
    async fn gitlab_issues_discussions_list(
        &self,
        Parameters(p): Parameters<issue_discussions::IssueDiscussionsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            issue_discussions::issue_discussions_list,
            p,
            "issue discussions"
        )
    }

    #[tool(
        description = "Get a single discussion thread on a GitLab issue by discussion ID (hex string)."
    )]
    async fn gitlab_issues_discussions_get(
        &self,
        Parameters(p): Parameters<issue_discussions::IssueDiscussionGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            issue_discussions::issue_discussion_get,
            p,
            "issue discussion"
        )
    }

    #[tool(
        description = "Start a new discussion thread on a GitLab issue. Required: project_id, issue_iid, body. Optional: created_at (ISO 8601; requires administrator or Owner role)."
    )]
    async fn gitlab_issues_discussions_create(
        &self,
        Parameters(p): Parameters<issue_discussions::IssueDiscussionCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            issue_discussions::issue_discussion_create,
            p,
            "issue discussion"
        )
    }

    #[tool(
        description = "Add a reply note to an existing discussion thread on a GitLab issue. Required: project_id, issue_iid, discussion_id, body. Optional: created_at (ISO 8601; requires administrator or Owner role)."
    )]
    async fn gitlab_issues_discussions_note_create(
        &self,
        Parameters(p): Parameters<issue_discussions::IssueDiscussionNoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            issue_discussions::issue_discussion_note_create,
            p,
            "issue discussion note"
        )
    }

    #[tool(
        description = "Update the body of a note in a GitLab issue discussion thread. Required: project_id, issue_iid, discussion_id, note_id, body."
    )]
    async fn gitlab_issues_discussions_note_update(
        &self,
        Parameters(p): Parameters<issue_discussions::IssueDiscussionNoteUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(
            self,
            issue_discussions::issue_discussion_note_update,
            p,
            "issue discussion note"
        )
    }

    #[tool(
        description = "Delete a note from a GitLab issue discussion thread. Required: project_id, issue_iid, discussion_id, note_id. This action is permanent."
    )]
    async fn gitlab_issues_discussions_note_delete(
        &self,
        Parameters(p): Parameters<issue_discussions::IssueDiscussionNoteDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            issue_discussions::issue_discussion_note_delete,
            p,
            "issue discussion note"
        )
    }

    #[tool(
        description = "List merge requests for a GitLab project. Filters: state (opened/closed/merged/all), source_branch, target_branch, author_id, assignee_id, reviewer_id, labels, search, draft, scope, created_after/created_before, updated_after/updated_before (ISO 8601), order_by, sort. Paginate with page and per_page."
    )]
    async fn gitlab_mrs_list(
        &self,
        Parameters(p): Parameters<merge_requests::MrsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, merge_requests::mrs_list, p, "merge requests")
    }

    #[tool(
        description = "Get a single GitLab merge request by project ID and merge request IID (the number shown in the GitLab UI). The response includes a closes_issues array (issues that will close when this MR is merged) and a related_issues array (all issues mentioned in or related to the MR; Premium/Ultimate — empty on lower tiers)."
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
        description = "Approve a GitLab merge request. Required: project_id and merge_request_iid. Optional: sha (HEAD commit SHA to guard against concurrent updates), approval_password (only needed if re-authentication is enabled). Returns the updated approval state including approvals_left and approved_by."
    )]
    async fn gitlab_mrs_approve(
        &self,
        Parameters(p): Parameters<merge_requests::MrApproveParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            merge_requests::mr_approve,
            p,
            "merge request approval"
        )
    }

    #[tool(
        description = "Unapprove a GitLab merge request that the current user has previously approved. Required: project_id and merge_request_iid."
    )]
    async fn gitlab_mrs_unapprove(
        &self,
        Parameters(p): Parameters<merge_requests::MrUnapproveParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.get_client()?;
        match merge_requests::mr_unapprove(client, p).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(
                "merge request unapproved",
            )])),
            Err(e) => tool_error(&format!(
                "unapproving merge request: {}",
                e.to_tool_message()
            )),
        }
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
        description = "List commits for a GitLab project. Optional filters: ref_name (branch/tag/range), since/until (ISO 8601), path (file filter), author, all, first_parent, order (default/topo), with_stats, trailers, follow. Paginate with page and per_page."
    )]
    async fn gitlab_commits_list(
        &self,
        Parameters(p): Parameters<commits::CommitsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commits::commits_list, p, "commits")
    }

    #[tool(
        description = "Get a single GitLab commit by SHA, branch name, or tag name. Optional: stats (include commit statistics, default true)."
    )]
    async fn gitlab_commits_get(
        &self,
        Parameters(p): Parameters<commits::CommitGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, commits::commit_get, p, "commit")
    }

    #[tool(
        description = "Create a commit in a GitLab project with one or more file actions (create, update, delete, move, chmod). Required: project_id, branch, commit_message, actions[]. Optional: start_branch, start_sha, start_project, author_name, author_email, force, allow_empty, stats."
    )]
    async fn gitlab_commits_create(
        &self,
        Parameters(p): Parameters<commits::CommitCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commits::commit_create, p, "commit")
    }

    #[tool(
        description = "List all branches and tags that contain a specific commit. Optional: type (\"branch\", \"tag\", or \"all\"), page, per_page."
    )]
    async fn gitlab_commits_refs(
        &self,
        Parameters(p): Parameters<commits::CommitRefsParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commits::commit_refs, p, "commit refs")
    }

    #[tool(
        description = "Get the sequence number of a commit (number of ancestors by following parent links). Optional: first_parent."
    )]
    async fn gitlab_commits_sequence(
        &self,
        Parameters(p): Parameters<commits::CommitSequenceParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, commits::commit_sequence, p, "commit sequence")
    }

    #[tool(
        description = "Cherry-pick a commit into a target branch. Required: project_id, sha, branch. Optional: dry_run (simulate without committing), message (custom commit message)."
    )]
    async fn gitlab_commits_cherry_pick(
        &self,
        Parameters(p): Parameters<commits::CommitCherryPickParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commits::commit_cherry_pick, p, "cherry-pick")
    }

    #[tool(
        description = "Revert a commit by creating a new revert commit on the target branch. Required: project_id, sha, branch. Optional: dry_run (simulate without committing)."
    )]
    async fn gitlab_commits_revert(
        &self,
        Parameters(p): Parameters<commits::CommitRevertParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commits::commit_revert, p, "revert")
    }

    #[tool(
        description = "Get the diff introduced by a specific commit. Optional: unidiff (use unified diff format, default false), page, per_page."
    )]
    async fn gitlab_commits_diff(
        &self,
        Parameters(p): Parameters<commits::CommitDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commits::commit_diff, p, "commit diff")
    }

    #[tool(description = "List all comments on a commit. Paginate with page and per_page.")]
    async fn gitlab_commits_comments_list(
        &self,
        Parameters(p): Parameters<commits::CommitCommentsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commits::commit_comments_list, p, "commit comments")
    }

    #[tool(
        description = "Post a comment on a commit. Required: project_id, sha, note. Optional: path (file path for inline comment), line (line number), line_type (\"new\" or \"old\")."
    )]
    async fn gitlab_commits_comment_create(
        &self,
        Parameters(p): Parameters<commits::CommitCommentCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commits::commit_comment_create, p, "commit comment")
    }

    #[tool(
        description = "List all discussion threads on a commit. Paginate with page and per_page."
    )]
    async fn gitlab_commits_discussions_list(
        &self,
        Parameters(p): Parameters<commits::CommitDiscussionsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            commits::commit_discussions_list,
            p,
            "commit discussions"
        )
    }

    #[tool(
        description = "List CI/CD pipeline statuses for a commit. Optional: ref (branch/tag), name (job name filter), stage, all (include non-latest), pipeline_id, order_by (id/pipeline_id), sort (asc/desc), page, per_page."
    )]
    async fn gitlab_commits_statuses_list(
        &self,
        Parameters(p): Parameters<commits::CommitStatusesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, commits::commit_statuses_list, p, "commit statuses")
    }

    #[tool(
        description = "Set a pipeline status on a commit (for external CI systems). Required: project_id, sha, state (pending/running/success/failed/canceled/skipped). Optional: name/context, ref, description, target_url, coverage, pipeline_id."
    )]
    async fn gitlab_commits_status_set(
        &self,
        Parameters(p): Parameters<commits::CommitStatusSetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, commits::commit_status_set, p, "commit status")
    }

    #[tool(
        description = "List merge requests that introduced a specific commit. Optional: state (opened/closed/locked/merged), page, per_page."
    )]
    async fn gitlab_commits_merge_requests(
        &self,
        Parameters(p): Parameters<commits::CommitMergeRequestsParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            commits::commit_merge_requests,
            p,
            "commit merge requests"
        )
    }

    #[tool(
        description = "Get the GPG, SSH, or X.509 signature for a signed commit. Returns 404 for unsigned commits."
    )]
    async fn gitlab_commits_signature(
        &self,
        Parameters(p): Parameters<commits::CommitSignatureParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, commits::commit_signature, p, "commit signature")
    }

    #[tool(
        description = "List pipelines for a GitLab project. Optional filters: scope, status, source, ref, sha, yaml_errors, username, updated_after/before, created_after/before, order_by, sort, name. Paginate with page and per_page."
    )]
    async fn gitlab_pipelines_list(
        &self,
        Parameters(p): Parameters<pipelines::PipelineListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, pipelines::pipeline_list, p, "pipelines")
    }

    #[tool(description = "Get a single GitLab pipeline by project ID and pipeline ID.")]
    async fn gitlab_pipelines_get(
        &self,
        Parameters(p): Parameters<pipelines::PipelineGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, pipelines::pipeline_get, p, "pipeline")
    }

    #[tool(
        description = "Get the latest pipeline for a GitLab project. Optional: ref (branch or tag name; defaults to project default branch)."
    )]
    async fn gitlab_pipelines_get_latest(
        &self,
        Parameters(p): Parameters<pipelines::PipelineGetLatestParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, pipelines::pipeline_get_latest, p, "latest pipeline")
    }

    #[tool(
        description = "List variables defined on a specific GitLab pipeline run. Returns key/value pairs used when the pipeline was triggered. Paginate with page and per_page."
    )]
    async fn gitlab_pipelines_get_variables(
        &self,
        Parameters(p): Parameters<pipelines::PipelineGetVariablesParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            pipelines::pipeline_get_variables,
            p,
            "pipeline variables"
        )
    }

    #[tool(
        description = "Get the full test report for a GitLab pipeline, including suite and case details with pass/fail/error counts."
    )]
    async fn gitlab_pipelines_get_test_report(
        &self,
        Parameters(p): Parameters<pipelines::PipelineGetTestReportParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            pipelines::pipeline_get_test_report,
            p,
            "pipeline test report"
        )
    }

    #[tool(
        description = "Get the test report summary for a GitLab pipeline — total counts only without per-case details."
    )]
    async fn gitlab_pipelines_get_test_report_summary(
        &self,
        Parameters(p): Parameters<pipelines::PipelineGetTestReportSummaryParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            pipelines::pipeline_get_test_report_summary,
            p,
            "pipeline test report summary"
        )
    }

    #[tool(
        description = "Create (trigger) a new GitLab pipeline. Required: project_id, ref (branch/tag/SHA). Optional: variables (array of {key, value, variable_type} objects), inputs."
    )]
    async fn gitlab_pipelines_create(
        &self,
        Parameters(p): Parameters<pipelines::PipelineCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, pipelines::pipeline_create, p, "pipeline")
    }

    #[tool(
        description = "Retry all failed and canceled jobs in a GitLab pipeline, creating a new pipeline run."
    )]
    async fn gitlab_pipelines_retry(
        &self,
        Parameters(p): Parameters<pipelines::PipelineRetryParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, pipelines::pipeline_retry, p, "retrying", "pipeline")
    }

    #[tool(description = "Cancel all running jobs in a GitLab pipeline.")]
    async fn gitlab_pipelines_cancel(
        &self,
        Parameters(p): Parameters<pipelines::PipelineCancelParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, pipelines::pipeline_cancel, p, "canceling", "pipeline")
    }

    #[tool(
        description = "Delete a GitLab pipeline and all its jobs. Requires at least Maintainer role. This action is permanent."
    )]
    async fn gitlab_pipelines_delete(
        &self,
        Parameters(p): Parameters<pipelines::PipelineDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, pipelines::pipeline_delete, p, "pipeline")
    }

    #[tool(
        description = "Update the name of a GitLab pipeline. Required: project_id, pipeline_id, name (new pipeline name)."
    )]
    async fn gitlab_pipelines_update_metadata(
        &self,
        Parameters(p): Parameters<pipelines::PipelineUpdateMetadataParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(
            self,
            pipelines::pipeline_update_metadata,
            p,
            "pipeline metadata"
        )
    }

    #[tool(
        description = "List pipeline schedules for a GitLab project. Optional: scope (\"active\" or \"inactive\"), page, per_page."
    )]
    async fn gitlab_pipeline_schedules_list(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineSchedulesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            pipeline_schedules::pipeline_schedules_list,
            p,
            "pipeline schedules"
        )
    }

    #[tool(description = "Get a single GitLab pipeline schedule by project ID and schedule ID.")]
    async fn gitlab_pipeline_schedules_get(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            pipeline_schedules::pipeline_schedule_get,
            p,
            "pipeline schedule"
        )
    }

    #[tool(
        description = "List pipelines triggered by a pipeline schedule. Optional filters: status, scope, sort, created_after, created_before, updated_after, updated_before, page, per_page."
    )]
    async fn gitlab_pipeline_schedules_pipelines_list(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineSchedulePipelinesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            pipeline_schedules::pipeline_schedule_pipelines_list,
            p,
            "schedule pipelines"
        )
    }

    #[tool(
        description = "Create a new pipeline schedule. Required: project_id, cron, description, ref. Optional: active, cron_timezone."
    )]
    async fn gitlab_pipeline_schedules_create(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            pipeline_schedules::pipeline_schedule_create,
            p,
            "pipeline schedule"
        )
    }

    #[tool(
        description = "Update an existing GitLab pipeline schedule. All fields optional: cron, description, ref, active, cron_timezone."
    )]
    async fn gitlab_pipeline_schedules_update(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(
            self,
            pipeline_schedules::pipeline_schedule_update,
            p,
            "pipeline schedule"
        )
    }

    #[tool(description = "Delete a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_delete(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            pipeline_schedules::pipeline_schedule_delete,
            p,
            "pipeline schedule"
        )
    }

    #[tool(description = "Take ownership of a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_take_ownership(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleTakeOwnershipParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(
            self,
            pipeline_schedules::pipeline_schedule_take_ownership,
            p,
            "taking ownership of",
            "pipeline schedule"
        )
    }

    #[tool(description = "Run a GitLab pipeline schedule immediately (trigger now).")]
    async fn gitlab_pipeline_schedules_play(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineSchedulePlayParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(
            self,
            pipeline_schedules::pipeline_schedule_play,
            p,
            "playing",
            "pipeline schedule"
        )
    }

    #[tool(
        description = "Create a variable for a GitLab pipeline schedule. Required: project_id, pipeline_schedule_id, key, value. Optional: variable_type (\"env_var\" or \"file\")."
    )]
    async fn gitlab_pipeline_schedules_variables_create(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleVariableCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            pipeline_schedules::pipeline_schedule_variable_create,
            p,
            "pipeline schedule variable"
        )
    }

    #[tool(description = "Get a variable from a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_variables_get(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleVariableGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            pipeline_schedules::pipeline_schedule_variable_get,
            p,
            "pipeline schedule variable"
        )
    }

    #[tool(description = "Update a variable in a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_variables_update(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleVariableUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(
            self,
            pipeline_schedules::pipeline_schedule_variable_update,
            p,
            "pipeline schedule variable"
        )
    }

    #[tool(description = "Delete a variable from a GitLab pipeline schedule.")]
    async fn gitlab_pipeline_schedules_variables_delete(
        &self,
        Parameters(p): Parameters<pipeline_schedules::PipelineScheduleVariableDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            pipeline_schedules::pipeline_schedule_variable_delete,
            p,
            "pipeline schedule variable"
        )
    }

    #[tool(
        description = "List jobs for a GitLab project. Optional: scope (array of states to filter by), order_by, sort, page, per_page."
    )]
    async fn gitlab_jobs_list(
        &self,
        Parameters(p): Parameters<jobs::JobListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, jobs::job_list, p, "jobs")
    }

    #[tool(
        description = "List jobs for a specific GitLab pipeline. Optional: scope (array of states), include_retried (include non-latest attempts), page, per_page."
    )]
    async fn gitlab_jobs_list_for_pipeline(
        &self,
        Parameters(p): Parameters<jobs::JobListForPipelineParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, jobs::job_list_for_pipeline, p, "pipeline jobs")
    }

    #[tool(
        description = "List bridge (downstream trigger) jobs for a GitLab pipeline. Optional: scope (array of states), page, per_page."
    )]
    async fn gitlab_jobs_list_bridges(
        &self,
        Parameters(p): Parameters<jobs::JobListBridgesParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, jobs::job_list_bridges, p, "pipeline bridges")
    }

    #[tool(
        description = "Get a single GitLab job by project ID and job ID. Returns full job metadata including stage, status, runner, timings, and artifacts."
    )]
    async fn gitlab_jobs_get(
        &self,
        Parameters(p): Parameters<jobs::JobGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, jobs::job_get, p, "job")
    }

    #[tool(description = "Get the raw log output (trace) of a GitLab job as plain text.")]
    async fn gitlab_jobs_get_trace(
        &self,
        Parameters(p): Parameters<jobs::JobGetTraceParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.get_client()?;
        match jobs::job_get_trace(client, p).await {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => tool_error(&format!("getting job trace: {}", e.to_tool_message())),
        }
    }

    #[tool(
        description = "Cancel a running GitLab job. Optional: force (force-cancel a job already in \"canceling\" state)."
    )]
    async fn gitlab_jobs_cancel(
        &self,
        Parameters(p): Parameters<jobs::JobCancelParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, jobs::job_cancel, p, "canceling", "job")
    }

    #[tool(description = "Retry a failed or canceled GitLab job, creating a new job run.")]
    async fn gitlab_jobs_retry(
        &self,
        Parameters(p): Parameters<jobs::JobRetryParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, jobs::job_retry, p, "retrying", "job")
    }

    #[tool(
        description = "Erase a GitLab job — removes the job log and artifacts. The job must be finished."
    )]
    async fn gitlab_jobs_erase(
        &self,
        Parameters(p): Parameters<jobs::JobEraseParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, jobs::job_erase, p, "erasing", "job")
    }

    #[tool(
        description = "Trigger a manual GitLab job. Optional: job_variables_attributes (array of {key, value, variable_type} objects to override job variables)."
    )]
    async fn gitlab_jobs_play(
        &self,
        Parameters(p): Parameters<jobs::JobPlayParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, jobs::job_play, p, "triggering", "job")
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

    #[tool(
        description = "List all discussion threads on a GitLab merge request. Each thread contains an individual_note flag and a notes[] array. Paginate with page and per_page."
    )]
    async fn gitlab_mrs_discussions_list(
        &self,
        Parameters(p): Parameters<discussions::MrDiscussionsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, discussions::mr_discussions_list, p, "MR discussions")
    }

    #[tool(
        description = "Get a single discussion thread on a GitLab merge request by discussion ID (hex string)."
    )]
    async fn gitlab_mrs_discussions_get(
        &self,
        Parameters(p): Parameters<discussions::MrDiscussionGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, discussions::mr_discussion_get, p, "MR discussion")
    }

    #[tool(
        description = "Start a new discussion thread on a GitLab merge request. Required: project_id, merge_request_iid, body. Optional: commit_id (pin to commit SHA). Advanced diff-note position: position_base_sha, position_head_sha, position_start_sha, position_type (\"text\"/\"image\"/\"file\"), position_new_path, position_old_path, position_new_line, position_old_line."
    )]
    async fn gitlab_mrs_discussions_create(
        &self,
        Parameters(p): Parameters<discussions::MrDiscussionCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, discussions::mr_discussion_create, p, "MR discussion")
    }

    #[tool(
        description = "Resolve or unresolve a discussion thread on a GitLab merge request. Required: project_id, merge_request_iid, discussion_id, resolved (true to resolve, false to unresolve). Requires Developer role or being the change author."
    )]
    async fn gitlab_mrs_discussions_resolve(
        &self,
        Parameters(p): Parameters<discussions::MrDiscussionResolveParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, discussions::mr_discussion_resolve, p, "MR discussion")
    }

    #[tool(
        description = "Add a reply note to an existing discussion thread on a GitLab merge request. Required: project_id, merge_request_iid, discussion_id, body. Optional: created_at (ISO 8601; requires administrator or Owner role)."
    )]
    async fn gitlab_mrs_discussions_note_create(
        &self,
        Parameters(p): Parameters<discussions::MrDiscussionNoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            discussions::mr_discussion_note_create,
            p,
            "MR discussion note"
        )
    }

    #[tool(
        description = "Update a note in a GitLab merge request discussion thread. Required: project_id, merge_request_iid, discussion_id, note_id. Provide exactly one of: body (new text) or resolved (true/false)."
    )]
    async fn gitlab_mrs_discussions_note_update(
        &self,
        Parameters(p): Parameters<discussions::MrDiscussionNoteUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(
            self,
            discussions::mr_discussion_note_update,
            p,
            "MR discussion note"
        )
    }

    #[tool(
        description = "Delete a note from a GitLab merge request discussion thread. Required: project_id, merge_request_iid, discussion_id, note_id. This action is permanent."
    )]
    async fn gitlab_mrs_discussions_note_delete(
        &self,
        Parameters(p): Parameters<discussions::MrDiscussionNoteDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            discussions::mr_discussion_note_delete,
            p,
            "MR discussion note"
        )
    }

    #[tool(
        description = "List GitLab groups accessible to the current user. Optional filters: search (by name or path), all_available (true to include all accessible groups, not just member groups), owned (limit to owned groups), min_access_level (10=Guest, 20=Reporter, 30=Developer, 40=Maintainer, 50=Owner), top_level_only (exclude subgroups). Sort with order_by (name/path/id/similarity) and sort (asc/desc). Paginate with page and per_page."
    )]
    async fn gitlab_groups_list(
        &self,
        Parameters(p): Parameters<groups::GroupsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, groups::groups_list, p, "groups")
    }

    #[tool(
        description = "Get details of a GitLab group by ID or full namespace path (e.g. \"mygroup\" or \"mygroup/subgroup\"). Returns id, name, path, full_path, description, visibility, web_url, parent_id, and created_at. Set with_projects=true to include the group's projects (max 100) in the response."
    )]
    async fn gitlab_groups_get(
        &self,
        Parameters(p): Parameters<groups::GroupGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, groups::group_get, p, "group")
    }

    #[tool(
        description = "Get a GitLab project by ID or namespace path. project_id accepts a numeric ID (e.g. \"42\") or a full namespace path (e.g. \"mygroup/myrepo\"). Optional: statistics=true to include commit/storage counts (requires Reporter role or higher). Returns core project details: id, name, path, path_with_namespace, description, visibility, default_branch, web_url, http_url_to_repo, namespace, created_at, and feature settings."
    )]
    async fn gitlab_projects_get(
        &self,
        Parameters(p): Parameters<projects::ProjectGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, projects::project_get, p, "project")
    }

    #[tool(
        description = "List epics in a GitLab group. Required: group_id (numeric ID or full namespace path like \"mygroup\"). Optional filters: state (opened/closed/all), search, author_username, label_name (array of label names), iids (array of epic IIDs from the URL). Sort: order_by (created_at/updated_at/title) and sort (asc/desc). Pagination: page and per_page (default 20, max 100). Returns each epic with id, iid, title, state, author, labels, dates, and web_url."
    )]
    async fn gitlab_epics_list(
        &self,
        Parameters(p): Parameters<epics::EpicsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, epics::epics_list, p, "epics")
    }

    #[tool(
        description = "Get a single GitLab epic by group and epic IID (the number from the URL `/groups/<g>/-/epics/<iid>`). group_id accepts a numeric ID or full namespace path. Returns full epic details: id, iid, title, description, state, author, labels, start_date, due_date, parent_id, parent_iid, web_url, and issues (child issues associated with the epic)."
    )]
    async fn gitlab_epics_get(
        &self,
        Parameters(p): Parameters<epics::EpicGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, epics::epic_get, p, "epic")
    }

    #[tool(
        description = "Create a new epic in a GitLab group. Required: group_id (numeric ID or full namespace path), title. Optional: description (Markdown), labels (comma-separated label names), parent_epic_iid (an existing epic IID in the same group to set as the hierarchy parent; 0 is not valid on create), start_date and due_date (ISO 8601)."
    )]
    async fn gitlab_epics_create(
        &self,
        Parameters(p): Parameters<epics::EpicCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, epics::epic_create, p, "epic")
    }

    #[tool(
        description = "Update an existing GitLab epic by group and epic IID. All fields are optional. Use state_event=\"close\" or \"reopen\" to change state. Use labels to replace all labels, add_labels/remove_labels to adjust them incrementally. For parent_epic_iid: pass an existing epic IID to set a new parent, or 0 to remove the existing parent."
    )]
    async fn gitlab_epics_update(
        &self,
        Parameters(p): Parameters<epics::EpicUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, epics::epic_update, p, "epic")
    }

    #[tool(
        description = "Delete a GitLab epic by group and epic IID. Requires sufficient group permissions. This action is permanent and cannot be undone."
    )]
    async fn gitlab_epics_delete(
        &self,
        Parameters(p): Parameters<epics::EpicDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, epics::epic_delete, p, "epic")
    }

    #[tool(
        description = "Assign an issue to a GitLab epic. Required: group_id (numeric ID or full namespace path), epic_iid (epic's IID from the URL), issue_id (the global numeric issue ID — not the project-scoped IID; use gitlab_issues_get to find it). Returns the epic-issue association object, which includes an `id` field (the epic_issue_id) needed to remove or reorder the issue."
    )]
    async fn gitlab_epics_issue_assign(
        &self,
        Parameters(p): Parameters<epics::EpicIssueAssignParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, epics::epic_issue_assign, p, "epic-issue association")
    }

    #[tool(
        description = "Remove an issue from a GitLab epic. Required: group_id (numeric ID or full namespace path), epic_iid (epic's IID from the URL), epic_issue_id (the association ID — the `id` field returned by gitlab_epics_get in the issues array, or by gitlab_epics_issue_assign). Returns the deleted association object."
    )]
    async fn gitlab_epics_issue_remove(
        &self,
        Parameters(p): Parameters<epics::EpicIssueRemoveParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(
            self,
            epics::epic_issue_remove,
            p,
            "removing",
            "epic-issue association"
        )
    }

    #[tool(
        description = "List snippets for the current authenticated user. Optional: created_after, created_before (ISO 8601). Paginate with page and per_page."
    )]
    async fn gitlab_snippets_list(
        &self,
        Parameters(p): Parameters<snippets::SnippetsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, snippets::snippets_list, p, "snippets")
    }

    #[tool(
        description = "List all public snippets. Optional: created_after, created_before (ISO 8601). Paginate with page and per_page."
    )]
    async fn gitlab_snippets_public_list(
        &self,
        Parameters(p): Parameters<snippets::SnippetsPublicListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, snippets::snippets_public_list, p, "public snippets")
    }

    #[tool(
        description = "List all snippets the current user has access to (administrators and auditors see all snippets). Optional: created_after, created_before, repository_storage (admin only). Paginate with page and per_page."
    )]
    async fn gitlab_snippets_all_list(
        &self,
        Parameters(p): Parameters<snippets::SnippetsAllListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, snippets::snippets_all_list, p, "all snippets")
    }

    #[tool(description = "Get a single GitLab snippet by ID.")]
    async fn gitlab_snippets_get(
        &self,
        Parameters(p): Parameters<snippets::SnippetGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, snippets::snippet_get, p, "snippet")
    }

    #[tool(description = "Get the raw content of a GitLab snippet. Returns {\"content\": \"...\"}")]
    async fn gitlab_snippets_raw(
        &self,
        Parameters(p): Parameters<snippets::SnippetRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, snippets::snippet_raw, p, "snippet raw content")
    }

    #[tool(
        description = "Get the raw content of a specific file in a GitLab snippet repository. Required: id, ref_name (branch/tag/commit), file_path (URL-encoded). Returns {\"content\": \"...\"}."
    )]
    async fn gitlab_snippets_file_raw(
        &self,
        Parameters(p): Parameters<snippets::SnippetFileRawParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, snippets::snippet_file_raw, p, "snippet file content")
    }

    #[tool(
        description = "Create a new GitLab snippet. Required: title, files (array of {content, file_path}). Optional: description, visibility (\"public\", \"internal\", or \"private\")."
    )]
    async fn gitlab_snippets_create(
        &self,
        Parameters(p): Parameters<snippets::SnippetCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, snippets::snippet_create, p, "snippet")
    }

    #[tool(
        description = "Update an existing GitLab snippet. Required: id. Optional: title, description, visibility, files (array of {action, file_path, previous_path, content}; action must be \"create\", \"update\", \"delete\", or \"move\")."
    )]
    async fn gitlab_snippets_update(
        &self,
        Parameters(p): Parameters<snippets::SnippetUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, snippets::snippet_update, p, "snippet")
    }

    #[tool(
        description = "Delete a GitLab snippet by ID. This action is permanent and cannot be undone."
    )]
    async fn gitlab_snippets_delete(
        &self,
        Parameters(p): Parameters<snippets::SnippetDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, snippets::snippet_delete, p, "snippet")
    }

    #[tool(
        description = "Get user agent details for a GitLab snippet (administrators only). Returns ip_address, user_agent, and akismet_submitted."
    )]
    async fn gitlab_snippets_user_agent_detail(
        &self,
        Parameters(p): Parameters<snippets::SnippetUserAgentDetailParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            snippets::snippet_user_agent_detail,
            p,
            "snippet user agent detail"
        )
    }

    #[tool(
        description = "List all emoji reactions on a GitLab issue. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_issues_list(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            emoji_reactions::issue_emoji_list,
            p,
            "issue emoji reactions"
        )
    }

    #[tool(description = "Get a single emoji reaction on a GitLab issue by award ID.")]
    async fn gitlab_emoji_reactions_issues_get(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            emoji_reactions::issue_emoji_get,
            p,
            "issue emoji reaction"
        )
    }

    #[tool(
        description = "Add an emoji reaction to a GitLab issue. Required: project_id, issue_iid, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_issues_create(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            emoji_reactions::issue_emoji_create,
            p,
            "issue emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a GitLab issue. Only the reaction author or administrators may delete. Required: project_id, issue_iid, award_id."
    )]
    async fn gitlab_emoji_reactions_issues_delete(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            emoji_reactions::issue_emoji_delete,
            p,
            "issue emoji reaction"
        )
    }

    #[tool(
        description = "List all emoji reactions on a GitLab merge request. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_mrs_list(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            emoji_reactions::mr_emoji_list,
            p,
            "MR emoji reactions"
        )
    }

    #[tool(description = "Get a single emoji reaction on a GitLab merge request by award ID.")]
    async fn gitlab_emoji_reactions_mrs_get(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, emoji_reactions::mr_emoji_get, p, "MR emoji reaction")
    }

    #[tool(
        description = "Add an emoji reaction to a GitLab merge request. Required: project_id, merge_request_iid, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_mrs_create(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            emoji_reactions::mr_emoji_create,
            p,
            "MR emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a GitLab merge request. Only the reaction author or administrators may delete. Required: project_id, merge_request_iid, award_id."
    )]
    async fn gitlab_emoji_reactions_mrs_delete(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            emoji_reactions::mr_emoji_delete,
            p,
            "MR emoji reaction"
        )
    }

    #[tool(
        description = "List all emoji reactions on a GitLab project snippet. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_snippets_list(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            emoji_reactions::snippet_emoji_list,
            p,
            "snippet emoji reactions"
        )
    }

    #[tool(description = "Get a single emoji reaction on a GitLab project snippet by award ID.")]
    async fn gitlab_emoji_reactions_snippets_get(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            emoji_reactions::snippet_emoji_get,
            p,
            "snippet emoji reaction"
        )
    }

    #[tool(
        description = "Add an emoji reaction to a GitLab project snippet. Required: project_id, snippet_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_snippets_create(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            emoji_reactions::snippet_emoji_create,
            p,
            "snippet emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a GitLab project snippet. Only the reaction author or administrators may delete. Required: project_id, snippet_id, award_id."
    )]
    async fn gitlab_emoji_reactions_snippets_delete(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            emoji_reactions::snippet_emoji_delete,
            p,
            "snippet emoji reaction"
        )
    }

    #[tool(
        description = "List all emoji reactions on a note (comment) on a GitLab issue. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_issue_notes_list(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueNoteEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            emoji_reactions::issue_note_emoji_list,
            p,
            "issue note emoji reactions"
        )
    }

    #[tool(
        description = "Get a single emoji reaction on a note (comment) on a GitLab issue by award ID."
    )]
    async fn gitlab_emoji_reactions_issue_notes_get(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueNoteEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            emoji_reactions::issue_note_emoji_get,
            p,
            "issue note emoji reaction"
        )
    }

    #[tool(
        description = "Add an emoji reaction to a note (comment) on a GitLab issue. Required: project_id, issue_iid, note_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_issue_notes_create(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueNoteEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            emoji_reactions::issue_note_emoji_create,
            p,
            "issue note emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a note (comment) on a GitLab issue. Only the reaction author or administrators may delete. Required: project_id, issue_iid, note_id, award_id."
    )]
    async fn gitlab_emoji_reactions_issue_notes_delete(
        &self,
        Parameters(p): Parameters<emoji_reactions::IssueNoteEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            emoji_reactions::issue_note_emoji_delete,
            p,
            "issue note emoji reaction"
        )
    }

    #[tool(
        description = "List all emoji reactions on a note (comment) on a GitLab merge request. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_mr_notes_list(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrNoteEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            emoji_reactions::mr_note_emoji_list,
            p,
            "MR note emoji reactions"
        )
    }

    #[tool(
        description = "Get a single emoji reaction on a note (comment) on a GitLab merge request by award ID."
    )]
    async fn gitlab_emoji_reactions_mr_notes_get(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrNoteEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            emoji_reactions::mr_note_emoji_get,
            p,
            "MR note emoji reaction"
        )
    }

    #[tool(
        description = "Add an emoji reaction to a note (comment) on a GitLab merge request. Required: project_id, merge_request_iid, note_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_mr_notes_create(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrNoteEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            emoji_reactions::mr_note_emoji_create,
            p,
            "MR note emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a note (comment) on a GitLab merge request. Only the reaction author or administrators may delete. Required: project_id, merge_request_iid, note_id, award_id."
    )]
    async fn gitlab_emoji_reactions_mr_notes_delete(
        &self,
        Parameters(p): Parameters<emoji_reactions::MrNoteEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            emoji_reactions::mr_note_emoji_delete,
            p,
            "MR note emoji reaction"
        )
    }

    #[tool(
        description = "List all emoji reactions on a note (comment) on a GitLab project snippet. Paginate with page and per_page."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_list(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetNoteEmojiListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(
            self,
            emoji_reactions::snippet_note_emoji_list,
            p,
            "snippet note emoji reactions"
        )
    }

    #[tool(
        description = "Get a single emoji reaction on a note (comment) on a GitLab project snippet by award ID."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_get(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetNoteEmojiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(
            self,
            emoji_reactions::snippet_note_emoji_get,
            p,
            "snippet note emoji reaction"
        )
    }

    #[tool(
        description = "Add an emoji reaction to a note (comment) on a GitLab project snippet. Required: project_id, snippet_id, note_id, name (emoji name without colons, e.g. \"thumbsup\")."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_create(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetNoteEmojiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            emoji_reactions::snippet_note_emoji_create,
            p,
            "snippet note emoji reaction"
        )
    }

    #[tool(
        description = "Delete an emoji reaction from a note (comment) on a GitLab project snippet. Only the reaction author or administrators may delete. Required: project_id, snippet_id, note_id, award_id."
    )]
    async fn gitlab_emoji_reactions_snippet_notes_delete(
        &self,
        Parameters(p): Parameters<emoji_reactions::SnippetNoteEmojiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(
            self,
            emoji_reactions::snippet_note_emoji_delete,
            p,
            "snippet note emoji reaction"
        )
    }

    #[tool(
        description = "List runners available to the current authenticated user. Optional filters: type (\"instance_type\", \"group_type\", \"project_type\"), status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_list(
        &self,
        Parameters(p): Parameters<runners::RunnersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners::runners_list, p, "runners")
    }

    #[tool(
        description = "List all runners registered on the GitLab instance (administrators only). Optional filters: type (\"instance_type\", \"group_type\", \"project_type\"), status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_all_list(
        &self,
        Parameters(p): Parameters<runners::RunnersAllListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners::runners_all_list, p, "runners")
    }

    #[tool(
        description = "Get details of a single GitLab runner by ID. Returns architecture, description, ip_address, status, tag_list, version, platform, projects, and more."
    )]
    async fn gitlab_runners_get(
        &self,
        Parameters(p): Parameters<runners::RunnerGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, runners::runner_get, p, "runner")
    }

    #[tool(
        description = "List jobs processed by a specific GitLab runner. Optional filters: system_id (runner manager), status (\"running\", \"success\", \"failed\", \"canceled\"), sort (\"asc\" or \"desc\"). Paginate with page and per_page."
    )]
    async fn gitlab_runners_jobs_list(
        &self,
        Parameters(p): Parameters<runners::RunnerJobsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners::runner_jobs_list, p, "runner jobs")
    }

    #[tool(
        description = "List runner managers (individual machines) registered under a GitLab runner. Returns system_id, version, platform, architecture, ip_address, status, and last contact time. Paginate with page and per_page."
    )]
    async fn gitlab_runners_managers_list(
        &self,
        Parameters(p): Parameters<runners::RunnerManagersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners::runner_managers_list, p, "runner managers")
    }

    #[tool(
        description = "List runners available to a GitLab project. project_id accepts a numeric ID or namespace path. Optional filters: type (\"instance_type\", \"group_type\", \"project_type\"), status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_list_for_project(
        &self,
        Parameters(p): Parameters<runners::ProjectRunnersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners::project_runners_list, p, "project runners")
    }

    #[tool(
        description = "List runners available to a GitLab group. group_id accepts a numeric ID or namespace path. Optional filters: status (\"online\", \"offline\", \"stale\", \"never_contacted\"), paused, tag_list, version_prefix. Paginate with page and per_page."
    )]
    async fn gitlab_runners_list_for_group(
        &self,
        Parameters(p): Parameters<runners::GroupRunnersListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_list!(self, runners::group_runners_list, p, "group runners")
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
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_logging()
                .build(),
        )
        .with_server_info(Implementation::new("gitlab-mcp", env!("CARGO_PKG_VERSION")))
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let _ = self.peer.set(context.peer);
    }

    /// Override the macro-generated dispatch so every tool call runs inside a
    /// `PROGRESS_CTX` scope. When the client sent a `progressToken`, `paginate`
    /// can then emit per-page `notifications/progress` during a fetch_all walk.
    /// Without a token the scope holds `None` and emission is a no-op.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let progress_ctx = context.meta.get_progress_token().map(|token| ProgressCtx {
            peer: context.peer.clone(),
            token,
        });
        PROGRESS_CTX
            .scope(progress_ctx, async move {
                let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
                self.tool_router.call(tcc).await
            })
            .await
    }

    async fn set_level(
        &self,
        request: SetLevelRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        *self.log_level.lock().unwrap() = request.level;
        let _ = self.peer.set(context.peer);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // unwrap_404_as_empty_array / unwrap_404_or_403_as_empty_array

    fn api_err(status: StatusCode) -> Result<Value, GitlabError> {
        Err(GitlabError::Api {
            status,
            body: "x".into(),
        })
    }

    #[test]
    fn unwrap_404_passes_through_ok() {
        let r = unwrap_404_as_empty_array(Ok(json!([{"id": 1}]))).unwrap();
        assert_eq!(r, json!([{"id": 1}]));
    }

    #[test]
    fn unwrap_404_swallows_404() {
        let r = unwrap_404_as_empty_array(api_err(StatusCode::NOT_FOUND)).unwrap();
        assert_eq!(r, json!([]));
    }

    #[test]
    fn unwrap_404_propagates_403() {
        let err = unwrap_404_as_empty_array(api_err(StatusCode::FORBIDDEN)).unwrap_err();
        assert!(matches!(err, GitlabError::Api { status, .. } if status == StatusCode::FORBIDDEN));
    }

    #[test]
    fn unwrap_404_propagates_500() {
        let err =
            unwrap_404_as_empty_array(api_err(StatusCode::INTERNAL_SERVER_ERROR)).unwrap_err();
        assert!(matches!(err, GitlabError::Api { .. }));
    }

    #[test]
    fn unwrap_404_or_403_passes_through_ok() {
        let r = unwrap_404_or_403_as_empty_array(Ok(json!([{"id": 1}]))).unwrap();
        assert_eq!(r, json!([{"id": 1}]));
    }

    #[test]
    fn unwrap_404_or_403_swallows_404() {
        let r = unwrap_404_or_403_as_empty_array(api_err(StatusCode::NOT_FOUND)).unwrap();
        assert_eq!(r, json!([]));
    }

    #[test]
    fn unwrap_404_or_403_swallows_403() {
        let r = unwrap_404_or_403_as_empty_array(api_err(StatusCode::FORBIDDEN)).unwrap();
        assert_eq!(r, json!([]));
    }

    #[test]
    fn unwrap_404_or_403_propagates_500() {
        let err = unwrap_404_or_403_as_empty_array(api_err(StatusCode::INTERNAL_SERVER_ERROR))
            .unwrap_err();
        assert!(matches!(err, GitlabError::Api { .. }));
    }

    // BodyBuilder

    #[test]
    fn body_builder_empty() {
        assert_eq!(BodyBuilder::new().build(), json!({}));
    }

    #[test]
    fn body_builder_req_string() {
        let v = BodyBuilder::new().req("title", "hello").build();
        assert_eq!(v["title"], json!("hello"));
    }

    #[test]
    fn body_builder_req_bool_and_number() {
        let v = BodyBuilder::new()
            .req("squash", true)
            .req("milestone_id", 42u64)
            .build();
        assert_eq!(v["squash"], json!(true));
        assert_eq!(v["milestone_id"], json!(42));
    }

    #[test]
    fn body_builder_opt_some_inserts() {
        let v = BodyBuilder::new().opt("description", Some("desc")).build();
        assert_eq!(v["description"], json!("desc"));
    }

    #[test]
    fn body_builder_opt_none_omits() {
        let v = BodyBuilder::new()
            .opt("description", None::<String>)
            .build();
        assert!(v.get("description").is_none());
    }

    #[test]
    fn body_builder_opt_vec_u64() {
        let v = BodyBuilder::new()
            .opt("assignee_ids", Some(vec![1u64, 2, 3]))
            .build();
        assert_eq!(v["assignee_ids"], json!([1, 2, 3]));
    }

    #[test]
    fn body_builder_mixed_req_and_opt() {
        let v = BodyBuilder::new()
            .req("title", "t")
            .opt("desc", Some("d"))
            .opt("missing", None::<String>)
            .build();
        assert_eq!(v["title"], json!("t"));
        assert_eq!(v["desc"], json!("d"));
        assert!(v.get("missing").is_none());
    }

    // encode_path_segment

    #[test]
    fn encode_path_segment_no_slash() {
        assert_eq!(encode_path_segment("main"), "main");
    }

    #[test]
    fn encode_path_segment_single_slash() {
        assert_eq!(encode_path_segment("feat/login"), "feat%2Flogin");
    }

    #[test]
    fn encode_path_segment_multiple_slashes() {
        assert_eq!(encode_path_segment("a/b/c"), "a%2Fb%2Fc");
    }

    // encode_namespace_id

    #[test]
    fn encode_namespace_id_numeric_passthrough() {
        assert_eq!(encode_namespace_id("12345"), "12345");
    }

    #[test]
    fn encode_namespace_id_path_encodes_slash() {
        assert_eq!(encode_namespace_id("mygroup/myrepo"), "mygroup%2Fmyrepo");
    }

    // QueryBuilder

    #[test]
    fn query_builder_opt_some_adds_param() {
        let params = QueryBuilder::new().opt("page", Some(2u32)).into_params();
        assert_eq!(params, vec![("page", "2".to_string())]);
    }

    #[test]
    fn query_builder_opt_none_omits() {
        let params = QueryBuilder::new().opt("page", None::<u32>).into_params();
        assert!(params.is_empty());
    }

    #[test]
    fn query_builder_multi_expands_to_repeated_key() {
        let params = QueryBuilder::new()
            .multi("labels[]", Some(vec!["bug".into(), "wip".into()]))
            .into_params();
        assert_eq!(
            params,
            vec![
                ("labels[]", "bug".to_string()),
                ("labels[]", "wip".to_string()),
            ]
        );
    }

    #[test]
    fn query_builder_multi_none_omits() {
        let params = QueryBuilder::new().multi("labels[]", None).into_params();
        assert!(params.is_empty());
    }

    // json_list_result

    fn parse_result(result: Result<CallToolResult, McpError>) -> Value {
        let result = result.unwrap();
        let text = &result.content[0].as_text().unwrap().text;
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn json_list_result_includes_all_meta_fields_when_present() {
        let meta = PaginationMeta {
            page: Some(2),
            per_page: Some(20),
            total: Some(49),
            total_pages: Some(3),
            next_page: Some(3),
        };
        let v = parse_result(json_list_result(json!([{"iid": 1}, {"iid": 2}]), meta));
        assert_eq!(v["items"], json!([{"iid": 1}, {"iid": 2}]));
        assert_eq!(v["page"], json!(2));
        assert_eq!(v["per_page"], json!(20));
        assert_eq!(v["total"], json!(49));
        assert_eq!(v["total_pages"], json!(3));
        assert_eq!(v["next_page"], json!(3));
    }

    #[test]
    fn json_list_result_omits_absent_meta_fields() {
        let v = parse_result(json_list_result(json!([]), PaginationMeta::default()));
        assert_eq!(v["items"], json!([]));
        assert!(v.get("page").is_none());
        assert!(v.get("per_page").is_none());
        assert!(v.get("total").is_none());
        assert!(v.get("total_pages").is_none());
        assert!(v.get("next_page").is_none());
    }

    #[test]
    fn json_list_result_partial_meta_only_includes_present_fields() {
        // GitLab omits x-total / x-total-pages on large endpoints; meta should mirror that.
        let meta = PaginationMeta {
            page: Some(1),
            per_page: Some(100),
            total: None,
            total_pages: None,
            next_page: Some(2),
        };
        let v = parse_result(json_list_result(json!([]), meta));
        assert_eq!(v["page"], json!(1));
        assert_eq!(v["per_page"], json!(100));
        assert_eq!(v["next_page"], json!(2));
        assert!(v.get("total").is_none());
        assert!(v.get("total_pages").is_none());
    }

    #[test]
    fn json_list_result_slims_items() {
        // slim_list strips description, _links, etc. from each item.
        let v = parse_result(json_list_result(
            json!([{
                "iid": 1,
                "title": "x",
                "description": "long",
                "_links": {"self": "https://example.com"},
            }]),
            PaginationMeta::default(),
        ));
        let item = &v["items"][0];
        assert_eq!(item["iid"], json!(1));
        assert_eq!(item["title"], json!("x"));
        assert!(item.get("description").is_none());
        assert!(item.get("_links").is_none());
    }

    // paginate (fetch_all)

    mod paginate_tests {
        use super::*;
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn mock_client(server: &MockServer) -> GitlabClient {
            GitlabClient::new(server.uri(), "test-token").unwrap()
        }

        /// JSON array of `n` minimal objects.
        fn page_body(n: u64) -> Value {
            Value::Array((0..n).map(|i| json!({ "id": i })).collect())
        }

        #[tokio::test]
        async fn fetch_all_false_is_single_page_with_real_headers() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("x-page", "2")
                        .insert_header("x-per-page", "20")
                        .insert_header("x-total", "49")
                        .insert_header("x-next-page", "3")
                        .set_body_json(page_body(20)),
                )
                .mount(&server)
                .await;

            let (body, meta) = paginate(
                &mock_client(&server),
                "/api/v4/projects/1/issues",
                &[],
                false,
            )
            .await
            .unwrap();
            assert_eq!(body.as_array().unwrap().len(), 20);
            // Real headers pass through untouched.
            assert_eq!(meta.page, Some(2));
            assert_eq!(meta.total, Some(49));
            assert_eq!(meta.next_page, Some(3));
        }

        #[tokio::test]
        async fn fetch_all_merges_pages_until_short_page() {
            let server = MockServer::start().await;
            // Page 1: full 100 items, signals a next page.
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .and(query_param("page", "1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("x-next-page", "2")
                        .set_body_json(page_body(100)),
                )
                .mount(&server)
                .await;
            // Page 2: short page (30) ends the walk.
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .and(query_param("page", "2"))
                .respond_with(ResponseTemplate::new(200).set_body_json(page_body(30)))
                .mount(&server)
                .await;

            let (body, meta) = paginate(
                &mock_client(&server),
                "/api/v4/projects/1/issues",
                &[],
                true,
            )
            .await
            .unwrap();
            assert_eq!(body.as_array().unwrap().len(), 130);
            // Merged result is presented as one complete page.
            assert_eq!(meta.total, Some(130));
            assert_eq!(meta.total_pages, Some(1));
            assert_eq!(meta.next_page, None);
        }

        #[tokio::test]
        async fn fetch_all_single_short_page_stops_immediately() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .respond_with(ResponseTemplate::new(200).set_body_json(page_body(5)))
                .mount(&server)
                .await;

            let (body, _) = paginate(
                &mock_client(&server),
                "/api/v4/projects/1/issues",
                &[],
                true,
            )
            .await
            .unwrap();
            assert_eq!(body.as_array().unwrap().len(), 5);
        }

        #[tokio::test]
        async fn fetch_all_stops_on_missing_next_header_despite_full_page() {
            let server = MockServer::start().await;
            // Full 100-item page but no x-next-page — GitLab omits it on some
            // large endpoints. The belt-and-suspenders check must terminate.
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .respond_with(ResponseTemplate::new(200).set_body_json(page_body(100)))
                .mount(&server)
                .await;

            let (body, _) = paginate(
                &mock_client(&server),
                "/api/v4/projects/1/issues",
                &[],
                true,
            )
            .await
            .unwrap();
            assert_eq!(body.as_array().unwrap().len(), 100);
        }

        #[tokio::test]
        async fn fetch_all_drops_caller_paging() {
            let server = MockServer::start().await;
            // fetch_all must override caller page/per_page: it always starts at
            // page 1 with per_page=100, regardless of what was passed in.
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .and(query_param("page", "1"))
                .and(query_param("per_page", "100"))
                .respond_with(ResponseTemplate::new(200).set_body_json(page_body(3)))
                .mount(&server)
                .await;

            let params = [("page", "7".to_string()), ("per_page", "20".to_string())];
            let (body, _) = paginate(
                &mock_client(&server),
                "/api/v4/projects/1/issues",
                &params,
                true,
            )
            .await
            .unwrap();
            assert_eq!(body.as_array().unwrap().len(), 3);
        }

        #[tokio::test]
        async fn fetch_all_enforces_page_limit() {
            let server = MockServer::start().await;
            // Always a full page that signals more — never terminates on its own.
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("x-next-page", "999")
                        .set_body_json(page_body(100)),
                )
                .mount(&server)
                .await;

            let err = paginate(
                &mock_client(&server),
                "/api/v4/projects/1/issues",
                &[],
                true,
            )
            .await
            .unwrap_err();
            assert!(matches!(err, GitlabError::Other(msg) if msg.contains("page limit")));
        }

        #[tokio::test]
        async fn fetch_all_inside_progress_scope_with_no_token_still_works() {
            // Exercises the `PROGRESS_CTX` set-to-`None` branch (a tool call
            // without a progressToken): emit_page_progress must be a no-op and
            // the merge must complete normally.
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v4/projects/1/issues"))
                .respond_with(ResponseTemplate::new(200).set_body_json(page_body(7)))
                .mount(&server)
                .await;

            let client = mock_client(&server);
            let (body, _) = PROGRESS_CTX
                .scope(None, async {
                    paginate(&client, "/api/v4/projects/1/issues", &[], true).await
                })
                .await
                .unwrap();
            assert_eq!(body.as_array().unwrap().len(), 7);
        }
    }
}
