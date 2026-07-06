//! Shared harness for the live integration suite: credentials/environment, the
//! skip macro, unique run tagging, the pagination helper, and the cross-domain
//! invariant assertions reused by every area module.

use serde_json::Value;

use crate::client::GitlabClient;
use crate::tools::{PaginationParams, branches, issues, merge_requests, repository_files};

/// A live client plus the project under test, or `None` when credentials are
/// absent (so tests skip rather than fail). Every test begins with
/// `let env = skip_unless_live!();`.
pub(super) struct LiveEnv {
    pub(super) client: GitlabClient,
    pub(super) project: String,
}

pub(super) fn live_env() -> Option<LiveEnv> {
    let url = std::env::var("GITLAB_URL").ok()?;
    let token = std::env::var("GITLAB_TOKEN").ok()?;
    if url.is_empty() || token.is_empty() {
        return None;
    }
    let project =
        std::env::var("GITLAB_TEST_PROJECT").unwrap_or_else(|_| "3kirt1/gitlab-mcp-testing".into());
    let client = GitlabClient::new(url, token).expect("build live client");
    Some(LiveEnv { client, project })
}

/// Bind a `LiveEnv` or return early (skipping) when credentials are absent.
/// Defined here and re-exported so every area module can `use` it; the body
/// references `live_env` by absolute path so callers need not import it.
macro_rules! skip_unless_live {
    () => {
        match $crate::tools::live::harness::live_env() {
            Some(env) => env,
            None => {
                eprintln!("SKIP: set GITLAB_URL + GITLAB_TOKEN to run live tests");
                return;
            }
        }
    };
}
pub(crate) use skip_unless_live;

/// A short unique tag so concurrent/repeated runs never collide on titles or
/// labels, and so a crashed run's leftovers are identifiable.
pub(super) fn run_tag() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("mcp-livetest-{nanos}")
}

/// Build the pagination triple without spelling out all three fields each time.
pub(super) const fn pg(page: Option<u64>, per_page: Option<u64>) -> PaginationParams {
    PaginationParams {
        page,
        per_page,
        fetch_all: None,
    }
}

/// Best-effort teardown of a throwaway branch (and all its commits). Shared by
/// every area that seeds git state. Ignores errors so cleanup never masks a
/// test failure.
pub(super) async fn delete_branch(env: &LiveEnv, branch: &str) {
    let _ = branches::branch_delete(
        &env.client,
        branches::BranchDeleteParams {
            project_id: env.project.clone().into(),
            branch: branch.to_string(),
        },
    )
    .await;
}

/// Create a branch off `source_ref` carrying one new file, so an MR opened from
/// it against `source_ref` has a real diff. Returns the branch name. Pair with
/// [`delete_branch`] in teardown.
pub(super) async fn seed_branch_with_file(env: &LiveEnv, branch: &str, source_ref: &str) -> String {
    repository_files::file_create(
        &env.client,
        repository_files::FileCreateParams {
            project_id: env.project.clone().into(),
            file_path: format!("livetest/{branch}.txt"),
            branch: branch.to_string(),
            commit_message: format!("seed {branch}"),
            content: format!("content for {branch}\n"),
            encoding: None,
            author_name: None,
            author_email: None,
            execute_filemode: None,
            // Branch the new ref off an existing one in the same call.
            start_branch: Some(source_ref.to_string()),
        },
    )
    .await
    .expect("seed branch with file");
    branch.to_string()
}

/// Create a minimal issue (title only) and return its iid — for tests that just
/// need an issue to attach things to. Pair with [`delete_issue`]. (Issue-domain
/// tests that assert on the create payload use their own richer helper.)
pub(super) async fn seed_issue(env: &LiveEnv, title: &str) -> u64 {
    let created = issues::issue_create(
        &env.client,
        issues::IssueCreateParams {
            project_id: env.project.clone().into(),
            title: title.to_string(),
            description: None,
            labels: None,
            assignee_ids: None,
            milestone_id: None,
            due_date: None,
            weight: None,
        },
    )
    .await
    .expect("seed issue");
    created["iid"].as_u64().expect("created issue has iid")
}

pub(super) async fn delete_issue(env: &LiveEnv, iid: u64) {
    let _ = issues::issue_delete(
        &env.client,
        issues::IssueDeleteParams {
            project_id: env.project.clone().into(),
            issue_iid: iid,
        },
    )
    .await;
}

/// Seed a minimal MR with a real diff (throwaway branch off `main`). Returns
/// `(mr_iid, branch)`; pair with [`delete_mr`] and [`delete_branch`].
pub(super) async fn seed_mr(env: &LiveEnv, tag: &str) -> (u64, String) {
    let branch = format!("{tag}-mr-feat");
    seed_branch_with_file(env, &branch, "main").await;
    let created = merge_requests::mr_create(
        &env.client,
        merge_requests::MrCreateParams {
            project_id: env.project.clone().into(),
            source_branch: branch.clone(),
            target_branch: "main".into(),
            title: format!("{tag} mr"),
            description: None,
            assignee_id: None,
            reviewer_ids: None,
            labels: None,
            milestone_id: None,
            squash: true,
            remove_source_branch: true,
            draft: None,
        },
    )
    .await
    .expect("seed mr");
    let iid = created["iid"].as_u64().expect("created MR has iid");

    // GitLab prepares MR diffs asynchronously; immediately after creation the
    // diffs endpoint returns an empty array (observed ~1s to fill on
    // gitlab.com). Wait until the diff exists so consumers (e.g. the
    // review-mr prompt test) see the fully-prepared MR the seed promises.
    let diffs_path = format!(
        "{}/merge_requests/{iid}/diffs",
        crate::tools::project_path(&env.project)
    );
    for _ in 0..20 {
        match env.client.get(&diffs_path).await {
            Ok(Value::Array(diffs)) if !diffs.is_empty() => return (iid, branch),
            _ => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
        }
    }
    panic!("seeded MR !{iid} still has no diff after 10s");
}

pub(super) async fn delete_mr(env: &LiveEnv, iid: u64) {
    let _ = merge_requests::mr_delete(
        &env.client,
        merge_requests::MrDeleteParams {
            project_id: env.project.clone().into(),
            merge_request_iid: iid,
        },
    )
    .await;
}

/// Invariants for a note object (issue or MR, single-get / create / list item).
pub(super) fn assert_note_invariants(note: &Value) {
    assert!(note.get("id").and_then(Value::as_u64).is_some(), "note id");
    assert!(note.get("body").and_then(Value::as_str).is_some(), "body");
    assert_no_stripped_keys(note);
    assert_user_collapsed(&note["author"]);
}

/// Count the notes inside a (slimmed) discussion object.
pub(super) fn discussion_note_count(disc: &Value) -> usize {
    disc["notes"].as_array().map_or(0, Vec::len)
}

// --------------------------------------------------------------------------
// Cross-domain invariant assertions (the protocol's "Universal Invariants")
// --------------------------------------------------------------------------

pub(super) fn assert_no_stripped_keys(v: &Value) {
    let obj = v.as_object().expect("object");
    assert!(obj.get("_links").is_none(), "_links must be stripped");
    assert!(
        obj.get("references").is_none(),
        "references must be stripped"
    );
}

pub(super) fn assert_nonempty_str(v: &Value, key: &str) {
    let s = v.get(key).and_then(Value::as_str).unwrap_or("");
    assert!(!s.is_empty(), "{key} must be a non-empty string");
}

/// A collapsed user object must be present and carry only id/username/name (the
/// slimmer drops avatar_url, web_url, state, etc.). Requiring presence keeps the
/// check from passing vacuously if the user/author field ever goes missing.
pub(super) fn assert_user_collapsed(user: &Value) {
    let obj = user
        .as_object()
        .expect("user/author must be a present object");
    for key in obj.keys() {
        assert!(
            matches!(key.as_str(), "id" | "username" | "name"),
            "user object should be collapsed, unexpected key {key:?}"
        );
    }
}
