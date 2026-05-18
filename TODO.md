# TODO

Findings from the codebase review. Ordered by impact.

## ~~1. Add a `BodyBuilder` to deduplicate JSON body construction~~ ✓

Every create/update/merge handler repeats this pattern:

```rust
let mut body = json!({ "title": p.title });
let obj = body.as_object_mut().unwrap();
if let Some(v) = p.description { obj.insert("description".into(), json!(v)); }
if let Some(v) = p.labels      { obj.insert("labels".into(), json!(v)); }
// ... 5–10 more identical blocks
```

About 14 functions and ~120 such blocks across `issues.rs`, `merge_requests.rs`, `commits.rs`, `repositories.rs`, `repository_files.rs`.

Mirror `QueryBuilder` in [src/tools/mod.rs](src/tools/mod.rs):

```rust
let body = BodyBuilder::new()
    .req("title", &p.title)
    .opt("description", p.description)
    .opt("labels", p.labels)
    .opt_serialize("assignee_ids", p.assignee_ids)
    .build();
```

Removes ~200 lines and 30+ `.unwrap()` calls.

## ~~2. Consolidate slash-to-%2F encoders~~ ✓

Three identical helpers:

- `encode_branch_name` in [src/tools/branches.rs:7-9](src/tools/branches.rs#L7-L9)
- `encode_ref` in [src/tools/commits.rs:7-9](src/tools/commits.rs#L7-L9)
- `encode_file_path` in [src/tools/repository_files.rs:7-9](src/tools/repository_files.rs#L7-L9)

All are `s.replace('/', "%2F")`. Promote one (e.g. `encode_path_segment`) into [src/tools/mod.rs](src/tools/mod.rs) next to `encode_project_id` and delete the others.

## 3. Fix `enforce_https` localhost prefix bypass (security)

[src/config.rs:97-110](src/config.rs#L97-L110) uses bare prefix matching:

```rust
let is_local = url.starts_with("http://localhost") || url.starts_with("http://127.0.0.1");
```

`http://localhost.evil.com` matches this prefix and bypasses HTTPS enforcement, allowing the GitLab token to leave the machine in plaintext. Same applies to `http://127.0.0.1.evil.com`.

Fix: parse with `url::Url` (already a dependency) and compare `host_str()` exactly against `"localhost"` / `"127.0.0.1"`, or require the following char to be `:`, `/`, or end-of-string.

## 4. Merge `get_text` and `get_text_with_params`

[src/client.rs:103-128](src/client.rs#L103-L128) has two near-identical methods. `get_text` is `get_text_with_params(path, &[])`. Collapse to one method.

## 5. Unify the five `delegate_*` macros

[src/tools/mod.rs:99-150](src/tools/mod.rs#L99-L150) — `delegate_list!`, `delegate_get!`, `delegate_create!`, `delegate_update!`, `delegate_delete!` only differ by verb. Collapse to one parameterized macro plus a separate `delegate_delete!` (which has a different success branch).

## 6. Rename or restructure `GitlabClient::list`

Several callers use `client.list(...)` for endpoints that return a single object, not a list:

- `commit_get` in [src/tools/commits.rs:208-218](src/tools/commits.rs#L208-L218)
- `pipeline_get_latest` in [src/tools/pipelines.rs:128-138](src/tools/pipelines.rs#L128-L138)
- `repo_compare` in [src/tools/repositories.rs:132-148](src/tools/repositories.rs#L132-L148)
- `file_get` in [src/tools/repository_files.rs:31-41](src/tools/repository_files.rs#L31-L41)

The method works (it's GET-with-query-params) but the name lies. Rename to `get_with_params`, or add it as a sibling and reserve `list` for actual list endpoints.

## 7. Investigate `use GitlabError as _;`

[src/tools/mod.rs:948-950](src/tools/mod.rs#L948-L950):

```rust
#[allow(unused_imports)]
use GitlabError as _;
```

The comment claims it's needed for macro expansions, but `to_tool_message` is an inherent method and doesn't need the type in scope. Try removing — if it builds, delete.

## 8. Verify `#[allow(dead_code)]` on router fields

[src/tools/mod.rs:162-165](src/tools/mod.rs#L162-L165) — `tool_router` and `prompt_router` are populated by macros, presumably read by `#[tool_handler]`/`#[prompt_handler]` expansions. Confirm the attribute is still required by the current `rmcp` version; remove if not.

## 9. Dedupe `project_id` schema description (low priority)

The description `"Project ID or URL-encoded path"` appears in ~30 param structs with slight wording variations. A shared newtype would dedupe but changes the generated JSON schema unless `#[serde(transparent)]` is used. Lower priority — only worth doing if doing it doesn't change the wire format.

## 10. Rename `tool_error` (cosmetic)

[src/tools/mod.rs:46-48](src/tools/mod.rs#L46-L48) — name suggests an error return but the signature is `Result<CallToolResult, McpError>` returning `Ok(...)`. Consider `tool_error_response` for clarity, or inline.
