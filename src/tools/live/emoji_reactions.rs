//! Live tests for the Emoji Reactions (award emoji) domain. Reactions attach to
//! many awardable types; this exercises three representative ones — issue,
//! issue note, and merge request — covering list/get/create/delete each.

use serde_json::Value;

use crate::tools::{emoji_reactions, issue_notes, slim};

use super::harness::{
    LiveEnv, assert_no_stripped_keys, assert_nonempty_str, assert_user_collapsed, delete_branch,
    delete_issue, delete_mr, pg, run_tag, seed_issue, seed_mr, skip_unless_live,
};

/// Invariants for an award-emoji object (single-get / create / list item).
fn assert_award_invariants(award: &Value, expected_name: &str) {
    assert!(award.get("id").and_then(Value::as_u64).is_some(), "award id");
    assert_eq!(award["name"], expected_name);
    assert_user_collapsed(&award["user"]);
    assert_nonempty_str(award, "awardable_type");
    assert!(
        award.get("awardable_id").and_then(Value::as_u64).is_some(),
        "awardable_id"
    );
    assert_no_stripped_keys(award);
}

// --------------------------------------------------------------------------
// Issue + issue-note reactions
// --------------------------------------------------------------------------

#[tokio::test]
async fn issue_and_note_emoji_crud() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let iid = seed_issue(&env, &format!("{tag} emoji")).await;

    // --- Reaction on the issue itself ---
    let created = slim::slim_get(
        emoji_reactions::issue_emoji_create(
            &env.client,
            emoji_reactions::IssueEmojiCreateParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                name: "thumbsup".into(),
            },
        )
        .await
        .expect("create issue emoji"),
    );
    assert_award_invariants(&created, "thumbsup");
    assert_eq!(created["awardable_type"], "Issue");
    let award_id = created["id"].as_u64().unwrap();

    let got = slim::slim_get(
        emoji_reactions::issue_emoji_get(
            &env.client,
            emoji_reactions::IssueEmojiGetParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                award_id,
            },
        )
        .await
        .expect("get issue emoji"),
    );
    assert_eq!(got["id"].as_u64().unwrap(), award_id);

    assert!(
        issue_emoji_ids(&env, iid).await.contains(&award_id),
        "reaction present in list"
    );

    emoji_reactions::issue_emoji_delete(
        &env.client,
        emoji_reactions::IssueEmojiDeleteParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            award_id,
        },
    )
    .await
    .expect("delete issue emoji");
    assert!(
        issue_emoji_ids(&env, iid).await.is_empty(),
        "reaction removed"
    );

    // --- Reaction on a note attached to the issue ---
    let note = issue_notes::issue_note_create(
        &env.client,
        issue_notes::IssueNoteCreateParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            body: format!("{tag} note"),
            created_at: None,
        },
    )
    .await
    .expect("create note");
    let note_id = note["id"].as_u64().unwrap();

    let created = slim::slim_get(
        emoji_reactions::issue_note_emoji_create(
            &env.client,
            emoji_reactions::IssueNoteEmojiCreateParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                note_id,
                name: "heart".into(),
            },
        )
        .await
        .expect("create note emoji"),
    );
    assert_award_invariants(&created, "heart");
    assert_eq!(created["awardable_type"], "Note");
    let note_award_id = created["id"].as_u64().unwrap();

    let got = slim::slim_get(
        emoji_reactions::issue_note_emoji_get(
            &env.client,
            emoji_reactions::IssueNoteEmojiGetParams {
                project_id: env.project.clone(),
                issue_iid: iid,
                note_id,
                award_id: note_award_id,
            },
        )
        .await
        .expect("get note emoji"),
    );
    assert_eq!(got["name"], "heart");

    let (body, _) = emoji_reactions::issue_note_emoji_list(
        &env.client,
        emoji_reactions::IssueNoteEmojiListParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            note_id,
            pagination: pg(None, None),
        },
    )
    .await
    .expect("list note emoji");
    assert!(
        slim::slim_list(body)
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"].as_u64() == Some(note_award_id)),
        "note reaction present"
    );

    emoji_reactions::issue_note_emoji_delete(
        &env.client,
        emoji_reactions::IssueNoteEmojiDeleteParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            note_id,
            award_id: note_award_id,
        },
    )
    .await
    .expect("delete note emoji");

    delete_issue(&env, iid).await;
}

/// The award ids currently on an issue (slimmed list path).
async fn issue_emoji_ids(env: &LiveEnv, iid: u64) -> Vec<u64> {
    let (body, _) = emoji_reactions::issue_emoji_list(
        &env.client,
        emoji_reactions::IssueEmojiListParams {
            project_id: env.project.clone(),
            issue_iid: iid,
            pagination: pg(None, None),
        },
    )
    .await
    .expect("list issue emoji");
    slim::slim_list(body)
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["id"].as_u64())
        .collect()
}

// --------------------------------------------------------------------------
// Merge request reactions
// --------------------------------------------------------------------------

#[tokio::test]
async fn mr_emoji_crud() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let (mr_iid, branch) = seed_mr(&env, &tag).await;

    let created = slim::slim_get(
        emoji_reactions::mr_emoji_create(
            &env.client,
            emoji_reactions::MrEmojiCreateParams {
                project_id: env.project.clone(),
                merge_request_iid: mr_iid,
                name: "rocket".into(),
            },
        )
        .await
        .expect("create mr emoji"),
    );
    assert_award_invariants(&created, "rocket");
    assert_eq!(created["awardable_type"], "MergeRequest");
    let award_id = created["id"].as_u64().unwrap();

    let got = slim::slim_get(
        emoji_reactions::mr_emoji_get(
            &env.client,
            emoji_reactions::MrEmojiGetParams {
                project_id: env.project.clone(),
                merge_request_iid: mr_iid,
                award_id,
            },
        )
        .await
        .expect("get mr emoji"),
    );
    assert_eq!(got["id"].as_u64().unwrap(), award_id);

    let (body, _) = emoji_reactions::mr_emoji_list(
        &env.client,
        emoji_reactions::MrEmojiListParams {
            project_id: env.project.clone(),
            merge_request_iid: mr_iid,
            pagination: pg(None, None),
        },
    )
    .await
    .expect("list mr emoji");
    assert!(
        slim::slim_list(body)
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"].as_u64() == Some(award_id)),
        "mr reaction present"
    );

    emoji_reactions::mr_emoji_delete(
        &env.client,
        emoji_reactions::MrEmojiDeleteParams {
            project_id: env.project.clone(),
            merge_request_iid: mr_iid,
            award_id,
        },
    )
    .await
    .expect("delete mr emoji");

    delete_mr(&env, mr_iid).await;
    delete_branch(&env, &branch).await;
}
