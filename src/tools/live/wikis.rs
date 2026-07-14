//! Live tests for the Wikis domain (project wikis — `/projects/:id/wikis`).
//!
//! Group wikis run through the same shared CRUD helpers (`wikis.rs` only
//! swaps the scope prefix) but require a Premium/Ultimate *group*, which the
//! test environment doesn't provide — so the project family is the live
//! coverage for the shared code path, and the group prefix is pinned by the
//! wiremock unit tests.

use serde_json::Value;

use crate::client::GitlabError;
use crate::tools::{slim, wikis};

use super::harness::{LiveEnv, assert_nonempty_str, run_tag, skip_unless_live};

fn assert_wiki_page_invariants(page: &Value) {
    assert_nonempty_str(page, "slug");
    assert_nonempty_str(page, "title");
    assert_nonempty_str(page, "format");
}

async fn get_page_slimmed(env: &LiveEnv, slug: &str) -> Result<Value, GitlabError> {
    wikis::project_wiki_get(
        &env.client,
        wikis::ProjectWikiGetParams {
            project_id: env.project.clone().into(),
            slug: slug.to_string(),
            render_html: None,
            version: None,
        },
    )
    .await
    .map(slim::slim_get)
}

// --------------------------------------------------------------------------
// Create / get / list / update / delete lifecycle
// --------------------------------------------------------------------------

#[tokio::test]
async fn project_wiki_page_crud_lifecycle() {
    let env = skip_unless_live!();
    let tag = run_tag();
    // A slash in the title creates a page nested in a directory, so the slug
    // itself contains a slash — exercising encode_path_segment against the
    // real API on every subsequent get/update/delete.
    let title = format!("{tag}/page");

    // Create.
    let created = slim::slim_get(
        wikis::project_wiki_create(
            &env.client,
            wikis::ProjectWikiCreateParams {
                project_id: env.project.clone().into(),
                title: title.clone(),
                content: "v1 wiki content\n".into(),
                format: Some("markdown".into()),
            },
        )
        .await
        .expect("project_wiki_create"),
    );
    assert_wiki_page_invariants(&created);
    assert_eq!(created["format"], "markdown");
    let slug = created["slug"]
        .as_str()
        .expect("created page slug")
        .to_string();
    assert!(
        slug.contains('/'),
        "slash title must produce a nested slug, got {slug:?}"
    );

    // Get by (slash-carrying) slug.
    let got = get_page_slimmed(&env, &slug)
        .await
        .expect("project_wiki_get");
    assert_wiki_page_invariants(&got);
    assert_eq!(got["content"], "v1 wiki content\n");

    // List with content: our page is present and carries its content.
    let pages = wikis::project_wikis_list(
        &env.client,
        wikis::ProjectWikisListParams {
            project_id: env.project.clone().into(),
            with_content: Some(true),
        },
    )
    .await
    .expect("project_wikis_list");
    let ours = pages
        .as_array()
        .expect("wiki list is an array")
        .iter()
        .find(|p| p["slug"].as_str() == Some(slug.as_str()))
        .unwrap_or_else(|| panic!("page {slug:?} missing from wiki list"))
        .clone();
    assert_eq!(ours["content"], "v1 wiki content\n");

    // Update, passing the full path-style title alongside the content.
    // Passing the title is load-bearing: a content-only update of a nested
    // page makes GitLab re-derive the title from the slug's last segment and
    // *move the page to the wiki root* (observed live on gitlab.com: updating
    // "dir/leaf" without a title relocated it to slug "leaf"). The tool
    // descriptions warn callers about this.
    let updated = slim::slim_get(
        wikis::project_wiki_update(
            &env.client,
            wikis::ProjectWikiUpdateParams {
                project_id: env.project.clone().into(),
                slug: slug.clone(),
                title: Some(title.clone()),
                content: Some("v2 wiki content\n".into()),
                format: None,
            },
        )
        .await
        .expect("project_wiki_update"),
    );
    assert_wiki_page_invariants(&updated);
    assert_eq!(
        updated["slug"].as_str(),
        Some(slug.as_str()),
        "update with the full title must keep the page in place"
    );
    let got = get_page_slimmed(&env, &slug)
        .await
        .expect("get after update");
    assert_eq!(got["content"], "v2 wiki content\n");

    // Delete; a follow-up get must 404.
    wikis::project_wiki_delete(
        &env.client,
        wikis::ProjectWikiDeleteParams {
            project_id: env.project.clone().into(),
            slug: slug.clone(),
        },
    )
    .await
    .expect("project_wiki_delete");
    let err = get_page_slimmed(&env, &slug)
        .await
        .expect_err("get after delete must 404");
    assert!(
        matches!(err, GitlabError::Api { status, .. } if status.as_u16() == 404),
        "expected 404 after delete, got {err:?}"
    );
}
