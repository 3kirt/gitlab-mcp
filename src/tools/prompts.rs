//! MCP Prompts: structured GitLab workflows exposed via `prompts/list` /
//! `prompts/get` (surfaced by clients as slash commands, e.g. Claude Code's
//! `/mcp__gitlab__review-mr`).
//!
//! Each prompt fetches the relevant GitLab data up front and returns it
//! embedded in a single user message, so the model starts with the full
//! context pre-loaded instead of making discovery tool calls. The builders are
//! free functions over `&GitlabClient` (like the resource dispatcher in
//! `resources.rs`) so they're unit-/live-testable without a `RequestContext`;
//! the `#[prompt]` shims in the `#[prompt_router]` block at the bottom are
//! thin wrappers.
//!
//! MCP prompt arguments are string-valued on the wire, so numeric IIDs are
//! `String` params parsed server-side (a non-numeric value is an
//! invalid-params error, not a silent mismatch).

use rmcp::ErrorData as McpError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{GetPromptResult, PromptMessage, Role};
use rmcp::{prompt, prompt_router};
use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{
    GitlabMcpServer, PaginationParams, ProjectId, QueryBuilder, discussions, issue_notes, issues,
    merge_requests, project_path, projects, repositories, slim,
};

/// Character budget for an embedded diff; beyond this the diff is cut with a
/// truncation notice. Keeps a huge MR from blowing out the client's context.
const MAX_DIFF_CHARS: usize = 48_000;
/// Character budget for embedded comment/discussion JSON.
const MAX_COMMENTS_CHARS: usize = 24_000;

// --------------------------------------------------------------------------
// Shared helpers
// --------------------------------------------------------------------------

fn parse_iid(value: &str, param: &str) -> Result<u64, McpError> {
    value.trim().parse().map_err(|_| {
        McpError::invalid_params(
            format!("{param} must be a positive integer, got \"{value}\""),
            None,
        )
    })
}

fn gitlab_err(what: &str, e: &GitlabError) -> McpError {
    McpError::internal_error(format!("{what}: {}", e.to_tool_message()), None)
}

/// Slim + pretty-print a payload for embedding, matching the tool output shape.
fn pretty(v: Value) -> String {
    serde_json::to_string_pretty(&slim::slim_get(v)).unwrap_or_else(|e| format!("<error: {e}>"))
}

fn pretty_list(v: Value) -> String {
    serde_json::to_string_pretty(&slim::slim_list(v)).unwrap_or_else(|e| format!("<error: {e}>"))
}

/// Cut an embedded block at `max_chars` (on a char boundary) with a notice.
fn truncate_block(mut s: String, max_chars: usize) -> String {
    if let Some((idx, _)) = s.char_indices().nth(max_chars) {
        s.truncate(idx);
        s.push_str("\n… (truncated: content exceeded the embedding budget)");
    }
    s
}

/// Render a GitLab diffs array (`/merge_requests/:iid/diffs` or the `diffs` of
/// a repository compare) as unified-diff-style text.
fn render_diffs(diffs: &Value) -> String {
    let Some(arr) = diffs.as_array() else {
        return String::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for d in arr {
        let old = d["old_path"].as_str().unwrap_or("?");
        let new = d["new_path"].as_str().unwrap_or("?");
        let marker = if d["new_file"].as_bool() == Some(true) {
            " (new file)"
        } else if d["deleted_file"].as_bool() == Some(true) {
            " (deleted)"
        } else if d["renamed_file"].as_bool() == Some(true) {
            " (renamed)"
        } else {
            ""
        };
        let body = d["diff"].as_str().unwrap_or("");
        out.push(format!("--- a/{old}\n+++ b/{new}{marker}\n{body}"));
    }
    out.join("\n")
}

fn user_prompt(description: String, text: String) -> GetPromptResult {
    GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
        .with_description(description)
}

// --------------------------------------------------------------------------
// review-mr
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReviewMrArgs {
    pub project_id: ProjectId,
    #[schemars(
        description = "Merge request internal ID (IID) — the number shown in the GitLab UI"
    )]
    pub merge_request_iid: String,
}

pub async fn review_mr(
    client: &GitlabClient,
    args: ReviewMrArgs,
) -> Result<GetPromptResult, McpError> {
    let iid = parse_iid(&args.merge_request_iid, "merge_request_iid")?;
    let proj = project_path(&args.project_id);

    let mr = merge_requests::mr_get(
        client,
        merge_requests::MrGetParams {
            project_id: args.project_id.clone(),
            merge_request_iid: iid,
        },
    )
    .await
    .map_err(|e| gitlab_err("fetching merge request", &e))?;

    let diff_params = QueryBuilder::new()
        .opt("per_page", Some("100"))
        .into_params();
    let diffs = client
        .get_with_params(&format!("{proj}/merge_requests/{iid}/diffs"), &diff_params)
        .await
        .map_err(|e| gitlab_err("fetching merge request diff", &e))?;

    let (threads, _meta) = discussions::mr_discussions_list(
        client,
        discussions::MrDiscussionsListParams {
            project_id: args.project_id.clone(),
            merge_request_iid: iid,
            pagination: PaginationParams {
                page: None,
                per_page: Some(100),
                fetch_all: None,
            },
        },
    )
    .await
    .map_err(|e| gitlab_err("fetching review threads", &e))?;

    let title = mr["title"].as_str().unwrap_or("").to_string();
    let threads_block = if threads.as_array().is_none_or(Vec::is_empty) {
        "(no review threads yet)".to_string()
    } else {
        truncate_block(pretty_list(threads), MAX_COMMENTS_CHARS)
    };

    let text = format!(
        "Review merge request !{iid} (\"{title}\") in GitLab project \"{}\".\n\
         \n\
         Assess correctness, clarity, and test coverage. Check whether the existing review \
         threads below have been addressed, and point out anything they missed. Cite file \
         paths and line numbers from the diff. Conclude with a short verdict: approve, \
         approve with nits, or request changes.\n\
         \n\
         ## Merge request\n\
         ```json\n{}\n```\n\
         \n\
         ## Diff\n\
         ```diff\n{}\n```\n\
         \n\
         ## Review threads\n\
         ```json\n{threads_block}\n```",
        &*args.project_id,
        pretty(mr),
        truncate_block(render_diffs(&diffs), MAX_DIFF_CHARS),
    );
    Ok(user_prompt(format!("Review MR !{iid}: {title}"), text))
}

// --------------------------------------------------------------------------
// summarize-issue
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SummarizeIssueArgs {
    pub project_id: ProjectId,
    #[schemars(description = "Issue internal ID (IID) — the number shown in the GitLab UI")]
    pub issue_iid: String,
}

pub async fn summarize_issue(
    client: &GitlabClient,
    args: SummarizeIssueArgs,
) -> Result<GetPromptResult, McpError> {
    let iid = parse_iid(&args.issue_iid, "issue_iid")?;

    let issue = issues::issue_get(
        client,
        issues::IssueGetParams {
            project_id: args.project_id.clone(),
            issue_iid: iid,
        },
    )
    .await
    .map_err(|e| gitlab_err("fetching issue", &e))?;

    let (notes, _meta) = issue_notes::issue_notes_list(
        client,
        issue_notes::IssueNotesListParams {
            project_id: args.project_id.clone(),
            issue_iid: iid,
            order_by: None,
            sort: Some("asc".to_string()),
            pagination: PaginationParams {
                page: None,
                per_page: Some(100),
                fetch_all: None,
            },
        },
    )
    .await
    .map_err(|e| gitlab_err("fetching issue comments", &e))?;

    // Keep human comments; system notes (label changes, mentions, …) are noise
    // for a summary.
    let comments: Vec<Value> = notes
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|n| n["system"].as_bool() != Some(true))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let comments_block = if comments.is_empty() {
        "(no comments yet)".to_string()
    } else {
        truncate_block(pretty_list(Value::Array(comments)), MAX_COMMENTS_CHARS)
    };

    let title = issue["title"].as_str().unwrap_or("").to_string();
    let text = format!(
        "Summarize issue #{iid} (\"{title}\") in GitLab project \"{}\".\n\
         \n\
         Give a concise summary covering: what is being reported or requested, the current \
         state of the discussion (key positions, decisions, and open questions), and any \
         proposed next steps. Note who is waiting on whom, if apparent.\n\
         \n\
         ## Issue\n\
         ```json\n{}\n```\n\
         \n\
         ## Comments (oldest first, system notes omitted)\n\
         ```json\n{comments_block}\n```",
        &*args.project_id,
        pretty(issue),
    );
    Ok(user_prompt(
        format!("Summarize issue #{iid}: {title}"),
        text,
    ))
}

// --------------------------------------------------------------------------
// create-mr-description
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateMrDescriptionArgs {
    pub project_id: ProjectId,
    #[schemars(description = "Source branch with the changes to describe")]
    pub branch: String,
    #[schemars(
        description = "Target branch to diff against (default: the project's default branch)"
    )]
    pub target_branch: Option<String>,
}

pub async fn create_mr_description(
    client: &GitlabClient,
    args: CreateMrDescriptionArgs,
) -> Result<GetPromptResult, McpError> {
    let target = if let Some(t) = args.target_branch {
        t
    } else {
        let project = projects::project_get(
            client,
            projects::ProjectGetParams {
                project_id: args.project_id.clone(),
                statistics: None,
            },
        )
        .await
        .map_err(|e| gitlab_err("fetching project", &e))?;
        project["default_branch"]
            .as_str()
            .ok_or_else(|| {
                McpError::internal_error(
                    "project has no default branch; pass target_branch explicitly",
                    None,
                )
            })?
            .to_string()
    };

    let compare = repositories::repo_compare(
        client,
        repositories::RepoCompareParams {
            project_id: args.project_id.clone(),
            from: target.clone(),
            to: args.branch.clone(),
            from_project_id: None,
            straight: None,
            unidiff: None,
        },
    )
    .await
    .map_err(|e| gitlab_err("comparing branches", &e))?;

    let commits_block = compare["commits"].as_array().map_or_else(
        || "(no commits)".to_string(),
        |commits| {
            commits
                .iter()
                .map(|c| {
                    format!(
                        "- {} {}",
                        c["short_id"].as_str().unwrap_or("?"),
                        c["title"].as_str().unwrap_or(""),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        },
    );

    let text = format!(
        "Draft a merge request title and description for branch \"{}\" targeting \"{target}\" \
         in GitLab project \"{}\".\n\
         \n\
         Write the description in Markdown with: a one-paragraph summary of what changed and \
         why, a bullet list of notable changes, and a testing note. Derive everything from \
         the commits and diff below — do not invent motivation that isn't visible. Keep the \
         title under 72 characters, imperative mood.\n\
         \n\
         ## Commits ({target}..{})\n\
         {commits_block}\n\
         \n\
         ## Diff\n\
         ```diff\n{}\n```",
        args.branch,
        &*args.project_id,
        args.branch,
        truncate_block(render_diffs(&compare["diffs"]), MAX_DIFF_CHARS),
    );
    Ok(user_prompt(
        format!("Draft MR description for {} -> {target}", args.branch),
        text,
    ))
}

// --------------------------------------------------------------------------
// MCP prompt shims
// --------------------------------------------------------------------------

#[prompt_router(vis = "pub(crate)")]
impl GitlabMcpServer {
    #[prompt(
        name = "review-mr",
        description = "Review a GitLab merge request: loads the MR, its full diff, and the \
                       existing review threads as context, then asks for a code review with a \
                       verdict"
    )]
    async fn review_mr_prompt(
        &self,
        Parameters(args): Parameters<ReviewMrArgs>,
    ) -> Result<GetPromptResult, McpError> {
        review_mr(self.get_client()?, args).await
    }

    #[prompt(
        name = "summarize-issue",
        description = "Summarize a GitLab issue: loads the issue body and all human comments \
                       as context, then asks for a summary of the state of the discussion and \
                       next steps"
    )]
    async fn summarize_issue_prompt(
        &self,
        Parameters(args): Parameters<SummarizeIssueArgs>,
    ) -> Result<GetPromptResult, McpError> {
        summarize_issue(self.get_client()?, args).await
    }

    #[prompt(
        name = "create-mr-description",
        description = "Draft a merge request title and description from a branch's commits and \
                       diff against the target branch (default: the project's default branch)"
    )]
    async fn create_mr_description_prompt(
        &self,
        Parameters(args): Parameters<CreateMrDescriptionArgs>,
    ) -> Result<GetPromptResult, McpError> {
        create_mr_description(self.get_client()?, args).await
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{GetPromptResult, Role};
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        CreateMrDescriptionArgs, ReviewMrArgs, SummarizeIssueArgs, create_mr_description,
        review_mr, summarize_issue, truncate_block,
    };
    use crate::test_util::mock_client;

    /// Unwrap the single user message every prompt returns.
    fn user_text(result: &GetPromptResult) -> &str {
        assert_eq!(result.messages.len(), 1, "expected one message");
        let msg = &result.messages[0];
        assert_eq!(msg.role, Role::User);
        msg.content.as_text().expect("text content").text.as_str()
    }

    #[tokio::test]
    async fn review_mr_embeds_mr_diff_and_threads() {
        let server = MockServer::start().await;
        // mr_get's closes/related embeds hit the mock's default 404 → [].
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/merge_requests/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "iid": 7,
                "title": "Add resources",
                "source_branch": "feat"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/merge_requests/7/diffs"))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "old_path": "src/lib.rs",
                "new_path": "src/lib.rs",
                "diff": "@@ -1 +1 @@\n-old line\n+new line\n"
            }])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/merge_requests/7/discussions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "id": "abc",
                "notes": [{"id": 1, "body": "please rename this", "resolved": false}]
            }])))
            .mount(&server)
            .await;

        let result = review_mr(
            &mock_client(&server),
            ReviewMrArgs {
                project_id: "42".into(),
                merge_request_iid: "7".into(),
            },
        )
        .await
        .unwrap();

        let text = user_text(&result);
        assert!(text.contains("Add resources"), "MR title embedded");
        assert!(text.contains("+new line"), "diff hunk embedded");
        assert!(text.contains("please rename this"), "thread embedded");
        assert!(
            result.description.as_deref().unwrap().contains("!7"),
            "description names the MR"
        );
    }

    #[tokio::test]
    async fn review_mr_rejects_non_numeric_iid_without_api_calls() {
        let server = MockServer::start().await;
        let err = review_mr(
            &mock_client(&server),
            ReviewMrArgs {
                project_id: "42".into(),
                merge_request_iid: "seven".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("merge_request_iid"));
    }

    #[tokio::test]
    async fn summarize_issue_includes_comments_but_not_system_notes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/issues/9"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "iid": 9,
                "title": "Crash on startup"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/issues/9/notes"))
            .and(query_param("sort", "asc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "body": "changed label to bug", "system": true},
                {"id": 2, "body": "reproduced on 18.2", "system": false}
            ])))
            .mount(&server)
            .await;

        let result = summarize_issue(
            &mock_client(&server),
            SummarizeIssueArgs {
                project_id: "42".into(),
                issue_iid: "9".into(),
            },
        )
        .await
        .unwrap();

        let text = user_text(&result);
        assert!(text.contains("Crash on startup"));
        assert!(text.contains("reproduced on 18.2"), "human comment kept");
        assert!(
            !text.contains("changed label to bug"),
            "system note filtered out"
        );
    }

    #[tokio::test]
    async fn create_mr_description_uses_explicit_target() {
        let server = MockServer::start().await;
        // No project_get mock: an explicit target must not fetch the project.
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/compare"))
            .and(query_param("from", "release"))
            .and(query_param("to", "feat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "commits": [{"short_id": "abc1234", "title": "Fix the frobnicator"}],
                "diffs": [{
                    "old_path": "a.rs", "new_path": "a.rs",
                    "diff": "@@ -1 +1 @@\n-x\n+y\n"
                }]
            })))
            .mount(&server)
            .await;

        let result = create_mr_description(
            &mock_client(&server),
            CreateMrDescriptionArgs {
                project_id: "42".into(),
                branch: "feat".into(),
                target_branch: Some("release".into()),
            },
        )
        .await
        .unwrap();

        let text = user_text(&result);
        assert!(
            text.contains("abc1234 Fix the frobnicator"),
            "commit listed"
        );
        assert!(text.contains("+y"), "diff embedded");
        assert!(text.contains("targeting \"release\""));
    }

    #[tokio::test]
    async fn create_mr_description_defaults_to_the_projects_default_branch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 42,
                "default_branch": "main"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/compare"))
            .and(query_param("from", "main"))
            .and(query_param("to", "feat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "commits": [],
                "diffs": []
            })))
            .mount(&server)
            .await;

        let result = create_mr_description(
            &mock_client(&server),
            CreateMrDescriptionArgs {
                project_id: "42".into(),
                branch: "feat".into(),
                target_branch: None,
            },
        )
        .await
        .unwrap();
        assert!(user_text(&result).contains("targeting \"main\""));
    }

    #[test]
    fn truncate_block_cuts_long_content_with_notice() {
        let long = "x".repeat(50);
        let cut = truncate_block(long, 10);
        assert!(cut.starts_with("xxxxxxxxxx"));
        assert!(cut.contains("truncated"));
        // Short content passes through untouched.
        assert_eq!(truncate_block("short".to_string(), 10), "short");
    }
}
