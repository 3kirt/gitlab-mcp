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
