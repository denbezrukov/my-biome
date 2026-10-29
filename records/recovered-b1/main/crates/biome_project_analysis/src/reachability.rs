//! File liveness and uncertainty are computed before report suppression or selection.

use crate::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    facts::{ParseStatus, ResolutionEvent, ResolutionOutcome, UnknownKind},
    inventory::{FileRole, Root, RootRole},
    model::{AnalysisSnapshot, FileId, Location, SourceContent, WorkspaceId},
    options::{AnalysisMode, AnalysisOptions},
    result::{Evidence, EvidenceGraph, EvidenceId},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct GapId(pub String);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GapImpact {
    FileTargets,
    Symbols,
    FileReporting,
    SourceReporting,
}

/// Consumers identify producers, including imports whose metadata gap is located in a manifest.
/// An empty consumer set denotes acquisition uncertainty without an attributable source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReachabilityGap {
    pub id: GapId,
    pub gap: CoverageGap,
    pub consumers: BTreeSet<FileId>,
    pub impact: GapImpact,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileMembership {
    pub file: FileId,
    pub definite: bool,
    pub possible: bool,
    pub diagnostic_eligible: bool,
    pub affecting_gaps: BTreeSet<GapId>,
}

/// Indexed candidates are not traversal seeds. Only visited files belong to `files`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisMembership {
    pub entry_roots: Vec<Root>,
    pub program_roots: Vec<Root>,
    pub reporting_candidates: BTreeSet<FileId>,
    pub files: Vec<FileMembership>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reachability {
    pub definite_files: BTreeSet<FileId>,
    pub possible_files: BTreeSet<FileId>,
    pub affecting_gaps: BTreeSet<GapId>,
    pub gaps: Vec<ReachabilityGap>,
    pub membership: AnalysisMembership,
    pub evidence: EvidenceGraph,
    pub coverage: Coverage,
}

/// Computes the fixed point from real roots, activating uncertainty only from visited producers.
/// Possible files retain affecting gap identities even when they are also definitely reachable.
pub fn compute_reachability(
    snapshot: &AnalysisSnapshot,
    options: &AnalysisOptions,
) -> Reachability {
    let mut membership = AnalysisMembership::default();
    membership.reporting_candidates.extend(
        snapshot
            .inventory
            .files
            .iter()
            .filter(|record| record.role == FileRole::ReportingCandidate)
            .map(|record| record.file),
    );
    let mut evidence = EvidenceGraph::default();
    let mut definite = BTreeSet::new();
    let mut queue = VecDeque::new();
    for root in &snapshot.inventory.roots {
        if options.mode != AnalysisMode::Default && root.role == RootRole::Default {
            continue;
        }
        if root.producer == "tsconfig program" {
            membership.program_roots.push(root.clone());
        } else {
            membership.entry_roots.push(root.clone());
        }
        if definite.insert(root.file) {
            queue.push_back(root.file);
        }
        let explanation = serde_json::json!({"target": path(snapshot, root.file), "role": root.role, "exports": root.exports}).to_string();
        add_evidence(
            snapshot,
            &mut evidence,
            &root.producer,
            root.location,
            root.workspace,
            explanation.clone(),
            &explanation,
        );
    }
    let mut edges: BTreeMap<FileId, Vec<(FileId, EvidenceId)>> = BTreeMap::new();
    let mut gaps = Vec::new();
    reporting_gaps(snapshot, &mut gaps, &options.ignore, None, false);
    for policy in &snapshot.inventory.reporting_policies {
        reporting_gaps(snapshot, &mut gaps, &policy.ignore, policy.workspace, false);
        reporting_gaps(
            snapshot,
            &mut gaps,
            &policy.ignore_files,
            policy.workspace,
            true,
        );
    }
    for gap in snapshot
        .inventory
        .gaps
        .iter()
        .chain(snapshot.coverage.gaps())
    {
        push_gap(snapshot, &mut gaps, gap.clone());
    }
    for record in &snapshot.inventory.files {
        let file = record.file;
        let location = Some(Location {
            file,
            range: Default::default(),
        });
        let Some(module) = snapshot
            .modules
            .get(&file)
            .and_then(|module| module.collected.as_ref())
        else {
            push_gap(
                snapshot,
                &mut gaps,
                CoverageGap {
                    producer: "module facts".into(),
                    domain: TargetDomain::Unbounded,
                    reason: GapReason::UnavailableInput(
                        "Collected module facts are unavailable".into(),
                    ),
                    location,
                },
            );
            continue;
        };
        if module.parse_status != ParseStatus::Valid
            || snapshot
                .sources
                .get(&file)
                .is_none_or(|source| matches!(source.content, SourceContent::Unavailable { .. }))
        {
            push_gap(
                snapshot,
                &mut gaps,
                CoverageGap {
                    producer: "module validity".into(),
                    domain: TargetDomain::Unbounded,
                    reason: GapReason::UnavailableInput(
                        "Module parsing or source availability is unverified".into(),
                    ),
                    location,
                },
            );
        }
        for unknown in &module.unknown {
            if !gaps.iter().any(|record| {
                record.gap.producer == "module collection"
                    && record.gap.location == Some(unknown.location)
                    && record.gap.reason == GapReason::DynamicReference(unknown.reason.clone())
            }) {
                push_gap(
                    snapshot,
                    &mut gaps,
                    CoverageGap {
                        producer: "module collection".into(),
                        domain: TargetDomain::Unbounded,
                        reason: GapReason::DynamicReference(unknown.reason.clone()),
                        location: Some(unknown.location),
                    },
                );
            }
        }
        for import in &module.imports {
            let resolutions: Vec<_> = module
                .resolutions
                .iter()
                .filter(|resolution| resolution.occurrence == import.ordinal)
                .collect();
            if resolutions.is_empty() {
                push_gap(
                    snapshot,
                    &mut gaps,
                    CoverageGap {
                        producer: "module resolution".into(),
                        domain: TargetDomain::Unbounded,
                        reason: GapReason::UnavailableInput(
                            "Import has no resolution evidence".into(),
                        ),
                        location: Some(import.location),
                    },
                );
            }
            for resolution in resolutions {
                let explanation = serde_json::to_string(&resolution.context).unwrap();
                let context_identity = hash(&(
                    resolution.context.profile,
                    resolution.context.purpose,
                    resolution.context.syntax,
                    &resolution.context.additional_conditions,
                    &resolution.context.source_mappings,
                ));
                let suffix = format!("{}:{}:{context_identity}", import.ordinal, import.specifier);
                let target = match &resolution.outcome {
                    ResolutionOutcome::Resolved { file, kind } => {
                        format!("{} ({kind})", path(snapshot, *file))
                    }
                    ResolutionOutcome::Unresolved { error } => format!("unresolved: {error}"),
                    ResolutionOutcome::Unknown { reason } => format!("unknown: {reason}"),
                    ResolutionOutcome::Builtin { name } => format!("builtin: {name}"),
                };
                let id = add_evidence(
                    snapshot,
                    &mut evidence,
                    "module edge",
                    Some(import.location),
                    record.workspace,
                    format!(
                        "{} -> {target}; possible={}; context={explanation}",
                        import.specifier, import.possible
                    ),
                    &suffix,
                );
                match &resolution.outcome {
                    ResolutionOutcome::Resolved { file: target, .. } => {
                        if !import.possible {
                            edges.entry(file).or_default().push((*target, id));
                        }
                    }
                    ResolutionOutcome::Unknown { reason } => push_gap(
                        snapshot,
                        &mut gaps,
                        CoverageGap {
                            producer: "module resolution".into(),
                            domain: TargetDomain::Unbounded,
                            reason: GapReason::UnavailableInput(reason.clone()),
                            location: Some(import.location),
                        },
                    ),
                    ResolutionOutcome::Unresolved { .. } | ResolutionOutcome::Builtin { .. } => {}
                }
            }
        }
    }
    while let Some(file) = queue.pop_front() {
        for (target, _) in edges.get(&file).into_iter().flatten() {
            if definite.insert(*target) {
                queue.push_back(*target);
            }
        }
    }
    let mut affected: BTreeMap<FileId, BTreeSet<GapId>> = BTreeMap::new();
    loop {
        let mut changed = false;
        for record in &mut gaps {
            if !record.active
                && (record.consumers.is_empty()
                    || record
                        .consumers
                        .iter()
                        .any(|file| definite.contains(file) || affected.contains_key(file)))
            {
                record.active = true;
                changed = true;
                if record.impact == GapImpact::FileTargets {
                    for file in domain_files(snapshot, &record.gap.domain) {
                        affected.entry(file).or_default().insert(record.id.clone());
                    }
                }
            }
        }
        let mut pending: VecDeque<_> = affected
            .iter()
            .map(|(file, ids)| (*file, ids.clone()))
            .collect();
        while let Some((file, ids)) = pending.pop_front() {
            for (target, _) in edges.get(&file).into_iter().flatten() {
                let target_ids = affected.entry(*target).or_default();
                let before = target_ids.len();
                target_ids.extend(ids.iter().cloned());
                if before != target_ids.len() {
                    changed = true;
                    pending.push_back((*target, target_ids.clone()));
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut active_coverage = Vec::new();
    for record in &gaps {
        if record.active || !matches!(record.gap.reason, GapReason::DynamicReference(_)) {
            active_coverage.push(record.gap.clone());
        }
        let workspace = record
            .gap
            .location
            .map(|location| owner(snapshot, location.file))
            .or_else(|| {
                snapshot
                    .inventory
                    .workspaces
                    .iter()
                    .min_by_key(|workspace| &workspace.path)
                    .map(|workspace| workspace.workspace)
            })
            .unwrap_or(WorkspaceId(0));
        evidence.nodes.insert(
            EvidenceId(record.id.0.clone()),
            Evidence {
                producer: record.gap.producer.clone(),
                location: record.gap.location,
                workspace,
                explanation: format!(
                    "{:?}; active={}; {:?}",
                    record.impact, record.active, record.gap.reason
                ),
                related: Vec::new(),
            },
        );
    }
    let visited: BTreeSet<_> = definite.iter().chain(affected.keys()).copied().collect();
    membership.files = visited
        .into_iter()
        .map(|file| FileMembership {
            file,
            definite: definite.contains(&file),
            possible: affected.contains_key(&file),
            diagnostic_eligible: membership.reporting_candidates.contains(&file)
                && !suppressed(snapshot, options, file, false),
            affecting_gaps: affected.get(&file).cloned().unwrap_or_default(),
        })
        .collect();
    let coverage = if matches!(snapshot.coverage, Coverage::Failed(_)) {
        Coverage::Failed(active_coverage)
    } else if active_coverage.is_empty() {
        Coverage::Complete
    } else {
        Coverage::Incomplete(active_coverage)
    };
    Reachability {
        definite_files: definite,
        possible_files: affected.keys().copied().collect(),
        affecting_gaps: gaps
            .iter()
            .filter(|gap| gap.active)
            .map(|gap| gap.id.clone())
            .collect(),
        gaps,
        membership,
        evidence,
        coverage,
    }
}

fn push_gap(snapshot: &AnalysisSnapshot, gaps: &mut Vec<ReachabilityGap>, gap: CoverageGap) {
    let mut consumers = BTreeSet::new();
    let mut impact = match gap.producer.as_str() {
        "file reporting policy" => GapImpact::FileReporting,
        "source reporting policy" => GapImpact::SourceReporting,
        _ => GapImpact::FileTargets,
    };
    if gap.producer == "resolution metadata" {
        if let Some(location) = gap.location {
            for (file, module) in &snapshot.modules {
                let uses_manifest = module.collected.as_ref().is_some_and(|module| {
                    module.resolutions.iter().any(|resolution| {
                        resolution.trace.iter().any(|event| matches!(event, ResolutionEvent::Manifest { file, .. } if *file == location.file))
                    })
                });
                if uses_manifest {
                    consumers.insert(*file);
                }
            }
        }
    } else if let Some(location) = gap.location {
        if snapshot
            .inventory
            .files
            .iter()
            .any(|record| record.file == location.file)
        {
            consumers.insert(location.file);
        }
        if gap.producer == "module collection"
            && snapshot
                .modules
                .get(&location.file)
                .and_then(|module| module.collected.as_ref())
                .is_some_and(|module| {
                    module.unknown.iter().any(|unknown| {
                        unknown.location == location
                            && gap.reason == GapReason::DynamicReference(unknown.reason.clone())
                            && matches!(
                                unknown.kind,
                                UnknownKind::Declaration
                                    | UnknownKind::Exports
                                    | UnknownKind::Registry
                            )
                    })
                })
        {
            impact = GapImpact::Symbols;
        }
    }
    let domain = match &gap.domain {
        TargetDomain::Unbounded => serde_json::json!("unbounded"),
        TargetDomain::Bounded {
            files,
            workspaces,
            packages,
        } => {
            serde_json::json!({ "files": files.iter().map(|file| path(snapshot,*file)).collect::<BTreeSet<_>>(), "workspaces": workspaces.iter().map(|id| snapshot.inventory.workspaces.iter().find(|workspace|workspace.workspace == *id).map(|workspace| workspace.path.clone())).collect::<BTreeSet<_>>(), "packages": packages.iter().collect::<BTreeSet<_>>() })
        }
    };
    let id = GapId(format!(
        "gap:{}",
        hash(&serde_json::json!([
            gap.producer,
            gap.reason,
            gap.location
                .map(|location| (path(snapshot, location.file), location.range)),
            domain
        ]))
    ));
    if !gaps.iter().any(|record| record.id == id) {
        gaps.push(ReachabilityGap {
            id,
            gap,
            consumers,
            impact,
            active: false,
        });
        gaps.sort_by(|left, right| left.id.cmp(&right.id));
    }
}
fn domain_files(snapshot: &AnalysisSnapshot, domain: &TargetDomain) -> BTreeSet<FileId> {
    let TargetDomain::Bounded {
        files,
        workspaces,
        packages,
    } = domain
    else {
        return snapshot
            .inventory
            .files
            .iter()
            .map(|record| record.file)
            .collect();
    };
    let mut targets: BTreeSet<_> = files.iter().copied().collect();
    let package_workspaces: BTreeSet<_> = snapshot
        .inventory
        .workspaces
        .iter()
        .filter(|workspace| {
            workspace
                .name
                .as_ref()
                .is_some_and(|name| packages.contains(name))
        })
        .map(|workspace| workspace.workspace)
        .collect();
    targets.extend(
        snapshot
            .inventory
            .files
            .iter()
            .filter(|file| {
                workspaces.contains(&file.workspace) || package_workspaces.contains(&file.workspace)
            })
            .map(|file| file.file),
    );
    for module in snapshot
        .modules
        .values()
        .filter_map(|module| module.collected.as_ref())
    {
        for resolution in &module.resolutions {
            if resolution.trace.iter().any(|event|matches!(event, ResolutionEvent::Package {name:Some(name),..} if packages.contains(name)))
                && let ResolutionOutcome::Resolved {file,..} = resolution.outcome {
                targets.insert(file);
            }
        }
    }
    targets
}
pub(crate) fn path(snapshot: &AnalysisSnapshot, file: FileId) -> String {
    snapshot.sources.get(&file).map_or_else(
        || "<unavailable>".into(),
        |source| source.logical_path.to_string(),
    )
}
pub(crate) fn owner(snapshot: &AnalysisSnapshot, file: FileId) -> WorkspaceId {
    snapshot
        .inventory
        .files
        .iter()
        .find(|record| record.file == file)
        .map(|record| record.workspace)
        .or_else(|| {
            snapshot
                .inventory
                .workspaces
                .iter()
                .find(|workspace| workspace.manifest == file)
                .map(|workspace| workspace.workspace)
        })
        .or_else(|| {
            snapshot
                .inventory
                .workspaces
                .iter()
                .min_by_key(|workspace| &workspace.path)
                .map(|workspace| workspace.workspace)
        })
        .unwrap_or(WorkspaceId(0))
}
pub(crate) fn hash(value: &impl Serialize) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn add_evidence(
    snapshot: &AnalysisSnapshot,
    graph: &mut EvidenceGraph,
    producer: &str,
    location: Option<Location>,
    workspace: WorkspaceId,
    explanation: String,
    suffix: &str,
) -> EvidenceId {
    let id = EvidenceId(format!(
        "{producer}:{}",
        hash(&(
            location.map(|location| (path(snapshot, location.file), location.range)),
            suffix
        ))
    ));
    graph.nodes.insert(
        id.clone(),
        Evidence {
            producer: producer.into(),
            location,
            workspace,
            explanation,
            related: Vec::new(),
        },
    );
    id
}

pub(crate) fn suppressed(
    snapshot: &AnalysisSnapshot,
    options: &AnalysisOptions,
    file: FileId,
    files_only: bool,
) -> bool {
    let Some(source) = snapshot.sources.get(&file) else {
        return false;
    };
    if matches_patterns(&options.ignore, &source.logical_path) {
        return true;
    }
    snapshot.inventory.reporting_policies.iter().any(|policy| {
        if policy
            .workspace
            .is_some_and(|workspace| workspace != owner(snapshot, file))
        {
            return false;
        }
        let relative = if policy.base.as_str() == "." || policy.base.as_str().is_empty() {
            source.logical_path.as_path()
        } else {
            let Ok(relative) = source.logical_path.strip_prefix(&policy.base) else {
                return false;
            };
            relative
        };
        matches_patterns(&policy.ignore, relative)
            || (files_only && matches_patterns(&policy.ignore_files, relative))
    })
}
fn matches_patterns(patterns: &[String], path: &camino::Utf8Path) -> bool {
    let Ok(patterns) = patterns
        .iter()
        .map(|pattern| pattern.parse::<biome_glob::NormalizedGlob>())
        .collect::<Result<Vec<_>, _>>()
    else {
        // An unsupported suppression pattern may match any path in this policy's scope.
        return true;
    };
    patterns
        .iter()
        .any(|pattern| !pattern.is_negated() && pattern.is_raw_match(path))
        && !patterns
            .iter()
            .any(|pattern| pattern.is_negated() && pattern.is_raw_match(path))
}

fn reporting_gaps(
    snapshot: &AnalysisSnapshot,
    gaps: &mut Vec<ReachabilityGap>,
    patterns: &[String],
    workspace: Option<WorkspaceId>,
    files_only: bool,
) {
    for pattern in patterns {
        if let Err(error) = pattern.parse::<biome_glob::NormalizedGlob>() {
            push_gap(
                snapshot,
                gaps,
                CoverageGap {
                    producer: if files_only {
                        "file reporting policy"
                    } else {
                        "source reporting policy"
                    }
                    .into(),
                    domain: workspace.map_or(TargetDomain::Unbounded, |workspace| {
                        TargetDomain::Bounded {
                            files: Vec::new(),
                            workspaces: vec![workspace],
                            packages: Vec::new(),
                        }
                    }),
                    reason: GapReason::Capability(format!(
                        "Unsupported reporting pattern {pattern}: {error}"
                    )),
                    location: None,
                },
            );
        }
    }
}
