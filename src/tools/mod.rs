use reqwest::StatusCode;
use rmcp::{
    ErrorData as McpError, Peer, RoleServer, ServerHandler,
    handler::server::router::{prompt::PromptRouter, tool::ToolRouter},
    model::*,
    prompt_handler,
    service::{NotificationContext, RequestContext},
    tool_handler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

use crate::client::{GitlabClient, GitlabError, ListResult, PaginationMeta};

// --------------------------------------------------------------------------
// Delegation macros
// --------------------------------------------------------------------------
// Defined before the `pub mod` declarations below so the macros are in
// textual scope inside every domain module (where the tool shims live).

macro_rules! delegate_json {
    ($self:expr, $domain_fn:path, $p:expr, $verb:literal, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(v) => $crate::tools::json_result(v),
            Err(e) => {
                let msg = format!("{} {}: {}", $verb, $noun, e.to_tool_message());
                tracing::error!("{msg}");
                $crate::tools::tool_error(&msg)
            }
        }
    }};
}

macro_rules! delegate_list {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok((v, meta)) => $crate::tools::json_list_result(v, meta),
            Err(e) => {
                let msg = format!("listing {}: {}", $noun, e.to_tool_message());
                tracing::error!("{msg}");
                $crate::tools::tool_error(&msg)
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

/// For domain functions returning `()` — success yields a fixed confirmation
/// message instead of a JSON body.
macro_rules! delegate_unit {
    ($self:expr, $domain_fn:path, $p:expr, $verb:literal, $noun:literal, $ok_msg:expr) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(()) => Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::ContentBlock::text($ok_msg),
            ])),
            Err(e) => {
                let msg = format!("{} {}: {}", $verb, $noun, e.to_tool_message());
                tracing::error!("{msg}");
                $crate::tools::tool_error(&msg)
            }
        }
    }};
}

macro_rules! delegate_delete {
    ($self:expr, $domain_fn:path, $p:expr, $noun:literal) => {
        delegate_unit!(
            $self,
            $domain_fn,
            $p,
            "deleting",
            $noun,
            concat!($noun, " deleted")
        )
    };
}

/// For domain functions returning plain text (`String`) rather than JSON.
macro_rules! delegate_text {
    ($self:expr, $domain_fn:path, $p:expr, $verb:literal, $noun:literal) => {{
        let client = $self.get_client()?;
        match $domain_fn(client, $p).await {
            Ok(text) => Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::ContentBlock::text(text),
            ])),
            Err(e) => {
                let msg = format!("{} {}: {}", $verb, $noun, e.to_tool_message());
                tracing::error!("{msg}");
                $crate::tools::tool_error(&msg)
            }
        }
    }};
}

pub mod branches;
pub mod commits;
pub mod completions;
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
pub mod prompts;
pub mod repositories;
pub mod repository_files;
pub mod resources;
pub mod runners;
pub mod search;
mod slim;
pub mod snippets;
pub mod users;
pub mod work_items;

// Opt-in live integration tests (cargo test --features live-tests).
// Placed inside `tools` so it can reach the private `slim` module and the
// pub(crate) helpers without widening the crate's public surface. The whole
// subtree is gated here, so each area module under `live/` need not repeat it.
#[cfg(all(test, feature = "live-tests"))]
mod live;

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
    let ctx = PROGRESS_CTX
        .try_with(std::clone::Clone::clone)
        .ok()
        .flatten();
    if let Some(ctx) = ctx {
        let mut param = ProgressNotificationParam::new(ctx.token, progress as f64);
        param.total = total.map(|t| t as f64);
        param.message = Some(format!("{progress} items"));
        let _ = ctx.peer.notify_progress(param).await;
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
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
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
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

// Returns `Ok` unconditionally so the shape matches `json_result` /
// `json_list_result`, keeping the delegate-macro match arms uniform (both arms
// yield `Result<CallToolResult, McpError>`).
#[allow(clippy::unnecessary_wraps)]
pub fn tool_error(msg: &str) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(msg)]))
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

/// Finalize a list request: append the shared `page`/`per_page` query params to
/// an endpoint-specific [`QueryBuilder`] and drive [`paginate`] with the
/// caller's `fetch_all` flag. Collapses the boilerplate tail every list domain
/// function would otherwise repeat.
pub async fn list_paginated(
    client: &GitlabClient,
    path: &str,
    qb: QueryBuilder,
    pagination: PaginationParams,
) -> ListResult {
    let params = qb
        .opt("page", pagination.page)
        .opt("per_page", pagination.per_page)
        .into_params();
    paginate(client, path, &params, pagination.fetch_all.unwrap_or(false)).await
}

// --------------------------------------------------------------------------
// Query construction
// --------------------------------------------------------------------------

/// Characters percent-encoded inside a single URL path segment: the `url`
/// crate's PATH_SEGMENT set (controls, space, `"#<>?\`{}`), plus `/` (the
/// segment separator — GitLab expects it as %2F in namespace paths, branch
/// names, and file paths) and `%` (so a literal percent sign survives the
/// round trip instead of being misread as an existing escape).
const PATH_SEGMENT_ENCODE_SET: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'%');

/// URL-encode a namespace ID (project or group) for use in REST API paths.
/// Numeric IDs pass through unchanged; path-style IDs like
/// "mygroup/myrepo" are percent-encoded ("mygroup%2Fmyrepo").
pub(crate) fn encode_namespace_id(id: &str) -> String {
    if id.chars().all(|c| c.is_ascii_digit()) {
        id.to_string()
    } else {
        encode_path_segment(id)
    }
}

/// Percent-encode a value used as one URL path segment (branch name, file
/// path, commit ref, …). Without this, a `#` would be parsed as a fragment
/// delimiter and a `?` would start the query string, silently truncating the
/// request path.
pub(crate) fn encode_path_segment(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, PATH_SEGMENT_ENCODE_SET).to_string()
}

/// A GitLab **project** identifier accepted by project-scoped tools: a numeric
/// ID or a URL-encoded namespace path. This newtype exists so the parameter
/// description — otherwise duplicated across ~130 `*Params` structs — lives in
/// exactly one place: its [`schemars::JsonSchema`] impl below. It is
/// `#[serde(transparent)]` over a plain string (wire shape unchanged) and
/// `Deref`s to `str`, so existing `project_path(&p.project_id)` call sites keep
/// working unchanged via deref coercion.
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(String);

/// A GitLab **group** identifier: a numeric ID or full namespace path. The
/// group-scoped counterpart of [`ProjectId`].
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct GroupId(String);

/// Implement the shared plumbing (Deref/From + the single-source-of-truth
/// `JsonSchema` description) for a namespace-id newtype.
macro_rules! namespace_id_newtype {
    ($ty:ty, $name:literal, $desc:literal) => {
        impl std::ops::Deref for $ty {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }
        impl From<String> for $ty {
            fn from(s: String) -> Self {
                Self(s)
            }
        }
        impl From<&str> for $ty {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
        impl schemars::JsonSchema for $ty {
            // Inline the schema at each use site so the description renders on the
            // field itself rather than behind a `$ref`.
            fn inline_schema() -> bool {
                true
            }
            fn schema_name() -> std::borrow::Cow<'static, str> {
                $name.into()
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({ "type": "string", "description": $desc })
            }
        }
    };
}

namespace_id_newtype!(
    ProjectId,
    "ProjectId",
    "Project ID (numeric) or URL-encoded namespace path (e.g. \"42\" or \"mygroup/myproject\")"
);
namespace_id_newtype!(
    GroupId,
    "GroupId",
    "Group ID (numeric) or full namespace path (e.g. \"42\" or \"mygroup/subgroup\")"
);

/// `/api/v4/projects/{id}` — the prefix shared by every project-scoped
/// endpoint. Domain functions append their own suffix:
/// `format!("{}/issues/{}", project_path(&p.project_id), p.issue_iid)`.
pub(crate) fn project_path(project_id: &str) -> String {
    format!("/api/v4/projects/{}", encode_namespace_id(project_id))
}

/// `/api/v4/groups/{id}` — the group-scoped counterpart of [`project_path`].
pub(crate) fn group_path(group_id: &str) -> String {
    format!("/api/v4/groups/{}", encode_namespace_id(group_id))
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
    pub const fn new() -> Self {
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

/// Serialize a request-body value to JSON. Every caller passes a scalar,
/// `String`, or `Vec` of those, whose `Serialize` impls are infallible, so this
/// never actually errors — but fall back to `Null` rather than panicking if a
/// future caller ever passes a type that can.
fn to_json<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
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
        self.map.insert(key.to_string(), to_json(v));
        self
    }

    pub fn opt<T: serde::Serialize>(mut self, key: &'static str, v: Option<T>) -> Self {
        if let Some(v) = v {
            self.map.insert(key.to_string(), to_json(v));
        }
        self
    }

    pub fn build(self) -> Value {
        Value::Object(self.map)
    }
}

// --------------------------------------------------------------------------
// Server struct
// --------------------------------------------------------------------------

#[derive(Clone)]
pub struct GitlabMcpServer {
    client: Arc<OnceLock<GitlabClient>>,
    // Used by the `call_tool` override below; reused per call rather than
    // rebuilt via `Self::tool_router()` each time.
    tool_router: ToolRouter<Self>,
    // Initialised by `new_stdio` but never read: `#[prompt_handler]` calls the
    // `prompt_router()` *function* (generated by the `#[prompt_router]` block
    // in `prompts.rs`), not this field. `expect` (not `allow`) so that if a
    // future rmcp starts reading the field, the unfulfilled-expectation lint
    // flags this for removal.
    #[expect(dead_code)]
    prompt_router: PromptRouter<Self>,
    peer: Arc<OnceLock<Peer<RoleServer>>>,
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
        })
    }

    fn get_client(&self) -> Result<&GitlabClient, McpError> {
        self.client
            .get()
            .ok_or_else(|| McpError::internal_error("GitLab client not initialized", None))
    }
}

// --------------------------------------------------------------------------
// Tool router assembly
// --------------------------------------------------------------------------

impl GitlabMcpServer {
    /// Combined tool router: one sub-router per domain module, each generated
    /// by `#[tool_router(router = tool_router_<domain>)]` next to the domain's
    /// shims. `#[tool_handler]` and `new_stdio` both consume this.
    fn tool_router() -> ToolRouter<Self> {
        Self::tool_router_metadata()
            + Self::tool_router_search()
            + Self::tool_router_issues()
            + Self::tool_router_issue_notes()
            + Self::tool_router_issue_discussions()
            + Self::tool_router_merge_requests()
            + Self::tool_router_branches()
            + Self::tool_router_commits()
            + Self::tool_router_pipelines()
            + Self::tool_router_pipeline_schedules()
            + Self::tool_router_jobs()
            + Self::tool_router_repositories()
            + Self::tool_router_repository_files()
            + Self::tool_router_discussions()
            + Self::tool_router_groups()
            + Self::tool_router_projects()
            + Self::tool_router_epics()
            + Self::tool_router_snippets()
            + Self::tool_router_emoji_reactions()
            + Self::tool_router_runners()
            + Self::tool_router_users()
            + Self::tool_router_work_items()
    }
}

// --------------------------------------------------------------------------
// ServerHandler
// --------------------------------------------------------------------------

/// Render a tool's input schema as a one-line field list, required fields
/// first: `project_id (required), issue_iid (required), page, per_page`.
/// Returns `None` when the schema has no properties to describe.
fn expected_fields_summary(schema: &JsonObject) -> Option<String> {
    let props = schema.get("properties")?.as_object()?;
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let (req, opt): (Vec<&String>, Vec<&String>) =
        props.keys().partition(|k| required.contains(&k.as_str()));
    let fields: Vec<String> = req
        .into_iter()
        .map(|k| format!("{k} (required)"))
        .chain(opt.into_iter().cloned())
        .collect();
    if fields.is_empty() {
        None
    } else {
        Some(fields.join(", "))
    }
}

/// Append the tool's accepted fields to an invalid-params error so a caller
/// that guessed a wrong or missing parameter name can self-correct from the
/// error alone, without a separate schema-lookup round-trip.
fn enrich_invalid_params(error: McpError, tool: Option<&Tool>) -> McpError {
    let Some(summary) = tool.and_then(|t| expected_fields_summary(&t.input_schema)) else {
        return error;
    };
    McpError::invalid_params(
        format!("{}. Expected fields: {}", error.message, summary),
        error.data,
    )
}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for GitlabMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .enable_completions()
                .build(),
        )
        .with_server_info(Implementation::new("gitlab-mcp", env!("CARGO_PKG_VERSION")));
        info.instructions = Some(
            "Parameter naming conventions: project_id and group_id accept a numeric ID or a \
             namespace path (e.g. \"mygroup/myproject\"). Resources addressed by the number in \
             their URL use <resource>_iid (issue_iid, merge_request_iid, epic_iid); resources \
             addressed by a globally unique ID use <resource>_id (note_id, pipeline_id, \
             snippet_id, runner_id, job_id, discussion_id). When unsure of a tool's exact \
             parameter names, call gitlab_tool_schema_get with the tool name first. \
             Read-only data is also available as MCP resources: resources/list returns \
             your recently active projects, and these gitlab:// URI templates are \
             supported: gitlab://{project_id} (project overview), \
             gitlab://{project_id}/files/{file_path}{?ref} (file content), \
             gitlab://{project_id}/issues/{issue_iid}, \
             gitlab://{project_id}/mrs/{merge_request_iid}, and \
             gitlab://{project_id}/pipelines/{pipeline_id}. In a resource URI a \
             namespace-path project_id must be percent-encoded (mygroup%2Fmyproject)."
                .to_string(),
        );
        info
    }

    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let _ = self.peer.set(context.peer);
    }

    /// The full resource space isn't enumerable, but client resource pickers
    /// only surface `resources/list`, so return the projects that plausibly
    /// matter: the caller's recently active member projects. Everything else
    /// stays discoverable through the templates.
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let client = self.get_client()?;
        let items = resources::list_recent_projects(client).await.map_err(|e| {
            let msg = format!("listing recent projects: {}", e.to_tool_message());
            tracing::error!("{msg}");
            McpError::internal_error(msg, None)
        })?;
        Ok(ListResourcesResult::with_all_items(items))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(
            resources::resource_templates(),
        ))
    }

    /// Argument autocompletion for prompts and resource templates. Dispatch is
    /// on the argument name (shared across prompts and templates by design);
    /// see `completions.rs`. A resource-template reference gets URI-encoded
    /// `project_id` suggestions, since they substitute into a `gitlab://` URI.
    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let client = self.get_client()?;
        let for_resource_uri = matches!(request.r#ref, Reference::Resource(_));
        let context_args = request.context.as_ref().and_then(|c| c.arguments.as_ref());
        let completion = completions::complete_argument(
            client,
            for_resource_uri,
            &request.argument.name,
            &request.argument.value,
            context_args,
        )
        .await
        .map_err(|e| {
            let msg = format!(
                "completing {}: {}",
                request.argument.name,
                e.to_tool_message()
            );
            tracing::error!("{msg}");
            McpError::internal_error(msg, None)
        })?;
        let info = CompletionInfo::with_pagination(completion.values, None, completion.has_more)
            .map_err(|e| McpError::internal_error(e, None))?;
        Ok(CompleteResult::new(info))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let client = self.get_client()?;
        let resource = resources::parse_uri(&request.uri).map_err(|reason| {
            McpError::resource_not_found(
                format!("unsupported resource URI \"{}\": {reason}", request.uri),
                None,
            )
        })?;
        match resources::read(client, resource, &request.uri).await {
            Ok(contents) => Ok(ReadResourceResult::new(contents)),
            Err(e) => {
                let msg = format!("reading {}: {}", request.uri, e.to_tool_message());
                tracing::error!("{msg}");
                match e {
                    GitlabError::Api { status, .. } if status == StatusCode::NOT_FOUND => {
                        Err(McpError::resource_not_found(msg, None))
                    }
                    _ => Err(McpError::internal_error(msg, None)),
                }
            }
        }
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
        let tool_name = request.name.clone();
        let result = PROGRESS_CTX
            .scope(progress_ctx, async move {
                let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
                self.tool_router.call(tcc).await
            })
            .await;
        match result {
            Err(e) if e.code == ErrorCode::INVALID_PARAMS => {
                Err(enrich_invalid_params(e, self.tool_router.get(&tool_name)))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Router assembly

    #[test]
    fn combined_router_exposes_every_tool() {
        // `tool_router()` sums the per-domain sub-routers by hand, and the
        // merge silently overwrites on duplicate tool names — so this exact
        // count catches both a forgotten `+ Self::tool_router_<domain>()`
        // and a name collision. Update it when adding or removing tools.
        let tools = GitlabMcpServer::tool_router().list_all();
        assert_eq!(tools.len(), 176);
    }

    #[test]
    fn every_tool_carries_behavior_annotations() {
        // Each tool shim must declare read/write hints (readOnlyHint etc.) so
        // clients can make auto-approval decisions. Fail closed: a newly added
        // tool without `annotations(...)` on its `#[tool]` trips this. Also
        // sanity-check that the hints are internally consistent — a read-only
        // tool must not also claim to be destructive.
        for tool in GitlabMcpServer::tool_router().list_all() {
            let ann = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} is missing tool annotations", tool.name));
            if ann.read_only_hint == Some(true) {
                assert_ne!(
                    ann.destructive_hint,
                    Some(true),
                    "{} is marked both read-only and destructive",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn annotation_profiles_match_operation() {
        let router = GitlabMcpServer::tool_router();
        let ann = |name: &str| router.get(name).unwrap().annotations.clone().unwrap();

        // Read: read-only.
        assert_eq!(ann("gitlab_issues_list").read_only_hint, Some(true));
        // Create: writes, but additive (not destructive).
        let create = ann("gitlab_issues_create");
        assert_eq!(create.read_only_hint, Some(false));
        assert_eq!(create.destructive_hint, Some(false));
        // Update: idempotent, non-destructive write.
        let update = ann("gitlab_issues_update");
        assert_eq!(update.idempotent_hint, Some(true));
        assert_eq!(update.destructive_hint, Some(false));
        // Delete: destructive and idempotent.
        let delete = ann("gitlab_issues_delete");
        assert_eq!(delete.destructive_hint, Some(true));
        assert_eq!(delete.idempotent_hint, Some(true));
    }

    // Invalid-params error enrichment

    #[test]
    fn expected_fields_summary_lists_required_first() {
        let router = GitlabMcpServer::tool_router();
        let tool = router.get("gitlab_issues_get").unwrap();
        let summary = expected_fields_summary(&tool.input_schema).unwrap();
        let required_part = summary.split(',').next().unwrap();
        assert!(
            required_part.contains("(required)"),
            "required fields must come first: {summary}"
        );
        assert!(summary.contains("project_id (required)"), "{summary}");
        assert!(summary.contains("issue_iid (required)"), "{summary}");
    }

    #[test]
    fn project_id_newtype_renders_description_inline() {
        // The ProjectId/GroupId newtypes carry the parameter description in one
        // place; verify it actually reaches the per-tool input schema inline
        // (not behind a `$ref`, which inline_schema() prevents) so LLM callers
        // still see it.
        let router = GitlabMcpServer::tool_router();

        let issue_get = router.get("gitlab_issues_get").unwrap();
        let pid = &issue_get.input_schema["properties"]["project_id"];
        assert!(
            pid.get("$ref").is_none(),
            "project_id must be inlined, not a $ref: {pid}"
        );
        assert_eq!(pid["type"], "string");
        assert!(
            pid["description"]
                .as_str()
                .unwrap()
                .contains("URL-encoded namespace path"),
            "project_id description missing: {pid}"
        );

        let epic_get = router.get("gitlab_epics_get").unwrap();
        let gid = &epic_get.input_schema["properties"]["group_id"];
        assert!(
            gid["description"]
                .as_str()
                .unwrap()
                .contains("full namespace path"),
            "group_id description missing: {gid}"
        );
    }

    #[test]
    fn project_id_deserializes_transparently_from_string() {
        // The wire contract is unchanged: a plain JSON string still deserializes
        // into the newtype, and it derefs back to that string.
        let id: ProjectId = serde_json::from_value(json!("mygroup/myproject")).unwrap();
        assert_eq!(&*id, "mygroup/myproject");
        // And inside a params struct.
        let p: issues::IssueGetParams =
            serde_json::from_value(json!({"project_id": "42", "issue_iid": 7})).unwrap();
        assert_eq!(&*p.project_id, "42");
        assert_eq!(project_path(&p.project_id), "/api/v4/projects/42");
    }

    #[test]
    fn enrich_invalid_params_appends_expected_fields() {
        let router = GitlabMcpServer::tool_router();
        let err = McpError::invalid_params(
            "failed to deserialize parameters: missing field `issue_iid`",
            None,
        );
        let enriched = enrich_invalid_params(err, router.get("gitlab_issues_get"));
        assert!(
            enriched.message.contains("missing field `issue_iid`"),
            "{}",
            enriched.message
        );
        assert!(
            enriched.message.contains("Expected fields: ")
                && enriched.message.contains("project_id (required)")
                && enriched.message.contains("issue_iid (required)"),
            "{}",
            enriched.message
        );
    }

    #[test]
    fn enrich_invalid_params_without_tool_keeps_error_unchanged() {
        let err = McpError::invalid_params("failed to deserialize parameters", None);
        let enriched = enrich_invalid_params(err, None);
        assert_eq!(enriched.message, "failed to deserialize parameters");
    }

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

    #[test]
    fn encode_path_segment_fragment_and_query_chars() {
        // `#` and `?` would otherwise truncate the path at URL parse time.
        assert_eq!(encode_path_segment("fix#123"), "fix%23123");
        assert_eq!(encode_path_segment("what?branch"), "what%3Fbranch");
    }

    #[test]
    fn encode_path_segment_space_and_percent() {
        assert_eq!(encode_path_segment("a b"), "a%20b");
        // A literal `%` must be encoded so it isn't misread as an escape.
        assert_eq!(encode_path_segment("100%done"), "100%25done");
    }

    #[test]
    fn encode_namespace_id_encodes_reserved_chars() {
        assert_eq!(encode_namespace_id("my group/repo"), "my%20group%2Frepo");
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
        use crate::test_util::mock_client;
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

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
