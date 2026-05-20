use serde_json::{Map, Value};

/// Fields stripped from every response (list items and single-gets).
const STRIP_ALWAYS: &[&str] = &[
    // UI navigation links — agents navigate via web_url and iid, not these.
    "_links",
    // short/relative/full variants of the iid — always redundant.
    "references",
];

/// Additional fields stripped from each item in a list response.
/// These are expensive in bulk but available on demand via single-get endpoints.
const STRIP_LIST_ITEM: &[&str] = &[
    "description",   // can be 0–100 KB; use gitlab_issues_get / gitlab_mrs_get when needed
    "pipeline",      // 2–5 KB per MR; use gitlab_pipelines_get
    "head_pipeline", // same
    "diff_stats",    // addition/deletion counts; use single-get if needed
    "time_stats",    // nearly always zeros; use single-get if needed
    // Issue-specific noise fields — almost always zero/false/duplicate/unknown.
    "assignees",             // array duplicates the scalar `assignee` field
    "blocking_issues_count", // almost always 0
    "confidential",          // almost always false
    "downvotes",             // rarely relevant in agentic workflows
    "has_tasks",             // redundant with `task_completion_status`
    "imported",              // almost always false
    "imported_from",         // almost always "none"
    "issue_type",            // duplicate of `type` (uppercase variant)
    "severity",              // almost always "UNKNOWN"
    "task_status",           // human-readable string duplicating `task_completion_status`
    "upvotes",               // rarely relevant in agentic workflows
];

/// Keys kept when collapsing a GitLab user object.
/// Full user objects carry avatar_url, web_url, state, etc. that agents never use.
const USER_KEEP_KEYS: &[&str] = &["id", "username", "name"];

/// Heavy slim for list responses. Strips list-only keys from each top-level array element
/// and applies light slim (null removal, always-strip keys, user collapsing) recursively.
pub fn slim_list(v: Value) -> Value {
    match v {
        Value::Array(arr) => Value::Array(arr.into_iter().map(slim_list_item).collect()),
        other => slim_nested(other),
    }
}

/// Light slim for single-get, create, and update responses.
/// Removes nulls, strips always-strip keys, collapses nested user objects.
pub fn slim_get(v: Value) -> Value {
    slim_nested(v)
}

fn slim_list_item(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let map: Map<_, _> = map
                .into_iter()
                .filter(|(k, v)| {
                    !v.is_null()
                        && !STRIP_ALWAYS.contains(&k.as_str())
                        && !STRIP_LIST_ITEM.contains(&k.as_str())
                })
                .map(|(k, v)| (k, slim_nested(v)))
                .collect();
            Value::Object(map)
        }
        other => slim_nested(other),
    }
}

fn slim_nested(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            if is_user_object(&map) {
                return collapse_user(map);
            }
            let map: Map<_, _> = map
                .into_iter()
                .filter(|(k, v)| !v.is_null() && !STRIP_ALWAYS.contains(&k.as_str()))
                .map(|(k, v)| (k, slim_nested(v)))
                .collect();
            Value::Object(map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(slim_nested).collect()),
        other => other,
    }
}

/// A GitLab user object has numeric id plus string username and name fields.
fn is_user_object(map: &Map<String, Value>) -> bool {
    map.contains_key("id")
        && matches!(map.get("username"), Some(Value::String(_)))
        && matches!(map.get("name"), Some(Value::String(_)))
}

fn collapse_user(map: Map<String, Value>) -> Value {
    Value::Object(
        map.into_iter()
            .filter(|(k, _)| USER_KEEP_KEYS.contains(&k.as_str()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- slim_list ---

    #[test]
    fn list_strips_list_only_keys() {
        let input = json!([{
            "iid": 1,
            "title": "MR 1",
            "description": "long text",
            "pipeline": {"id": 99, "status": "running"},
            "head_pipeline": {"id": 100},
            "diff_stats": {"additions": 10},
            "time_stats": {"time_estimate": 0},
        }]);
        let out = slim_list(input);
        let item = &out[0];
        assert_eq!(item["iid"], json!(1));
        assert_eq!(item["title"], json!("MR 1"));
        assert!(item.get("description").is_none());
        assert!(item.get("pipeline").is_none());
        assert!(item.get("head_pipeline").is_none());
        assert!(item.get("diff_stats").is_none());
        assert!(item.get("time_stats").is_none());
    }

    #[test]
    fn list_strips_always_keys() {
        let input = json!([{
            "iid": 1,
            "_links": {"self": "https://example.com"},
            "references": {"short": "!1"},
        }]);
        let out = slim_list(input);
        assert!(out[0].get("_links").is_none());
        assert!(out[0].get("references").is_none());
    }

    #[test]
    fn list_removes_null_fields() {
        let input = json!([{"iid": 1, "merged_at": null, "closed_at": null}]);
        let out = slim_list(input);
        assert_eq!(out[0]["iid"], json!(1));
        assert!(out[0].get("merged_at").is_none());
        assert!(out[0].get("closed_at").is_none());
    }

    #[test]
    fn list_collapses_top_level_user_objects() {
        let input = json!([{
            "iid": 1,
            "author": {
                "id": 5,
                "username": "alice",
                "name": "Alice Smith",
                "avatar_url": "https://example.com/avatar.png",
                "web_url": "https://example.com/alice",
                "state": "active",
            },
        }]);
        let out = slim_list(input);
        let author = &out[0]["author"];
        assert_eq!(author["id"], json!(5));
        assert_eq!(author["username"], json!("alice"));
        assert_eq!(author["name"], json!("Alice Smith"));
        assert!(author.get("avatar_url").is_none());
        assert!(author.get("web_url").is_none());
        assert!(author.get("state").is_none());
    }

    #[test]
    fn list_collapses_users_in_nested_arrays() {
        // `assignees` is stripped, so use `participants` to test nested-array user collapsing.
        let input = json!([{
            "iid": 1,
            "participants": [
                {"id": 1, "username": "alice", "name": "Alice", "avatar_url": "https://a.com"},
                {"id": 2, "username": "bob", "name": "Bob", "state": "active"},
            ],
        }]);
        let out = slim_list(input);
        let participants = &out[0]["participants"];
        assert_eq!(participants[0]["username"], json!("alice"));
        assert!(participants[0].get("avatar_url").is_none());
        assert_eq!(participants[1]["username"], json!("bob"));
        assert!(participants[1].get("state").is_none());
    }

    #[test]
    fn list_strips_issue_noise_fields() {
        let input = json!([{
            "iid": 1,
            "title": "Bug",
            "type": "ISSUE",
            "assignee": {"id": 1, "username": "alice", "name": "Alice"},
            "assignees": [{"id": 1, "username": "alice", "name": "Alice"}],
            "blocking_issues_count": 0,
            "confidential": false,
            "downvotes": 0,
            "has_tasks": false,
            "imported": false,
            "imported_from": "none",
            "issue_type": "issue",
            "severity": "UNKNOWN",
            "task_completion_status": {"count": 0, "completed_count": 0},
            "task_status": "0 of 0 checklist items completed",
            "upvotes": 0,
        }]);
        let out = slim_list(input);
        let item = &out[0];
        // Kept fields
        assert_eq!(item["iid"], json!(1));
        assert_eq!(item["title"], json!("Bug"));
        assert_eq!(item["type"], json!("ISSUE"));
        assert!(item.get("assignee").is_some());
        assert!(item.get("task_completion_status").is_some());
        // Stripped fields
        assert!(item.get("assignees").is_none());
        assert!(item.get("blocking_issues_count").is_none());
        assert!(item.get("confidential").is_none());
        assert!(item.get("downvotes").is_none());
        assert!(item.get("has_tasks").is_none());
        assert!(item.get("imported").is_none());
        assert!(item.get("imported_from").is_none());
        assert!(item.get("issue_type").is_none());
        assert!(item.get("severity").is_none());
        assert!(item.get("task_status").is_none());
        assert!(item.get("upvotes").is_none());
    }

    #[test]
    fn get_keeps_issue_noise_fields() {
        // Issue noise fields must survive slim_get (only stripped from list responses).
        let input = json!({
            "iid": 1,
            "confidential": false,
            "severity": "UNKNOWN",
            "task_status": "0 of 0 checklist items completed",
            "assignees": [{"id": 1, "username": "alice", "name": "Alice"}],
        });
        let out = slim_get(input);
        assert!(out.get("confidential").is_some());
        assert!(out.get("severity").is_some());
        assert!(out.get("task_status").is_some());
        assert!(out.get("assignees").is_some());
    }

    #[test]
    fn list_empty_array_passthrough() {
        assert_eq!(slim_list(json!([])), json!([]));
    }

    // --- slim_get ---

    #[test]
    fn get_keeps_description_and_pipeline() {
        let input = json!({
            "iid": 1,
            "description": "important details",
            "pipeline": {"id": 99, "status": "running"},
        });
        let out = slim_get(input);
        assert_eq!(out["description"], json!("important details"));
        assert_eq!(out["pipeline"]["id"], json!(99));
    }

    #[test]
    fn get_strips_always_keys_and_nulls() {
        let input = json!({
            "iid": 1,
            "_links": {"self": "https://example.com"},
            "references": {"short": "!1"},
            "closed_at": null,
        });
        let out = slim_get(input);
        assert!(out.get("_links").is_none());
        assert!(out.get("references").is_none());
        assert!(out.get("closed_at").is_none());
        assert_eq!(out["iid"], json!(1));
    }

    #[test]
    fn get_collapses_nested_user_objects() {
        let input = json!({
            "iid": 1,
            "author": {
                "id": 5,
                "username": "alice",
                "name": "Alice",
                "avatar_url": "https://example.com/avatar.png",
            },
        });
        let out = slim_get(input);
        assert!(out["author"].get("avatar_url").is_none());
        assert_eq!(out["author"]["username"], json!("alice"));
    }

    // --- user detection edge cases ---

    #[test]
    fn non_user_objects_not_collapsed() {
        let input = json!([{
            "diff_refs": {
                "base_sha": "abc",
                "head_sha": "def",
                "start_sha": "ghi",
            },
        }]);
        let out = slim_list(input);
        assert_eq!(out[0]["diff_refs"]["base_sha"], json!("abc"));
        assert_eq!(out[0]["diff_refs"]["head_sha"], json!("def"));
    }

    #[test]
    fn object_with_only_username_not_collapsed() {
        // Must have id + username + name to be treated as a user object.
        let input = json!([{
            "meta": {"username": "alice"},
        }]);
        let out = slim_list(input);
        assert_eq!(out[0]["meta"]["username"], json!("alice"));
    }

    #[test]
    fn nulls_preserved_inside_arrays() {
        // null elements inside an array (not object fields) should survive.
        let input = json!([{"tags": [null, "bug"]}]);
        let out = slim_list(input);
        assert_eq!(out[0]["tags"], json!([null, "bug"]));
    }
}
