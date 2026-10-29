mod support;

use biome_diagnostics::Severity;
use biome_project_analysis::{
    analyze,
    model::FileId,
    options::{AnalysisMode, AnalysisOptions},
    result::IssueKind,
};

#[test]
fn default_issue_selection() {
    let options = AnalysisOptions::default();
    assert_eq!(
        IssueKind::ALL
            .into_iter()
            .filter(|kind| !options.includes(*kind))
            .collect::<Vec<_>>(),
        [IssueKind::NsExports, IssueKind::NsTypes, IssueKind::Cycles]
    );
    let mut enabled = options;
    enabled.enabled_issues.insert(IssueKind::Cycles);
    assert_eq!(enabled.severity(IssueKind::Cycles), Some(Severity::Warning));
}

#[test]
fn production_and_strict_filters() {
    for mode in [AnalysisMode::Production, AnalysisMode::Strict] {
        let mut options = AnalysisOptions {
            mode,
            ..Default::default()
        };
        for kind in [
            IssueKind::DevDependencies,
            IssueKind::Catalog,
            IssueKind::CatalogReferences,
        ] {
            options.issue_severities.insert(kind, Some(Severity::Error));
            options.enabled_issues.insert(kind);
            assert!(!options.includes(kind));
        }
        assert!(!options.configuration_hints_enabled());
    }
}

#[test]
fn intrinsic_and_custom_tags_preserve_source_identity() {
    for tag in ["public", "beta", "alias", "internal", "probe_1", "foo2"] {
        for mode in [
            AnalysisMode::Default,
            AnalysisMode::Production,
            AnalysisMode::Strict,
        ] {
            let mut snapshot = support::load_case("known_dot");
            snapshot
                .modules
                .get_mut(&FileId(1))
                .unwrap()
                .collected
                .as_mut()
                .unwrap()
                .declarations[1]
                .tags = vec![tag.into()];
            let options = AnalysisOptions {
                mode,
                ..Default::default()
            };
            let result = analyze(&snapshot, &options);
            let protected = matches!(tag, "public" | "beta" | "alias")
                || tag == "internal" && mode != AnalysisMode::Default;
            assert_eq!(result.issues.is_empty(), protected, "{tag} {mode:?}");
            let mut excluded = options;
            excluded.excluded_tags.insert(tag.into());
            assert!(analyze(&snapshot, &excluded).issues.is_empty(), "{tag}");
        }
    }
}

#[test]
fn explicit_selection_is_not_explicit_off() {
    let mut value = serde_json::to_value(AnalysisOptions::default()).unwrap();
    value["issue_selection"] = serde_json::json!(["cycles"]);
    let options: AnalysisOptions = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        IssueKind::ALL
            .into_iter()
            .filter(|kind| options.includes(*kind))
            .collect::<Vec<_>>(),
        [IssueKind::Cycles]
    );
    value["issue_selection"] = serde_json::json!([]);
    let options: AnalysisOptions = serde_json::from_value(value).unwrap();
    assert!(
        IssueKind::ALL
            .into_iter()
            .all(|kind| !options.includes(kind))
    );
}

#[test]
fn explicit_empty_report_retains_evidence() {
    let full = analyze(
        &support::load_case("bounded_loader_keeps_unrelated_findings"),
        &AnalysisOptions::default(),
    );
    assert!(!full.issues.is_empty());
    let selection = serde_json::from_value(
        serde_json::json!({"workspaces":[],"paths":[],"issues":[],"restrict_issues":true}),
    )
    .unwrap();
    let selected = biome_project_analysis::select_report(&full, &selection);
    assert!(selected.issues.is_empty());
    assert_eq!(full.coverage, selected.coverage);
    assert_eq!(full.evidence, selected.evidence);
    assert_eq!(full.membership, selected.membership);
    assert_eq!(full.capabilities, selected.capabilities);
}

#[test]
fn workspace_local_use_overrides_before_consumption() {
    use biome_project_analysis::facts::{Use, UseKind, UseRole};
    let mut snapshot = support::load_case("known_dot");
    let module = snapshot
        .modules
        .get_mut(&FileId(1))
        .unwrap()
        .collected
        .as_mut()
        .unwrap();
    let id = module.declarations[1].id;
    module.uses.push(Use {
        kind: UseKind::Runtime,
        declaration: id,
        location: id.location,
        write: false,
        namespace_access: None,
        member_path: Vec::new(),
        owner: None,
        role: UseRole::Local,
    });
    let workspace = snapshot
        .inventory
        .files
        .iter()
        .find(|file| file.file == FileId(1))
        .unwrap()
        .workspace;
    let mut value = serde_json::to_value(AnalysisOptions::default()).unwrap();
    value["workspaces"] =
        serde_json::json!({workspace.0.to_string(): {"ignore_local_uses":["variable"]}});
    let options = serde_json::from_value(value).unwrap();
    assert!(analyze(&snapshot, &options).issues.is_empty());
}

#[test]
fn stale_tags_and_unused_ignores_have_distinct_causes() {
    let mut snapshot = support::load_case("known_dot");
    snapshot
        .modules
        .get_mut(&FileId(1))
        .unwrap()
        .collected
        .as_mut()
        .unwrap()
        .declarations[0]
        .tags = vec!["custom".into()];
    let options = AnalysisOptions {
        excluded_tags: ["custom".into()].into(),
        ignore: vec!["never/**".into()],
        ..Default::default()
    };
    let result = analyze(&snapshot, &options);
    assert_eq!(result.hints.len(), 2);
    assert!(
        result
            .hints
            .iter()
            .all(|hint| !hint.related_evidence.is_empty())
    );
    let hints = serde_json::to_value(&result.hints).unwrap();
    assert!(
        hints
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint["kind"] == "configuration")
    );
    assert!(
        hints
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint["kind"] == "tag")
    );
}

#[test]
fn include_exclude_precedence() {
    use IssueKind::*;
    use biome_project_analysis::policy::select_issues;
    let mode = AnalysisMode::Default;
    assert_eq!(
        select_issues(mode, &[], &[Exports], &[Exports], &[], false, false),
        [Exports].into()
    );
    assert_eq!(
        select_issues(mode, &[Exports], &[], &[], &[Exports], false, false).len(),
        13
    );
    assert!(select_issues(mode, &[], &[], &[Files], &[Files], false, false).is_empty());
    assert_eq!(
        select_issues(mode, &[], &[], &[Cycles], &[], false, false),
        [Cycles].into()
    );
    assert_eq!(
        select_issues(mode, &[], &[], &[NsTypes], &[], false, false).len(),
        15
    );
    assert_eq!(
        select_issues(mode, &[], &[], &[Dependencies], &[], false, false),
        [Dependencies, DevDependencies, OptionalPeerDependencies].into()
    );
    assert_eq!(
        select_issues(mode, &[], &[Exports], &[], &[], false, true),
        [Exports, Types, EnumMembers, NamespaceMembers, Duplicates].into()
    );
    assert_eq!(
        select_issues(mode, &[], &[], &[], &[], false, true),
        [Exports, Types, EnumMembers, NamespaceMembers, Duplicates].into()
    );
    assert_eq!(
        select_issues(mode, &[], &[], &[], &[], true, false).len(),
        8
    );
    assert_eq!(
        select_issues(AnalysisMode::Production, &[], &[], &[], &[], true, false),
        [Dependencies, Unlisted, Binaries, Unresolved].into()
    );
    let options = AnalysisOptions {
        issue_selection: Some([Exports].into()),
        issue_severities: [(Exports, None)].into(),
        ..Default::default()
    };
    assert!(!options.includes(Exports));
}

#[test]
fn compatibility_tag_mapping_is_explicit() {
    use biome_project_analysis::policy::map_knip_tags;
    let mapped = map_knip_tags(&["-probe_1,-foo2,+@Custom".into()]);
    assert_eq!(
        mapped
            .iter()
            .map(|item| (
                item.original.as_str(),
                item.effective.as_deref(),
                item.excluded
            ))
            .collect::<Vec<_>>(),
        [
            ("-probe_1", Some("probe"), true),
            ("-foo2", Some("foo"), true),
            ("+@Custom", Some("Custom"), false)
        ]
    );
    let mut snapshot = support::load_case("known_dot");
    snapshot
        .modules
        .get_mut(&FileId(1))
        .unwrap()
        .collected
        .as_mut()
        .unwrap()
        .declarations[1]
        .tags = vec!["probe_1".into()];
    let native = AnalysisOptions {
        excluded_tags: ["probe_1".into()].into(),
        ..Default::default()
    };
    let compatibility = AnalysisOptions {
        excluded_tags: [mapped[0].effective.clone().unwrap()].into(),
        tag_mappings: mapped,
        ..Default::default()
    };
    assert!(analyze(&snapshot, &native).issues.is_empty());
    assert_eq!(analyze(&snapshot, &compatibility).issues.len(), 1);
}

#[test]
fn support_fixture_expectations_are_retained() {
    assert_eq!(
        support::run_case("known_dot")["issues"][0]["symbol"],
        support::expected_case("known_dot")["unused"]
    );
}

#[test]
fn repeated_hint_policy_does_not_duplicate_escalated_hints() {
    let snapshot = support::load_case("known_dot");
    let mut options = AnalysisOptions {
        ignore: vec!["never/**".into()],
        ..Default::default()
    };
    options.hints.configuration_errors = true;
    let result = analyze(&snapshot, &options);
    assert_eq!(
        biome_project_analysis::policy::apply_policy(result.clone(), &options),
        result
    );
}
