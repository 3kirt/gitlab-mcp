use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};

// --------------------------------------------------------------------------
// Get metadata
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MetadataParams {}

pub async fn metadata_get(client: &GitlabClient, _p: MetadataParams) -> Result<Value, GitlabError> {
    client.get("/api/v4/metadata").await
}

// --------------------------------------------------------------------------
// Tool schema introspection
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolSchemaGetParams {
    #[schemars(description = "Exact tool name, e.g. \"gitlab_issues_get\"")]
    pub tool_name: String,
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    tool, tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_metadata, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "Get metadata about the GitLab instance, including version, revision, enterprise status, and Kubernetes agent server (KAS) information."
    )]
    async fn gitlab_metadata_get(
        &self,
        Parameters(p): Parameters<MetadataParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, metadata_get, p, "metadata")
    }

    #[tool(
        description = "Get the parameter schema for a named tool on this GitLab MCP server (lightweight introspection). Returns the tool's description and JSON Schema of its parameters, including which fields are required. Use this before calling a tool when unsure of its exact parameter names."
    )]
    async fn gitlab_tool_schema_get(
        &self,
        Parameters(p): Parameters<ToolSchemaGetParams>,
    ) -> Result<CallToolResult, McpError> {
        let Some(tool) = self.tool_router.get(&p.tool_name) else {
            // Suggest tools sharing a name token (e.g. "issues" or "get") so a
            // near-miss query still lands somewhere useful.
            let tokens: Vec<&str> = p
                .tool_name
                .split('_')
                .filter(|t| !t.is_empty() && *t != "gitlab")
                .collect();
            let mut candidates: Vec<String> = self
                .tool_router
                .list_all()
                .iter()
                .map(|t| t.name.to_string())
                .filter(|name| tokens.iter().any(|t| name.contains(t)))
                .collect();
            candidates.sort();
            candidates.truncate(15);
            let hint = if candidates.is_empty() {
                "no similarly named tools found".to_string()
            } else {
                format!("similarly named tools: {}", candidates.join(", "))
            };
            return crate::tools::tool_error(&format!("no tool named \"{}\"; {hint}", p.tool_name));
        };
        // Serialized directly (not via json_result) so the schema is returned
        // verbatim, bypassing response slimming.
        let payload = serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": &*tool.input_schema,
        });
        let text = serde_json::to_string_pretty(&payload)
            .map_err(|e| McpError::internal_error(format!("marshalling response: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> GitlabMcpServer {
        GitlabMcpServer::new_stdio("https://gitlab.com".into(), "test-token".into()).unwrap()
    }

    fn result_text(result: &CallToolResult) -> &str {
        &result.content[0].as_text().unwrap().text
    }

    #[tokio::test]
    async fn tool_schema_get_returns_schema_for_known_tool() {
        let result = server()
            .gitlab_tool_schema_get(Parameters(ToolSchemaGetParams {
                tool_name: "gitlab_issues_get".into(),
            }))
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true));
        let payload: serde_json::Value = serde_json::from_str(result_text(&result)).unwrap();
        assert_eq!(payload["name"], "gitlab_issues_get");
        assert!(payload["input_schema"]["properties"]["project_id"].is_object());
        assert!(
            payload["input_schema"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "issue_iid")
        );
    }

    #[tokio::test]
    async fn tool_schema_get_unknown_tool_suggests_candidates() {
        let result = server()
            .gitlab_tool_schema_get(Parameters(ToolSchemaGetParams {
                tool_name: "gitlab_issues_fetch".into(),
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        let text = result_text(&result);
        assert!(text.contains("no tool named"), "{text}");
        assert!(text.contains("gitlab_issues_get"), "{text}");
    }

    #[tokio::test]
    async fn tool_schema_get_gibberish_reports_no_candidates() {
        let result = server()
            .gitlab_tool_schema_get(Parameters(ToolSchemaGetParams {
                tool_name: "zzzqqq".into(),
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result_text(&result).contains("no similarly named tools found"));
    }
}
