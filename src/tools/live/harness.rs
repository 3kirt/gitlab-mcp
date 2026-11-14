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
    let url = live_var("GITLAB_URL")?;
    let token = live_var("GITLAB_TOKEN")?;
    let project =
        live_var("GITLAB_TEST_PROJECT").unwrap_or_else(|| "3kirt1/gitlab-mcp-testing".into());
    let client = GitlabClient::new(url, token).expect("build live client");
    Some(LiveEnv { client, project })
}

/// Gitignored `KEY=VALUE` file at the crate root that supplies live-test
/// settings (`GITLAB_URL`, `GITLAB_TOKEN`, `GITLAB_TEST_PROJECT`) when they
/// aren't set in the environment. See `.env.example`. Only the live harness
/// reads it — the server binary uses the environment / `~/.gitlab_mcp.json`.
const ENV_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.env");

/// A live-test setting: the environment variable wins, then [`ENV_FILE`].
/// Blank values count as unset (cf. `config::first_non_empty`).
fn live_var(key: &str) -> Option<String> {
    let non_blank = |v: &String| !v.trim().is_empty();
    std::env::var(key)
        .ok()
        .filter(non_blank)
        .or_else(|| env_file_vars().get(key).filter(|v| non_blank(v)).cloned())
}

/// [`ENV_FILE`]'s contents, parsed once per test binary. A missing file is
/// empty; an unreadable or world-readable one panics with a fix-it message
/// rather than silently skipping every test.
fn env_file_vars() -> &'static std::collections::HashMap<String, String> {
    static VARS: std::sync::OnceLock<std::collections::HashMap<String, String>> =
        std::sync::OnceLock::new();
    VARS.get_or_init(|| {
        let path = std::path::Path::new(ENV_FILE);
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return std::collections::HashMap::default();
            }
            Err(e) => panic!("reading {}: {e}", path.display()),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path)
                .unwrap_or_else(|e| panic!("checking {}: {e}", path.display()))
                .permissions()
                .mode();
            assert!(
                mode & 0o004 == 0,
                "{} holds a GitLab token but is world-readable; run: chmod 600 {}",
                path.display(),
                path.display()
            );
        }
        parse_env_file(&contents)
    })
}

/// Minimal dotenv parsing: `KEY=VALUE` per line, `#` comments and blank lines
/// ignored, an optional leading `export `, and one pair of matching surrounding
/// quotes stripped from the value. No interpolation or escapes.
fn parse_env_file(contents: &str) -> std::collections::HashMap<String, String> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            let value = ['"', '\'']
                .iter()
                .find_map(|q| value.strip_prefix(*q)?.strip_suffix(*q))
                .unwrap_or(value);
            Some((key.trim().to_owned(), value.to_owned()))
        })
        .collect()
}

#[test]
fn parse_env_file_handles_comments_export_and_quotes() {
    let vars = parse_env_file(
        "# live-test credentials\n\
         \n\
         GITLAB_URL=https://gitlab.com\n\
         export GITLAB_TOKEN=\"glpat-xxxxxxxxxxxxxxxxxxxx\"\n\
         GITLAB_TEST_PROJECT = 'grp/proj'\n\
         not a pair\n",
    );
    assert_eq!(vars.len(), 3);
    assert_eq!(vars["GITLAB_URL"], "https://gitlab.com");
    assert_eq!(vars["GITLAB_TOKEN"], "glpat-xxxxxxxxxxxxxxxxxxxx");
    assert_eq!(vars["GITLAB_TEST_PROJECT"], "grp/proj");
}

/// Whether a missing-credentials skip must fail instead (`LIVE_REQUIRED=1`,
/// set by the scheduled CI live job and the release process). Without it a
/// credential-less run "passes" while verifying nothing. Intentional
/// per-test skips (license 403, CI identity verification) are unaffected.
pub(super) fn live_required() -> bool {
    std::env::var("LIVE_REQUIRED").is_ok_and(|v| v == "1")
}

/// Bind a `LiveEnv` or return early (skipping) when credentials are absent —
/// or panic under `LIVE_REQUIRED=1`. Defined here and re-exported so every
/// area module can `use` it; the body references the harness by absolute
/// path so callers need not import it.
macro_rules! skip_unless_live {
    () => {
        match $crate::tools::live::harness::live_env() {
            Some(env) => env,
            None => {
                assert!(
                    !$crate::tools::live::harness::live_required(),
                    "LIVE_REQUIRED=1 but GITLAB_URL + GITLAB_TOKEN are not set \
                     (environment or .env)"
                );
                eprintln!(
                    "SKIP: set GITLAB_URL + GITLAB_TOKEN (environment or .env) to run live tests"
                );
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
            issue_iid: iid.into(),
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
            merge_request_iid: iid.into(),
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
