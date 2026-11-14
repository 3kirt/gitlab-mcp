Release gitlab-mcp by bumping the minor version, verifying the build, and tagging.

## Steps

### 1. Determine the new version

Read `Cargo.toml` and extract the current `version` field. Compute the next minor
version by incrementing the middle component and resetting the patch to 0
(e.g. `0.5.0` → `0.6.0`). If the user supplied a version as an argument to this
command, use that instead.

### 2. Update Cargo.toml

Change the `version` field in `Cargo.toml` to the new version string.

Also change the `version` field in `plugin/.claude-plugin/plugin.json` to the
same string — the Claude Code plugin's bootstrap script downloads the release
binary matching that version, so it must stay in lockstep.

### 3. Update Cargo.lock

Run `cargo build` (without `--locked`) so Cargo rewrites `Cargo.lock` to match
the new version. This is the only step that runs without `--locked`.

```
cargo build
```

### 4. Audit dependencies

Run these two checks:

```
cargo outdated
cargo audit
```

**Security (`cargo audit`):** If any vulnerabilities are reported, stop immediately and
report them to the user. Do not proceed to the quality gate or tagging until the
vulnerabilities are resolved.

**Outdated dependencies (`cargo outdated`):** Report the results to the user. Focus on
direct dependencies listed in `Cargo.toml` (the top-level rows). If any direct
dependencies are behind, ask the user whether to update them before continuing;
transitive-only updates are informational and do not require confirmation. Either
way, proceed to the quality gate once the user has been informed.

If `cargo outdated` or `cargo audit` are not installed, install them first:

```
cargo install cargo-outdated
cargo install cargo-audit
```

### 5. Run the full quality gate — every check must pass

Run these commands. If any fails, stop and report the error; do not proceed to
tagging.

```
cargo test --all --locked
cargo clippy --locked -- -D warnings
cargo fmt --check
cargo build --release
```

If `cargo fmt --check` fails, run `cargo fmt` to fix formatting, then re-run
`cargo fmt --check` to confirm it passes before continuing.

### 6. Run the live integration tests — they must actually run and pass

The release must also pass the live suite under `src/tools/live/`, which verifies
the tools against a real GitLab instance (the one risk the unit tests structurally
cannot cover — see `docs/testing.md`). These are gated behind the `live-tests`
cargo feature and read `GITLAB_URL` / `GITLAB_TOKEN` (and optionally
`GITLAB_TEST_PROJECT`) from the environment, falling back per variable to a
gitignored `.env` at the repo root.

**Critical:** without credentials the live tests *silently skip and still report
`ok`*, which verifies nothing. So the release process must run them with real
credentials and confirm they actually executed.

1. Credentials come from the repo-root `.env` (template: `.env.example`), which
   the live harness reads directly — no need to export anything. Check it exists
   (`test -f .env`) **without printing its contents**. If it is missing, **stop
   and ask the user** to create it (`cp .env.example .env && chmod 600 .env`, then
   fill in the token) — do not treat a skipped run as a pass. Don't go digging
   for the token elsewhere (e.g. `~/.claude.json`). If the run fails with a
   "world-readable" panic, `chmod 600 .env` and re-run.

2. Run the suite serially (it creates and deletes real resources in the test
   project, so parallel runs against the same project can interfere):

   ```
   cargo test --features live-tests --locked live:: -- --test-threads=1 --nocapture
   ```

   (`make live-test` is equivalent.) It takes several minutes.

3. Confirm from the output that the tests **ran**, not skipped:
   - the `test result` line shows **0 failed** and a passed count equal to the
     number of live tests — count them rather than trusting a fixed number:
     `grep -c '#\[tokio::test\]' src/tools/live/*.rs | awk -F: '{s+=$2} END {print s}'`
     (the harness's one plain `#[test]` parser test runs too, so expect that
     count + 1);
   - there are **no** `SKIP: set GITLAB_URL + GITLAB_TOKEN` notices.

   Some tests skip *by design* and still count as a pass: the License tests print
   `SKIP license_…: … 403` without an admin token, and the pipeline tests print
   `SKIP pipeline_create: … identity verification` if the account can't run CI
   (the test account is CI-verified, so report that one to the user if it
   appears). If any live test fails — or the run skipped for missing
   credentials — stop and report; do not proceed to tagging.

### 7. Update CHANGELOG.md

Read `CHANGELOG.md`. Add a new section for the new version at the top of the
changelog (immediately after the `# Changelog` heading and any introductory
prose, before the previous release). Use today's date.

Derive the content by reading the git log since the previous release tag:

```
git log v<previous version>..HEAD --oneline
```

Group the commits into the standard changelog categories (Added, Changed, Fixed,
Removed, Documentation) — omit any category that has no entries. Exclude
housekeeping commits such as version bumps, `cargo fmt`, and `rustfmt` runs.
Write the entries in the same style as the existing changelog entries.

### 8. Commit the version bump and changelog

Stage `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and
`plugin/.claude-plugin/plugin.json`, then commit:

```
git add Cargo.toml Cargo.lock CHANGELOG.md plugin/.claude-plugin/plugin.json
git commit -m "Bump version to <new version>"
```

### 9. Tag the release

```
git tag v<new version>
```

### 10. Confirm before pushing

Show the user the new version, the commit SHA, and the tag, then ask whether to
push both to origin. If they confirm, run:

```
git push origin main
git push origin v<new version>
```

### 11. Verify the release through the plugin

The tests above run the binary from the checkout, launched by the test
harness. Users get the released binary launched by Claude Code from
`plugin/.mcp.json`, through the bootstrap script and Claude Code's `${VAR}`
expansion. Check that path once the release workflow has published the
assets:

1. Wait for the Release workflow to finish and confirm the GitHub release
   has all four platform binaries plus `checksums.txt`:
   `gh release view v<new version> --json assets --jq '.assets[].name'`.
2. Ask the user to update the plugin (`/plugin` → update, then
   `/reload-plugins`).
3. Call a read-only tool through the plugin's server
   (`mcp__plugin_gitlab-mcp_gitlab__gitlab_metadata_get`) and confirm it
   succeeds. If the user's setup uses a `pass_entry`, confirm the server
   process has no real `GITLAB_TOKEN` (check the variable's presence and
   length; never print its value), so the call actually exercised the pass
   path.

If the call fails, report it to the user as a release regression.
