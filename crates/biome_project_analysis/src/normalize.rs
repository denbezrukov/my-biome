use crate::{
    inventory::WorkspaceRecord,
    model::{AnalysisSnapshot, FileId, SourceRecord, WorkspaceId},
    result::ProjectAnalysisResult,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Compares snapshot semantics across independently allocated service sessions.
/// This projection must never replace revision equality when validating stale fixes.
pub fn normalize_snapshot(snapshot: &AnalysisSnapshot) -> Value {
    let workspaces = snapshot
        .inventory
        .workspaces
        .iter()
        .map(|record| (record.workspace, record))
        .collect();
    normalize(
        serde_json::to_value(snapshot).expect("snapshot serialization"),
        &snapshot.sources,
        &workspaces,
    )
}

/// Retains findings, source text, digests, scope, evidence and coverage while replacing local IDs.
pub fn normalize_result(result: &ProjectAnalysisResult) -> Value {
    let workspaces = result
        .workspaces
        .iter()
        .map(|(id, record)| (*id, record))
        .collect();
    normalize(
        serde_json::to_value(result).expect("result serialization"),
        &result.sources,
        &workspaces,
    )
}

fn normalize(
    mut value: Value,
    sources: &BTreeMap<FileId, SourceRecord>,
    workspaces: &BTreeMap<WorkspaceId, &WorkspaceRecord>,
) -> Value {
    let files: BTreeMap<_, _> = sources
        .iter()
        .map(|(id, record)| (id.0.to_string(), json!(record.logical_path)))
        .collect();
    let spaces: BTreeMap<_, _> = workspaces
        .iter()
        .map(|(id, record)| (id.0.to_string(), json!(record.path)))
        .collect();
    if let Some(revision) = value.get_mut("revision").and_then(Value::as_object_mut) {
        revision.remove("session_id");
        revision.remove("epoch");
    }
    walk(&mut value, "", &files, &spaces);
    value
}

fn mapped(value: &Value, ids: &BTreeMap<String, Value>) -> Value {
    ids.get(&value.to_string())
        .cloned()
        .unwrap_or_else(|| json!({"unmapped_local_id": value}))
}

fn walk(
    value: &mut Value,
    key: &str,
    files: &BTreeMap<String, Value>,
    spaces: &BTreeMap<String, Value>,
) {
    match value {
        Value::Number(_) if matches!(key, "file" | "manifest") => *value = mapped(value, files),
        Value::Number(_) if key == "workspace" => *value = mapped(value, spaces),
        Value::Object(entries) if matches!(key, "sources" | "modules" | "workspaces") => {
            let ids = if key == "workspaces" { spaces } else { files };
            let mut normalized = serde_json::Map::new();
            for (id, mut entry) in std::mem::take(entries) {
                walk(&mut entry, "", files, spaces);
                let path = ids
                    .get(&id)
                    .and_then(Value::as_str)
                    .map_or_else(|| format!("<unmapped:{id}>"), str::to_owned);
                normalized.insert(path, entry);
            }
            *entries = normalized;
        }
        Value::Object(entries) if key == "nodes" => {
            for node in entries.values_mut() {
                walk(node, "", files, spaces);
            }
        }
        Value::Object(entries) => {
            for (key, entry) in entries {
                walk(entry, key, files, spaces);
            }
        }
        Value::Array(entries) => {
            for entry in entries.iter_mut() {
                if entry.is_number() {
                    match key {
                        "files" => *entry = mapped(entry, files),
                        "workspaces" | "scope" => *entry = mapped(entry, spaces),
                        _ => {}
                    }
                } else {
                    walk(entry, "", files, spaces);
                }
            }
            if matches!(
                key,
                "files"
                    | "workspaces"
                    | "scope"
                    | "roots"
                    | "declarations"
                    | "edges"
                    | "exports"
                    | "uses"
                    | "tags"
                    | "issues"
                    | "hints"
                    | "gaps"
                    | "packages"
                    | "related"
                    | "related_evidence"
                    | "declaration_locations"
                    | "metadata_diagnostics"
                    | "eligible_fixes"
            ) {
                entries.sort_by_cached_key(Value::to_string);
            }
        }
        _ => {}
    }
}
