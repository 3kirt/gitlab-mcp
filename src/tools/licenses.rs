//! Instance license information via the REST License API (`/api/v4/license*`).
//!
//! Read-only by design: the create/delete/refresh endpoints exist but are
//! deliberately not exposed — uploading or deleting an instance license is an
//! administrative action with no agentic use case. Every endpoint here
//! requires an administrator token and only exists on GitLab Self-Managed /
//! Dedicated (gitlab.com returns 403/404).

use serde::Deserialize;
use serde_json::Value;

use crate::client::{GitlabClient, GitlabError};

// --------------------------------------------------------------------------
// List all licenses
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LicensesListParams {}

/// `GET /licenses` returns the handful of licenses ever added to the instance;
/// the endpoint has no pagination, so this is a plain get.
pub async fn licenses_list(
    client: &GitlabClient,
    _p: LicensesListParams,
) -> Result<Value, GitlabError> {
    client.get("/api/v4/licenses").await
}

// --------------------------------------------------------------------------
// Get the current license, or one by ID
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LicenseGetParams {
    #[schemars(
        description = "ID of a specific license (from gitlab_licenses_list). Omit to get the currently active license."
    )]
    pub license_id: Option<u64>,
}

pub async fn license_get(client: &GitlabClient, p: LicenseGetParams) -> Result<Value, GitlabError> {
    let path = p.license_id.map_or_else(
        || "/api/v4/license".to_string(),
        |id| format!("/api/v4/license/{id}"),
    );
    client.get(&path).await
}

// --------------------------------------------------------------------------
// License usage export (CSV)
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LicenseUsageExportParams {}

pub async fn license_usage_export(
    client: &GitlabClient,
    _p: LicenseUsageExportParams,
) -> Result<String, GitlabError> {
    client
        .get_text("/api/v4/license/usage_export.csv", &[])
        .await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_licenses, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "List all licenses (subscriptions) ever added to this GitLab instance, current and past. Returns each license's plan (premium/ultimate), validity dates, user limit, billable-user counts, overage, and licensee. Requires administrator access; Self-Managed/Dedicated only (not gitlab.com). For just the active license, use gitlab_licenses_get.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_licenses_list(
        &self,
        Parameters(p): Parameters<LicensesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, licenses_list, p, "licenses")
    }

    #[tool(
        description = "Get a GitLab instance license (subscription): omit license_id for the currently active license, or pass an ID from gitlab_licenses_list for a specific one. Returns plan (premium/ultimate), start/expiry dates, expired flag, user limit, active/billable user counts, overage, licensee, and add-ons. Requires administrator access; Self-Managed/Dedicated only (not gitlab.com).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_licenses_get(
        &self,
        Parameters(p): Parameters<LicenseGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, license_get, p, "license")
    }

    #[tool(
        description = "Export license usage (seat usage) for this GitLab instance as CSV: license key details plus a dated history of billable user counts. Requires administrator access; Self-Managed/Dedicated only (not gitlab.com).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_licenses_usage_export(
        &self,
        Parameters(p): Parameters<LicenseUsageExportParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_text!(self, license_usage_export, p, "exporting", "license usage")
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        LicenseGetParams, LicenseUsageExportParams, LicensesListParams, license_get,
        license_usage_export, licenses_list,
    };
    use crate::test_util::mock_client;

    #[tokio::test]
    async fn license_get_without_id_hits_current_license() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/license"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 2, "plan": "ultimate", "expired": false
            })))
            .mount(&server)
            .await;

        let v = license_get(&mock_client(&server), LicenseGetParams { license_id: None })
            .await
            .unwrap();
        assert_eq!(v["plan"], "ultimate");
    }

    #[tokio::test]
    async fn license_get_with_id_hits_license_by_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/license/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 1, "plan": "premium"
            })))
            .mount(&server)
            .await;

        let v = license_get(
            &mock_client(&server),
            LicenseGetParams {
                license_id: Some(1),
            },
        )
        .await
        .unwrap();
        assert_eq!(v["id"], 1);
    }

    #[tokio::test]
    async fn licenses_list_hits_licenses_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v4/licenses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": 1, "plan": "premium"},
                {"id": 2, "plan": "ultimate"}
            ])))
            .mount(&server)
            .await;

        let v = licenses_list(&mock_client(&server), LicensesListParams {})
            .await
            .unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn license_usage_export_returns_raw_csv() {
        let server = MockServer::start().await;
        let csv = "Date,Billable User Count\n2023-07-11 12:00:05,21\n";
        Mock::given(method("GET"))
            .and(path("/api/v4/license/usage_export.csv"))
            .respond_with(ResponseTemplate::new(200).set_body_string(csv))
            .mount(&server)
            .await;

        let text = license_usage_export(&mock_client(&server), LicenseUsageExportParams {})
            .await
            .unwrap();
        assert_eq!(text, csv);
    }
}
