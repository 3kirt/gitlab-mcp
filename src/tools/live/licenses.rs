//! Live tests for the Licenses domain.
//!
//! The License API exists only on Self-Managed/Dedicated and requires an
//! administrator token; against gitlab.com (the usual test target) every
//! endpoint answers 403. These tests therefore assert *either* a well-formed
//! license payload *or* a clean 403/404 API rejection — which still proves the
//! request reached GitLab on the intended route and our error mapping is
//! intact, without requiring an EE admin environment. On a Self-Managed
//! instance with an admin token the full shape assertions run.

use serde_json::Value;

use crate::client::GitlabError;
use crate::tools::{licenses, slim};

use super::harness::{assert_nonempty_str, skip_unless_live};

/// Unwrap a License API result, tolerating the rejection a non-admin token or
/// gitlab.com gives (403; 404 on instances that disable the endpoint). Any
/// other error — transport, parse, 5xx — is still a failure.
fn ok_or_unlicensed<T>(what: &str, result: Result<T, GitlabError>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(GitlabError::Api { status, .. }) if matches!(status.as_u16(), 403 | 404) => {
            eprintln!("SKIP {what}: License API needs an admin token on Self-Managed ({status})");
            None
        }
        Err(e) => panic!("{what}: {e:?}"),
    }
}

fn assert_license_invariants(license: &Value) {
    assert!(
        license.get("id").and_then(Value::as_u64).is_some(),
        "license id"
    );
    assert_nonempty_str(license, "plan");
    assert!(
        license.get("expired").and_then(Value::as_bool).is_some(),
        "expired flag"
    );
}

#[tokio::test]
async fn current_license_shape() {
    let env = skip_unless_live!();
    let result =
        licenses::license_get(&env.client, licenses::LicenseGetParams { license_id: None }).await;
    if let Some(license) = ok_or_unlicensed("license_get", result) {
        assert_license_invariants(&slim::slim_get(license));
    }
}

#[tokio::test]
async fn license_list_shape() {
    let env = skip_unless_live!();
    let result = licenses::licenses_list(&env.client, licenses::LicensesListParams {}).await;
    if let Some(list) = ok_or_unlicensed("licenses_list", result) {
        let list = slim::slim_get(list);
        for license in list.as_array().expect("licenses list is an array") {
            assert_license_invariants(license);
        }
    }
}

#[tokio::test]
async fn usage_export_is_nonempty_csv() {
    let env = skip_unless_live!();
    let result =
        licenses::license_usage_export(&env.client, licenses::LicenseUsageExportParams {}).await;
    if let Some(csv) = ok_or_unlicensed("license_usage_export", result) {
        assert!(!csv.trim().is_empty(), "usage export CSV must be non-empty");
    }
}
