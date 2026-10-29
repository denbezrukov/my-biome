mod support;
use biome_project_analysis::{
    analyze, facts::NamespaceAccess, model::FileId, options::AnalysisOptions, result::IssueKind,
};
#[test]
fn namespace_controls_are_independent() {
    let mut snapshot = support::load_case("known_dot");
    snapshot
        .modules
        .get_mut(&FileId(0))
        .unwrap()
        .collected
        .as_mut()
        .unwrap()
        .uses[0]
        .namespace_access = Some(NamespaceAccess::Escape);
    let mut options = AnalysisOptions::default();
    options.enabled_issues.insert(IssueKind::NsExports);
    let result = analyze(&snapshot, &options);
    assert_eq!(
        result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::NsExports)
            .count(),
        2
    );
    assert_eq!(
        support::run_case("known_dot")["issues"][0]["symbol"],
        "unused"
    );
    assert_eq!(support::expected_case("known_dot")["unused"], "unused");
}
#[test]
fn duplicate_binding_not_duplicate_spelling() {
    assert_eq!(
        support::run_case("duplicate_binding")["issues"][0]["kind"],
        "duplicates"
    );
    assert_eq!(
        support::run_case("duplicate_spelling")["issues"],
        serde_json::json!([])
    );
}

#[test]
fn c4_metadata_duplicate_symbols_follow_source_order() {
    use biome_project_analysis::normalize::normalize_result;
    let mut snapshot = support::load_case("duplicate_binding");
    let result = analyze(&snapshot, &Default::default());
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Duplicates)
    );
    for module in snapshot
        .modules
        .values_mut()
        .filter_map(|module| module.collected.as_mut())
    {
        module.exports.reverse();
        module.declarations.reverse();
    }
    assert_eq!(
        normalize_result(&result),
        normalize_result(&analyze(&snapshot, &Default::default()))
    );
}
#[test]
fn owned_member_payload_survives_selection_and_id_normalization() {
    use biome_project_analysis::{
        model::AnalysisSnapshot,
        normalize::{normalize_result, normalize_snapshot},
        options::ReportSelection,
        select_report,
    };
    let snapshot = support::load_case("known_dot");
    let mut value = serde_json::to_value(snapshot).unwrap();
    let parent = value["modules"]["1"]["collected"]["declarations"][0]["id"].clone();
    let leaf = value["modules"]["1"]["collected"]["declarations"][1]["id"].clone();
    value["modules"]["1"]["collected"]["declarations"][0]["kind"] = "Namespace".into();
    value["modules"]["1"]["collected"]["members"] = serde_json::json!([{"parent":parent,"declaration":leaf,"path":["nested","unused"],"tags":[]}]);
    value["modules"]["0"]["collected"]["uses"][0]["member_path"] =
        serde_json::json!([{"Known":"used"},"Computed"]);
    let mut type_parent = value["modules"]["1"]["collected"]["declarations"][0].clone();
    type_parent["id"]["namespace"] = "type".into();
    value["modules"]["1"]["collected"]["declarations"]
        .as_array_mut()
        .unwrap()
        .push(type_parent);
    let snapshot: AnalysisSnapshot = serde_json::from_value(value.clone()).unwrap();
    let result = analyze(&snapshot, &Default::default());
    assert_eq!(result.members.uncertainty.len(), 1);
    assert!(result.members.used.is_empty());
    let selected = select_report(
        &result,
        &ReportSelection {
            paths: vec!["target.ts".into()],
            ..Default::default()
        },
    );
    assert_eq!(selected.members, result.members);
    fn remap(value: &mut serde_json::Value, key: &str) {
        match value {
            serde_json::Value::Number(_)
                if matches!(key, "file" | "manifest" | "target" | "namespace_target") =>
            {
                *value = serde_json::json!(value.as_u64().unwrap() + 9)
            }
            serde_json::Value::Object(map) if matches!(key, "sources" | "modules") => {
                let old = std::mem::take(map);
                for (key, mut value) in old {
                    remap(&mut value, "");
                    map.insert((key.parse::<u32>().unwrap() + 9).to_string(), value);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    remap(value, key);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    remap(value, "");
                }
            }
            _ => {}
        }
    }
    remap(&mut value, "");
    let mut other: AnalysisSnapshot = serde_json::from_value(value).unwrap();
    for module in other
        .modules
        .values_mut()
        .filter_map(|module| module.collected.as_mut())
    {
        module.declarations.reverse();
    }
    assert_eq!(normalize_snapshot(&snapshot), normalize_snapshot(&other));
    assert_eq!(
        normalize_result(&result),
        normalize_result(&analyze(&other, &Default::default()))
    );
}
#[test]
fn implemented_member_categories_have_native_capabilities() {
    let result = analyze(&support::load_case("empty"), &Default::default());
    for kind in [
        IssueKind::NsExports,
        IssueKind::NsTypes,
        IssueKind::EnumMembers,
        IssueKind::NamespaceMembers,
        IssueKind::Duplicates,
    ] {
        assert_eq!(
            result.capabilities[&kind],
            biome_project_analysis::coverage::CapabilityStatus::Supported
        );
    }
}
#[test]
fn member_ignore_patterns_are_reporting_policy() {
    let mut value = serde_json::to_value(support::load_case("known_dot")).unwrap();
    let parent = value["modules"]["1"]["collected"]["declarations"][0]["id"].clone();
    let leaf = value["modules"]["1"]["collected"]["declarations"][1]["id"].clone();
    value["modules"]["1"]["collected"]["declarations"][0]["kind"] = "Namespace".into();
    value["modules"]["1"]["collected"]["members"] = serde_json::json!([{"parent":parent,"declaration":leaf,"path":["Nested","unused"],"tags":[]}]);
    value["modules"]["0"]["collected"]["uses"][0]["member_path"] =
        serde_json::json!([{"Known":"used"},{"Known":"selected"}]);
    let mut snapshot: biome_project_analysis::model::AnalysisSnapshot =
        serde_json::from_value(value).unwrap();
    let mut options = serde_json::to_value(AnalysisOptions::default()).unwrap();
    options["ignore_members"] = serde_json::json!(["Nested.*"]);
    let options: AnalysisOptions = serde_json::from_value(options).unwrap();
    let result = analyze(&snapshot, &options);
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::NamespaceMembers)
    );
    snapshot
        .modules
        .get_mut(&FileId(0))
        .unwrap()
        .collected
        .as_mut()
        .unwrap()
        .uses[0]
        .member_path[1] = NamespaceAccess::Computed;
    let normal = analyze(&snapshot, &Default::default());
    assert!(!normal.members.uncertainty.is_empty());
    let ignored = analyze(&snapshot, &options);
    assert_eq!(ignored.members.uncertainty, normal.members.uncertainty);
}
