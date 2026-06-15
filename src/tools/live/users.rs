//! Live tests for the Users domain. Read-only: every test pivots on the
//! authenticated user (`GET /user`), which always exists, so nothing is seeded
//! or cleaned up. Verifies the three tools (`users_list`, `user_get`,
//! `users_keys_list`) against the real API: param/response shapes, the
//! numeric-ID-or-username resolution, and the server's slimming envelope.

use serde_json::Value;

use crate::tools::{slim, users};

use super::harness::{LiveEnv, assert_no_stripped_keys, assert_nonempty_str, pg, skip_unless_live};

/// The authenticated user (`GET /user`) — the deterministic pivot for every
/// test here. Returns the raw object; callers read `id`/`username`.
async fn current_user(env: &LiveEnv) -> Value {
    env.client
        .get("/api/v4/user")
        .await
        .expect("GET /user (current authenticated user)")
}

/// Fetch a user through the server's path: domain function + `slim_get`.
async fn get_user_slimmed(env: &LiveEnv, user_id: &str) -> Value {
    let raw = users::user_get(
        &env.client,
        users::UserGetParams {
            user_id: user_id.to_string(),
        },
    )
    .await
    .expect("user_get");
    slim::slim_get(raw)
}

/// Invariants for a user object (single-get or list item): a numeric id plus a
/// non-empty username, and the always-stripped keys gone.
fn assert_user_invariants(user: &Value) {
    assert!(user.get("id").and_then(Value::as_u64).is_some(), "user id");
    assert_nonempty_str(user, "username");
    assert_no_stripped_keys(user);
}

// --------------------------------------------------------------------------
// user_get — by numeric ID and by username resolve to the same user, with the
// full top-level resource retained (not collapsed to id/username/name).
// --------------------------------------------------------------------------

#[tokio::test]
async fn user_get_by_id_and_username_match() {
    let env = skip_unless_live!();
    let me = current_user(&env).await;
    let id = me["id"].as_u64().expect("current user id");
    let username = me["username"].as_str().expect("current user username");

    // By numeric ID.
    let by_id = get_user_slimmed(&env, &id.to_string()).await;
    assert_user_invariants(&by_id);
    assert_eq!(by_id["id"].as_u64(), Some(id));
    assert_eq!(by_id["username"].as_str(), Some(username));
    // slim_get must NOT collapse the top-level user resource: detail beyond
    // id/username/name survives (web_url is always present on a user object).
    assert_nonempty_str(&by_id, "web_url");

    // By username — resolved to the same numeric ID, same user back.
    let by_name = get_user_slimmed(&env, username).await;
    assert_eq!(by_name["id"].as_u64(), Some(id));
    assert_eq!(by_name["username"].as_str(), Some(username));
}

// --------------------------------------------------------------------------
// users_list — the exact-username filter returns the current user, and every
// list item is a valid (full, uncollapsed) user object.
// --------------------------------------------------------------------------

#[tokio::test]
async fn users_list_filters_by_username() {
    let env = skip_unless_live!();
    let me = current_user(&env).await;
    let id = me["id"].as_u64().expect("current user id");
    let username = me["username"].as_str().expect("current user username");

    let (body, _) = users::users_list(
        &env.client,
        users::UsersListParams {
            username: Some(username.to_string()),
            search: None,
            active: None,
            blocked: None,
            external: None,
            humans: None,
            created_after: None,
            created_before: None,
            order_by: None,
            sort: None,
            pagination: pg(None, None),
        },
    )
    .await
    .expect("users_list");
    let items = slim::slim_list(body);
    let arr = items.as_array().expect("items array");

    assert_eq!(arr.len(), 1, "exact-username filter returns one user");
    assert_user_invariants(&arr[0]);
    assert_eq!(arr[0]["id"].as_u64(), Some(id));
    assert_eq!(arr[0]["username"].as_str(), Some(username));
}

// --------------------------------------------------------------------------
// users_keys_list — returns a (possibly empty) array of well-formed SSH keys
// for the current user, resolved by username.
// --------------------------------------------------------------------------

#[tokio::test]
async fn users_keys_list_returns_key_array() {
    let env = skip_unless_live!();
    let me = current_user(&env).await;
    let username = me["username"].as_str().expect("current user username");

    let (body, _) = users::users_keys_list(
        &env.client,
        users::UsersKeysListParams {
            user_id: username.to_string(),
            pagination: pg(None, None),
        },
    )
    .await
    .expect("users_keys_list");
    let items = slim::slim_list(body);
    let arr = items.as_array().expect("keys array");

    // The account may have zero keys; only assert shape on whatever is present.
    for key in arr {
        assert!(key.get("id").and_then(Value::as_u64).is_some(), "key id");
        assert_nonempty_str(key, "title");
        assert_nonempty_str(key, "key");
        assert_no_stripped_keys(key);
    }
}
