//! Shared harness for the live integration suite: credentials/environment, the
//! skip macro, unique run tagging, the pagination helper, and the cross-domain
//! invariant assertions reused by every area module.

use serde_json::Value;

use crate::client::GitlabClient;
use crate::tools::{PaginationParams, branches, repository_files};

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
pub(super) fn pg(page: Option<u64>, per_page: Option<u64>) -> PaginationParams {
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
            project_id: env.project.clone(),
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
            project_id: env.project.clone(),
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

/// Invariants for a note object (issue or MR, single-get / create / list item).
pub(super) fn assert_note_invariants(note: &Value) {
    assert!(note.get("id").and_then(Value::as_u64).is_some(), "note id");
    assert!(note.get("body").and_then(Value::as_str).is_some(), "body");
    assert_no_stripped_keys(note);
    assert_user_collapsed(&note["author"]);
}

/// Count the notes inside a (slimmed) discussion object.
pub(super) fn discussion_note_count(disc: &Value) -> usize {
    disc["notes"].as_array().map(Vec::len).unwrap_or(0)
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

/// A collapsed user object must carry only id/username/name (the slimmer drops
/// avatar_url, web_url, state, etc.).
pub(super) fn assert_user_collapsed(user: &Value) {
    if let Some(obj) = user.as_object() {
        for key in obj.keys() {
            assert!(
                matches!(key.as_str(), "id" | "username" | "name"),
                "user object should be collapsed, unexpected key {key:?}"
            );
        }
    }
}
