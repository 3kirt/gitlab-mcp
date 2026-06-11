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
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
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
}
