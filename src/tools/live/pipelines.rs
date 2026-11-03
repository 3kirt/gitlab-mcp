//! Pipelines & jobs: the blocking wait tools (`gitlab_pipelines_wait`,
//! `gitlab_jobs_wait`) against real pipelines.
//!
//! Each test commits a throwaway `.gitlab-ci.yml` to its own branch, triggers
//! a pipeline through `pipeline_create`, waits on it, then tears everything
//! down *before* asserting so a failure never leaves pipelines behind.
//!
//! The CI file's `workflow:` rule admits only API-sourced pipelines, so the
//! seeding commit (a `push` source) doesn't start a second, competing pipeline.

use serde_json::{Value, json};

use super::harness::*;
use crate::client::GitlabError;
use crate::tools::{TERMINAL_STATUSES, jobs, pipelines, repository_files};

/// Commit `ci_yaml` as `.gitlab-ci.yml` on a new branch off `main`.
async fn seed_ci_branch(env: &LiveEnv, branch: &str, ci_yaml: &str) {
    repository_files::file_create(
        &env.client,
        repository_files::FileCreateParams {
            project_id: env.project.clone().into(),
            file_path: ".gitlab-ci.yml".into(),
            branch: branch.to_string(),
            commit_message: format!("seed CI config for {branch}"),
            content: ci_yaml.to_string(),
            encoding: None,
            author_name: None,
            author_email: None,
            execute_filemode: None,
            start_branch: Some("main".into()),
        },
    )
    .await
    .expect("seed .gitlab-ci.yml");
}

/// Trigger a pipeline on `branch`, returning its id — or `None` (after
/// deleting the branch and printing a skip notice) when the instance refuses
/// to run CI for this account. gitlab.com answers 400 "Identity verification
/// is required in order to run CI jobs" for unverified accounts, for *every*
/// pipeline, even one with only a manual job. Any other error fails the test
/// (after cleanup).
async fn create_pipeline(env: &LiveEnv, branch: &str) -> Option<u64> {
    let result = pipelines::pipeline_create(
        &env.client,
        pipelines::PipelineCreateParams {
            project_id: env.project.clone().into(),
            ref_: branch.to_string(),
            variables: None,
            inputs: None,
        },
    )
    .await;
    match result {
        Ok(created) => Some(created["id"].as_u64().expect("created pipeline has id")),
        Err(GitlabError::Api { status, body })
            if status == reqwest::StatusCode::BAD_REQUEST
                && body.contains("Identity verification is required") =>
        {
            delete_branch(env, branch).await;
            eprintln!(
                "SKIP pipeline_create: this account can't run CI ({status}: identity \
                 verification required on gitlab.com)"
            );
            None
        }
        Err(e) => {
            delete_branch(env, branch).await;
            panic!("create pipeline: {e:?}");
        }
    }
}

/// Best-effort: cancel (in case it is still running), delete the pipeline,
/// delete the branch. Errors are ignored so cleanup never masks a failure.
async fn teardown(env: &LiveEnv, pipeline_id: u64, branch: &str) {
    let project_id = || env.project.clone().into();
    let _ = pipelines::pipeline_cancel(
        &env.client,
        pipelines::PipelineCancelParams {
            project_id: project_id(),
            pipeline_id,
        },
    )
    .await;
    let _ = pipelines::pipeline_delete(
        &env.client,
        pipelines::PipelineDeleteParams {
            project_id: project_id(),
            pipeline_id,
        },
    )
    .await;
    delete_branch(env, branch).await;
}

async fn wait_pipeline(env: &LiveEnv, pipeline_id: u64, timeout_seconds: u64) -> Value {
    pipelines::pipeline_wait(
        &env.client,
        pipelines::PipelineWaitParams {
            project_id: env.project.clone().into(),
            pipeline_id,
            timeout_seconds: Some(timeout_seconds),
            poll_interval_seconds: Some(5),
        },
    )
    .await
    .expect("pipeline wait")
}

fn is_terminal(status: &Value) -> bool {
    status
        .as_str()
        .is_some_and(|s| TERMINAL_STATUSES.contains(&s))
}

/// Deterministic, runner-free: a pipeline whose only job is manual settles
/// immediately without a runner. Verifies the pipeline wait returns promptly
/// (no timeout) with the per-job summary, and that the job wait agrees on the
/// job's `manual` status. Note (observed on gitlab.com): because manual jobs
/// default to `allow_failure: true`, the *pipeline* reports `skipped`, not
/// `manual` — only the job is `manual`. Both are terminal for the wait tools.
#[tokio::test]
async fn pipeline_and_job_wait_return_on_manual_gate() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let branch = format!("{tag}-ci-manual");
    seed_ci_branch(
        &env,
        &branch,
        "workflow:\n  rules:\n    - if: $CI_PIPELINE_SOURCE == \"api\"\n\n\
         gate:\n  script: echo \"manual gate\"\n  when: manual\n",
    )
    .await;
    let Some(pipeline_id) = create_pipeline(&env, &branch).await else {
        return;
    };

    let waited = wait_pipeline(&env, pipeline_id, 120).await;
    let gate = waited["jobs"]
        .as_array()
        .and_then(|jobs| jobs.iter().find(|j| j["name"] == "gate").cloned());
    let job_waited = match gate.as_ref().and_then(|g| g["id"].as_u64()) {
        Some(job_id) => Some(
            jobs::job_wait(
                &env.client,
                jobs::JobWaitParams {
                    project_id: env.project.clone().into(),
                    job_id,
                    timeout_seconds: Some(60),
                    poll_interval_seconds: Some(5),
                },
            )
            .await
            .expect("job wait"),
        ),
        None => None,
    };
    teardown(&env, pipeline_id, &branch).await;

    eprintln!(
        "manual-gate pipeline {pipeline_id}: status={} waited={}s",
        waited["status"], waited["waited_seconds"]
    );
    assert_eq!(waited["id"], json!(pipeline_id));
    assert_eq!(waited["timed_out"], json!(false), "{waited}");
    assert!(is_terminal(&waited["status"]), "non-terminal: {waited}");
    assert!(waited["waited_seconds"].is_u64(), "{waited}");

    let gate = gate.unwrap_or_else(|| panic!("job summary lacks `gate`: {waited}"));
    assert_eq!(gate["status"], "manual", "{gate}");
    assert_eq!(gate["stage"], "test", "{gate}");
    assert_eq!(
        gate.as_object().map(serde_json::Map::len),
        Some(4),
        "summary is id/name/stage/status only: {gate}"
    );

    let job_waited = job_waited.expect("job wait ran");
    assert_eq!(job_waited["status"], "manual", "{job_waited}");
    assert_eq!(job_waited["timed_out"], json!(false), "{job_waited}");
}

/// A pipeline with a real job, which needs a runner. Both outcomes satisfy the
/// tool's contract, so the test asserts whichever happened and reports it:
/// - a runner picks it up → the wait returns `success` with `timed_out: false`;
/// - no runner available → the wait gives up at the timeout with
///   `timed_out: true` and a non-terminal status, rather than hanging.
#[tokio::test]
async fn pipeline_wait_follows_real_job_or_times_out() {
    let env = skip_unless_live!();
    let tag = run_tag();
    let branch = format!("{tag}-ci-run");
    seed_ci_branch(
        &env,
        &branch,
        "workflow:\n  rules:\n    - if: $CI_PIPELINE_SOURCE == \"api\"\n\n\
         hello:\n  image: alpine:3.20\n  script: echo \"hello from the wait test\"\n",
    )
    .await;
    let Some(pipeline_id) = create_pipeline(&env, &branch).await else {
        return;
    };

    let waited = wait_pipeline(&env, pipeline_id, 240).await;
    teardown(&env, pipeline_id, &branch).await;

    eprintln!(
        "real-job pipeline {pipeline_id}: status={} timed_out={} waited={}s jobs={}",
        waited["status"], waited["timed_out"], waited["waited_seconds"], waited["jobs"]
    );
    assert_eq!(waited["id"], json!(pipeline_id));
    if waited["timed_out"] == json!(true) {
        assert!(
            !is_terminal(&waited["status"]),
            "timed out yet terminal: {waited}"
        );
        let waited_s = waited["waited_seconds"].as_u64().unwrap();
        assert!(
            (235..=260).contains(&waited_s),
            "should give up near the 240s timeout, waited {waited_s}s"
        );
    } else {
        assert_eq!(waited["status"], "success", "{waited}");
        let hello = waited["jobs"]
            .as_array()
            .and_then(|jobs| jobs.iter().find(|j| j["name"] == "hello"))
            .unwrap_or_else(|| panic!("job summary lacks `hello`: {waited}"));
        assert_eq!(hello["status"], "success", "{hello}");
    }
}
