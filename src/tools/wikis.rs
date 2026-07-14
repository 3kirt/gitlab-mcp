//! Group wikis via the REST API (`/api/v4/groups/:id/wikis[/:slug]`).
//!
//! Group wikis are Premium/Ultimate. Pages are addressed by *slug* — a unique
//! string that may contain slashes (`dir/page_name`), so it is percent-encoded
//! into the URL via [`encode_path_segment`] like a file path. The attachment
//! upload endpoint (`POST …/wikis/attachments`) is not exposed: it takes a
//! multipart file upload, which the JSON-only [`GitlabClient`] doesn't speak.
//!
//! Comments on wiki pages are "notes" served by the Notes API, not this one.

use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{BodyBuilder, QueryBuilder, encode_path_segment, group_path};

/// `…/wikis/{slug}` — the page-scoped suffix shared by get, update, delete.
fn wiki_page_path(group_id: &str, slug: &str) -> String {
    format!(
        "{}/wikis/{}",
        group_path(group_id),
        encode_path_segment(slug)
    )
}

// --------------------------------------------------------------------------
// List wiki pages
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupWikisListParams {
    pub group_id: crate::tools::GroupId,
    #[schemars(description = "Include each page's content in the response (default: false)")]
    pub with_content: Option<bool>,
}

/// `GET /groups/:id/wikis` returns every page; the endpoint has no pagination.
pub async fn group_wikis_list(
    client: &GitlabClient,
    p: GroupWikisListParams,
) -> Result<Value, GitlabError> {
    let params = QueryBuilder::new()
        .opt("with_content", p.with_content)
        .into_params();
    client
        .get_with_params(&format!("{}/wikis", group_path(&p.group_id)), &params)
        .await
}

// --------------------------------------------------------------------------
// Get a wiki page
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupWikiGetParams {
    pub group_id: crate::tools::GroupId,
    #[schemars(
        description = "Slug of the wiki page (e.g. \"home\" or \"dir/page_name\"); slashes are encoded automatically"
    )]
    pub slug: String,
    #[schemars(description = "Return the page content rendered as HTML")]
    pub render_html: Option<bool>,
    #[schemars(description = "Wiki page version SHA to retrieve (default: latest)")]
    pub version: Option<String>,
}

pub async fn group_wiki_get(
    client: &GitlabClient,
    p: GroupWikiGetParams,
) -> Result<Value, GitlabError> {
    let params = QueryBuilder::new()
        .opt("render_html", p.render_html)
        .opt("version", p.version)
        .into_params();
    client
        .get_with_params(&wiki_page_path(&p.group_id, &p.slug), &params)
        .await
}

// --------------------------------------------------------------------------
// Create a wiki page
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupWikiCreateParams {
    pub group_id: crate::tools::GroupId,
    #[schemars(description = "Title of the wiki page (also determines the slug)")]
    pub title: String,
    #[schemars(description = "Content of the wiki page")]
    pub content: String,
    #[schemars(
        description = "Markup format: \"markdown\" (default), \"rdoc\", \"asciidoc\", or \"org\""
    )]
    pub format: Option<String>,
}

pub async fn group_wiki_create(
    client: &GitlabClient,
    p: GroupWikiCreateParams,
) -> Result<Value, GitlabError> {
    let body = BodyBuilder::new()
        .req("title", &p.title)
        .req("content", &p.content)
        .opt("format", p.format)
        .build();
    client
        .post(&format!("{}/wikis", group_path(&p.group_id)), &body)
        .await
}

// --------------------------------------------------------------------------
// Update a wiki page
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupWikiUpdateParams {
    pub group_id: crate::tools::GroupId,
    #[schemars(
        description = "Slug of the wiki page to update (e.g. \"home\" or \"dir/page_name\"); slashes are encoded automatically"
    )]
    pub slug: String,
    #[schemars(description = "New title (at least one of title/content is required)")]
    pub title: Option<String>,
    #[schemars(description = "New content (at least one of title/content is required)")]
    pub content: Option<String>,
    #[schemars(
        description = "Markup format: \"markdown\" (default), \"rdoc\", \"asciidoc\", or \"org\""
    )]
    pub format: Option<String>,
}

pub async fn group_wiki_update(
    client: &GitlabClient,
    p: GroupWikiUpdateParams,
) -> Result<Value, GitlabError> {
    let body = BodyBuilder::new()
        .opt("title", p.title)
        .opt("content", p.content)
        .opt("format", p.format)
        .build();
    client
        .put(&wiki_page_path(&p.group_id, &p.slug), &body)
        .await
}

// --------------------------------------------------------------------------
// Delete a wiki page
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupWikiDeleteParams {
    pub group_id: crate::tools::GroupId,
    #[schemars(
        description = "Slug of the wiki page to delete (e.g. \"home\" or \"dir/page_name\"); slashes are encoded automatically"
    )]
    pub slug: String,
}

pub async fn group_wiki_delete(
    client: &GitlabClient,
    p: GroupWikiDeleteParams,
) -> Result<(), GitlabError> {
    client.delete(&wiki_page_path(&p.group_id, &p.slug)).await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_wikis, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List all wiki pages (documentation pages) of a GitLab group wiki. Returns each page's slug, title, and format; pass with_content=true to include page contents. group_id accepts a numeric ID or full namespace path. Group wikis require Premium/Ultimate.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_group_wikis_list(
        &self,
        Parameters(p): Parameters<GroupWikisListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, group_wikis_list, p, "group wiki pages")
    }

    #[tool(
        description = "Get a single page of a GitLab group wiki by slug (the page's URL name, e.g. \"home\" or \"dir/page_name\"). Returns the page content, title, format, and slug. Optional: render_html (content as rendered HTML), version (a page version SHA for history). Group wikis require Premium/Ultimate.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_group_wikis_get(
        &self,
        Parameters(p): Parameters<GroupWikiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, group_wiki_get, p, "group wiki page")
    }

    #[tool(
        description = "Create a new page in a GitLab group wiki. Required: group_id, title (also becomes the slug), content. Optional: format (\"markdown\" default, \"rdoc\", \"asciidoc\", \"org\"). Group wikis require Premium/Ultimate.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_group_wikis_create(
        &self,
        Parameters(p): Parameters<GroupWikiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, group_wiki_create, p, "group wiki page")
    }

    #[tool(
        description = "Update (edit) a page of a GitLab group wiki by slug: change its content, retitle it, or switch format. Required: group_id, slug, and at least one of title/content. Note a title change also changes the page's slug. Group wikis require Premium/Ultimate.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_group_wikis_update(
        &self,
        Parameters(p): Parameters<GroupWikiUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, group_wiki_update, p, "group wiki page")
    }

    #[tool(
        description = "Delete a page from a GitLab group wiki by slug. Required: group_id, slug. This action is permanent (the page's git history remains in the wiki repository, but the page is removed). Group wikis require Premium/Ultimate.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_group_wikis_delete(
        &self,
        Parameters(p): Parameters<GroupWikiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, group_wiki_delete, p, "group wiki page")
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        GroupWikiCreateParams, GroupWikiDeleteParams, GroupWikiGetParams, GroupWikiUpdateParams,
        GroupWikisListParams, group_wiki_create, group_wiki_delete, group_wiki_get,
        group_wiki_update, group_wikis_list,
    };
    use crate::test_util::mock_client;

    #[tokio::test]
    async fn list_hits_wikis_url_with_content_param() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/1/wikis"))
            .and(query_param("with_content", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"slug": "home", "title": "home", "format": "markdown"}
            ])))
            .mount(&server)
            .await;

        let v = group_wikis_list(
            &mock_client(&server),
            GroupWikisListParams {
                group_id: "1".into(),
                with_content: Some(true),
            },
        )
        .await
        .unwrap();
        assert_eq!(v[0]["slug"], "home");
    }

    #[tokio::test]
    async fn get_encodes_slug_slashes_and_passes_version() {
        let server = MockServer::start().await;
        // "dir/page_name" must be one encoded path segment, not two segments.
        Mock::given(method("GET"))
            .and(path("/api/v4/groups/mygroup%2Fsub/wikis/dir%2Fpage_name"))
            .and(query_param("version", "abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "slug": "dir/page_name", "title": "page_name", "content": "hello"
            })))
            .mount(&server)
            .await;

        let v = group_wiki_get(
            &mock_client(&server),
            GroupWikiGetParams {
                group_id: "mygroup/sub".into(),
                slug: "dir/page_name".into(),
                render_html: None,
                version: Some("abc123".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(v["content"], "hello");
    }

    #[tokio::test]
    async fn create_posts_title_content_and_format() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/groups/1/wikis"))
            .and(body_json(json!({
                "title": "Hello", "content": "Hello world", "format": "rdoc"
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "slug": "Hello", "title": "Hello"
            })))
            .mount(&server)
            .await;

        let v = group_wiki_create(
            &mock_client(&server),
            GroupWikiCreateParams {
                group_id: "1".into(),
                title: "Hello".into(),
                content: "Hello world".into(),
                format: Some("rdoc".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(v["slug"], "Hello");
    }

    #[tokio::test]
    async fn update_puts_only_provided_fields() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v4/groups/1/wikis/foo"))
            .and(body_json(json!({ "content": "documentation" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "slug": "foo", "content": "documentation"
            })))
            .mount(&server)
            .await;

        let v = group_wiki_update(
            &mock_client(&server),
            GroupWikiUpdateParams {
                group_id: "1".into(),
                slug: "foo".into(),
                title: None,
                content: Some("documentation".into()),
                format: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(v["content"], "documentation");
    }

    #[tokio::test]
    async fn delete_hits_slug_url() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/groups/1/wikis/foo"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        group_wiki_delete(
            &mock_client(&server),
            GroupWikiDeleteParams {
                group_id: "1".into(),
                slug: "foo".into(),
            },
        )
        .await
        .unwrap();
    }
}
