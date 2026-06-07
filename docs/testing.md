# Testing

This project tests at two layers that verify **different risks**. Neither
replaces the other; together they cover the whole surface.

| Layer | Lives in | Hits network? | Verifies |
|---|---|---|---|
| **Unit tests** | `#[cfg(test)] mod tests` in each `src/**.rs` | No (wiremock or pure logic) | Our code against *our* assumptions — request construction, response transforms, slimming, pagination, config parsing |
| **Live integration tests** | `src/tools/live/` (feature-gated) | Yes — a real GitLab instance | Fidelity to the *actual* GitLab API — param names, body shapes, response shapes, tier/licensing behavior |

## Why two layers

A unit test mocks GitLab, so **the mock is our assumption**. It can prove "given
params X we build request Y and transform response Z correctly," which is
excellent regression protection — but it can never prove GitLab actually accepts
request Y or returns response Z. That fidelity check only a live call can make.

The corollary matters for planning: increasing unit coverage drives down
*regression* risk but has a hard ceiling on *contract* risk. You can reach 100%
unit coverage and still be calling GitLab wrong. So we keep a thin live layer
whose job is exactly the part unit tests structurally cannot reach.

A concrete example this split caught: the live link-embed test got a
`403 "Blocked issues not available for current license"` from gitlab.com Free,
because `blocks`/`is_blocked_by` link types are Premium-gated. A wiremock test
would have happily accepted `"blocks"` — because the mock was our assumption.

## Layer 1 — Unit tests

155 tests today, run by default with `cargo test`. Two flavors:

**Pure-logic tests** — no HTTP. Cover the transform/builder helpers directly:
`src/tools/slim.rs` (field stripping, user collapsing), `src/config.rs` (config
loading, permission and HTTPS checks), `src/tools/discussions.rs` (the nested
`position` object builder).

**Wiremock HTTP tests** — stand up an in-process mock server and assert the
exact request we send and how we handle the response. The pattern is uniform
across `src/client.rs` and every domain module:

```rust
use wiremock::matchers::{method, path, query_param, body_json};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_client(server: &MockServer) -> GitlabClient {
    GitlabClient::new(server.uri(), "test-token").unwrap()
}

#[tokio::test]
async fn issue_get_embeds_links_and_closed_by() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects/mygroup%2Fmyrepo/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue_json(7)))
        .mount(&server)
        .await;
    // ... mount the embedded /links and /closed_by endpoints ...

    let item = issue_get(&mock_client(&server), params).await.unwrap();
    assert_eq!(item["linked_issues"][0]["link_type"], "blocks");
}
```

What these reliably cover:

- **Request construction** — URL path, namespace encoding (`%2F`), query params,
  JSON body shape. (See `src/tools/issues.rs` tests.)
- **Response handling** — status→error mapping, 204→`Null`, pagination header
  extraction, body truncation. (See `src/client.rs` tests.)
- **Enrichment + error semantics** — e.g. `issue_get` embeds `linked_issues` /
  `closed_by`, tolerates a 404 on the supplemental fetch (`[]`), but propagates
  a 500. (See `issue_get_*` tests in `src/tools/issues.rs`.)

To add unit coverage for a new domain, copy the `mock_client` helper and the
`Mock::given(...).and(path(...)).respond_with(...)` shape from any existing
module's `mod tests`.

## Layer 2 — Live integration tests

A deterministic suite that verifies the tools against a real GitLab instance —
the one risk the unit tests structurally cannot cover. Covers the **Issues**
(plus issue links, notes, and discussions), **Merge Requests**, **MR
Discussions**, **Branches**, **Repository Files**, **Emoji Reactions**, and
**Snippets** domains so far. The suite lives under
[`src/tools/live/`](../src/tools/live/), one module per API area plus a shared
`harness`.

The MR tests also exercise the seed pattern for resources that need git state:
`file_create` with `start_branch` creates a source branch *and* a
differentiating commit in one call, and the merge test targets a throwaway base
branch so `main` is never modified. They also poll `detailed_merge_status` and
retry the merge on a transient `405`, since GitLab computes mergeability
asynchronously and briefly rejects an early merge even once the status reads
ready.

### How it's wired

- **Cargo feature `live-tests`** (`Cargo.toml`) gates compilation. Default
  `cargo test` never builds or runs these, so the everyday loop stays fast and
  offline.
- **Located inside the `tools` module** (`#[cfg(all(test, feature = "live-tests"))]
  mod live;` in `src/tools/mod.rs`), *not* in a top-level `tests/` directory.
  This is deliberate: an external `tests/` crate can only see the public API, but
  the live tests need the private `slim` module and the `pub(crate)`
  `PaginationParams` to reproduce the server's exact output. Placing them as a
  child of `tools` grants that access **without widening the crate's public
  surface**. The single `#[cfg]` on `mod live;` gates the whole subtree, so each
  area module under `live/` inherits the gating without repeating it.
- **One module per API area, plus a shared `harness`.** `live/harness.rs` holds
  the environment/credentials (`LiveEnv`, `live_env`), the `skip_unless_live!`
  macro, `run_tag`/`pg`, and the cross-domain invariants
  (`assert_no_stripped_keys`, `assert_user_collapsed`, …); each area file
  (`live/issues.rs`, `live/merge_requests.rs`) does `use super::harness::*` and
  holds that area's seeding helpers, area-specific invariants, and tests.

### Design properties

- **Tests the server's real output path.** Helpers run the domain function *and*
  apply `slim::slim_get` / `slim::slim_list` — the same transforms
  `json_result` / `json_list_result` apply at the rmcp boundary — so assertions
  run against exactly what an MCP client receives (e.g. `description` stripped
  from list items but present on single-get).
- **Self-seeding and self-cleaning.** Each test creates the resources it needs
  with a unique `run_tag()` and deletes them in teardown. No reliance on
  pre-seeded state; runs are idempotent and repeatable.
- **Invariants-as-code.** Helpers like `assert_issue_get_invariants` /
  `assert_issue_list_item_invariants` encode each resource type's expected shape
  (the "universal invariants": identifying fields present, stripped keys absent,
  users collapsed, …) as reusable assertions instead of prose a human eyeballs.
- **Skips without credentials.** `skip_unless_live!` returns early (printing a
  notice) when `GITLAB_URL`/`GITLAB_TOKEN` are absent, so the feature is safe to
  enable in CI without secrets — supply credentials in a dedicated job to
  actually exercise it.

### Running the live tests

```sh
GITLAB_URL=https://gitlab.com \
GITLAB_TOKEN=glpat-xxxxxxxxxxxxxxxxxxxx \
GITLAB_TEST_PROJECT=3kirt1/gitlab-mcp-testing \
  cargo test --features live-tests -- --test-threads=1
```

- `GITLAB_TEST_PROJECT` is optional; defaults to `3kirt1/gitlab-mcp-testing`.
- `--test-threads=1` avoids interleaving create/delete traffic against the same
  project. (Each test is self-isolated by `run_tag()`, but serial keeps output
  and rate-limiting predictable.)
- The credentials must belong to an account with write access to the test
  project — tests create and delete real issues there.

## Coverage

The live suite is being grown domain-by-domain. Covered today: Issues (including
issue links, issue notes, and issue discussions), Merge Requests, MR Discussions,
Branches, Repository Files, Snippets (personal snippets), and Emoji Reactions
(the issue, issue-note, and MR awardable types — MR-note and snippet awardables
remain). Not yet automated: pipeline schedules and the read-only families
(commits, repository tree/compare, search, runners, jobs, pipelines).

**Epics** are Premium/Ultimate-only. The standing test token is on a Free-tier
gitlab.com account, so epic operations return `403`/`404` there; epics can only be
exercised live once a Premium test instance is available.

## Command reference

```sh
cargo test --all --locked                  # unit tests (live tests excluded)
cargo test --features live-tests …         # + live tests (needs credentials; see above)
cargo clippy --features live-tests --tests --locked -- -D warnings
```

## Adding a new domain to the live layer

1. Add a `src/tools/live/<area>.rs` module (declared in `src/tools/live/mod.rs`)
   covering that API area's operations (list/get/create/update/delete and any
   embeds or domain-specific actions).
2. `use super::harness::*` for the shared bits (`live_env`, `skip_unless_live!`,
   `run_tag`, `pg`, the cross-domain invariants) and add an area-specific
   invariants helper for the domain's response shape.
3. Make every test self-seed and self-clean — create what you need, delete it in
   teardown — so the suite stays idempotent.
4. Expect tier/licensing surprises (like the Premium-gated link types): they are
   precisely the fidelity gaps unit tests cannot surface, and the reason the live
   layer exists.
