//! Live integration tests — a deterministic, scriptable replacement for the
//! LLM-driven `docs/testing-protocol.md`.
//!
//! These run against a *real* GitLab instance and verify the one thing wiremock
//! unit tests cannot: fidelity to the actual API (param names, body shapes,
//! response shapes, the slimming/envelope the server emits). They are
//! self-seeding and self-cleaning — every test creates the resources it needs
//! and deletes them in a teardown — so they are idempotent and repeatable, with
//! no reliance on pre-seeded state.
//!
//! The suite lives inside `tools` (rather than a top-level `tests/` crate) so it
//! can reach the private `slim` module and the `pub(crate)` helpers without
//! widening the crate's public surface. The whole subtree is gated at the
//! `mod live;` declaration in `tools/mod.rs`, so it compiles only under
//! `cargo test --features live-tests`.
//!
//! Layout: [`harness`] holds the shared environment, skip macro, and
//! cross-domain invariants; each remaining module is one API area.
//!
//! Run with:
//! ```sh
//! GITLAB_URL=https://gitlab.com \
//! GITLAB_TOKEN=glpat-xxx \
//! GITLAB_TEST_PROJECT=3kirt1/gitlab-mcp-testing \
//!   cargo test --features live-tests -- --test-threads=1
//! ```
//! Absent `GITLAB_URL`/`GITLAB_TOKEN`, each test prints a skip notice and
//! passes, so the feature is safe to enable in CI without credentials.

mod harness;

mod branches;
mod issues;
mod merge_requests;
mod mr_discussions;
mod repository_files;
