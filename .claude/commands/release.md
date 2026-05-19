Release gitlab-mcp by bumping the minor version, verifying the build, and tagging.

## Steps

### 1. Determine the new version

Read `Cargo.toml` and extract the current `version` field. Compute the next minor
version by incrementing the middle component and resetting the patch to 0
(e.g. `0.5.0` → `0.6.0`). If the user supplied a version as an argument to this
command, use that instead.

### 2. Update Cargo.toml

Change the `version` field in `Cargo.toml` to the new version string.

### 3. Update Cargo.lock

Run `cargo build` (without `--locked`) so Cargo rewrites `Cargo.lock` to match
the new version. This is the only step that runs without `--locked`.

```
cargo build
```

### 4. Run the full quality gate — all four checks must pass

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

### 5. Update CHANGELOG.md

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

### 6. Commit the version bump and changelog

Stage `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`, then commit:

```
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "Bump version to <new version>"
```

### 7. Tag the release

```
git tag v<new version>
```

### 8. Confirm before pushing

Show the user the new version, the commit SHA, and the tag, then ask whether to
push both to origin. If they confirm, run:

```
git push origin main
git push origin v<new version>
```
