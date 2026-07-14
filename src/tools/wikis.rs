//! Project and group wikis via the REST API
//! (`/api/v4/projects/:id/wikis[/:slug]` and `/api/v4/groups/:id/wikis[/:slug]`).
//!
//! The two APIs are shape-identical, differing only in the scope prefix, so
//! the CRUD helpers here take the scope path and the per-scope `*Params`
//! structs stay thin (cf. `emoji_reactions.rs`). Project wikis are available
//! on every tier; group wikis are Premium/Ultimate.
//!
//! Pages are addressed by *slug* — a unique string that may contain slashes
//! (`dir/page_name`), so it is percent-encoded into the URL via
//! [`encode_path_segment`] like a file path. The attachment upload endpoints
//! (`POST …/wikis/attachments`) are not exposed: they take a multipart file
//! upload, which the JSON-only [`GitlabClient`] doesn't speak.
//!
//! Comments on wiki pages are "notes" served by the Notes API, not this one.

use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{BodyBuilder, QueryBuilder, encode_path_segment, group_path, project_path};

// --------------------------------------------------------------------------
// Shared CRUD helpers
//
// `scope` is the `/api/v4/projects/{id}` or `/api/v4/groups/{id}` prefix the
// wiki hangs off; everything after it is identical for both APIs.
// --------------------------------------------------------------------------

/// `{scope}/wikis/{slug}` — the page URL shared by get, update, delete.
fn wiki_page_path(scope: &str, slug: &str) -> String {
    format!("{scope}/wikis/{}", encode_path_segment(slug))
}

/// `GET {scope}/wikis` returns every page; the endpoint has no pagination.
async fn wikis_list(
    client: &GitlabClient,
    scope: &str,
    with_content: Option<bool>,
) -> Result<Value, GitlabError> {
    let params = QueryBuilder::new()
        .opt("with_content", with_content)
        .into_params();
    client
        .get_with_params(&format!("{scope}/wikis"), &params)
        .await
}

async fn wiki_get(
    client: &GitlabClient,
    scope: &str,
    slug: &str,
    render_html: Option<bool>,
    version: Option<String>,
) -> Result<Value, GitlabError> {
    let params = QueryBuilder::new()
        .opt("render_html", render_html)
        .opt("version", version)
        .into_params();
    client
        .get_with_params(&wiki_page_path(scope, slug), &params)
        .await
}

async fn wiki_create(
    client: &GitlabClient,
    scope: &str,
    title: &str,
    content: &str,
    format: Option<String>,
) -> Result<Value, GitlabError> {
    let body = BodyBuilder::new()
        .req("title", title)
        .req("content", content)
        .opt("format", format)
        .build();
    client.post(&format!("{scope}/wikis"), &body).await
}

async fn wiki_update(
    client: &GitlabClient,
    scope: &str,
    slug: &str,
    title: Option<String>,
    content: Option<String>,
    format: Option<String>,
) -> Result<Value, GitlabError> {
    let body = BodyBuilder::new()
        .opt("title", title)
        .opt("content", content)
        .opt("format", format)
        .build();
    client.put(&wiki_page_path(scope, slug), &body).await
}

async fn wiki_delete(client: &GitlabClient, scope: &str, slug: &str) -> Result<(), GitlabError> {
    client.delete(&wiki_page_path(scope, slug)).await
}

// --------------------------------------------------------------------------
// Project wikis
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectWikisListParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Include each page's content in the response (default: false)")]
    pub with_content: Option<bool>,
}

pub async fn project_wikis_list(
    client: &GitlabClient,
    p: ProjectWikisListParams,
) -> Result<Value, GitlabError> {
    wikis_list(client, &project_path(&p.project_id), p.with_content).await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectWikiGetParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Slug of the wiki page (e.g. \"home\" or \"dir/page_name\"); slashes are encoded automatically"
    )]
    pub slug: String,
    #[schemars(description = "Return the page content rendered as HTML")]
    pub render_html: Option<bool>,
    #[schemars(description = "Wiki page version SHA to retrieve (default: latest)")]
    pub version: Option<String>,
}

pub async fn project_wiki_get(
    client: &GitlabClient,
    p: ProjectWikiGetParams,
) -> Result<Value, GitlabError> {
    wiki_get(
        client,
        &project_path(&p.project_id),
        &p.slug,
        p.render_html,
        p.version,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectWikiCreateParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(description = "Title of the wiki page (also determines the slug)")]
    pub title: String,
    #[schemars(description = "Content of the wiki page")]
    pub content: String,
    #[schemars(
        description = "Markup format: \"markdown\" (default), \"rdoc\", \"asciidoc\", or \"org\""
    )]
    pub format: Option<String>,
}

pub async fn project_wiki_create(
    client: &GitlabClient,
    p: ProjectWikiCreateParams,
) -> Result<Value, GitlabError> {
    wiki_create(
        client,
        &project_path(&p.project_id),
        &p.title,
        &p.content,
        p.format,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectWikiUpdateParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Slug of the wiki page to update (e.g. \"home\" or \"dir/page_name\"); slashes are encoded automatically"
    )]
    pub slug: String,
    #[schemars(
        description = "New title (at least one of title/content is required). For a page in a directory, pass the full path-style title (e.g. \"dir/page_name\") — omitting the title moves the page to the wiki root."
    )]
    pub title: Option<String>,
    #[schemars(description = "New content (at least one of title/content is required)")]
    pub content: Option<String>,
    #[schemars(
        description = "Markup format: \"markdown\" (default), \"rdoc\", \"asciidoc\", or \"org\""
    )]
    pub format: Option<String>,
}

pub async fn project_wiki_update(
    client: &GitlabClient,
    p: ProjectWikiUpdateParams,
) -> Result<Value, GitlabError> {
    wiki_update(
        client,
        &project_path(&p.project_id),
        &p.slug,
        p.title,
        p.content,
        p.format,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectWikiDeleteParams {
    pub project_id: crate::tools::ProjectId,
    #[schemars(
        description = "Slug of the wiki page to delete (e.g. \"home\" or \"dir/page_name\"); slashes are encoded automatically"
    )]
    pub slug: String,
}

pub async fn project_wiki_delete(
    client: &GitlabClient,
    p: ProjectWikiDeleteParams,
) -> Result<(), GitlabError> {
    wiki_delete(client, &project_path(&p.project_id), &p.slug).await
}

// --------------------------------------------------------------------------
// Group wikis
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupWikisListParams {
    pub group_id: crate::tools::GroupId,
    #[schemars(description = "Include each page's content in the response (default: false)")]
    pub with_content: Option<bool>,
}

pub async fn group_wikis_list(
    client: &GitlabClient,
    p: GroupWikisListParams,
) -> Result<Value, GitlabError> {
    wikis_list(client, &group_path(&p.group_id), p.with_content).await
}

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
    wiki_get(
        client,
        &group_path(&p.group_id),
        &p.slug,
        p.render_html,
        p.version,
    )
    .await
}

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
    wiki_create(
        client,
        &group_path(&p.group_id),
        &p.title,
        &p.content,
        p.format,
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GroupWikiUpdateParams {
    pub group_id: crate::tools::GroupId,
    #[schemars(
        description = "Slug of the wiki page to update (e.g. \"home\" or \"dir/page_name\"); slashes are encoded automatically"
    )]
    pub slug: String,
    #[schemars(
        description = "New title (at least one of title/content is required). For a page in a directory, pass the full path-style title (e.g. \"dir/page_name\") — omitting the title moves the page to the wiki root."
    )]
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
    wiki_update(
        client,
        &group_path(&p.group_id),
        &p.slug,
        p.title,
        p.content,
        p.format,
    )
    .await
}

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
    wiki_delete(client, &group_path(&p.group_id), &p.slug).await
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
        description = "List all wiki pages (documentation pages) of a GitLab project wiki. Returns each page's slug, title, and format; pass with_content=true to include page contents. project_id accepts a numeric ID or namespace path. For a group-level wiki, use gitlab_group_wikis_list.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_project_wikis_list(
        &self,
        Parameters(p): Parameters<ProjectWikisListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, project_wikis_list, p, "project wiki pages")
    }

    #[tool(
        description = "Get a single page of a GitLab project wiki by slug (the page's URL name, e.g. \"home\" or \"dir/page_name\"). Returns the page content, title, format, and slug. Optional: render_html (content as rendered HTML), version (a page version SHA for history). For a group-level wiki, use gitlab_group_wikis_get.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_project_wikis_get(
        &self,
        Parameters(p): Parameters<ProjectWikiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, project_wiki_get, p, "project wiki page")
    }

    #[tool(
        description = "Create a new page in a GitLab project wiki. Required: project_id, title (also becomes the slug), content. Optional: format (\"markdown\" default, \"rdoc\", \"asciidoc\", \"org\"). For a group-level wiki, use gitlab_group_wikis_create.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_project_wikis_create(
        &self,
        Parameters(p): Parameters<ProjectWikiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, project_wiki_create, p, "project wiki page")
    }

    #[tool(
        description = "Update (edit) a page of a GitLab project wiki by slug: change its content, retitle it, or switch format. Required: project_id, slug, and at least one of title/content. Note a title change also changes the page's slug, and for a page in a directory (slug with slashes) the full path-style title must be passed with every update — omitting it moves the page to the wiki root. For a group-level wiki, use gitlab_group_wikis_update.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_project_wikis_update(
        &self,
        Parameters(p): Parameters<ProjectWikiUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, project_wiki_update, p, "project wiki page")
    }

    #[tool(
        description = "Delete a page from a GitLab project wiki by slug. Required: project_id, slug. This action is permanent (the page's git history remains in the wiki repository, but the page is removed). For a group-level wiki, use gitlab_group_wikis_delete.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_project_wikis_delete(
        &self,
        Parameters(p): Parameters<ProjectWikiDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, project_wiki_delete, p, "project wiki page")
    }

    #[tool(
        description = "List all wiki pages (documentation pages) of a GitLab group wiki. Returns each page's slug, title, and format; pass with_content=true to include page contents. group_id accepts a numeric ID or full namespace path. Group wikis require Premium/Ultimate; for a project's wiki, use gitlab_project_wikis_list.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_group_wikis_list(
        &self,
        Parameters(p): Parameters<GroupWikisListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, group_wikis_list, p, "group wiki pages")
    }

    #[tool(
        description = "Get a single page of a GitLab group wiki by slug (the page's URL name, e.g. \"home\" or \"dir/page_name\"). Returns the page content, title, format, and slug. Optional: render_html (content as rendered HTML), version (a page version SHA for history). Group wikis require Premium/Ultimate; for a project's wiki, use gitlab_project_wikis_get.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_group_wikis_get(
        &self,
        Parameters(p): Parameters<GroupWikiGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, group_wiki_get, p, "group wiki page")
    }

    #[tool(
        description = "Create a new page in a GitLab group wiki. Required: group_id, title (also becomes the slug), content. Optional: format (\"markdown\" default, \"rdoc\", \"asciidoc\", \"org\"). Group wikis require Premium/Ultimate; for a project's wiki, use gitlab_project_wikis_create.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_group_wikis_create(
        &self,
        Parameters(p): Parameters<GroupWikiCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, group_wiki_create, p, "group wiki page")
    }

    #[tool(
        description = "Update (edit) a page of a GitLab group wiki by slug: change its content, retitle it, or switch format. Required: group_id, slug, and at least one of title/content. Note a title change also changes the page's slug, and for a page in a directory (slug with slashes) the full path-style title must be passed with every update — omitting it moves the page to the wiki root. Group wikis require Premium/Ultimate; for a project's wiki, use gitlab_project_wikis_update.",
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
        description = "Delete a page from a GitLab group wiki by slug. Required: group_id, slug. This action is permanent (the page's git history remains in the wiki repository, but the page is removed). Group wikis require Premium/Ultimate; for a project's wiki, use gitlab_project_wikis_delete.",
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
//
// The two scope families share the CRUD helpers, so the failure mode to guard
// is prefix mix-ups (a project call hitting /groups/… or vice versa) and slug
// encoding. Each family therefore pins the URL prefix for all five verbs;
// query/body *assembly* is covered once per verb, since it lives in the shared
// helpers and is scope-independent.

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        GroupWikiCreateParams, GroupWikiDeleteParams, GroupWikiGetParams, GroupWikiUpdateParams,
        GroupWikisListParams, ProjectWikiCreateParams, ProjectWikiDeleteParams,
        ProjectWikiGetParams, ProjectWikiUpdateParams, ProjectWikisListParams, group_wiki_create,
        group_wiki_delete, group_wiki_get, group_wiki_update, group_wikis_list,
        project_wiki_create, project_wiki_delete, project_wiki_get, project_wiki_update,
        project_wikis_list,
    };
    use crate::test_util::mock_client;

    #[tokio::test]
    async fn project_list_hits_projects_wikis_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/wikis"))
            .and(query_param("with_content", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"slug": "home", "title": "home", "format": "markdown"}
            ])))
            .mount(&server)
            .await;

        let v = project_wikis_list(
            &mock_client(&server),
            ProjectWikisListParams {
                project_id: "42".into(),
                with_content: Some(true),
            },
        )
        .await
        .unwrap();
        assert_eq!(v[0]["slug"], "home");
    }

    #[tokio::test]
    async fn project_get_encodes_slug_and_namespace_path() {
        let server = MockServer::start().await;
        // Both the namespace-path project ID and the slug's slash must each be
        // one encoded segment.
        Mock::given(method("GET"))
            .and(path(
                "/api/v4/projects/mygroup%2Fproj/wikis/dir%2Fpage_name",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "slug": "dir/page_name", "content": "hello"
            })))
            .mount(&server)
            .await;

        let v = project_wiki_get(
            &mock_client(&server),
            ProjectWikiGetParams {
                project_id: "mygroup/proj".into(),
                slug: "dir/page_name".into(),
                render_html: None,
                version: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(v["content"], "hello");
    }

    #[tokio::test]
    async fn project_create_posts_to_projects_wikis_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/projects/42/wikis"))
            .and(body_json(json!({ "title": "Hello", "content": "hi" })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "slug": "Hello", "title": "Hello"
            })))
            .mount(&server)
            .await;

        let v = project_wiki_create(
            &mock_client(&server),
            ProjectWikiCreateParams {
                project_id: "42".into(),
                title: "Hello".into(),
                content: "hi".into(),
                format: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(v["slug"], "Hello");
    }

    #[tokio::test]
    async fn project_update_puts_to_projects_slug_url() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v4/projects/42/wikis/foo"))
            .and(body_json(json!({ "content": "v2" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "slug": "foo", "content": "v2"
            })))
            .mount(&server)
            .await;

        let v = project_wiki_update(
            &mock_client(&server),
            ProjectWikiUpdateParams {
                project_id: "42".into(),
                slug: "foo".into(),
                title: None,
                content: Some("v2".into()),
                format: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(v["content"], "v2");
    }

    #[tokio::test]
    async fn project_delete_hits_projects_slug_url() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v4/projects/42/wikis/foo"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        project_wiki_delete(
            &mock_client(&server),
            ProjectWikiDeleteParams {
                project_id: "42".into(),
                slug: "foo".into(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn group_list_hits_groups_wikis_url() {
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
    async fn group_get_encodes_slug_slashes_and_passes_version() {
        let server = MockServer::start().await;
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
    async fn group_delete_hits_groups_slug_url() {
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
