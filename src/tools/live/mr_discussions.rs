//! Live tests for the Merge Request Discussions domain (threaded comments on an
//! MR, plus the MR-specific resolve/unresolve).
//!
//! Seeds a real MR (throwaway branch off `main` with a diff), exercises the
//! thread, then deletes the MR and branch in teardown so `main` is untouched.

use serde_json::Value;

use crate::tools::{discussions, slim};

use super::harness::{
    LiveEnv, assert_note_invariants, delete_branch, delete_mr, discussion_note_count, pg, run_tag,
    seed_mr, skip_unless_live,
};

// --------------------------------------------------------------------------
// MR discussion helpers
// --------------------------------------------------------------------------

async fn get_discussion(env: &LiveEnv, mr_iid: u64, discussion_id: &str) -> Value {
    slim::slim_get(
        discussions::mr_discussion_get(
            &env.client,
            discussions::MrDiscussionGetParams {
                project_id: env.project.clone().into(),
                merge_request_iid: mr_iid.into(),
                discussion_id: discussion_id.to_string(),
            },
        )
        .await
        .expect("mr_discussion_get"),
    )
}

async fn resolve(env: &LiveEnv, mr_iid: u64, discussion_id: &str, resolved: bool) -> Value {
    slim::slim_get(
        discussions::mr_discussion_resolve(
            &env.client,
            discussions::MrDiscussionResolveParams {
                project_id: env.project.clone().into(),
                merge_request_iid: mr_iid.into(),
                discussion_id: discussion_id.to_string(),
                resolved,
            },
        )
        .await
        .expect("mr_discussion_resolve"),
    )
}

// --------------------------------------------------------------------------
// Thread CRUD + resolve/unresolve
// --------------------------------------------------------------------------

#[tokio::test]
async fn mr_discussions_crud_and_resolve() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let (mr_iid, branch) = seed_mr(&env, &tag).await;

    // Start a discussion thread. The id is a hex string, not an integer.
    let created = slim::slim_get(
        discussions::mr_discussion_create(
            &env.client,
            discussions::MrDiscussionCreateParams {
                project_id: env.project.clone().into(),
                merge_request_iid: mr_iid.into(),
                body: format!("{tag} thread root"),
                commit_id: None,
                position_base_sha: None,
                position_head_sha: None,
                position_start_sha: None,
                position_type: None,
                position_new_path: None,
                position_old_path: None,
                position_new_line: None,
                position_old_line: None,
            },
        )
        .await
        .expect("create discussion"),
    );
    let discussion_id = created["id"]
        .as_str()
        .expect("discussion id is a string")
        .to_string();
    assert_eq!(discussion_note_count(&created), 1, "root note present");
    assert_eq!(created["notes"][0]["body"], format!("{tag} thread root"));

    // Reply — adds a second note to the same thread.
    let reply = slim::slim_get(
        discussions::mr_discussion_note_create(
            &env.client,
            discussions::MrDiscussionNoteCreateParams {
                project_id: env.project.clone().into(),
                merge_request_iid: mr_iid.into(),
                discussion_id: discussion_id.clone(),
                body: format!("{tag} reply"),
                created_at: None,
            },
        )
        .await
        .expect("reply to discussion"),
    );
    let reply_id = reply["id"].as_u64().expect("reply note id");
    assert_eq!(reply["body"], format!("{tag} reply"));

    // Get the thread — both notes present, author collapsed on each.
    let got = get_discussion(&env, mr_iid, &discussion_id).await;
    assert_eq!(got["id"].as_str(), Some(discussion_id.as_str()));
    assert_eq!(discussion_note_count(&got), 2, "root + reply");
    for note in got["notes"].as_array().unwrap() {
        assert_note_invariants(note);
    }

    // List — our discussion must appear (alongside any system threads).
    let (body, _) = discussions::mr_discussions_list(
        &env.client,
        discussions::MrDiscussionsListParams {
            project_id: env.project.clone().into(),
            merge_request_iid: mr_iid.into(),
            pagination: pg(None, None),
        },
    )
    .await
    .expect("list discussions");
    let listed = slim::slim_list(body);
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"].as_str() == Some(discussion_id.as_str())),
        "created discussion must appear in the list"
    );

    // Resolve then unresolve — the MR-specific capability. A body-only MR thread
    // is resolvable, so the notes' `resolved` flag toggles.
    let resolved = resolve(&env, mr_iid, &discussion_id, true).await;
    assert_eq!(resolved["notes"][0]["resolved"], true, "thread resolved");
    let unresolved = resolve(&env, mr_iid, &discussion_id, false).await;
    assert_eq!(
        unresolved["notes"][0]["resolved"], false,
        "thread unresolved"
    );

    // Edit the reply note within the thread.
    let edited = slim::slim_get(
        discussions::mr_discussion_note_update(
            &env.client,
            discussions::MrDiscussionNoteUpdateParams {
                project_id: env.project.clone().into(),
                merge_request_iid: mr_iid.into(),
                discussion_id: discussion_id.clone(),
                note_id: reply_id.into(),
                body: Some(format!("{tag} reply edited")),
                resolved: None,
            },
        )
        .await
        .expect("edit discussion note"),
    );
    assert_eq!(edited["body"], format!("{tag} reply edited"));

    // Delete the reply — thread drops back to a single note.
    discussions::mr_discussion_note_delete(
        &env.client,
        discussions::MrDiscussionNoteDeleteParams {
            project_id: env.project.clone().into(),
            merge_request_iid: mr_iid.into(),
            discussion_id: discussion_id.clone(),
            note_id: reply_id.into(),
        },
    )
    .await
    .expect("delete discussion note");
    let after = get_discussion(&env, mr_iid, &discussion_id).await;
    assert_eq!(
        discussion_note_count(&after),
        1,
        "reply removed, root remains"
    );

    delete_mr(&env, mr_iid).await;
    delete_branch(&env, &branch).await;
}
