//! Live tests for MCP completions — read-only, pivoting on the test project
//! (no seeding): `project_id` exercises the `/projects?membership` search that
//! only completions call, and `branch` exercises the branch-name search
//! scoped to a context project.

use std::collections::HashMap;

use crate::tools::completions::complete_argument;

use super::harness::skip_unless_live;

#[tokio::test]
async fn completions_suggest_real_projects_and_branches() {
    let env = skip_unless_live!();

    // The project search must find the test project by the tail of its path.
    let search = env
        .project
        .rsplit('/')
        .next()
        .expect("project has a path component");
    let projects = complete_argument(&env.client, false, "project_id", search, None)
        .await
        .expect("project_id completion");
    assert!(
        projects.values.iter().any(|v| v == &env.project),
        "expected {} in {:?}",
        env.project,
        projects.values
    );

    // A resource-template reference URI-encodes the same suggestion.
    let encoded = complete_argument(&env.client, true, "project_id", search, None)
        .await
        .expect("encoded project_id completion");
    let expect = env.project.replace('/', "%2F");
    assert!(
        encoded.values.iter().any(|v| v == &expect),
        "expected {expect} in {:?}",
        encoded.values
    );

    // Branch completion scoped to the context project must at least find the
    // default branch when searching for it.
    let ctx = HashMap::from([("project_id".to_string(), env.project.clone())]);
    let branches = complete_argument(&env.client, false, "branch", "ma", Some(&ctx))
        .await
        .expect("branch completion");
    assert!(
        branches.values.iter().any(|v| v == "main"),
        "expected main in {:?}",
        branches.values
    );
}
