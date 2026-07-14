//! MCP Resources: read-only GitLab data addressable by `gitlab://` URI.
//!
//! Unlike tools (model-driven), resources are application-driven: the client
//! can pre-load them as context without the model making a tool call. This
//! module owns the resource *templates* the server advertises, the URI parser,
//! and the read dispatcher that maps a parsed URI onto the existing domain
//! functions. The `ServerHandler` methods in `mod.rs` are thin wrappers so
//! everything here stays testable against a plain `GitlabClient`.
//!
//! URI scheme: `gitlab://{project_id}/<kind>/...`. Per RFC 6570 simple
//! expansion, a client filling `{project_id}` with a namespace path
//! (`mygroup/myproject`) percent-encodes the slash (`mygroup%2Fmyproject`),
//! which is what keeps the segments unambiguous; the parser percent-decodes
//! each segment. The one deliberate liberty: everything after `/files/` is
//! taken greedily as the file path, so file URIs work whether the client
//! encoded the path's slashes or left them literal.

use base64::Engine as _;
use percent_encoding::percent_decode_str;
use rmcp::model::{Resource, ResourceContents, ResourceTemplate};
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};
use crate::tools::{
    encode_namespace_id, issues, merge_requests, pipelines, projects, recent_member_projects,
    repository_files, slim,
};

/// The resource templates advertised via `resources/templates/list`, one per
/// single-get tool domain that makes sense to pre-load as context.
pub fn resource_templates() -> Vec<ResourceTemplate> {
    let id_note = "{project_id} is a numeric ID or percent-encoded namespace path \
                   (e.g. mygroup%2Fmyproject).";
    vec![
        ResourceTemplate::new("gitlab://{project_id}", "gitlab-project")
            .with_title("Project overview")
            .with_description(format!(
                "A GitLab project (repository) as JSON: description, default branch, \
                 visibility, web URL, and feature settings. The entry point for the other \
                 gitlab:// resources of the same project. {id_note}"
            ))
            .with_mime_type("application/json"),
        ResourceTemplate::new(
            "gitlab://{project_id}/files/{file_path}{?ref}",
            "gitlab-file",
        )
        .with_title("Repository file")
        .with_description(format!(
            "Content of a file in a GitLab repository, decoded (text files are returned \
                 as-is, binary files as base64). Optional ?ref= selects a branch, tag, or \
                 commit SHA (default: HEAD of the default branch). {id_note}"
        )),
        ResourceTemplate::new("gitlab://{project_id}/issues/{issue_iid}", "gitlab-issue")
            .with_title("Issue")
            .with_description(format!(
                "A GitLab issue (bug report / ticket) as JSON, including linked issues and \
                 the merge requests that close it. {{issue_iid}} is the number shown in the \
                 GitLab UI. {id_note}"
            ))
            .with_mime_type("application/json"),
        ResourceTemplate::new(
            "gitlab://{project_id}/mrs/{merge_request_iid}",
            "gitlab-merge-request",
        )
        .with_title("Merge request")
        .with_description(format!(
            "A GitLab merge request (pull request) as JSON, including the issues it \
                 closes and related issues. {{merge_request_iid}} is the number shown in the \
                 GitLab UI. {id_note}"
        ))
        .with_mime_type("application/json"),
        ResourceTemplate::new(
            "gitlab://{project_id}/pipelines/{pipeline_id}",
            "gitlab-pipeline",
        )
        .with_title("Pipeline")
        .with_description(format!(
            "A GitLab CI/CD pipeline (build/test run) as JSON. {{pipeline_id}} is the \
                 globally unique pipeline ID. {id_note}"
        ))
        .with_mime_type("application/json"),
    ]
}

/// A parsed `gitlab://` resource URI.
#[derive(Debug, PartialEq, Eq)]
pub enum ResourceRef {
    Project {
        project_id: String,
    },
    File {
        project_id: String,
        file_path: String,
        ref_name: Option<String>,
    },
    Issue {
        project_id: String,
        issue_iid: u64,
    },
    MergeRequest {
        project_id: String,
        merge_request_iid: u64,
    },
    Pipeline {
        project_id: String,
        pipeline_id: u64,
    },
}

fn decode(segment: &str) -> Result<String, String> {
    percent_decode_str(segment)
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .map_err(|_| format!("\"{segment}\" is not valid percent-encoded UTF-8"))
}

/// Parse a `gitlab://` URI into a [`ResourceRef`]. The error is a human-readable
/// reason suitable for a `resource_not_found` message.
pub fn parse_uri(uri: &str) -> Result<ResourceRef, String> {
    let rest = uri
        .strip_prefix("gitlab://")
        .ok_or_else(|| "expected a gitlab:// URI".to_string())?;
    let (path, query) = rest
        .split_once('?')
        .map_or((rest, None), |(p, q)| (p, Some(q)));
    // Tolerate one trailing slash on any kind (URI-normalizing clients add
    // them); a file path's own trailing slash would be invalid anyway.
    let path = path.strip_suffix('/').unwrap_or(path);

    let mut segments = path.split('/');
    let project_seg = segments
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "missing project ID".to_string())?;
    let project_id = decode(project_seg)?;
    // No kind segment → the project itself.
    let Some(kind) = segments.next() else {
        reject_query(query, "project")?;
        return Ok(ResourceRef::Project { project_id });
    };
    if kind.is_empty() {
        return Err("empty resource kind after the project ID".to_string());
    }

    // `files` takes the rest of the path greedily; the numeric kinds take
    // exactly one trailing segment.
    if kind == "files" {
        let raw: Vec<&str> = segments.collect();
        let raw = raw.join("/");
        if raw.is_empty() {
            return Err("missing file path".to_string());
        }
        return Ok(ResourceRef::File {
            project_id,
            file_path: decode(&raw)?,
            ref_name: query_param(query, "ref")?,
        });
    }

    if !matches!(kind, "issues" | "mrs" | "pipelines") {
        return Err(format!(
            "unknown resource kind \"{kind}\" (expected files, issues, mrs, or pipelines)"
        ));
    }
    let id_seg = segments
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing identifier after \"{kind}\""))?;
    if segments.next().is_some() {
        return Err(format!(
            "unexpected path segments after \"{kind}/{id_seg}\""
        ));
    }
    let id: u64 = id_seg
        .parse()
        .map_err(|_| format!("\"{id_seg}\" is not a numeric identifier"))?;
    reject_query(query, kind)?;

    match kind {
        "issues" => Ok(ResourceRef::Issue {
            project_id,
            issue_iid: id,
        }),
        "mrs" => Ok(ResourceRef::MergeRequest {
            project_id,
            merge_request_iid: id,
        }),
        // Exhaustive per the membership check above.
        _ => Ok(ResourceRef::Pipeline {
            project_id,
            pipeline_id: id,
        }),
    }
}

/// Only file resources take query parameters (`?ref=`); rejecting a query on
/// every other kind beats silently ignoring it — a caller who wrote
/// `gitlab://proj?ref=x` should learn the parameter did nothing.
fn reject_query(query: Option<&str>, kind: &str) -> Result<(), String> {
    match query {
        Some(q) if !q.is_empty() => Err(format!(
            "{kind} resources take no query parameters (got \"?{q}\")"
        )),
        _ => Ok(()),
    }
}

/// Extract and decode one query parameter; unknown parameters are ignored.
fn query_param(query: Option<&str>, name: &str) -> Result<Option<String>, String> {
    let Some(query) = query else { return Ok(None) };
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == name {
            return decode(v).map(Some);
        }
    }
    Ok(None)
}

/// Fetch the content for a parsed resource URI. `uri` is echoed back in the
/// returned contents, as the MCP spec requires.
pub async fn read(
    client: &GitlabClient,
    resource: ResourceRef,
    uri: &str,
) -> Result<Vec<ResourceContents>, GitlabError> {
    let contents = match resource {
        ResourceRef::Project { project_id } => {
            let v = projects::project_get(
                client,
                projects::ProjectGetParams {
                    project_id: project_id.into(),
                    statistics: None,
                },
            )
            .await?;
            json_contents(v, uri)?
        }
        ResourceRef::File {
            project_id,
            file_path,
            ref_name,
        } => {
            let v = repository_files::file_get(
                client,
                repository_files::FileGetParams {
                    project_id: project_id.into(),
                    file_path: file_path.clone(),
                    ref_name: ref_name.unwrap_or_else(|| "HEAD".to_string()),
                },
            )
            .await?;
            file_contents(&v, &file_path, uri)?
        }
        ResourceRef::Issue {
            project_id,
            issue_iid,
        } => {
            let v = issues::issue_get(
                client,
                issues::IssueGetParams {
                    project_id: project_id.into(),
                    issue_iid: issue_iid.into(),
                },
            )
            .await?;
            json_contents(v, uri)?
        }
        ResourceRef::MergeRequest {
            project_id,
            merge_request_iid,
        } => {
            let v = merge_requests::mr_get(
                client,
                merge_requests::MrGetParams {
                    project_id: project_id.into(),
                    merge_request_iid: merge_request_iid.into(),
                },
            )
            .await?;
            json_contents(v, uri)?
        }
        ResourceRef::Pipeline {
            project_id,
            pipeline_id,
        } => {
            let v = pipelines::pipeline_get(
                client,
                pipelines::PipelineGetParams {
                    project_id: project_id.into(),
                    pipeline_id,
                },
            )
            .await?;
            json_contents(v, uri)?
        }
    };
    Ok(vec![contents])
}

/// The concrete resources for `resources/list`: the caller's most recently
/// active member projects, as `gitlab://{project_id}` project resources. The
/// full resource space isn't enumerable, but "which projects matter" is the
/// shared [`recent_member_projects`] query the `project_id` completer also
/// uses, and listing them gives client resource pickers real entries instead
/// of an empty list.
pub async fn list_recent_projects(client: &GitlabClient) -> Result<Vec<Resource>, GitlabError> {
    let projects = recent_member_projects(client, None).await?;
    let resources = projects
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let path = p["path_with_namespace"].as_str()?;
                    let mut r = Resource::new(
                        format!("gitlab://{}", encode_namespace_id(path)),
                        path.to_string(),
                    )
                    .with_mime_type("application/json");
                    if let Some(name) = p["name"].as_str() {
                        r = r.with_title(name);
                    }
                    if let Some(desc) = p["description"].as_str().filter(|d| !d.is_empty()) {
                        r = r.with_description(desc);
                    }
                    Some(r)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(resources)
}

/// JSON resources go through the same `slim_get` shaping as the tool results,
/// so a resource read and a single-get tool call show the same object.
fn json_contents(v: Value, uri: &str) -> Result<ResourceContents, GitlabError> {
    let text = serde_json::to_string_pretty(&slim::slim_get(v))
        .map_err(|e| GitlabError::Other(format!("marshalling response: {e}")))?;
    Ok(ResourceContents::text(text, uri).with_mime_type("application/json"))
}

/// Decode the base64 `content` from a `file_get` response: UTF-8 files become
/// text contents, anything else stays base64 as blob contents.
fn file_contents(v: &Value, file_path: &str, uri: &str) -> Result<ResourceContents, GitlabError> {
    let b64 = v
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| GitlabError::Other("file response is missing content".to_string()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| GitlabError::Other(format!("decoding file content: {e}")))?;
    Ok(String::from_utf8(bytes).map_or_else(
        |_| ResourceContents::blob(b64, uri).with_mime_type("application/octet-stream"),
        |text| ResourceContents::text(text, uri).with_mime_type(mime_for_path(file_path)),
    ))
}

/// A minimal extension→MIME map for the formats a client plausibly renders
/// differently; everything else is served as `text/plain`.
fn mime_for_path(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
    {
        Some("json") => "application/json",
        Some("md" | "markdown") => "text/markdown",
        Some("html" | "htm") => "text/html",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("csv") => "text/csv",
        Some("yaml" | "yml") => "application/yaml",
        _ => "text/plain",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{ResourceRef, list_recent_projects, parse_uri, read, resource_templates};
    use crate::test_util::mock_client;
    use rmcp::model::ResourceContents;

    fn text_of(contents: &[ResourceContents]) -> (&str, Option<&str>) {
        match contents {
            [
                ResourceContents::TextResourceContents {
                    text, mime_type, ..
                },
            ] => (text.as_str(), mime_type.as_deref()),
            other => panic!("expected one text contents, got {other:?}"),
        }
    }

    #[test]
    fn parses_bare_project_uri() {
        let expected = ResourceRef::Project {
            project_id: "mygroup/myproject".into(),
        };
        assert_eq!(parse_uri("gitlab://mygroup%2Fmyproject").unwrap(), expected);
        // One trailing slash is tolerated.
        assert_eq!(
            parse_uri("gitlab://mygroup%2Fmyproject/").unwrap(),
            expected
        );
    }

    #[test]
    fn trailing_slash_is_tolerated_on_every_kind() {
        assert_eq!(
            parse_uri("gitlab://42/issues/7/").unwrap(),
            ResourceRef::Issue {
                project_id: "42".into(),
                issue_iid: 7
            }
        );
        assert_eq!(
            parse_uri("gitlab://42/pipelines/9/").unwrap(),
            ResourceRef::Pipeline {
                project_id: "42".into(),
                pipeline_id: 9
            }
        );
    }

    #[test]
    fn parses_numeric_project_issue_uri() {
        assert_eq!(
            parse_uri("gitlab://42/issues/7").unwrap(),
            ResourceRef::Issue {
                project_id: "42".into(),
                issue_iid: 7
            }
        );
    }

    #[test]
    fn parses_encoded_namespace_path_project() {
        assert_eq!(
            parse_uri("gitlab://mygroup%2Fmyproject/mrs/3").unwrap(),
            ResourceRef::MergeRequest {
                project_id: "mygroup/myproject".into(),
                merge_request_iid: 3
            }
        );
    }

    #[test]
    fn file_path_accepts_literal_and_encoded_slashes() {
        let expected = ResourceRef::File {
            project_id: "42".into(),
            file_path: "src/main.rs".into(),
            ref_name: None,
        };
        assert_eq!(
            parse_uri("gitlab://42/files/src/main.rs").unwrap(),
            expected
        );
        assert_eq!(
            parse_uri("gitlab://42/files/src%2Fmain.rs").unwrap(),
            expected
        );
    }

    #[test]
    fn file_ref_query_is_decoded() {
        assert_eq!(
            parse_uri("gitlab://42/files/README.md?ref=feature%2Fbranch").unwrap(),
            ResourceRef::File {
                project_id: "42".into(),
                file_path: "README.md".into(),
                ref_name: Some("feature/branch".into()),
            }
        );
    }

    #[test]
    fn rejects_malformed_uris() {
        for uri in [
            "https://gitlab.com/42/issues/7", // wrong scheme
            "gitlab://42//issues/7",          // empty kind segment
            "gitlab://42/branches/main",      // unknown kind
            "gitlab://42/issues/seven",       // non-numeric IID
            "gitlab://42/issues",             // missing IID
            "gitlab://42/issues/7/notes",     // trailing segments
            "gitlab://42/files/",             // empty file path
            "gitlab:///issues/7",             // empty project
            "gitlab://42?ref=main",           // query on a project resource
            "gitlab://42/issues/7?page=2",    // query on a non-file kind
        ] {
            assert!(parse_uri(uri).is_err(), "expected {uri} to be rejected");
        }
    }

    /// Every advertised template, expanded per RFC 6570 simple expansion, must
    /// parse — keeps the templates and the parser from drifting apart.
    #[test]
    fn advertised_templates_round_trip_through_the_parser() {
        for template in resource_templates() {
            let uri = template
                .uri_template
                .replace("{project_id}", "mygroup%2Fmyproject")
                .replace("{file_path}", "docs%2Fguide.md")
                .replace("{issue_iid}", "7")
                .replace("{merge_request_iid}", "7")
                .replace("{pipeline_id}", "7")
                .replace("{?ref}", "?ref=main");
            assert!(
                parse_uri(&uri).is_ok(),
                "template {} expanded to unparseable {uri}",
                template.uri_template
            );
        }
    }

    #[tokio::test]
    async fn reads_project_as_slimmed_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/mygroup%2Fmyproject"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 42,
                "path_with_namespace": "mygroup/myproject",
                "default_branch": "main",
                "description": null
            })))
            .mount(&server)
            .await;

        let uri = "gitlab://mygroup%2Fmyproject";
        let contents = read(&mock_client(&server), parse_uri(uri).unwrap(), uri)
            .await
            .unwrap();
        let (text, mime) = text_of(&contents);
        assert_eq!(mime, Some("application/json"));
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["default_branch"], "main");
        // slim_get drops nulls here too.
        assert!(v.get("description").is_none());
    }

    #[tokio::test]
    async fn lists_recent_projects_as_encoded_project_uris() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects"))
            .and(query_param("membership", "true"))
            .and(query_param("order_by", "last_activity_at"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "path_with_namespace": "mygroup/active",
                    "name": "Active",
                    "description": "The busy one"
                },
                {
                    "path_with_namespace": "mygroup/quiet",
                    "name": "Quiet",
                    "description": ""
                }
            ])))
            .mount(&server)
            .await;

        let resources = list_recent_projects(&mock_client(&server)).await.unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].uri, "gitlab://mygroup%2Factive");
        assert_eq!(resources[0].name, "mygroup/active");
        assert_eq!(resources[0].title.as_deref(), Some("Active"));
        assert_eq!(resources[0].description.as_deref(), Some("The busy one"));
        // Every listed URI must be readable, i.e. parse as a project resource.
        for r in &resources {
            assert!(
                matches!(parse_uri(&r.uri), Ok(ResourceRef::Project { .. })),
                "listed URI {} should parse as a project",
                r.uri
            );
        }
        // Empty descriptions are omitted rather than surfaced as "".
        assert_eq!(resources[1].description, None);
    }

    #[tokio::test]
    async fn reads_text_file_decoded_with_default_ref() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/files/src%2Fmain.rs"))
            .and(query_param("ref", "HEAD"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "file_name": "main.rs",
                "content": "Zm4gbWFpbigpIHt9Cg==" // "fn main() {}\n"
            })))
            .mount(&server)
            .await;

        let uri = "gitlab://42/files/src/main.rs";
        let contents = read(&mock_client(&server), parse_uri(uri).unwrap(), uri)
            .await
            .unwrap();
        let (text, mime) = text_of(&contents);
        assert_eq!(text, "fn main() {}\n");
        assert_eq!(mime, Some("text/plain"));
    }

    #[tokio::test]
    async fn reads_binary_file_as_blob() {
        let server = MockServer::start().await;
        // 0xFF 0xFE 0x00: not valid UTF-8.
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/repository/files/logo.png"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "file_name": "logo.png",
                "content": "//4A"
            })))
            .mount(&server)
            .await;

        let uri = "gitlab://42/files/logo.png";
        let contents = read(&mock_client(&server), parse_uri(uri).unwrap(), uri)
            .await
            .unwrap();
        match &contents[..] {
            [
                ResourceContents::BlobResourceContents {
                    blob, mime_type, ..
                },
            ] => {
                assert_eq!(blob, "//4A");
                assert_eq!(mime_type.as_deref(), Some("application/octet-stream"));
            }
            other => panic!("expected blob contents, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reads_issue_as_slimmed_json() {
        let server = MockServer::start().await;
        // issue_get's supplemental links/closed_by fetches hit the mock's
        // default 404, which unwrap_404_as_empty_array turns into [].
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/issues/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "iid": 7,
                "title": "A bug",
                "description": null,
                "_links": {"self": "..."}
            })))
            .mount(&server)
            .await;

        let uri = "gitlab://42/issues/7";
        let contents = read(&mock_client(&server), parse_uri(uri).unwrap(), uri)
            .await
            .unwrap();
        let (text, mime) = text_of(&contents);
        assert_eq!(mime, Some("application/json"));
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["title"], "A bug");
        // slim_get drops nulls and _links.
        assert!(v.get("description").is_none());
        assert!(v.get("_links").is_none());
    }

    #[tokio::test]
    async fn reads_pipeline_as_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/pipelines/123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 123,
                "status": "success"
            })))
            .mount(&server)
            .await;

        let uri = "gitlab://42/pipelines/123";
        let contents = read(&mock_client(&server), parse_uri(uri).unwrap(), uri)
            .await
            .unwrap();
        let (text, _) = text_of(&contents);
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["status"], "success");
    }

    #[tokio::test]
    async fn gitlab_404_propagates_as_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/projects/42/pipelines/999"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "message": "404 Not Found"
            })))
            .mount(&server)
            .await;

        let uri = "gitlab://42/pipelines/999";
        let err = read(&mock_client(&server), parse_uri(uri).unwrap(), uri)
            .await
            .unwrap_err();
        match err {
            crate::client::GitlabError::Api { status, .. } => {
                assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }
}
