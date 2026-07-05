//! Live tests for MCP prompts.
//!
//! One seeded MR exercises both prompt builders end-to-end: `review-mr` is the
//! only caller of the `/merge_requests/:iid/diffs` endpoint (not covered by any
//! tool), and `create-mr-description` drives `repo_compare` against the real
//! default branch.

use crate::tools::prompts::{
    CreateMrDescriptionArgs, ReviewMrArgs, create_mr_description, review_mr,
};

use super::harness::{delete_branch, delete_mr, run_tag, seed_mr, skip_unless_live};

#[tokio::test]
async fn prompts_load_real_mr_context() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let (mr_iid, branch) = seed_mr(&env, &tag).await;

    // review-mr: the seeded MR adds livetest/{branch}.txt, which must appear
    // in the embedded diff.
    let review = review_mr(
        &env.client,
        ReviewMrArgs {
            project_id: env.project.clone().into(),
            merge_request_iid: mr_iid.to_string(),
        },
    )
    .await;

    // create-mr-description: no target_branch, so it must resolve the
    // project's real default branch and compare against it.
    let draft = create_mr_description(
        &env.client,
        CreateMrDescriptionArgs {
            project_id: env.project.clone().into(),
            branch: branch.clone(),
            target_branch: None,
        },
    )
    .await;

    delete_mr(&env, mr_iid).await;
    delete_branch(&env, &branch).await;

    let review = review.expect("review_mr");
    let review_text = review.messages[0]
        .content
        .as_text()
        .expect("text")
        .text
        .as_str();
    assert!(
        review_text.contains(&format!("livetest/{branch}.txt")),
        "review prompt embeds the MR diff"
    );

    let draft = draft.expect("create_mr_description");
    let draft_text = draft.messages[0]
        .content
        .as_text()
        .expect("text")
        .text
        .as_str();
    assert!(
        draft_text.contains(&format!("seed {branch}")),
        "draft prompt lists the seeded commit"
    );
    assert!(
        draft_text.contains(&format!("livetest/{branch}.txt")),
        "draft prompt embeds the compare diff"
    );
}
