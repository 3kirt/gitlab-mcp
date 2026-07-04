//! GitLab work items via the GraphQL API (`POST /api/graphql`).
//!
//! Work items are GitLab's unified model for issues, tasks, epics, incidents,
//! objectives, and key results. They are the forward-looking replacement for
//! the deprecated REST Issues/Epics endpoints, so new capability lands here.
//!
//! Two traits set this domain apart from the REST modules:
//!
//! * **Widget architecture.** Beyond `title`/`state`, every attribute
//!   (description, assignees, labels, hierarchy, dates) arrives inside a typed
//!   `widgets[]` array. [`flatten_work_item`] lifts the widgets we care about up
//!   to the top level so callers see a flat object instead of digging through
//!   `widgets`.
//! * **Addressing.** GraphQL addresses namespaces by full path string, so this
//!   module takes `namespace_path` (a project *or* group full path, e.g.
//!   "mygroup/myproject" or "mygroup/subgroup") rather than the numeric-or-path
//!   `project_id`/`group_id` the REST modules accept. The GraphQL queries,
//!   response reads, and mutation inputs are all camelCase (the wire format),
//!   but **output keys are converted to snake_case** (`createdAt` → `created_at`)
//!   by [`snake_case_keys`] at the output boundary so they match the REST tools.
//!
//! Covers get, list, create, update, and delete. The mutations carry the extra
//! GraphQL machinery the REST modules don't need: [`resolve_work_item_type_id`]
//! (type name → `WorkItems::Type` GID, required by create),
//! [`resolve_work_item_gid`] (namespace IID → `WorkItem` GID, required by
//! update/delete), and [`check_mutation_errors`] — because GitLab reports
//! business-logic failures in a payload `errors` array while returning HTTP 200,
//! a channel `GitlabClient::graphql` cannot catch on its own.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitlabClient, GitlabError};

// --------------------------------------------------------------------------
// Shared GraphQL fragment + response shaping
// --------------------------------------------------------------------------

/// The work-item field selection shared by the get and list queries. Kept as a
/// fragment body (no surrounding braces) so each query can splice it into its
/// own `nodes { ... }` selection.
const WORK_ITEM_FIELDS: &str = r#"
    id
    iid
    title
    state
    confidential
    createdAt
    updatedAt
    webUrl
    userDiscussionsCount
    workItemType { name }
    author { id username name }
    widgets {
        type
        ... on WorkItemWidgetDescription { description }
        ... on WorkItemWidgetAssignees { assignees { nodes { id username name } } }
        ... on WorkItemWidgetLabels { labels { nodes { id title color } } }
        ... on WorkItemWidgetHierarchy {
            parent { id iid title }
            children { count nodes { id iid title state } }
        }
        ... on WorkItemWidgetStartAndDueDate { startDate dueDate }
        ... on WorkItemWidgetMilestone { milestone { id iid title } }
        ... on WorkItemWidgetWeight { weight }
        ... on WorkItemWidgetLinkedItems {
            blocked blockingCount blockedByCount
            linkedItems { nodes { linkId linkType workItem { id iid title state } } }
        }
        ... on WorkItemWidgetAwardEmoji {
            upvotes downvotes
            awardEmoji { nodes { name user { id username name } } }
        }
        ... on WorkItemWidgetDevelopment {
            closingMergeRequests { nodes { mergeRequest { iid title webUrl state } } }
        }
        ... on WorkItemWidgetIteration { iteration { id iid title startDate dueDate } }
        ... on WorkItemWidgetHealthStatus { healthStatus }
    }
"#;

/// Convert a single camelCase identifier to snake_case (`webUrl` → `web_url`,
/// `closingMergeRequests` → `closing_merge_requests`). The keys are simple
/// camelCase (no consecutive capitals), so an underscore-before-each-capital
/// pass is sufficient.
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Recursively rename every object key from GraphQL camelCase to snake_case, so
/// work-item output matches the REST tools (`createdAt` → `created_at`). Applied
/// at the output boundary — the GraphQL queries, response reads, and mutation
/// inputs all stay camelCase. Idempotent (snake keys are unchanged).
fn snake_case_keys(v: Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, val)| (to_snake_case(&k), snake_case_keys(val)))
                .collect(),
        ),
        Value::Array(arr) => Value::Array(arr.into_iter().map(snake_case_keys).collect()),
        other => other,
    }
}

/// Collapse a raw work-item node into a flat object: replace `workItemType`
/// with its name string and lift the widgets we surface (description,
/// assignees, labels, hierarchy, dates) to the top level, dropping the
/// `widgets` envelope. Unknown widget types are discarded. Null fields left
/// behind (e.g. an unset `dueDate`) are removed later by `slim_get`.
///
/// Output keys are snake_case (via [`snake_case_keys`]) to match the REST tools.
///
/// Output casing is normalized to match the *input* conventions so a caller can
/// round-trip / client-filter returned values: `workItemType` to the UPPER_SNAKE
/// `IssueType` enum form (GraphQL returns "Issue"/"Key Result"), and `state` to
/// the lowercase REST/`IssuableState` form (GraphQL returns "OPEN"/"CLOSED").
fn flatten_work_item(mut node: Value) -> Value {
    let Some(obj) = node.as_object_mut() else {
        return node;
    };

    // workItemType { name } -> "ISSUE" (matches the `types: ["ISSUE"]` input).
    if let Some(name) = obj
        .get("workItemType")
        .and_then(|wt| wt.get("name"))
        .and_then(Value::as_str)
    {
        let normalized = name.to_uppercase().replace(' ', "_");
        obj.insert("workItemType".into(), json!(normalized));
    }

    // state "OPEN"/"CLOSED" -> "opened"/"closed" (matches the `state` input).
    if let Some(state) = obj.get("state").and_then(Value::as_str) {
        let normalized = match state {
            "OPEN" => "opened",
            "CLOSED" => "closed",
            other => other,
        };
        obj.insert("state".into(), json!(normalized));
    }

    let Some(Value::Array(widgets)) = obj.remove("widgets") else {
        return node;
    };
    for widget in widgets {
        let Some(w) = widget.as_object() else {
            continue;
        };
        match w.get("type").and_then(Value::as_str) {
            Some("DESCRIPTION") => {
                if let Some(d) = w.get("description") {
                    obj.insert("description".into(), d.clone());
                }
            }
            Some("ASSIGNEES") => {
                if let Some(nodes) = w.get("assignees").and_then(|a| a.get("nodes")) {
                    obj.insert("assignees".into(), nodes.clone());
                }
            }
            Some("LABELS") => {
                // Collapse label objects to their title strings, mirroring how
                // the REST issue/MR endpoints present `labels`.
                if let Some(Value::Array(nodes)) =
                    w.get("labels").and_then(|l| l.get("nodes")).cloned()
                {
                    let titles: Vec<Value> = nodes
                        .iter()
                        .filter_map(|n| n.get("title").cloned())
                        .collect();
                    obj.insert("labels".into(), Value::Array(titles));
                }
            }
            Some("HIERARCHY") => {
                if let Some(parent) = w.get("parent") {
                    obj.insert("parent".into(), parent.clone());
                }
                if let Some(children) = w.get("children") {
                    if let Some(nodes) = children.get("nodes") {
                        obj.insert("children".into(), nodes.clone());
                    }
                    if let Some(count) = children.get("count") {
                        obj.insert("childrenCount".into(), count.clone());
                    }
                }
            }
            Some("START_AND_DUE_DATE") => {
                if let Some(s) = w.get("startDate") {
                    obj.insert("startDate".into(), s.clone());
                }
                if let Some(d) = w.get("dueDate") {
                    obj.insert("dueDate".into(), d.clone());
                }
            }
            Some("MILESTONE") => {
                if let Some(m) = w.get("milestone") {
                    obj.insert("milestone".into(), m.clone());
                }
            }
            Some("WEIGHT") => {
                if let Some(weight) = w.get("weight") {
                    obj.insert("weight".into(), weight.clone());
                }
            }
            Some("LINKED_ITEMS") => {
                for k in ["blocked", "blockingCount", "blockedByCount"] {
                    if let Some(v) = w.get(k) {
                        obj.insert(k.into(), v.clone());
                    }
                }
                if let Some(nodes) = w.get("linkedItems").and_then(|l| l.get("nodes")) {
                    obj.insert("linkedItems".into(), nodes.clone());
                }
            }
            Some("AWARD_EMOJI") => {
                for k in ["upvotes", "downvotes"] {
                    if let Some(v) = w.get(k) {
                        obj.insert(k.into(), v.clone());
                    }
                }
                if let Some(nodes) = w.get("awardEmoji").and_then(|a| a.get("nodes")) {
                    obj.insert("awardEmoji".into(), nodes.clone());
                }
            }
            Some("DEVELOPMENT") => {
                // The merge requests that close this item when merged — the
                // work-item equivalent of REST issue_get's `closed_by`.
                if let Some(nodes) = w
                    .get("closingMergeRequests")
                    .and_then(|c| c.get("nodes"))
                    .and_then(Value::as_array)
                {
                    let mrs: Vec<Value> = nodes
                        .iter()
                        .filter_map(|n| n.get("mergeRequest").cloned())
                        .collect();
                    obj.insert("closingMergeRequests".into(), Value::Array(mrs));
                }
            }
            Some("ITERATION") => {
                if let Some(it) = w.get("iteration") {
                    obj.insert("iteration".into(), it.clone());
                }
            }
            Some("HEALTH_STATUS") => {
                if let Some(h) = w.get("healthStatus") {
                    obj.insert("healthStatus".into(), h.clone());
                }
            }
            _ => {}
        }
    }

    snake_case_keys(node)
}

// --------------------------------------------------------------------------
// Get single work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemGetParams {
    #[schemars(
        description = "Full path of the project or group the work item belongs to (e.g. \"mygroup/myproject\" or \"mygroup/subgroup\"). Not a numeric ID."
    )]
    pub namespace_path: String,
    #[schemars(
        description = "Work item IID — the number shown in its URL and references (e.g. #42)."
    )]
    pub work_item_iid: u64,
}

pub async fn work_item_get(
    client: &GitlabClient,
    p: WorkItemGetParams,
) -> Result<Value, GitlabError> {
    let query = format!(
        "query($fullPath: ID!, $iids: [String!]) {{ \
            namespace(fullPath: $fullPath) {{ \
                workItems(iids: $iids, first: 1) {{ nodes {{ {WORK_ITEM_FIELDS} }} }} \
            }} \
        }}"
    );
    let vars = json!({
        "fullPath": p.namespace_path,
        "iids": [p.work_item_iid.to_string()],
    });

    let data = client.graphql(&query, vars).await?;
    match data
        .pointer("/namespace/workItems/nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| nodes.first())
    {
        Some(node) => Ok(flatten_work_item(node.clone())),
        None => Err(GitlabError::Other(format!(
            "work item not found: {} #{}",
            p.namespace_path, p.work_item_iid
        ))),
    }
}

// --------------------------------------------------------------------------
// List work items
// --------------------------------------------------------------------------

/// Page size used when the caller omits `first`. GraphQL connections have no
/// server default that matches the REST `per_page`, so we set our own.
const DEFAULT_FIRST: u64 = 20;
/// GitLab caps work-item connection page size at 100.
const MAX_FIRST: u64 = 100;

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct WorkItemsListParams {
    #[schemars(
        description = "Full path of the project or group to list work items in (e.g. \"mygroup/myproject\" or \"mygroup/subgroup\"). Not a numeric ID."
    )]
    pub namespace_path: String,
    #[schemars(
        description = "Filter by work item type(s): ISSUE, TASK, EPIC, INCIDENT, OBJECTIVE, KEY_RESULT, TEST_CASE, REQUIREMENT, TICKET."
    )]
    pub types: Option<Vec<String>>,
    #[schemars(description = "Filter by state: \"opened\", \"closed\", or \"all\".")]
    pub state: Option<String>,
    #[schemars(description = "Search text matched against title and description.")]
    pub search: Option<String>,
    #[schemars(description = "Filter by author username.")]
    pub author_username: Option<String>,
    #[schemars(description = "Filter by assignee username(s).")]
    pub assignee_usernames: Option<Vec<String>>,
    #[schemars(description = "Filter by label name(s) (all must be present).")]
    pub labels: Option<Vec<String>>,
    #[schemars(description = "Filter by milestone title.")]
    pub milestone_title: Option<String>,
    #[schemars(description = "Filter by confidentiality.")]
    pub confidential: Option<bool>,
    #[schemars(description = "Only items created at/after this ISO 8601 timestamp.")]
    pub created_after: Option<String>,
    #[schemars(description = "Only items created at/before this ISO 8601 timestamp.")]
    pub created_before: Option<String>,
    #[schemars(description = "Only items updated at/after this ISO 8601 timestamp.")]
    pub updated_after: Option<String>,
    #[schemars(description = "Only items updated at/before this ISO 8601 timestamp.")]
    pub updated_before: Option<String>,
    #[schemars(description = "Only items due at/after this ISO 8601 timestamp.")]
    pub due_after: Option<String>,
    #[schemars(description = "Only items due at/before this ISO 8601 timestamp.")]
    pub due_before: Option<String>,
    #[schemars(
        description = "Sort order (WorkItemSort), e.g. CREATED_DESC (default), CREATED_ASC, UPDATED_DESC, TITLE_ASC, DUE_DATE_ASC, DUE_DATE_DESC."
    )]
    pub sort: Option<String>,
    #[schemars(
        description = "Page size (default 20, max 100). GraphQL cursor pagination: combine with `after`. Ignored when `fetch_all` is true."
    )]
    pub first: Option<u64>,
    #[schemars(
        description = "Pagination cursor: pass the `pageInfo.endCursor` from a previous response to fetch the next page."
    )]
    pub after: Option<String>,
    #[schemars(
        description = "Fetch every page and merge the results into one `nodes` array, ignoring `first`/`after`. Use sparingly: large namespaces require many sequential requests."
    )]
    pub fetch_all: Option<bool>,
}

/// The list query, with every filter as a (nullable) variable. Built once and
/// reused across pages for `fetch_all`.
fn work_items_list_query() -> String {
    format!(
        "query($fullPath: ID!, $state: IssuableState, $types: [IssueType!], $search: String, \
               $authorUsername: String, $assigneeUsernames: [String!], $labelName: [String!], \
               $milestoneTitle: [String!], $confidential: Boolean, \
               $createdAfter: Time, $createdBefore: Time, $updatedAfter: Time, $updatedBefore: Time, \
               $dueAfter: Time, $dueBefore: Time, $sort: WorkItemSort, \
               $first: Int, $after: String) {{ \
            namespace(fullPath: $fullPath) {{ \
                workItems(state: $state, types: $types, search: $search, \
                    authorUsername: $authorUsername, assigneeUsernames: $assigneeUsernames, \
                    labelName: $labelName, milestoneTitle: $milestoneTitle, confidential: $confidential, \
                    createdAfter: $createdAfter, createdBefore: $createdBefore, \
                    updatedAfter: $updatedAfter, updatedBefore: $updatedBefore, \
                    dueAfter: $dueAfter, dueBefore: $dueBefore, sort: $sort, \
                    first: $first, after: $after) {{ \
                    pageInfo {{ hasNextPage endCursor }} \
                    nodes {{ {WORK_ITEM_FIELDS} }} \
                }} \
            }} \
        }}"
    )
}

/// The filter variables shared by every page (everything except `first`/`after`).
fn work_items_list_filters(p: &WorkItemsListParams) -> serde_json::Map<String, Value> {
    let mut v = serde_json::Map::new();
    v.insert("fullPath".into(), json!(p.namespace_path));
    v.insert("state".into(), json!(p.state));
    v.insert("types".into(), json!(p.types));
    v.insert("search".into(), json!(p.search));
    v.insert("authorUsername".into(), json!(p.author_username));
    v.insert("assigneeUsernames".into(), json!(p.assignee_usernames));
    v.insert("labelName".into(), json!(p.labels));
    // milestoneTitle is [String!]; wrap the single friendly title in a list.
    v.insert(
        "milestoneTitle".into(),
        json!(p.milestone_title.clone().map(|t| vec![t])),
    );
    v.insert("confidential".into(), json!(p.confidential));
    v.insert("createdAfter".into(), json!(p.created_after));
    v.insert("createdBefore".into(), json!(p.created_before));
    v.insert("updatedAfter".into(), json!(p.updated_after));
    v.insert("updatedBefore".into(), json!(p.updated_before));
    v.insert("dueAfter".into(), json!(p.due_after));
    v.insert("dueBefore".into(), json!(p.due_before));
    v.insert("sort".into(), json!(p.sort));
    v
}

/// Flatten a `workItems` connection's `nodes` array and slim each for list use.
fn flatten_nodes(conn: &Value) -> Vec<Value> {
    conn.get("nodes")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .cloned()
                .map(flatten_work_item)
                .map(slim_list_node)
                .collect()
        })
        .unwrap_or_default()
}

/// Drop the bulk-expensive fields from a flattened work item for list responses
/// (mirrors how `slim_list` strips `description` from REST list items). The full
/// `description` and `children` remain available via the single-item get;
/// `childrenCount` is kept so list callers still see whether an item has
/// sub-items.
fn slim_list_node(mut node: Value) -> Value {
    if let Some(obj) = node.as_object_mut() {
        // Keys are already snake_case here: `flatten_work_item` runs
        // `snake_case_keys` before this, so remove the snake_case names.
        obj.remove("description");
        obj.remove("children");
        // Bulk relation arrays — the cheap scalar signals (children_count,
        // blocked/blocking_count, upvotes/downvotes) are kept.
        obj.remove("linked_items");
        obj.remove("award_emoji");
        obj.remove("closing_merge_requests");
    }
    node
}

pub async fn work_items_list(
    client: &GitlabClient,
    p: WorkItemsListParams,
) -> Result<Value, GitlabError> {
    let query = work_items_list_query();
    let filters = work_items_list_filters(&p);
    let not_found = || GitlabError::Other(format!("namespace not found: {}", p.namespace_path));

    if !p.fetch_all.unwrap_or(false) {
        let first = p.first.unwrap_or(DEFAULT_FIRST).min(MAX_FIRST);
        let mut vars = filters;
        vars.insert("first".into(), json!(first));
        vars.insert("after".into(), json!(p.after));
        let data = client.graphql(&query, Value::Object(vars)).await?;
        let conn = data.pointer("/namespace/workItems").ok_or_else(not_found)?;
        return Ok(snake_case_keys(json!({
            "nodes": flatten_nodes(conn),
            "pageInfo": conn.get("pageInfo").cloned().unwrap_or(Value::Null),
        })));
    }

    // fetch_all: walk pages at MAX_FIRST each until there is no next page,
    // bounded by MAX_PAGES (mirrors the REST `paginate` helper).
    let mut all: Vec<Value> = Vec::new();
    let mut after: Option<String> = None;
    let mut page = 0u64;
    loop {
        page += 1;
        if page > crate::client::MAX_PAGES {
            return Err(GitlabError::Other(format!(
                "fetch_all exceeded the {}-page limit; narrow the query",
                crate::client::MAX_PAGES
            )));
        }
        let mut vars = filters.clone();
        vars.insert("first".into(), json!(MAX_FIRST));
        vars.insert("after".into(), json!(after));
        let data = client.graphql(&query, Value::Object(vars)).await?;
        let conn = data.pointer("/namespace/workItems").ok_or_else(not_found)?;
        all.extend(flatten_nodes(conn));

        let page_info = &conn["pageInfo"];
        if !page_info["hasNextPage"].as_bool().unwrap_or(false) {
            break;
        }
        match page_info["endCursor"].as_str() {
            Some(c) => after = Some(c.to_string()),
            None => break,
        }
    }
    Ok(snake_case_keys(json!({
        "nodes": all,
        "pageInfo": { "hasNextPage": false, "endCursor": Value::Null },
    })))
}

// --------------------------------------------------------------------------
// Mutation helpers
// --------------------------------------------------------------------------

/// Resolve a work-item-type *name* (e.g. "Issue", "Task") to the
/// `gid://gitlab/WorkItems::Type/N` global ID that `workItemCreate` requires.
/// The available types depend on the namespace's license tier (Epic/Objective/
/// KeyResult are Premium/Ultimate), so an unknown name lists what *is* available.
async fn resolve_work_item_type_id(
    client: &GitlabClient,
    namespace_path: &str,
    type_name: &str,
) -> Result<String, GitlabError> {
    let query =
        "query($p: ID!) { namespace(fullPath: $p) { workItemTypes { nodes { id name } } } }";
    let data = client
        .graphql(query, json!({ "p": namespace_path }))
        .await?;
    let nodes = data
        .pointer("/namespace/workItemTypes/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GitlabError::Other(format!("namespace not found: {namespace_path}")))?;

    for node in nodes {
        if node["name"]
            .as_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(type_name))
            && let Some(id) = node["id"].as_str()
        {
            return Ok(id.to_string());
        }
    }
    let available: Vec<&str> = nodes.iter().filter_map(|n| n["name"].as_str()).collect();
    Err(GitlabError::Other(format!(
        "unknown work item type {type_name:?} for {namespace_path}; available: {}",
        available.join(", ")
    )))
}

/// Resolve a namespace-relative work-item IID to its `gid://gitlab/WorkItem/N`
/// global ID, which the update and delete mutations require (they don't accept
/// an IID). One extra query, mirroring `resolve_epic_id` in the REST epics module.
async fn resolve_work_item_gid(
    client: &GitlabClient,
    namespace_path: &str,
    iid: u64,
) -> Result<String, GitlabError> {
    let query = "query($p: ID!, $iids: [String!]) { namespace(fullPath: $p) { workItems(iids: $iids, first: 1) { nodes { id } } } }";
    let data = client
        .graphql(
            query,
            json!({ "p": namespace_path, "iids": [iid.to_string()] }),
        )
        .await?;
    data.pointer("/namespace/workItems/nodes/0/id")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| GitlabError::Other(format!("work item not found: {namespace_path} #{iid}")))
}

/// Resolve several work-item IIDs to their global IDs in one query, preserving
/// the caller's order. Used for linked-item targets.
async fn resolve_work_item_gids(
    client: &GitlabClient,
    namespace_path: &str,
    iids: &[u64],
) -> Result<Vec<String>, GitlabError> {
    if iids.is_empty() {
        return Ok(Vec::new());
    }
    let iid_strs: Vec<String> = iids.iter().map(u64::to_string).collect();
    let query = "query($p: ID!, $iids: [String!]) { namespace(fullPath: $p) { workItems(iids: $iids, first: 100) { nodes { id iid } } } }";
    let data = client
        .graphql(query, json!({ "p": namespace_path, "iids": iid_strs }))
        .await?;
    let mut by_iid: HashMap<String, String> = HashMap::new();
    if let Some(nodes) = data
        .pointer("/namespace/workItems/nodes")
        .and_then(Value::as_array)
    {
        for n in nodes {
            if let (Some(iid), Some(id)) = (n["iid"].as_str(), n["id"].as_str()) {
                by_iid.insert(iid.to_string(), id.to_string());
            }
        }
    }
    map_names_to_ids(
        &iid_strs,
        &by_iid,
        &format!("work item(s) not found in {namespace_path}"),
    )
}

/// Resolve assignee *usernames* to `gid://gitlab/User/N` global IDs (one query
/// for all). Order is preserved; an unknown username is an error listing the
/// misses. Username matching is case-insensitive.
async fn resolve_user_ids(
    client: &GitlabClient,
    usernames: &[String],
) -> Result<Vec<String>, GitlabError> {
    if usernames.is_empty() {
        return Ok(Vec::new());
    }
    let query = "query($u: [String!]) { users(usernames: $u) { nodes { id username } } }";
    let data = client.graphql(query, json!({ "u": usernames })).await?;

    let mut by_name: HashMap<String, String> = HashMap::new();
    if let Some(nodes) = data.pointer("/users/nodes").and_then(Value::as_array) {
        for n in nodes {
            if let (Some(u), Some(id)) = (n["username"].as_str(), n["id"].as_str()) {
                by_name.insert(u.to_ascii_lowercase(), id.to_string());
            }
        }
    }
    map_names_to_ids(usernames, &by_name, "user(s) not found")
}

/// Resolve label *names* to `gid://gitlab/.../Label/N` global IDs. Fetches the
/// namespace's labels in one query (trying the path as both a project — with
/// ancestor group labels — and a group; exactly one resolves), then matches by
/// case-insensitive title. Bounded at 100 labels per source; a requested label
/// beyond that, or absent, is reported as not found.
async fn resolve_label_ids(
    client: &GitlabClient,
    namespace_path: &str,
    names: &[String],
) -> Result<Vec<String>, GitlabError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let query = "query($p: ID!) { \
        project(fullPath: $p) { labels(first: 100, includeAncestorGroups: true) { nodes { id title } } } \
        group(fullPath: $p) { labels(first: 100) { nodes { id title } } } \
    }";
    let data = client
        .graphql(query, json!({ "p": namespace_path }))
        .await?;

    let mut by_title: HashMap<String, String> = HashMap::new();
    for src in ["project", "group"] {
        if let Some(nodes) = data
            .pointer(&format!("/{src}/labels/nodes"))
            .and_then(Value::as_array)
        {
            for n in nodes {
                if let (Some(t), Some(id)) = (n["title"].as_str(), n["id"].as_str()) {
                    by_title
                        .entry(t.to_ascii_lowercase())
                        .or_insert_with(|| id.to_string());
                }
            }
        }
    }
    map_names_to_ids(
        names,
        &by_title,
        &format!("label(s) not found in {namespace_path}"),
    )
}

/// Map each requested name (case-insensitively) to its resolved GID via `lookup`,
/// preserving order; collect misses into a single error.
fn map_names_to_ids(
    names: &[String],
    lookup: &HashMap<String, String>,
    err_prefix: &str,
) -> Result<Vec<String>, GitlabError> {
    let mut ids = Vec::with_capacity(names.len());
    let mut missing = Vec::new();
    for name in names {
        match lookup.get(&name.to_ascii_lowercase()) {
            Some(id) => ids.push(id.clone()),
            None => missing.push(name.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(GitlabError::Other(format!(
            "{err_prefix}: {}",
            missing.join(", ")
        )));
    }
    Ok(ids)
}

/// Append the start/due-date, weight, and milestone widget inputs shared by
/// create and update (both accept the same widget shapes). Each is added only
/// when set. Setting a date sends `isFixed: true` — work items distinguish
/// *fixed* dates from rolled-up/inherited ones (cf. the REST epics `*_is_fixed`
/// pair). Milestone takes a numeric ID and builds the `Milestone` GID directly
/// (no lookup). Weight needs Premium/Ultimate; on lower tiers the widget is
/// absent and the mutation rejects it.
fn apply_scalar_widgets(
    input: &mut serde_json::Map<String, Value>,
    start_date: Option<String>,
    due_date: Option<String>,
    weight: Option<u32>,
    milestone_id: Option<u64>,
) {
    if start_date.is_some() || due_date.is_some() {
        let mut w = serde_json::Map::new();
        w.insert("isFixed".into(), json!(true));
        if let Some(s) = start_date {
            w.insert("startDate".into(), json!(s));
        }
        if let Some(d) = due_date {
            w.insert("dueDate".into(), json!(d));
        }
        input.insert("startAndDueDateWidget".into(), Value::Object(w));
    }
    if let Some(weight) = weight {
        input.insert("weightWidget".into(), json!({ "weight": weight }));
    }
    if let Some(mid) = milestone_id {
        input.insert(
            "milestoneWidget".into(),
            json!({ "milestoneId": format!("gid://gitlab/Milestone/{mid}") }),
        );
    }
}

/// GitLab mutations report business-logic failures (e.g. "Title can't be blank")
/// in a payload `errors` array while still returning HTTP 200 with no top-level
/// `errors` — so `GitlabClient::graphql` cannot catch them. Every mutation must
/// run its payload through this. `data` is the unwrapped GraphQL `data`; `field`
/// is the mutation name (e.g. "workItemCreate").
fn check_mutation_errors(data: &Value, field: &str) -> Result<(), GitlabError> {
    let errs = data
        .pointer(&format!("/{field}/errors"))
        .and_then(Value::as_array);
    if let Some(errs) = errs
        && !errs.is_empty()
    {
        let joined = errs
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(GitlabError::Other(format!("{field} failed: {joined}")));
    }
    Ok(())
}

// --------------------------------------------------------------------------
// Create work item
// --------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct WorkItemCreateParams {
    #[schemars(
        description = "Full path of the project or group to create the work item in (e.g. \"mygroup/myproject\"). Not a numeric ID."
    )]
    pub namespace_path: String,
    #[schemars(
        description = "Work item type by name: ISSUE, TASK, EPIC, INCIDENT, OBJECTIVE, KEY_RESULT, TICKET (case-insensitive). Availability depends on the namespace's license tier — EPIC/OBJECTIVE/KEY_RESULT need Premium/Ultimate."
    )]
    pub work_item_type: String,
    #[schemars(description = "Title of the work item.")]
    pub title: String,
    #[schemars(description = "Description body (Markdown).")]
    pub description: Option<String>,
    #[schemars(description = "Mark the work item confidential.")]
    pub confidential: Option<bool>,
    #[schemars(description = "Label names to apply (must already exist in the project/group).")]
    pub labels: Option<Vec<String>>,
    #[schemars(description = "Usernames to assign.")]
    pub assignees: Option<Vec<String>>,
    #[schemars(
        description = "IID of an existing work item in the same namespace to set as the hierarchy parent."
    )]
    pub parent_work_item_iid: Option<u64>,
    #[schemars(description = "Start date (ISO 8601, e.g. \"2026-01-01\").")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601, e.g. \"2026-12-31\").")]
    pub due_date: Option<String>,
    #[schemars(description = "Numeric milestone ID to assign (not the title).")]
    pub milestone_id: Option<u64>,
    #[schemars(description = "Weight (non-negative integer). Requires Premium/Ultimate.")]
    pub weight: Option<u32>,
}

pub async fn work_item_create(
    client: &GitlabClient,
    p: WorkItemCreateParams,
) -> Result<Value, GitlabError> {
    let type_id = resolve_work_item_type_id(client, &p.namespace_path, &p.work_item_type).await?;

    let mut input = serde_json::Map::new();
    input.insert("namespacePath".into(), json!(p.namespace_path));
    input.insert("workItemTypeId".into(), json!(type_id));
    input.insert("title".into(), json!(p.title));
    if let Some(d) = p.description {
        input.insert("description".into(), json!(d));
    }
    if let Some(c) = p.confidential {
        input.insert("confidential".into(), json!(c));
    }
    if let Some(names) = p.labels {
        let ids = resolve_label_ids(client, &p.namespace_path, &names).await?;
        input.insert("labelsWidget".into(), json!({ "labelIds": ids }));
    }
    if let Some(usernames) = p.assignees {
        let ids = resolve_user_ids(client, &usernames).await?;
        input.insert("assigneesWidget".into(), json!({ "assigneeIds": ids }));
    }
    if let Some(parent_iid) = p.parent_work_item_iid {
        let parent_gid = resolve_work_item_gid(client, &p.namespace_path, parent_iid).await?;
        input.insert("hierarchyWidget".into(), json!({ "parentId": parent_gid }));
    }
    apply_scalar_widgets(
        &mut input,
        p.start_date,
        p.due_date,
        p.weight,
        p.milestone_id,
    );

    let mutation = format!(
        "mutation($input: WorkItemCreateInput!) {{ \
            workItemCreate(input: $input) {{ errors workItem {{ {WORK_ITEM_FIELDS} }} }} \
        }}"
    );
    let data = client
        .graphql(&mutation, json!({ "input": Value::Object(input) }))
        .await?;
    check_mutation_errors(&data, "workItemCreate")?;

    data.pointer("/workItemCreate/workItem")
        .cloned()
        .map(flatten_work_item)
        .ok_or_else(|| GitlabError::Other("workItemCreate returned no work item".into()))
}

// --------------------------------------------------------------------------
// Update work item
// --------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct WorkItemUpdateParams {
    #[schemars(description = "Full path of the project or group the work item belongs to.")]
    pub namespace_path: String,
    #[schemars(description = "Work item IID (the number from its URL/reference, e.g. #42).")]
    pub work_item_iid: u64,
    #[schemars(description = "New title.")]
    pub title: Option<String>,
    #[schemars(description = "New description body (Markdown).")]
    pub description: Option<String>,
    #[schemars(description = "State change: \"close\" or \"reopen\".")]
    pub state_event: Option<String>,
    #[schemars(description = "Set/clear confidential.")]
    pub confidential: Option<bool>,
    #[schemars(description = "Label names to add (must already exist in the project/group).")]
    pub add_labels: Option<Vec<String>>,
    #[schemars(description = "Label names to remove.")]
    pub remove_labels: Option<Vec<String>>,
    #[schemars(
        description = "Usernames to assign — replaces the current assignees. Pass [] to unassign all."
    )]
    pub assignees: Option<Vec<String>>,
    #[schemars(
        description = "IID of an existing work item in the same namespace to set as the hierarchy parent. Pass 0 to detach the current parent."
    )]
    pub parent_work_item_iid: Option<u64>,
    #[schemars(description = "Start date (ISO 8601, e.g. \"2026-01-01\").")]
    pub start_date: Option<String>,
    #[schemars(description = "Due date (ISO 8601, e.g. \"2026-12-31\").")]
    pub due_date: Option<String>,
    #[schemars(description = "Numeric milestone ID to assign (not the title).")]
    pub milestone_id: Option<u64>,
    #[schemars(description = "Weight (non-negative integer). Requires Premium/Ultimate.")]
    pub weight: Option<u32>,
}

pub async fn work_item_update(
    client: &GitlabClient,
    p: WorkItemUpdateParams,
) -> Result<Value, GitlabError> {
    let gid = resolve_work_item_gid(client, &p.namespace_path, p.work_item_iid).await?;

    // REST-style "close"/"reopen" → GraphQL WorkItemStateEvent CLOSE/REOPEN.
    let state_event = match p.state_event.as_deref() {
        None => None,
        Some(s) if s.eq_ignore_ascii_case("close") => Some("CLOSE"),
        Some(s) if s.eq_ignore_ascii_case("reopen") => Some("REOPEN"),
        Some(other) => {
            return Err(GitlabError::Other(format!(
                "invalid state_event {other:?}; use \"close\" or \"reopen\""
            )));
        }
    };

    let mut input = serde_json::Map::new();
    input.insert("id".into(), json!(gid));
    if let Some(t) = p.title {
        input.insert("title".into(), json!(t));
    }
    if let Some(d) = p.description {
        // Update has no top-level description field; it goes through the widget.
        input.insert("descriptionWidget".into(), json!({ "description": d }));
    }
    if let Some(se) = state_event {
        input.insert("stateEvent".into(), json!(se));
    }
    if let Some(c) = p.confidential {
        input.insert("confidential".into(), json!(c));
    }

    // Labels: add/remove are independent; build the widget only if either is set.
    let mut labels_widget = serde_json::Map::new();
    if let Some(names) = p.add_labels {
        let ids = resolve_label_ids(client, &p.namespace_path, &names).await?;
        labels_widget.insert("addLabelIds".into(), json!(ids));
    }
    if let Some(names) = p.remove_labels {
        let ids = resolve_label_ids(client, &p.namespace_path, &names).await?;
        labels_widget.insert("removeLabelIds".into(), json!(ids));
    }
    if !labels_widget.is_empty() {
        input.insert("labelsWidget".into(), Value::Object(labels_widget));
    }

    if let Some(usernames) = p.assignees {
        // Replaces the full assignee set (empty list unassigns all).
        let ids = resolve_user_ids(client, &usernames).await?;
        input.insert("assigneesWidget".into(), json!({ "assigneeIds": ids }));
    }
    match p.parent_work_item_iid {
        None => {}
        // Sentinel 0 detaches the current parent (mirrors epics' parent_epic_iid=0).
        Some(0) => {
            input.insert("hierarchyWidget".into(), json!({ "parentId": Value::Null }));
        }
        Some(parent_iid) => {
            let parent_gid = resolve_work_item_gid(client, &p.namespace_path, parent_iid).await?;
            input.insert("hierarchyWidget".into(), json!({ "parentId": parent_gid }));
        }
    }
    apply_scalar_widgets(
        &mut input,
        p.start_date,
        p.due_date,
        p.weight,
        p.milestone_id,
    );

    let mutation = format!(
        "mutation($input: WorkItemUpdateInput!) {{ \
            workItemUpdate(input: $input) {{ errors workItem {{ {WORK_ITEM_FIELDS} }} }} \
        }}"
    );
    let data = client
        .graphql(&mutation, json!({ "input": Value::Object(input) }))
        .await?;
    check_mutation_errors(&data, "workItemUpdate")?;

    data.pointer("/workItemUpdate/workItem")
        .cloned()
        .map(flatten_work_item)
        .ok_or_else(|| GitlabError::Other("workItemUpdate returned no work item".into()))
}

// --------------------------------------------------------------------------
// Delete work item
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemDeleteParams {
    #[schemars(description = "Full path of the project or group the work item belongs to.")]
    pub namespace_path: String,
    #[schemars(description = "Work item IID (the number from its URL/reference, e.g. #42).")]
    pub work_item_iid: u64,
}

pub async fn work_item_delete(
    client: &GitlabClient,
    p: WorkItemDeleteParams,
) -> Result<(), GitlabError> {
    let gid = resolve_work_item_gid(client, &p.namespace_path, p.work_item_iid).await?;
    let mutation =
        "mutation($input: WorkItemDeleteInput!) { workItemDelete(input: $input) { errors } }";
    let data = client
        .graphql(mutation, json!({ "input": { "id": gid } }))
        .await?;
    check_mutation_errors(&data, "workItemDelete")
}

// --------------------------------------------------------------------------
// Notes (comments / discussion threads)
// --------------------------------------------------------------------------
// GitLab's "notes" are what users call comments. Work items expose them through
// the NOTES widget (read) and the generic createNote/updateNote/destroyNote
// mutations (write), which take the work item's GID as `noteableId`. Note IDs
// are global (`gid://gitlab/Note/N`) — update/delete take that GID directly, so
// they need no namespace/IID.

/// Field selection for a single note, shared across list/create/update.
const NOTE_FIELDS: &str = "id body system internal createdAt updatedAt url \
     author { id username name } discussion { id }";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemNotesListParams {
    #[schemars(description = "Full path of the project or group the work item belongs to.")]
    pub namespace_path: String,
    #[schemars(description = "Work item IID (the number from its URL/reference, e.g. #42).")]
    pub work_item_iid: u64,
    #[schemars(
        description = "Page size (default 20, max 100). Cursor pagination: combine with `after`."
    )]
    pub first: Option<u64>,
    #[schemars(
        description = "Pagination cursor: the `pageInfo.endCursor` from a previous response."
    )]
    pub after: Option<String>,
}

pub async fn work_item_notes_list(
    client: &GitlabClient,
    p: WorkItemNotesListParams,
) -> Result<Value, GitlabError> {
    let first = p.first.unwrap_or(DEFAULT_FIRST).min(MAX_FIRST);
    let query = format!(
        "query($p: ID!, $iids: [String!], $first: Int, $after: String) {{ \
            namespace(fullPath: $p) {{ workItems(iids: $iids, first: 1) {{ nodes {{ widgets {{ \
                type \
                ... on WorkItemWidgetNotes {{ notes(first: $first, after: $after) {{ \
                    pageInfo {{ hasNextPage endCursor }} \
                    nodes {{ {NOTE_FIELDS} }} \
                }} }} \
            }} }} }} }} }}"
    );
    let vars = json!({
        "p": p.namespace_path,
        "iids": [p.work_item_iid.to_string()],
        "first": first,
        "after": p.after,
    });
    let data = client.graphql(&query, vars).await?;

    let widgets = data
        .pointer("/namespace/workItems/nodes/0/widgets")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            GitlabError::Other(format!(
                "work item not found: {} #{}",
                p.namespace_path, p.work_item_iid
            ))
        })?;
    let notes = widgets
        .iter()
        .find(|w| w.get("type").and_then(Value::as_str) == Some("NOTES"))
        .and_then(|w| w.get("notes"));

    Ok(snake_case_keys(json!({
        "nodes": notes.and_then(|n| n.get("nodes")).cloned().unwrap_or(json!([])),
        "pageInfo": notes.and_then(|n| n.get("pageInfo")).cloned().unwrap_or(Value::Null),
    })))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemNoteCreateParams {
    #[schemars(description = "Full path of the project or group the work item belongs to.")]
    pub namespace_path: String,
    #[schemars(description = "Work item IID (the number from its URL/reference, e.g. #42).")]
    pub work_item_iid: u64,
    #[schemars(description = "Comment body (Markdown).")]
    pub body: String,
    #[schemars(
        description = "Make this an internal note (visible only to project members), rather than a public comment."
    )]
    pub internal: Option<bool>,
    #[schemars(
        description = "To reply within an existing thread, the discussion global ID (the `discussion.id` from the notes list). Omit to start a new top-level comment."
    )]
    pub discussion_id: Option<String>,
}

pub async fn work_item_note_create(
    client: &GitlabClient,
    p: WorkItemNoteCreateParams,
) -> Result<Value, GitlabError> {
    let gid = resolve_work_item_gid(client, &p.namespace_path, p.work_item_iid).await?;
    let mut input = serde_json::Map::new();
    input.insert("noteableId".into(), json!(gid));
    input.insert("body".into(), json!(p.body));
    if let Some(i) = p.internal {
        input.insert("internal".into(), json!(i));
    }
    if let Some(d) = p.discussion_id {
        input.insert("discussionId".into(), json!(d));
    }
    let mutation = format!(
        "mutation($input: CreateNoteInput!) {{ createNote(input: $input) {{ errors note {{ {NOTE_FIELDS} }} }} }}"
    );
    let data = client
        .graphql(&mutation, json!({ "input": Value::Object(input) }))
        .await?;
    check_mutation_errors(&data, "createNote")?;
    data.pointer("/createNote/note")
        .cloned()
        .map(snake_case_keys)
        .ok_or_else(|| GitlabError::Other("createNote returned no note".into()))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemNoteUpdateParams {
    #[schemars(
        description = "Note global ID — the `id` from the notes list, e.g. \"gid://gitlab/Note/123\"."
    )]
    pub note_id: String,
    #[schemars(description = "New comment body (Markdown).")]
    pub body: String,
}

pub async fn work_item_note_update(
    client: &GitlabClient,
    p: WorkItemNoteUpdateParams,
) -> Result<Value, GitlabError> {
    let mutation = format!(
        "mutation($input: UpdateNoteInput!) {{ updateNote(input: $input) {{ errors note {{ {NOTE_FIELDS} }} }} }}"
    );
    let data = client
        .graphql(
            &mutation,
            json!({ "input": { "id": p.note_id, "body": p.body } }),
        )
        .await?;
    check_mutation_errors(&data, "updateNote")?;
    data.pointer("/updateNote/note")
        .cloned()
        .map(snake_case_keys)
        .ok_or_else(|| GitlabError::Other("updateNote returned no note".into()))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemNoteDeleteParams {
    #[schemars(
        description = "Note global ID — the `id` from the notes list, e.g. \"gid://gitlab/Note/123\"."
    )]
    pub note_id: String,
}

pub async fn work_item_note_delete(
    client: &GitlabClient,
    p: WorkItemNoteDeleteParams,
) -> Result<(), GitlabError> {
    let mutation = "mutation($input: DestroyNoteInput!) { destroyNote(input: $input) { errors } }";
    let data = client
        .graphql(mutation, json!({ "input": { "id": p.note_id } }))
        .await?;
    check_mutation_errors(&data, "destroyNote")
}

// --------------------------------------------------------------------------
// Linked items (relates to / blocks / is blocked by)
// --------------------------------------------------------------------------

/// Map a REST-style link type to the GraphQL `WorkItemRelatedLinkType` enum.
fn link_type_enum(link_type: Option<&str>) -> Result<&'static str, GitlabError> {
    match link_type {
        None => Ok("RELATED"),
        Some(s) if s.eq_ignore_ascii_case("relates_to") || s.eq_ignore_ascii_case("related") => {
            Ok("RELATED")
        }
        Some(s) if s.eq_ignore_ascii_case("blocks") => Ok("BLOCKS"),
        Some(s)
            if s.eq_ignore_ascii_case("is_blocked_by") || s.eq_ignore_ascii_case("blocked_by") =>
        {
            Ok("BLOCKED_BY")
        }
        Some(other) => Err(GitlabError::Other(format!(
            "invalid link_type {other:?}; use \"relates_to\", \"blocks\", or \"is_blocked_by\""
        ))),
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemLinkAddParams {
    #[schemars(description = "Full path of the project or group the work item belongs to.")]
    pub namespace_path: String,
    #[schemars(description = "IID of the work item to link from.")]
    pub work_item_iid: u64,
    #[schemars(description = "IID(s) of the work item(s) in the same namespace to link to.")]
    pub target_work_item_iids: Vec<u64>,
    #[schemars(
        description = "Link type: \"relates_to\" (default), \"blocks\", or \"is_blocked_by\"."
    )]
    pub link_type: Option<String>,
}

pub async fn work_item_link_add(
    client: &GitlabClient,
    p: WorkItemLinkAddParams,
) -> Result<Value, GitlabError> {
    let gid = resolve_work_item_gid(client, &p.namespace_path, p.work_item_iid).await?;
    let target_gids =
        resolve_work_item_gids(client, &p.namespace_path, &p.target_work_item_iids).await?;
    let link_type = link_type_enum(p.link_type.as_deref())?;

    let mutation = format!(
        "mutation($input: WorkItemAddLinkedItemsInput!) {{ \
            workItemAddLinkedItems(input: $input) {{ errors workItem {{ {WORK_ITEM_FIELDS} }} }} \
        }}"
    );
    let data = client
        .graphql(
            &mutation,
            json!({ "input": { "id": gid, "linkType": link_type, "workItemsIds": target_gids } }),
        )
        .await?;
    check_mutation_errors(&data, "workItemAddLinkedItems")?;
    data.pointer("/workItemAddLinkedItems/workItem")
        .cloned()
        .map(flatten_work_item)
        .ok_or_else(|| GitlabError::Other("workItemAddLinkedItems returned no work item".into()))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemLinkRemoveParams {
    #[schemars(description = "Full path of the project or group the work item belongs to.")]
    pub namespace_path: String,
    #[schemars(description = "IID of the work item to unlink from.")]
    pub work_item_iid: u64,
    #[schemars(description = "IID(s) of the linked work item(s) to remove.")]
    pub target_work_item_iids: Vec<u64>,
}

pub async fn work_item_link_remove(
    client: &GitlabClient,
    p: WorkItemLinkRemoveParams,
) -> Result<Value, GitlabError> {
    let gid = resolve_work_item_gid(client, &p.namespace_path, p.work_item_iid).await?;
    let target_gids =
        resolve_work_item_gids(client, &p.namespace_path, &p.target_work_item_iids).await?;

    let mutation = format!(
        "mutation($input: WorkItemRemoveLinkedItemsInput!) {{ \
            workItemRemoveLinkedItems(input: $input) {{ errors workItem {{ {WORK_ITEM_FIELDS} }} }} \
        }}"
    );
    let data = client
        .graphql(
            &mutation,
            json!({ "input": { "id": gid, "workItemsIds": target_gids } }),
        )
        .await?;
    check_mutation_errors(&data, "workItemRemoveLinkedItems")?;
    data.pointer("/workItemRemoveLinkedItems/workItem")
        .cloned()
        .map(flatten_work_item)
        .ok_or_else(|| GitlabError::Other("workItemRemoveLinkedItems returned no work item".into()))
}

// --------------------------------------------------------------------------
// Emoji reactions
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemEmojiParams {
    #[schemars(description = "Full path of the project or group the work item belongs to.")]
    pub namespace_path: String,
    #[schemars(description = "Work item IID (the number from its URL/reference, e.g. #42).")]
    pub work_item_iid: u64,
    #[schemars(description = "Emoji name, e.g. \"thumbsup\", \"rocket\", \"eyes\".")]
    pub name: String,
}

/// Add/remove an award emoji on any awardable (a work item or a note), given
/// its global ID. `mutation_field` is `awardEmojiAdd` or `awardEmojiRemove`.
async fn award_emoji_mutate(
    client: &GitlabClient,
    awardable_id: &str,
    name: &str,
    mutation_field: &str,
    input_type: &str,
) -> Result<Value, GitlabError> {
    let mutation = format!(
        "mutation($input: {input_type}!) {{ \
            {mutation_field}(input: $input) {{ errors awardEmoji {{ name user {{ id username name }} }} }} \
        }}"
    );
    let data = client
        .graphql(
            &mutation,
            json!({ "input": { "awardableId": awardable_id, "name": name } }),
        )
        .await?;
    check_mutation_errors(&data, mutation_field)?;
    Ok(data
        .pointer(&format!("/{mutation_field}/awardEmoji"))
        .cloned()
        .map(snake_case_keys)
        .unwrap_or(Value::Null))
}

pub async fn work_item_emoji_add(
    client: &GitlabClient,
    p: WorkItemEmojiParams,
) -> Result<Value, GitlabError> {
    let gid = resolve_work_item_gid(client, &p.namespace_path, p.work_item_iid).await?;
    award_emoji_mutate(client, &gid, &p.name, "awardEmojiAdd", "AwardEmojiAddInput").await
}

pub async fn work_item_emoji_remove(
    client: &GitlabClient,
    p: WorkItemEmojiParams,
) -> Result<Value, GitlabError> {
    let gid = resolve_work_item_gid(client, &p.namespace_path, p.work_item_iid).await?;
    award_emoji_mutate(
        client,
        &gid,
        &p.name,
        "awardEmojiRemove",
        "AwardEmojiRemoveInput",
    )
    .await
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkItemNoteEmojiParams {
    #[schemars(
        description = "Note global ID — the `id` from the notes list, e.g. \"gid://gitlab/Note/123\"."
    )]
    pub note_id: String,
    #[schemars(description = "Emoji name, e.g. \"thumbsup\", \"rocket\", \"eyes\".")]
    pub name: String,
}

pub async fn work_item_note_emoji_add(
    client: &GitlabClient,
    p: WorkItemNoteEmojiParams,
) -> Result<Value, GitlabError> {
    award_emoji_mutate(
        client,
        &p.note_id,
        &p.name,
        "awardEmojiAdd",
        "AwardEmojiAddInput",
    )
    .await
}

pub async fn work_item_note_emoji_remove(
    client: &GitlabClient,
    p: WorkItemNoteEmojiParams,
) -> Result<Value, GitlabError> {
    award_emoji_mutate(
        client,
        &p.note_id,
        &p.name,
        "awardEmojiRemove",
        "AwardEmojiRemoveInput",
    )
    .await
}

// --------------------------------------------------------------------------
// MCP tool shims
// --------------------------------------------------------------------------

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};

use crate::tools::GitlabMcpServer;

#[tool_router(router = tool_router_work_items, vis = "pub(crate)")]
impl GitlabMcpServer {
    #[tool(
        description = "Get a single GitLab work item (issue, task, epic, incident, objective/OKR, key result) by namespace path and IID. Work items are the unified successor to the issues/epics REST API. Required: namespace_path (full project or group path like \"mygroup/myproject\", not a numeric ID) and work_item_iid (the number from the URL/reference, e.g. #42). Returns a flattened object with id (global ID), iid, title, state, work_item_type, author, user_discussions_count (number of comment threads), and lifted widget data: description, assignees, labels, parent/children + children_count (hierarchy), start_date/due_date, milestone, weight, linked_items, award_emoji, closing_merge_requests (MRs that close it), iteration, and health_status. Field names are snake_case (matching the REST tools); values are normalized to match inputs too — state is \"opened\"/\"closed\" and work_item_type is UPPER_SNAKE (e.g. \"ISSUE\", \"KEY_RESULT\"). For classic project issues you can also use gitlab_issues_get.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_work_items_get(
        &self,
        Parameters(p): Parameters<WorkItemGetParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_get!(self, work_item_get, p, "work item")
    }

    #[tool(
        description = "List GitLab work items (issues, tasks, epics, incidents, objectives/OKRs, key results) in a project or group. Work items are the unified successor to the issues/epics REST API. Required: namespace_path (full project or group path like \"mygroup/myproject\", not a numeric ID). Optional filters: types (array of ISSUE/TASK/EPIC/INCIDENT/OBJECTIVE/KEY_RESULT/...), state (opened/closed/all), search (title + description), author_username, assignee_usernames, labels (names), milestone_title, confidential, and date ranges created_after/before, updated_after/before, due_after/before (ISO 8601). sort takes a WorkItemSort value (e.g. CREATED_DESC, UPDATED_DESC, DUE_DATE_ASC, TITLE_ASC). Pagination is cursor-based: first sets the page size (default 20, max 100) and after takes a cursor from the previous response; or pass fetch_all=true to merge every page into one nodes array. Returns { nodes: [...flattened work items...], page_info: { has_next_page, end_cursor } }. Output keys are snake_case (matching the REST tools). List nodes omit the bulk arrays (description, children, linked_items, award_emoji, closing_merge_requests) to save tokens — all kept on the single-item get — while the cheap scalar signals (children_count, user_discussions_count, etc.) remain. Values are normalized like get (state \"opened\"/\"closed\", work_item_type UPPER_SNAKE). For classic project issues you can also use gitlab_issues_list.",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_work_items_list(
        &self,
        Parameters(p): Parameters<WorkItemsListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, work_items_list, p, "listing", "work items")
    }

    #[tool(
        description = "Create a GitLab work item (issue, task, epic, incident, objective/OKR, key result). Work items are the unified successor to the issues/epics REST API. Required: namespace_path (full project or group path like \"mygroup/myproject\", not a numeric ID), work_item_type (ISSUE/TASK/EPIC/INCIDENT/OBJECTIVE/KEY_RESULT/TICKET — case-insensitive; EPIC/OBJECTIVE/KEY_RESULT need Premium/Ultimate), and title. Optional: description (Markdown), confidential, labels (names), assignees (usernames), parent_work_item_iid, start_date / due_date (ISO 8601), milestone_id (numeric), weight (Premium/Ultimate). Returns the created work item (flattened, GraphQL camelCase fields). To create a classic project issue you can also use gitlab_issues_create.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_work_items_create(
        &self,
        Parameters(p): Parameters<WorkItemCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, work_item_create, p, "work item")
    }

    #[tool(
        description = "Update a GitLab work item (issue, task, epic, incident, objective/OKR, key result) by namespace path and IID. Required: namespace_path (full project or group path) and work_item_iid (the number from the URL/reference). All other fields optional: title, description (Markdown), state_event (\"close\" or \"reopen\"), confidential, add_labels / remove_labels (names), assignees (usernames, replaces), parent_work_item_iid, start_date / due_date (ISO 8601), milestone_id (numeric), weight (Premium/Ultimate). Returns the updated work item (flattened). To update a classic project issue you can also use gitlab_issues_update.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_work_items_update(
        &self,
        Parameters(p): Parameters<WorkItemUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, work_item_update, p, "work item")
    }

    #[tool(
        description = "Delete a GitLab work item (issue, task, epic, incident, objective/OKR, key result) by namespace path and IID. Required: namespace_path (full project or group path) and work_item_iid (the number from the URL/reference). This is permanent and cannot be undone.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_work_items_delete(
        &self,
        Parameters(p): Parameters<WorkItemDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, work_item_delete, p, "work item")
    }

    #[tool(
        description = "List comments (notes / discussion threads) on a GitLab work item (issue, task, epic, etc.). Required: namespace_path (full project or group path) and work_item_iid (the number from the URL/reference). Pagination is cursor-based: first (default 20, max 100) and after. Returns { nodes: [...notes...], pageInfo }. Each note has id (global ID, used for update/delete), body, author, createdAt, and a `system` flag (true for auto-generated activity like label changes; false for real comments).",
        annotations(read_only_hint = true)
    )]
    async fn gitlab_work_items_notes_list(
        &self,
        Parameters(p): Parameters<WorkItemNotesListParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(self, work_item_notes_list, p, "listing", "work item notes")
    }

    #[tool(
        description = "Comment on a GitLab work item (issue, task, epic, etc.) — creates a note. Required: namespace_path (full project or group path), work_item_iid (the number from the URL/reference), and body (Markdown). Optional: internal (true makes it an internal note visible only to project members), discussion_id (a thread's global ID from the notes list, to reply within that thread instead of starting a new one). Returns the created note including its id (global ID) for later edit/delete.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_work_items_notes_create(
        &self,
        Parameters(p): Parameters<WorkItemNoteCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, work_item_note_create, p, "work item note")
    }

    #[tool(
        description = "Edit a comment (note) on a GitLab work item. Required: note_id (the note's global ID, e.g. \"gid://gitlab/Note/123\", from gitlab_work_items_notes_list or _create) and body (the new Markdown). Returns the updated note.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_work_items_notes_update(
        &self,
        Parameters(p): Parameters<WorkItemNoteUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, work_item_note_update, p, "work item note")
    }

    #[tool(
        description = "Delete a comment (note) on a GitLab work item. Required: note_id (the note's global ID, e.g. \"gid://gitlab/Note/123\", from gitlab_work_items_notes_list). Permanent.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn gitlab_work_items_notes_delete(
        &self,
        Parameters(p): Parameters<WorkItemNoteDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_delete!(self, work_item_note_delete, p, "work item note")
    }

    #[tool(
        description = "Link a GitLab work item to other work item(s) (relates-to / blocks / is-blocked-by — the work-item equivalent of issue links). Required: namespace_path (full project or group path), work_item_iid (the item to link from), target_work_item_iids (array of IIDs in the same namespace to link to). Optional: link_type — \"relates_to\" (default), \"blocks\", or \"is_blocked_by\". Returns the updated work item with its linkedItems. For classic project issues you can also use gitlab_issues_links_create.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_work_items_link_add(
        &self,
        Parameters(p): Parameters<WorkItemLinkAddParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, work_item_link_add, p, "work item link")
    }

    #[tool(
        description = "Remove a link between a GitLab work item and other work item(s). Required: namespace_path (full project or group path), work_item_iid, target_work_item_iids (array of linked IIDs to unlink). Returns the updated work item.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_work_items_link_remove(
        &self,
        Parameters(p): Parameters<WorkItemLinkRemoveParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_update!(self, work_item_link_remove, p, "work item link")
    }

    #[tool(
        description = "Add an emoji reaction (award emoji) to a GitLab work item. Required: namespace_path (full project or group path), work_item_iid, and name (emoji name, e.g. \"thumbsup\", \"rocket\", \"eyes\"). Returns the created award emoji. For classic project issues you can also use gitlab_emoji_reactions_issues_create.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_work_items_emoji_add(
        &self,
        Parameters(p): Parameters<WorkItemEmojiParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(self, work_item_emoji_add, p, "work item emoji reaction")
    }

    #[tool(
        description = "Remove an emoji reaction (award emoji) from a GitLab work item. Required: namespace_path (full project or group path), work_item_iid, and name (the emoji name to remove). Returns the removed award emoji.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_work_items_emoji_remove(
        &self,
        Parameters(p): Parameters<WorkItemEmojiParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(
            self,
            work_item_emoji_remove,
            p,
            "removing",
            "work item emoji reaction"
        )
    }

    #[tool(
        description = "Add an emoji reaction (award emoji) to a comment (note) on a GitLab work item. Required: note_id (the note's global ID, e.g. \"gid://gitlab/Note/123\", from gitlab_work_items_notes_list) and name (emoji name, e.g. \"thumbsup\"). Returns the created award emoji.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn gitlab_work_items_notes_emoji_add(
        &self,
        Parameters(p): Parameters<WorkItemNoteEmojiParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_create!(
            self,
            work_item_note_emoji_add,
            p,
            "work item note emoji reaction"
        )
    }

    #[tool(
        description = "Remove an emoji reaction (award emoji) from a comment (note) on a GitLab work item. Required: note_id (the note's global ID) and name (the emoji name to remove). Returns the removed award emoji.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn gitlab_work_items_notes_emoji_remove(
        &self,
        Parameters(p): Parameters<WorkItemNoteEmojiParams>,
    ) -> Result<CallToolResult, McpError> {
        delegate_json!(
            self,
            work_item_note_emoji_remove,
            p,
            "removing",
            "work item note emoji reaction"
        )
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_partial_json, body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        WorkItemCreateParams, WorkItemDeleteParams, WorkItemEmojiParams, WorkItemGetParams,
        WorkItemLinkAddParams, WorkItemNoteCreateParams, WorkItemNoteDeleteParams,
        WorkItemNoteEmojiParams, WorkItemNoteUpdateParams, WorkItemNotesListParams,
        WorkItemUpdateParams, WorkItemsListParams, work_item_create, work_item_delete,
        work_item_emoji_add, work_item_get, work_item_link_add, work_item_note_create,
        work_item_note_delete, work_item_note_emoji_add, work_item_note_update,
        work_item_notes_list, work_item_update, work_items_list,
    };
    use crate::test_util::mock_client;

    /// A raw work-item node as GitLab's GraphQL API returns it, widgets and all.
    fn work_item_node(iid: u64, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": format!("gid://gitlab/WorkItem/{}", iid * 100),
            "iid": iid.to_string(),
            "title": title,
            "state": "OPEN",
            "confidential": false,
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-02T00:00:00Z",
            "webUrl": format!("https://gitlab.example.com/mygroup/myproject/-/work_items/{iid}"),
            "userDiscussionsCount": 4,
            "workItemType": { "name": "Task" },
            "author": {
                "id": "gid://gitlab/User/1",
                "username": "alice",
                "name": "Alice",
                "webUrl": "https://gitlab.example.com/alice"
            },
            "widgets": [
                { "type": "DESCRIPTION", "description": "the body" },
                { "type": "ASSIGNEES", "assignees": { "nodes": [
                    { "id": "gid://gitlab/User/2", "username": "bob", "name": "Bob" }
                ] } },
                { "type": "LABELS", "labels": { "nodes": [
                    { "id": "gid://gitlab/Label/1", "title": "bug", "color": "#ff0000" },
                    { "id": "gid://gitlab/Label/2", "title": "p1", "color": "#00ff00" }
                ] } },
                { "type": "HIERARCHY",
                  "parent": { "id": "gid://gitlab/WorkItem/9", "iid": "1", "title": "Parent" },
                  "children": { "count": 1, "nodes": [
                      { "id": "gid://gitlab/WorkItem/11", "iid": "3", "title": "Child", "state": "OPEN" }
                  ] } },
                { "type": "START_AND_DUE_DATE", "startDate": "2026-01-01", "dueDate": null },
                { "type": "MILESTONE", "milestone": { "id": "gid://gitlab/Milestone/3", "iid": "1", "title": "v1.0" } },
                { "type": "WEIGHT", "weight": 5 },
                { "type": "LINKED_ITEMS", "blocked": false, "blockingCount": 1, "blockedByCount": 0,
                  "linkedItems": { "nodes": [
                      { "linkId": "gid://gitlab/WorkItems::RelatedWorkItemLink/1", "linkType": "blocks",
                        "workItem": { "id": "gid://gitlab/WorkItem/50", "iid": "8", "title": "Blocked one", "state": "OPEN" } }
                  ] } },
                { "type": "AWARD_EMOJI", "upvotes": 2, "downvotes": 0,
                  "awardEmoji": { "nodes": [
                      { "name": "thumbsup", "user": { "id": "gid://gitlab/User/3", "username": "carol", "name": "Carol" } }
                  ] } },
                { "type": "DEVELOPMENT", "closingMergeRequests": { "nodes": [
                    { "mergeRequest": { "iid": "12", "title": "Fix it", "webUrl": "https://gitlab.example.com/mr/12", "state": "opened" } }
                ] } },
                { "type": "ITERATION", "iteration": { "id": "gid://gitlab/Iteration/4", "iid": "2", "title": "Sprint 4", "startDate": "2026-02-01", "dueDate": "2026-02-14" } },
                { "type": "HEALTH_STATUS", "healthStatus": "onTrack" }
            ]
        })
    }

    #[test]
    fn to_snake_case_converts_camel() {
        assert_eq!(super::to_snake_case("webUrl"), "web_url");
        assert_eq!(super::to_snake_case("workItemType"), "work_item_type");
        assert_eq!(
            super::to_snake_case("closingMergeRequests"),
            "closing_merge_requests"
        );
        assert_eq!(super::to_snake_case("iid"), "iid"); // no change
        assert_eq!(super::to_snake_case("_links"), "_links"); // leading underscore kept
    }

    // ------------------------------------------------------------------
    // work_item_get
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_get_flattens_widgets() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "fullPath": "mygroup/myproject", "iids": ["42"] }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": {
                    "nodes": [work_item_node(42, "Do the thing")]
                } } }
            })))
            .mount(&server)
            .await;

        let item = work_item_get(
            &mock_client(&server),
            WorkItemGetParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
            },
        )
        .await
        .unwrap();

        // Top-level fields preserved.
        assert_eq!(item["title"], "Do the thing");
        assert_eq!(item["iid"], "42");
        // Casing normalized to match inputs.
        assert_eq!(item["work_item_type"], "TASK");
        assert_eq!(item["state"], "opened");
        // Comment-thread count surfaced.
        assert_eq!(item["user_discussions_count"], 4);
        // Widgets lifted to the top level, `widgets` envelope gone.
        assert!(item.get("widgets").is_none());
        assert_eq!(item["description"], "the body");
        assert_eq!(item["assignees"][0]["username"], "bob");
        // Labels collapsed to title strings.
        assert_eq!(item["labels"], serde_json::json!(["bug", "p1"]));
        assert_eq!(item["parent"]["title"], "Parent");
        // get keeps the full children array AND the count.
        assert_eq!(item["children"][0]["iid"], "3");
        assert_eq!(item["children_count"], 1);
        assert_eq!(item["start_date"], "2026-01-01");
        assert_eq!(item["milestone"]["title"], "v1.0");
        assert_eq!(item["weight"], 5);
        // Linked items + emoji reactions lifted (arrays + cheap scalar signals).
        assert_eq!(item["blocked"], false);
        assert_eq!(item["blocking_count"], 1);
        assert_eq!(item["linked_items"][0]["link_type"], "blocks");
        assert_eq!(item["linked_items"][0]["work_item"]["iid"], "8");
        assert_eq!(item["upvotes"], 2);
        assert_eq!(item["award_emoji"][0]["name"], "thumbsup");
        assert_eq!(item["award_emoji"][0]["user"]["username"], "carol");
        // closingMergeRequests (the closed_by equivalent) flattened to the MRs.
        assert_eq!(item["closing_merge_requests"][0]["iid"], "12");
        assert_eq!(item["closing_merge_requests"][0]["title"], "Fix it");
        // iteration + health status (Premium), and snake_case output keys.
        assert_eq!(item["iteration"]["title"], "Sprint 4");
        assert_eq!(item["iteration"]["start_date"], "2026-02-01");
        assert_eq!(item["health_status"], "onTrack");
        // Nested camelCase keys converted too.
        assert_eq!(
            item["closing_merge_requests"][0]["web_url"],
            "https://gitlab.example.com/mr/12"
        );
    }

    #[tokio::test]
    async fn work_item_get_missing_node_is_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [] } } }
            })))
            .mount(&server)
            .await;

        let err = work_item_get(
            &mock_client(&server),
            WorkItemGetParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 999,
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => assert!(msg.contains("not found")),
            other => panic!("expected Other error, got {other}"),
        }
    }

    #[tokio::test]
    async fn work_item_get_surfaces_graphql_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [{ "message": "Field 'bogus' doesn't exist" }],
                "data": null
            })))
            .mount(&server)
            .await;

        let err = work_item_get(
            &mock_client(&server),
            WorkItemGetParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 1,
            },
        )
        .await
        .unwrap_err();
        // The graphql() helper maps a top-level errors array to Api { status: 200 }.
        assert!(matches!(err, crate::client::GitlabError::Api { .. }));
    }

    // ------------------------------------------------------------------
    // work_items_list
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_items_list_returns_nodes_and_page_info() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": {
                    "fullPath": "mygroup/myproject",
                    "state": "opened",
                    "types": ["TASK"],
                    "first": 20
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": {
                    "pageInfo": { "hasNextPage": true, "endCursor": "CURSOR123" },
                    "nodes": [work_item_node(1, "Alpha"), work_item_node(2, "Beta")]
                } } }
            })))
            .mount(&server)
            .await;

        let result = work_items_list(
            &mock_client(&server),
            WorkItemsListParams {
                namespace_path: "mygroup/myproject".into(),
                types: Some(vec!["TASK".into()]),
                state: Some("opened".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let nodes = result["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        // Flattening applied to each node.
        assert_eq!(nodes[0]["title"], "Alpha");
        assert_eq!(nodes[0]["labels"], serde_json::json!(["bug", "p1"]));
        assert!(nodes[0].get("widgets").is_none());
        // List slimming: bulk fields dropped, cheap signals kept.
        assert!(
            nodes[0].get("description").is_none(),
            "description stripped from list nodes"
        );
        assert!(
            nodes[0].get("children").is_none(),
            "children array stripped from list nodes"
        );
        assert_eq!(nodes[0]["children_count"], 1, "child count retained");
        assert_eq!(
            nodes[0]["user_discussions_count"], 4,
            "comment count retained"
        );
        // Relation arrays dropped from list; cheap scalar signals kept.
        assert!(
            nodes[0].get("linked_items").is_none(),
            "linked_items stripped"
        );
        assert!(
            nodes[0].get("award_emoji").is_none(),
            "award_emoji stripped"
        );
        assert!(
            nodes[0].get("closing_merge_requests").is_none(),
            "closing_merge_requests stripped"
        );
        assert_eq!(nodes[0]["upvotes"], 2, "upvote count retained");
        assert_eq!(nodes[0]["blocking_count"], 1, "blocking count retained");
        // Cursor surfaced for the caller to paginate.
        assert_eq!(result["page_info"]["has_next_page"], true);
        assert_eq!(result["page_info"]["end_cursor"], "CURSOR123");
    }

    #[tokio::test]
    async fn work_items_list_caps_first_at_100() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "first": 100 }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": {
                    "pageInfo": { "hasNextPage": false, "endCursor": null },
                    "nodes": []
                } } }
            })))
            .mount(&server)
            .await;

        let result = work_items_list(
            &mock_client(&server),
            WorkItemsListParams {
                namespace_path: "mygroup/myproject".into(),
                first: Some(500),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(result["nodes"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn work_items_list_missing_namespace_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": null }
            })))
            .mount(&server)
            .await;

        let err = work_items_list(
            &mock_client(&server),
            WorkItemsListParams {
                namespace_path: "ghost/nope".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => assert!(msg.contains("namespace not found")),
            other => panic!("expected Other error, got {other}"),
        }
    }

    #[tokio::test]
    async fn work_items_list_passes_filters_and_sort() {
        let server = MockServer::start().await;
        // The mock matches only if every filter is forwarded as the right variable.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": {
                    "authorUsername": "alice",
                    "assigneeUsernames": ["bob"],
                    "labelName": ["bug"],
                    "milestoneTitle": ["v1"],
                    "confidential": true,
                    "createdAfter": "2026-01-01T00:00:00Z",
                    "dueBefore": "2026-12-31T00:00:00Z",
                    "sort": "DUE_DATE_ASC"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": {
                    "pageInfo": { "hasNextPage": false, "endCursor": null },
                    "nodes": [work_item_node(1, "Filtered")]
                } } }
            })))
            .mount(&server)
            .await;

        let result = work_items_list(
            &mock_client(&server),
            WorkItemsListParams {
                namespace_path: "mygroup/myproject".into(),
                author_username: Some("alice".into()),
                assignee_usernames: Some(vec!["bob".into()]),
                labels: Some(vec!["bug".into()]),
                milestone_title: Some("v1".into()),
                confidential: Some(true),
                created_after: Some("2026-01-01T00:00:00Z".into()),
                due_before: Some("2026-12-31T00:00:00Z".into()),
                sort: Some("DUE_DATE_ASC".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(result["nodes"][0]["title"], "Filtered");
    }

    #[tokio::test]
    async fn work_items_list_fetch_all_walks_pages() {
        let server = MockServer::start().await;
        // Page 1 (after = null): one node, hasNextPage = true.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains(r#""after":null"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": {
                    "pageInfo": { "hasNextPage": true, "endCursor": "P2" },
                    "nodes": [work_item_node(1, "Page1")]
                } } }
            })))
            .mount(&server)
            .await;
        // Page 2 (after = "P2"): one node, hasNextPage = false.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains(r#""after":"P2""#))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": {
                    "pageInfo": { "hasNextPage": false, "endCursor": null },
                    "nodes": [work_item_node(2, "Page2")]
                } } }
            })))
            .mount(&server)
            .await;

        let result = work_items_list(
            &mock_client(&server),
            WorkItemsListParams {
                namespace_path: "mygroup/myproject".into(),
                fetch_all: Some(true),
                first: Some(1), // ignored when fetch_all
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let nodes = result["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2, "both pages merged");
        assert_eq!(nodes[0]["title"], "Page1");
        assert_eq!(nodes[1]["title"], "Page2");
        // Merged result is presented as a single complete page.
        assert_eq!(result["page_info"]["has_next_page"], false);
    }

    // ------------------------------------------------------------------
    // work_item_create
    // ------------------------------------------------------------------

    /// A `workItemTypes` introspection response, for the create path's type
    /// resolution step. Matched by `variables.p` (the only call that sends it).
    async fn mount_type_resolution(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "p": "mygroup/myproject" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItemTypes": { "nodes": [
                    { "id": "gid://gitlab/WorkItems::Type/1", "name": "Issue" },
                    { "id": "gid://gitlab/WorkItems::Type/5", "name": "Task" }
                ] } } }
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn work_item_create_resolves_type_then_returns_flattened() {
        let server = MockServer::start().await;
        mount_type_resolution(&server).await;
        // The mutation is the only call carrying `variables.input`.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": {
                    "workItemTypeId": "gid://gitlab/WorkItems::Type/5",
                    "title": "New task"
                } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemCreate": {
                    "errors": [],
                    "workItem": work_item_node(7, "New task")
                } }
            })))
            .mount(&server)
            .await;

        let item = work_item_create(
            &mock_client(&server),
            WorkItemCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_type: "task".into(), // case-insensitive
                title: "New task".into(),
                description: Some("body".into()),
                confidential: None,
                labels: None,
                assignees: None,
                parent_work_item_iid: None,
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(item["title"], "New task");
        assert_eq!(item["work_item_type"], "TASK");
        assert!(item.get("widgets").is_none());
    }

    #[tokio::test]
    async fn work_item_create_unknown_type_lists_available() {
        let server = MockServer::start().await;
        mount_type_resolution(&server).await;

        let err = work_item_create(
            &mock_client(&server),
            WorkItemCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_type: "epic".into(), // not in the Free-tier list
                title: "x".into(),
                description: None,
                confidential: None,
                labels: None,
                assignees: None,
                parent_work_item_iid: None,
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => {
                assert!(msg.contains("unknown work item type"));
                assert!(msg.contains("Issue") && msg.contains("Task"));
            }
            other => panic!("expected Other error, got {other}"),
        }
    }

    #[tokio::test]
    async fn work_item_create_surfaces_mutation_payload_errors() {
        let server = MockServer::start().await;
        mount_type_resolution(&server).await;
        // Mutation returns HTTP 200 with a populated payload `errors` array and a
        // null workItem — the silent failure channel graphql() can't catch.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "title": "" } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemCreate": {
                    "errors": ["Title can't be blank"],
                    "workItem": null
                } }
            })))
            .mount(&server)
            .await;

        let err = work_item_create(
            &mock_client(&server),
            WorkItemCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_type: "issue".into(),
                title: "".into(),
                description: None,
                confidential: None,
                labels: None,
                assignees: None,
                parent_work_item_iid: None,
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => {
                assert!(msg.contains("workItemCreate failed"));
                assert!(msg.contains("Title can't be blank"));
            }
            other => panic!("expected Other error, got {other}"),
        }
    }

    // ------------------------------------------------------------------
    // create with label / assignee / parent resolution
    // ------------------------------------------------------------------
    // The type-resolution and label-resolution queries both send only
    // `variables.p`, so these tests disambiguate mocks by a unique substring of
    // the query string (`body_string_contains`) rather than the variables.

    #[tokio::test]
    async fn work_item_create_resolves_labels_assignees_parent() {
        let server = MockServer::start().await;
        // type name -> type id
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("workItemTypes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItemTypes": { "nodes": [
                    { "id": "gid://gitlab/WorkItems::Type/1", "name": "Issue" }
                ] } } }
            })))
            .mount(&server)
            .await;
        // label names -> label ids
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("labels(first"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "project": { "labels": { "nodes": [
                        { "id": "gid://gitlab/ProjectLabel/10", "title": "bug" }
                    ] } },
                    "group": null
                }
            })))
            .mount(&server)
            .await;
        // usernames -> user ids
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("users(usernames"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "users": { "nodes": [
                    { "id": "gid://gitlab/User/5", "username": "alice" }
                ] } }
            })))
            .mount(&server)
            .await;
        // parent IID -> work item GID
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(
                serde_json::json!({ "variables": { "iids": ["9"] } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [
                    { "id": "gid://gitlab/WorkItem/900" }
                ] } } }
            })))
            .mount(&server)
            .await;
        // mutation — matches only if the resolved GIDs were wired into the input,
        // so this asserts the whole resolution chain end-to-end.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": {
                    "labelsWidget": { "labelIds": ["gid://gitlab/ProjectLabel/10"] },
                    "assigneesWidget": { "assigneeIds": ["gid://gitlab/User/5"] },
                    "hierarchyWidget": { "parentId": "gid://gitlab/WorkItem/900" }
                } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemCreate": {
                    "errors": [],
                    "workItem": work_item_node(12, "With widgets")
                } }
            })))
            .mount(&server)
            .await;

        let item = work_item_create(
            &mock_client(&server),
            WorkItemCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_type: "ISSUE".into(),
                title: "With widgets".into(),
                description: None,
                confidential: None,
                labels: Some(vec!["bug".into()]),
                assignees: Some(vec!["alice".into()]),
                parent_work_item_iid: Some(9),
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "With widgets");
    }

    #[tokio::test]
    async fn work_item_create_label_not_found_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("workItemTypes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItemTypes": { "nodes": [
                    { "id": "gid://gitlab/WorkItems::Type/1", "name": "Issue" }
                ] } } }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("labels(first"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "project": { "labels": { "nodes": [] } }, "group": null }
            })))
            .mount(&server)
            .await;

        let err = work_item_create(
            &mock_client(&server),
            WorkItemCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_type: "ISSUE".into(),
                title: "x".into(),
                description: None,
                confidential: None,
                labels: Some(vec!["ghost".into()]),
                assignees: None,
                parent_work_item_iid: None,
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => {
                assert!(msg.contains("label(s) not found"));
                assert!(msg.contains("ghost"));
            }
            other => panic!("expected Other error, got {other}"),
        }
    }

    #[tokio::test]
    async fn work_item_create_builds_date_milestone_weight_widgets() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("workItemTypes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItemTypes": { "nodes": [
                    { "id": "gid://gitlab/WorkItems::Type/1", "name": "Issue" }
                ] } } }
            })))
            .mount(&server)
            .await;
        // Mutation matches only if the widgets are built correctly: dates carry
        // isFixed:true, milestone is the constructed GID, weight is the int.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": {
                    "startAndDueDateWidget": { "isFixed": true, "startDate": "2026-01-01", "dueDate": "2026-12-31" },
                    "milestoneWidget": { "milestoneId": "gid://gitlab/Milestone/7" },
                    "weightWidget": { "weight": 3 }
                } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemCreate": { "errors": [], "workItem": work_item_node(1, "scheduled") } }
            })))
            .mount(&server)
            .await;

        let item = work_item_create(
            &mock_client(&server),
            WorkItemCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_type: "ISSUE".into(),
                title: "scheduled".into(),
                description: None,
                confidential: None,
                labels: None,
                assignees: None,
                parent_work_item_iid: None,
                start_date: Some("2026-01-01".into()),
                due_date: Some("2026-12-31".into()),
                milestone_id: Some(7),
                weight: Some(3),
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "scheduled");
    }

    // ------------------------------------------------------------------
    // work_item_update
    // ------------------------------------------------------------------

    /// An IID→GID resolution response, for the update/delete path. Matched by
    /// `variables.iids` (the only call that sends it).
    async fn mount_gid_resolution(server: &MockServer, gid: &str) {
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "iids": ["42"] }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [ { "id": gid } ] } } }
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn work_item_update_resolves_gid_and_maps_state_event() {
        let server = MockServer::start().await;
        let gid = "gid://gitlab/WorkItem/700";
        mount_gid_resolution(&server, gid).await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": {
                    "id": gid,
                    "title": "Renamed",
                    "stateEvent": "CLOSE",
                    "descriptionWidget": { "description": "new body" }
                } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemUpdate": {
                    "errors": [],
                    "workItem": work_item_node(42, "Renamed")
                } }
            })))
            .mount(&server)
            .await;

        let item = work_item_update(
            &mock_client(&server),
            WorkItemUpdateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                title: Some("Renamed".into()),
                description: Some("new body".into()),
                state_event: Some("close".into()),
                confidential: None,
                add_labels: None,
                remove_labels: None,
                assignees: None,
                parent_work_item_iid: None,
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Renamed");
    }

    #[tokio::test]
    async fn work_item_update_rejects_invalid_state_event() {
        let server = MockServer::start().await;
        mount_gid_resolution(&server, "gid://gitlab/WorkItem/700").await;

        let err = work_item_update(
            &mock_client(&server),
            WorkItemUpdateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                title: None,
                description: None,
                state_event: Some("archive".into()),
                confidential: None,
                add_labels: None,
                remove_labels: None,
                assignees: None,
                parent_work_item_iid: None,
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => assert!(msg.contains("invalid state_event")),
            other => panic!("expected Other error, got {other}"),
        }
    }

    #[tokio::test]
    async fn work_item_update_resolves_add_remove_labels_and_assignees() {
        let server = MockServer::start().await;
        let gid = "gid://gitlab/WorkItem/700";
        mount_gid_resolution(&server, gid).await;
        // Both add and remove names resolve through the same labels query.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("labels(first"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "project": { "labels": { "nodes": [
                    { "id": "gid://gitlab/ProjectLabel/10", "title": "bug" },
                    { "id": "gid://gitlab/ProjectLabel/11", "title": "wontfix" }
                ] } }, "group": null }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_string_contains("users(usernames"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "users": { "nodes": [
                    { "id": "gid://gitlab/User/5", "username": "alice" }
                ] } }
            })))
            .mount(&server)
            .await;
        // Mutation matches only with the correctly-routed add/remove ids + assignee.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": {
                    "id": gid,
                    "labelsWidget": {
                        "addLabelIds": ["gid://gitlab/ProjectLabel/10"],
                        "removeLabelIds": ["gid://gitlab/ProjectLabel/11"]
                    },
                    "assigneesWidget": { "assigneeIds": ["gid://gitlab/User/5"] }
                } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemUpdate": {
                    "errors": [],
                    "workItem": work_item_node(42, "Updated")
                } }
            })))
            .mount(&server)
            .await;

        let item = work_item_update(
            &mock_client(&server),
            WorkItemUpdateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                title: None,
                description: None,
                state_event: None,
                confidential: None,
                add_labels: Some(vec!["bug".into()]),
                remove_labels: Some(vec!["wontfix".into()]),
                assignees: Some(vec!["alice".into()]),
                parent_work_item_iid: None,
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Updated");
    }

    // ------------------------------------------------------------------
    // linked items + emoji reactions
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_link_add_resolves_source_and_targets() {
        let server = MockServer::start().await;
        // Resolve the source iid (42) -> GID.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "iids": ["42"] }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [
                    { "id": "gid://gitlab/WorkItem/420" }
                ] } } }
            })))
            .mount(&server)
            .await;
        // Resolve the target iids (7, 8) -> GIDs.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "iids": ["7", "8"] }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [
                    { "id": "gid://gitlab/WorkItem/70", "iid": "7" },
                    { "id": "gid://gitlab/WorkItem/80", "iid": "8" }
                ] } } }
            })))
            .mount(&server)
            .await;
        // Mutation matches only with the mapped link type + resolved target GIDs.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": {
                    "id": "gid://gitlab/WorkItem/420",
                    "linkType": "BLOCKS",
                    "workItemsIds": ["gid://gitlab/WorkItem/70", "gid://gitlab/WorkItem/80"]
                } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemAddLinkedItems": {
                    "errors": [],
                    "workItem": work_item_node(42, "Linker")
                } }
            })))
            .mount(&server)
            .await;

        let item = work_item_link_add(
            &mock_client(&server),
            WorkItemLinkAddParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                target_work_item_iids: vec![7, 8],
                link_type: Some("blocks".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Linker");
    }

    #[tokio::test]
    async fn work_item_link_add_rejects_invalid_link_type() {
        let server = MockServer::start().await;
        mount_gid_resolution(&server, "gid://gitlab/WorkItem/420").await;
        // Targets resolve (the call happens before link_type validation).
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "iids": ["7"] }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [
                    { "id": "gid://gitlab/WorkItem/70", "iid": "7" }
                ] } } }
            })))
            .mount(&server)
            .await;

        let err = work_item_link_add(
            &mock_client(&server),
            WorkItemLinkAddParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                target_work_item_iids: vec![7],
                link_type: Some("supersedes".into()),
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => assert!(msg.contains("invalid link_type")),
            other => panic!("expected Other error, got {other}"),
        }
    }

    #[tokio::test]
    async fn work_item_emoji_add_resolves_gid_and_returns_award() {
        let server = MockServer::start().await;
        mount_gid_resolution(&server, "gid://gitlab/WorkItem/420").await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "awardableId": "gid://gitlab/WorkItem/420", "name": "rocket" } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "awardEmojiAdd": {
                    "errors": [],
                    "awardEmoji": { "name": "rocket", "user": { "id": "gid://gitlab/User/1", "username": "alice", "name": "Alice" } }
                } }
            })))
            .mount(&server)
            .await;

        let award = work_item_emoji_add(
            &mock_client(&server),
            WorkItemEmojiParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                name: "rocket".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(award["name"], "rocket");
        assert_eq!(award["user"]["username"], "alice");
    }

    // ------------------------------------------------------------------
    // work_item_delete
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_delete_resolves_gid_and_succeeds() {
        let server = MockServer::start().await;
        let gid = "gid://gitlab/WorkItem/700";
        mount_gid_resolution(&server, gid).await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "id": gid } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemDelete": { "errors": [] } }
            })))
            .mount(&server)
            .await;

        let result = work_item_delete(
            &mock_client(&server),
            WorkItemDeleteParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
            },
        )
        .await;
        assert!(result.is_ok());
    }

    // ------------------------------------------------------------------
    // notes (comments)
    // ------------------------------------------------------------------

    fn note_node(id: u64, body: &str) -> serde_json::Value {
        serde_json::json!({
            "id": format!("gid://gitlab/Note/{id}"),
            "body": body,
            "system": false,
            "internal": false,
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "url": "https://gitlab.example.com/note",
            "author": { "id": "gid://gitlab/User/1", "username": "alice", "name": "Alice" }
        })
    }

    #[tokio::test]
    async fn work_item_notes_list_extracts_notes_widget() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "iids": ["42"] }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [ { "widgets": [
                    { "type": "DESCRIPTION", "description": "ignored" },
                    { "type": "NOTES", "notes": {
                        "pageInfo": { "hasNextPage": false, "endCursor": "C1" },
                        "nodes": [ note_node(1, "first comment"), note_node(2, "second") ]
                    } }
                ] } ] } } }
            })))
            .mount(&server)
            .await;

        let result = work_item_notes_list(
            &mock_client(&server),
            WorkItemNotesListParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                first: None,
                after: None,
            },
        )
        .await
        .unwrap();

        let nodes = result["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["body"], "first comment");
        assert_eq!(result["page_info"]["end_cursor"], "C1");
    }

    #[tokio::test]
    async fn work_item_notes_list_no_widget_returns_empty() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "namespace": { "workItems": { "nodes": [ { "widgets": [
                    { "type": "DESCRIPTION", "description": "x" }
                ] } ] } } }
            })))
            .mount(&server)
            .await;

        let result = work_item_notes_list(
            &mock_client(&server),
            WorkItemNotesListParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                first: None,
                after: None,
            },
        )
        .await
        .unwrap();
        assert!(result["nodes"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn work_item_note_create_resolves_gid_and_returns_note() {
        let server = MockServer::start().await;
        let gid = "gid://gitlab/WorkItem/700";
        mount_gid_resolution(&server, gid).await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "noteableId": gid, "body": "hello", "internal": true } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "createNote": { "errors": [], "note": note_node(9, "hello") } }
            })))
            .mount(&server)
            .await;

        let note = work_item_note_create(
            &mock_client(&server),
            WorkItemNoteCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                body: "hello".into(),
                internal: Some(true),
                discussion_id: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(note["body"], "hello");
        assert_eq!(note["id"], "gid://gitlab/Note/9");
    }

    #[tokio::test]
    async fn work_item_note_create_surfaces_payload_errors() {
        let server = MockServer::start().await;
        let gid = "gid://gitlab/WorkItem/700";
        mount_gid_resolution(&server, gid).await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "noteableId": gid } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "createNote": { "errors": ["Body can't be blank"], "note": null } }
            })))
            .mount(&server)
            .await;

        let err = work_item_note_create(
            &mock_client(&server),
            WorkItemNoteCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                body: "".into(),
                internal: None,
                discussion_id: None,
            },
        )
        .await
        .unwrap_err();
        match err {
            crate::client::GitlabError::Other(msg) => {
                assert!(msg.contains("createNote failed"));
                assert!(msg.contains("Body can't be blank"));
            }
            other => panic!("expected Other error, got {other}"),
        }
    }

    #[tokio::test]
    async fn work_item_note_update_sends_id_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "id": "gid://gitlab/Note/9", "body": "edited" } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "updateNote": { "errors": [], "note": note_node(9, "edited") } }
            })))
            .mount(&server)
            .await;

        let note = work_item_note_update(
            &mock_client(&server),
            WorkItemNoteUpdateParams {
                note_id: "gid://gitlab/Note/9".into(),
                body: "edited".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(note["body"], "edited");
    }

    #[tokio::test]
    async fn work_item_note_delete_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "id": "gid://gitlab/Note/9" } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "destroyNote": { "errors": [] } }
            })))
            .mount(&server)
            .await;

        let result = work_item_note_delete(
            &mock_client(&server),
            WorkItemNoteDeleteParams {
                note_id: "gid://gitlab/Note/9".into(),
            },
        )
        .await;
        assert!(result.is_ok());
    }

    // ------------------------------------------------------------------
    // #23 follow-ups: clear parent, threaded reply, note emoji
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn work_item_update_parent_zero_detaches() {
        let server = MockServer::start().await;
        let gid = "gid://gitlab/WorkItem/700";
        mount_gid_resolution(&server, gid).await;
        // Sentinel 0 → hierarchyWidget { parentId: null } (no target resolution).
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "id": gid, "hierarchyWidget": { "parentId": null } } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "workItemUpdate": { "errors": [], "workItem": work_item_node(42, "Detached") } }
            })))
            .mount(&server)
            .await;

        let item = work_item_update(
            &mock_client(&server),
            WorkItemUpdateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                title: None,
                description: None,
                state_event: None,
                confidential: None,
                add_labels: None,
                remove_labels: None,
                assignees: None,
                parent_work_item_iid: Some(0),
                start_date: None,
                due_date: None,
                milestone_id: None,
                weight: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(item["title"], "Detached");
    }

    #[tokio::test]
    async fn work_item_note_create_replies_in_thread() {
        let server = MockServer::start().await;
        mount_gid_resolution(&server, "gid://gitlab/WorkItem/700").await;
        // discussion_id flows through to the mutation as discussionId.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": {
                    "body": "a reply",
                    "discussionId": "gid://gitlab/Discussion/abc"
                } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "createNote": { "errors": [], "note": note_node(9, "a reply") } }
            })))
            .mount(&server)
            .await;

        let note = work_item_note_create(
            &mock_client(&server),
            WorkItemNoteCreateParams {
                namespace_path: "mygroup/myproject".into(),
                work_item_iid: 42,
                body: "a reply".into(),
                internal: None,
                discussion_id: Some("gid://gitlab/Discussion/abc".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(note["body"], "a reply");
    }

    #[tokio::test]
    async fn work_item_note_emoji_add_uses_note_gid_directly() {
        let server = MockServer::start().await;
        // No GID resolution: the note GID is the awardableId.
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(body_partial_json(serde_json::json!({
                "variables": { "input": { "awardableId": "gid://gitlab/Note/9", "name": "eyes" } }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "awardEmojiAdd": { "errors": [], "awardEmoji": { "name": "eyes", "user": { "id": "gid://gitlab/User/1", "username": "alice", "name": "Alice" } } } }
            })))
            .mount(&server)
            .await;

        let award = work_item_note_emoji_add(
            &mock_client(&server),
            WorkItemNoteEmojiParams {
                note_id: "gid://gitlab/Note/9".into(),
                name: "eyes".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(award["name"], "eyes");
    }
}
