//! Owned project-analysis contracts, independent of acquisition and mutable graph storage.

pub mod catalogs;
pub mod coverage;
pub mod cycles;
pub mod dependencies;
pub mod exports;
pub mod facts;
pub mod inventory;
pub mod members;
pub mod model;
pub mod normalize;
pub mod options;
pub mod policy;
pub mod reachability;
pub mod report_metadata;
pub mod resolution;
pub mod result;
pub mod scripts;

use coverage::CapabilityStatus;
use facts::ParseStatus;
use model::{AnalysisSnapshot, Location};
use options::{AnalysisOptions, ReportSelection};
use result::{
    AnalysisContext, Evidence, EvidenceId, IssueKind, ProjectAnalysisResult, ProjectIssue,
};

/// Reports supported file, symbol and dependency findings from immutable inputs.
pub fn analyze(snapshot: &AnalysisSnapshot, options: &AnalysisOptions) -> ProjectAnalysisResult {
    let mut reachability = reachability::compute_reachability(snapshot, options);
    let exports = exports::analyze_exports(snapshot, &reachability, options);
    let members = members::analyze_members(snapshot, &exports, &reachability, options);
    let dependencies =
        dependencies::analyze_dependencies(snapshot, &reachability, &exports, options);
    let catalogs = catalogs::analyze_catalogs(snapshot, &reachability, options);
    let cycles = cycles::analyze_cycles(snapshot, &reachability.membership, options);
    let resolution =
        resolution::analyze_unresolved(snapshot, &reachability.membership, &dependencies, options);
    reachability
        .evidence
        .nodes
        .extend(catalogs.evidence.nodes.clone());
    reachability
        .evidence
        .nodes
        .extend(cycles.evidence.nodes.clone());
    reachability
        .evidence
        .nodes
        .extend(resolution.evidence.nodes.clone());
    let mut issues = dependencies.issues.clone();
    issues.extend(catalogs.issues.clone());
    issues.extend(cycles.issues.clone());
    issues.extend(resolution.issues.clone());
    issues.extend(exports.issues.clone());
    reachability
        .evidence
        .nodes
        .extend(dependencies.evidence.nodes.clone());
    issues.extend(members.issues.clone());
    reachability
        .evidence
        .nodes
        .extend(members.evidence.nodes.clone());
    reachability
        .evidence
        .nodes
        .extend(exports.evidence.nodes.clone());
    if !exports.gaps.is_empty()
        || !members.gaps.is_empty()
        || !dependencies.gaps.is_empty()
        || !catalogs.gaps.is_empty()
    {
        let mut gaps = reachability.coverage.gaps().to_vec();
        gaps.extend(exports.gaps.clone());
        gaps.extend(members.gaps.clone());
        gaps.extend(dependencies.gaps.clone());
        gaps.extend(catalogs.gaps.clone());
        reachability.coverage = if matches!(reachability.coverage, coverage::Coverage::Failed(_)) {
            coverage::Coverage::Failed(gaps)
        } else {
            coverage::Coverage::Incomplete(gaps)
        };
    }
    if let Some(severity) = options.severity(IssueKind::Files) {
        for record in &snapshot.inventory.files {
            if !reachability
                .membership
                .reporting_candidates
                .contains(&record.file)
                || reachability.definite_files.contains(&record.file)
                || reachability.possible_files.contains(&record.file)
                || matches!(snapshot.coverage, coverage::Coverage::Failed(_))
            {
                continue;
            }
            let ambient = options.compatibility_profile == "knip-6.43.0-native-core"
                && snapshot
                    .modules
                    .get(&record.file)
                    .and_then(|module| module.collected.as_ref())
                    .is_some_and(|module| {
                        module.parse_status == ParseStatus::Valid
                            && module.declaration_file.is_some()
                            && !module.has_static_exports
                    });
            if ambient {
                continue;
            }
            let path = reachability::path(snapshot, record.file);
            let location = Location {
                file: record.file,
                range: Default::default(),
            };
            let id = EvidenceId(format!("unreachable:{path}"));
            reachability.evidence.nodes.insert(id.clone(), Evidence {
                producer: "file reachability".into(),
                location: Some(location),
                workspace: record.workspace,
                explanation: "No entry, program seed, definite edge or possible reference reaches this candidate".into(),
                related: Vec::new(),
            });
            issues.push(ProjectIssue {
                kind: IssueKind::Files,
                severity,
                identity: format!("files:{path}"),
                symbol: None,
                location,
                workspace: record.workspace,
                related_evidence: vec![id],
                eligible_fixes: Vec::new(),
            });
        }
    }
    issues.sort_by(|left, right| left.identity.cmp(&right.identity));
    let mut result = ProjectAnalysisResult {
        report_metadata: None,
        policy: Default::default(),
        catalogs,
        cycles,
        resolution,
        dependencies,
        exports,
        members,
        revision: snapshot.revision.clone(),
        context: AnalysisContext {
            virtual_scope: snapshot.inventory.virtual_scope.clone(),
            mode: options.mode,
            profile_id: options.compatibility_profile.clone(),
            semantic_options_digest: reachability::hash(options),
            scope: snapshot
                .inventory
                .analysis_scope
                .clone()
                .unwrap_or_else(|| {
                    snapshot
                        .inventory
                        .workspaces
                        .iter()
                        .map(|workspace| workspace.workspace)
                        .collect()
                }),
        },
        sources: snapshot.sources.clone(),
        workspaces: snapshot
            .inventory
            .workspaces
            .iter()
            .map(|workspace| (workspace.workspace, workspace.clone()))
            .collect(),
        coverage: reachability.coverage,
        capabilities: IssueKind::ALL
            .into_iter()
            .map(|kind| {
                (
                    kind,
                    if options.compatibility_profile == "knip-6.43.0-native-core" {
                        CapabilityStatus::Supported
                    } else {
                        CapabilityStatus::Untested
                    },
                )
            })
            .collect(),
        issues,
        hints: Vec::new(),
        evidence: reachability.evidence,
        membership: reachability.membership,
        gaps: reachability.gaps,
    };
    result.report_metadata = Some(report_metadata::collect(snapshot, options, &result));
    policy::collect_policy_causes(snapshot, &mut result, options);
    policy::apply_policy(result, options)
}

/// Report selection retains the complete analysis context and uncertainty evidence.
/// Tag hints keep semantic scope. Workspace selection shows configuration hints only
/// for selected non-root workspaces; path and category predicates apply to findings.
pub fn select_report(
    result: &ProjectAnalysisResult,
    selection: &ReportSelection,
) -> ProjectAnalysisResult {
    let mut selected = result.clone();
    selected.issues.retain(|issue| {
        ((!selection.restrict_workspaces && selection.workspaces.is_empty())
            || selection.workspaces.contains(&issue.workspace))
            && ((!selection.restrict_issues && selection.issues.is_empty())
                || selection.issues.contains(&issue.kind))
            && selection.changed_paths.as_ref().is_none_or(|paths| {
                result
                    .sources
                    .get(&issue.location.file)
                    .is_some_and(|source| paths.contains(&source.logical_path))
            })
            && (selection.paths.is_empty()
                || result
                    .sources
                    .get(&issue.location.file)
                    .is_some_and(|source| {
                        selection
                            .paths
                            .iter()
                            .any(|path| source.logical_path.starts_with(path))
                    }))
    });
    selected.hints.retain(|hint| {
        hint.kind == crate::result::HintKind::Tag
            || (!selection.restrict_workspaces && selection.workspaces.is_empty())
            || (selection.workspaces.contains(&hint.workspace)
                && result
                    .workspaces
                    .get(&hint.workspace)
                    .is_some_and(|workspace| workspace.path != "."))
    });
    selected
}
