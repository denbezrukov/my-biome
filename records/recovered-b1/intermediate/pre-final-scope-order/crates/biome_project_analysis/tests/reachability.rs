mod support;
use serde_json::json;
use support::run_case;

#[test]
fn unreachable_chain_and_cycle() {
    let result = run_case("unreachable_chain_and_cycle");
    let paths: Vec<_> = result["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["path"].clone())
        .collect();
    assert_eq!(
        json!(paths),
        support::expected_case("unreachable_chain_and_cycle")["files"]
    );
}

#[test]
fn side_effect_import_keeps_file_only() {
    assert_eq!(
        run_case("side_effect_import_keeps_file_only")["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|issue| issue["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["c/provider.ts", "orphan.ts"]
    );
}
#[test]
fn sibling_unbounded_loader() {
    assert_eq!(
        run_case("sibling_unbounded_loader")["coverage"],
        "incomplete"
    );
    assert_eq!(
        run_case("sibling_unbounded_loader")["issues"],
        serde_json::json!([])
    );
}
#[test]
fn bounded_loader_keeps_unrelated_findings() {
    assert_eq!(
        run_case("bounded_loader_keeps_unrelated_findings")["issues"][0]["path"],
        "orphan.ts"
    );
    assert_eq!(
        run_case("bounded_loader_keeps_unrelated_findings")["issues"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn unreachable_loader_does_not_self_root() {
    let result = run_case("unreachable_loader_does_not_self_root");
    assert_eq!(result["coverage"], "complete");
    assert_eq!(result["issues"].as_array().unwrap().len(), 3);
}
#[test]
fn ignored_consumer_still_keeps_target() {
    let result = run_case("ignored_consumer_still_keeps_target");
    assert_eq!(result["issues"].as_array().unwrap().len(), 1);
    assert_eq!(result["issues"][0]["path"], json!("orphan.ts"));
}

#[test]
fn possible_producers_activate_uncertainty_at_the_fixed_point() {
    let mut snapshot = support::load_case("bounded_loader_keeps_unrelated_findings");
    let mut extra = snapshot.inventory.gaps[0].clone();
    extra.location.as_mut().unwrap().file = biome_project_analysis::model::FileId(2);
    extra.domain = biome_project_analysis::coverage::TargetDomain::Unbounded;
    snapshot.inventory.gaps.push(extra);
    let result = biome_project_analysis::analyze(&snapshot, &Default::default());
    assert!(result.issues.is_empty());
    assert!(result.gaps.iter().all(|gap| gap.active));
}

#[test]
fn missing_payload_is_not_an_import_free_module() {
    let mut snapshot = support::load_case("unreachable_chain_and_cycle");
    snapshot
        .modules
        .get_mut(&biome_project_analysis::model::FileId(0))
        .unwrap()
        .collected = None;
    let result = biome_project_analysis::analyze(&snapshot, &Default::default());
    assert!(result.issues.is_empty());
    assert!(matches!(
        result.coverage,
        biome_project_analysis::coverage::Coverage::Incomplete(_)
    ));
}

#[test]
fn symbol_uncertainty_preserves_independent_file_findings() {
    let mut snapshot = support::load_case("sibling_unbounded_loader");
    snapshot
        .modules
        .get_mut(&biome_project_analysis::model::FileId(0))
        .unwrap()
        .collected
        .as_mut()
        .unwrap()
        .unknown[0]
        .kind = biome_project_analysis::facts::UnknownKind::Exports;
    let result = biome_project_analysis::analyze(&snapshot, &Default::default());
    assert_eq!(result.issues.len(), 3);
    assert!(matches!(
        result.coverage,
        biome_project_analysis::coverage::Coverage::Incomplete(_)
    ));
    assert!(
        result.gaps.iter().all(|gap| gap.active
            && gap.impact == biome_project_analysis::reachability::GapImpact::Symbols)
    );
}

#[test]
fn input_order_and_report_selection_preserve_analysis_evidence() {
    use biome_project_analysis::{
        analyze, model::WorkspaceId, normalize::normalize_result, options::ReportSelection,
        select_report,
    };
    let snapshot = support::load_case("bounded_loader_keeps_unrelated_findings");
    let result = analyze(&snapshot, &Default::default());
    let mut shuffled = snapshot.clone();
    shuffled.inventory.files.reverse();
    shuffled.inventory.workspaces.reverse();
    shuffled.inventory.roots.reverse();
    assert_eq!(
        normalize_result(&result),
        normalize_result(&analyze(&shuffled, &Default::default()))
    );
    let selection = ReportSelection {
        workspaces: [WorkspaceId(1)].into(),
        ..Default::default()
    };
    let selected = select_report(&result, &selection);
    assert!(selected.issues.is_empty());
    assert_eq!(selected.coverage, result.coverage);
    assert_eq!(selected.sources, result.sources);
    assert_eq!(selected.gaps, result.gaps);
    assert_eq!(selected.evidence, result.evidence);
    assert_eq!(selected.membership, result.membership);
}

#[test]
fn ignore_patterns_suppress_reports_without_excluding_consumers() {
    use biome_project_analysis::{analyze, model::FileId, options::AnalysisOptions};
    let snapshot = support::load_case("unreachable_chain_and_cycle");
    let result = analyze(
        &snapshot,
        &AnalysisOptions {
            ignore: vec!["**/provider.ts".into()],
            ..Default::default()
        },
    );
    assert_eq!(result.issues.len(), 1);
    let snapshot = support::load_case("ignored_consumer_still_keeps_target");
    let result = analyze(
        &snapshot,
        &AnalysisOptions {
            ignore: vec!["**/provider.ts".into()],
            ..Default::default()
        },
    );
    assert!(
        result
            .membership
            .files
            .iter()
            .any(|file| file.file == FileId(2) && file.definite && !file.diagnostic_eligible)
    );
}

#[test]
fn metadata_gap_activates_from_its_consumers_not_its_manifest_location() {
    use biome_project_analysis::{
        analyze,
        coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
        facts::ResolutionEvent,
        model::{FileId, Location},
    };
    let mut snapshot = support::load_case("unreachable_chain_and_cycle");
    snapshot.inventory.gaps.push(CoverageGap {
        producer: "resolution metadata".into(),
        domain: TargetDomain::Unbounded,
        reason: GapReason::UnavailableInput("manifest incomplete".into()),
        location: Some(Location {
            file: FileId(0),
            range: Default::default(),
        }),
    });
    snapshot
        .modules
        .get_mut(&FileId(1))
        .unwrap()
        .collected
        .as_mut()
        .unwrap()
        .resolutions[0]
        .trace
        .push(ResolutionEvent::Manifest {
            file: FileId(0),
            known: false,
            diagnostics: vec![],
        });
    let result = analyze(&snapshot, &Default::default());
    assert_eq!(result.issues.len(), 3);
    assert!(!result.gaps[0].active);
    assert_eq!(result.gaps[0].consumers, [FileId(1)].into());
    assert!(matches!(result.coverage, Coverage::Incomplete(_)));
    snapshot.inventory.roots[0].file = FileId(1);
    assert!(analyze(&snapshot, &Default::default()).issues.is_empty());
}

#[test]
fn unknown_kind_association_includes_its_reason() {
    use biome_project_analysis::{analyze, facts::UnknownKind, model::FileId};
    let mut snapshot = support::load_case("sibling_unbounded_loader");
    let unknown = &mut snapshot
        .modules
        .get_mut(&FileId(0))
        .unwrap()
        .collected
        .as_mut()
        .unwrap()
        .unknown;
    let mut symbol = unknown[0].clone();
    symbol.kind = UnknownKind::Exports;
    symbol.reason = "unsupported exports".into();
    unknown.push(symbol);
    assert!(analyze(&snapshot, &Default::default()).issues.is_empty());
}

#[test]
fn located_acquisition_gaps_without_source_consumers_are_global() {
    use biome_project_analysis::{
        analyze,
        coverage::{CoverageGap, GapReason, TargetDomain},
        model::{FileId, Location},
    };
    let mut snapshot = support::load_case("unreachable_chain_and_cycle");
    let mut manifest = snapshot.sources[&FileId(0)].clone();
    manifest.logical_path = "package.json".into();
    snapshot.sources.insert(FileId(4), manifest);
    snapshot.inventory.gaps.push(CoverageGap {
        producer: "manifest".into(),
        domain: TargetDomain::Unbounded,
        reason: GapReason::UnavailableInput("missing root metadata".into()),
        location: Some(Location {
            file: FileId(4),
            range: Default::default(),
        }),
    });
    assert!(analyze(&snapshot, &Default::default()).issues.is_empty());
}

#[test]
fn root_protection_and_global_gap_evidence_are_stable() {
    use biome_project_analysis::{
        analyze,
        coverage::{CoverageGap, GapReason, TargetDomain},
        inventory::RootRole,
        normalize::normalize_result,
    };
    let mut snapshot = support::load_case("unreachable_chain_and_cycle");
    let mut root = snapshot.inventory.roots[0].clone();
    root.role = RootRole::Default;
    snapshot.inventory.roots.push(root);
    let result = analyze(&snapshot, &Default::default());
    assert_eq!(
        result
            .evidence
            .nodes
            .values()
            .filter(|evidence| evidence.producer == "configuration entry")
            .count(),
        2
    );
    snapshot.inventory.gaps.push(CoverageGap {
        producer: "analysis boundary".into(),
        domain: TargetDomain::Unbounded,
        reason: GapReason::UnavailableInput("excluded consumer".into()),
        location: None,
    });
    let result = analyze(&snapshot, &Default::default());
    snapshot.inventory.workspaces.reverse();
    assert_eq!(
        normalize_result(&result),
        normalize_result(&analyze(&snapshot, &Default::default()))
    );
}

#[test]
fn edge_evidence_identity_does_not_depend_on_acquisition_revision() {
    use biome_project_analysis::analyze;
    let mut snapshot = support::load_case("side_effect_import_keeps_file_only");
    let before = analyze(&snapshot, &Default::default());
    snapshot.revision.epoch += 1;
    for module in snapshot.modules.values_mut() {
        for resolution in &mut module.collected.as_mut().unwrap().resolutions {
            resolution.context.config_revision = "new-acquisition-revision".into();
        }
    }
    let after = analyze(&snapshot, &Default::default());
    assert_eq!(
        before.evidence.nodes.keys().collect::<Vec<_>>(),
        after.evidence.nodes.keys().collect::<Vec<_>>()
    );
    assert_ne!(before.evidence, after.evidence);
}

#[test]
fn explicit_empty_scope_is_not_legacy_unspecified_scope() {
    use biome_project_analysis::{analyze, normalize::normalize_snapshot};
    let legacy = support::load_case("unreachable_chain_and_cycle");
    assert_eq!(analyze(&legacy, &Default::default()).context.scope.len(), 4);
    let mut empty = legacy.clone();
    empty.inventory.analysis_scope = Some(Vec::new());
    assert!(
        analyze(&empty, &Default::default())
            .context
            .scope
            .is_empty()
    );
    assert_ne!(normalize_snapshot(&legacy), normalize_snapshot(&empty));
}
