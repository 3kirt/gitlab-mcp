use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError, ListResult};
use crate::tools::{BodyBuilder, PaginationParams, QueryBuilder, encode_namespace_id};

// --------------------------------------------------------------------------
// Issues
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issue_emoji_list(client: &GitlabClient, p: IssueEmojiListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_emoji_get(
    client: &GitlabClient,
    p: IssueEmojiGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.award_id
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn issue_emoji_create(
    client: &GitlabClient,
    p: IssueEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.issue_iid
    );
    let body = BodyBuilder::new().req("name", &p.name).build();
    client.post(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_emoji_delete(
    client: &GitlabClient,
    p: IssueEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.award_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Merge Requests
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mr_emoji_list(client: &GitlabClient, p: MrEmojiListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_emoji_get(
    client: &GitlabClient,
    p: MrEmojiGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid,
        p.award_id
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn mr_emoji_create(
    client: &GitlabClient,
    p: MrEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid
    );
    let body = BodyBuilder::new().req("name", &p.name).build();
    client.post(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_emoji_delete(
    client: &GitlabClient,
    p: MrEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid,
        p.award_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Snippets
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn snippet_emoji_list(client: &GitlabClient, p: SnippetEmojiListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.snippet_id
    );
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_emoji_get(
    client: &GitlabClient,
    p: SnippetEmojiGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.snippet_id,
        p.award_id
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn snippet_emoji_create(
    client: &GitlabClient,
    p: SnippetEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.snippet_id
    );
    let body = BodyBuilder::new().req("name", &p.name).build();
    client.post(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_emoji_delete(
    client: &GitlabClient,
    p: SnippetEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.snippet_id,
        p.award_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Issue Notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn issue_note_emoji_list(
    client: &GitlabClient,
    p: IssueNoteEmojiListParams,
) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.note_id
    );
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_note_emoji_get(
    client: &GitlabClient,
    p: IssueNoteEmojiGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.note_id,
        p.award_id
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn issue_note_emoji_create(
    client: &GitlabClient,
    p: IssueNoteEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.note_id
    );
    let body = BodyBuilder::new().req("name", &p.name).build();
    client.post(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueNoteEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Issue internal ID (IID) within the project")]
    pub issue_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn issue_note_emoji_delete(
    client: &GitlabClient,
    p: IssueNoteEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/issues/{}/notes/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.issue_iid,
        p.note_id,
        p.award_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Merge Request Notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn mr_note_emoji_list(client: &GitlabClient, p: MrNoteEmojiListParams) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/notes/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid,
        p.note_id
    );
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_note_emoji_get(
    client: &GitlabClient,
    p: MrNoteEmojiGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/notes/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid,
        p.note_id,
        p.award_id
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn mr_note_emoji_create(
    client: &GitlabClient,
    p: MrNoteEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/notes/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid,
        p.note_id
    );
    let body = BodyBuilder::new().req("name", &p.name).build();
    client.post(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MrNoteEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Merge request internal ID (IID) within the project")]
    pub merge_request_iid: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn mr_note_emoji_delete(
    client: &GitlabClient,
    p: MrNoteEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/notes/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.merge_request_iid,
        p.note_id,
        p.award_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Snippet Notes
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiListParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub async fn snippet_note_emoji_list(
    client: &GitlabClient,
    p: SnippetNoteEmojiListParams,
) -> ListResult {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/notes/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.snippet_id,
        p.note_id
    );
    let params = QueryBuilder::new()
        .opt("page", p.pagination.page)
        .opt("per_page", p.pagination.per_page)
        .into_params();
    client.list(&path, &params).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiGetParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_note_emoji_get(
    client: &GitlabClient,
    p: SnippetNoteEmojiGetParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/notes/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.snippet_id,
        p.note_id,
        p.award_id
    );
    client.get(&path).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiCreateParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Emoji name without colons (e.g. \"thumbsup\")")]
    pub name: String,
}

pub async fn snippet_note_emoji_create(
    client: &GitlabClient,
    p: SnippetNoteEmojiCreateParams,
) -> Result<Value, GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/notes/{}/award_emoji",
        encode_namespace_id(&p.project_id),
        p.snippet_id,
        p.note_id
    );
    let body = BodyBuilder::new().req("name", &p.name).build();
    client.post(&path, &body).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnippetNoteEmojiDeleteParams {
    #[schemars(description = "Project ID or URL-encoded path")]
    pub project_id: String,
    #[schemars(description = "Snippet ID")]
    pub snippet_id: u64,
    #[schemars(description = "Note ID")]
    pub note_id: u64,
    #[schemars(description = "Award emoji ID")]
    pub award_id: u64,
}

pub async fn snippet_note_emoji_delete(
    client: &GitlabClient,
    p: SnippetNoteEmojiDeleteParams,
) -> Result<(), GitlabError> {
    let path = format!(
        "/api/v4/projects/{}/snippets/{}/notes/{}/award_emoji/{}",
        encode_namespace_id(&p.project_id),
        p.snippet_id,
        p.note_id,
        p.award_id
    );
    client.delete(&path).await
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------
//
// These tests cover URL routing across the six emoji-reaction families. The
// real failure mode this guards against is path-template mix-ups (e.g. the
// issue family accidentally hitting the merge_requests URL after a refactor),
// so the assertions are deliberately path-shape focused. One representative
// from each family + the deepest nested family (issue notes) is enough; the
// other families share identical structure.

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        IssueEmojiCreateParams, IssueNoteEmojiCreateParams, MrEmojiDeleteParams,
        SnippetEmojiListParams, issue_emoji_create, issue_note_emoji_create, mr_emoji_delete,
        snippet_emoji_list,
    };
    use crate::client::GitlabClient;
    use crate::tools::PaginationParams;

    fn mock_client(server: &MockServer) -> GitlabClient {
        GitlabClient::new(server.uri(), "test-token").unwrap()
    }

    #[tokio::test]
    async fn issue_emoji_create_hits_issue_award_emoji_url_with_name_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/issues/7/award_emoji"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 1, "name": "thumbsup"
            })))
            .mount(&server)
            .await;

        let item = issue_emoji_create(
            &mock_client(&server),
            IssueEmojiCreateParams {
                project_id: "42".into(),
                issue_iid: 7,
                name: "thumbsup".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(item["name"], "thumbsup");

        let reqs = server.received_requests().await.unwrap();
        let body = reqs
            .iter()
            .find(|r| r.method == wiremock::http::Method::POST)
            .and_then(|r| r.body_json::<serde_json::Value>().ok())
            .expect("POST request not found");
        assert_eq!(body["name"], "thumbsup");
    }

    #[tokio::test]
    async fn mr_emoji_delete_hits_merge_requests_award_emoji_url() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/42/merge_requests/3/award_emoji/99"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        mr_emoji_delete(
            &mock_client(&server),
            MrEmojiDeleteParams {
                project_id: "42".into(),
                merge_request_iid: 3,
                award_id: 99,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn snippet_emoji_list_hits_snippets_award_emoji_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/snippets/5/award_emoji"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        snippet_emoji_list(
            &mock_client(&server),
            SnippetEmojiListParams {
                project_id: "42".into(),
                snippet_id: 5,
                pagination: PaginationParams {
                    page: None,
                    per_page: None,
                },
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn issue_note_emoji_create_hits_nested_notes_award_emoji_url() {
        // Deepest-nested family — the easiest to break by miswriting the path template.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/issues/7/notes/11/award_emoji"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 1, "name": "tada"
            })))
            .mount(&server)
            .await;

        issue_note_emoji_create(
            &mock_client(&server),
            IssueNoteEmojiCreateParams {
                project_id: "42".into(),
                issue_iid: 7,
                note_id: 11,
                name: "tada".into(),
            },
        )
        .await
        .unwrap();
    }
}
