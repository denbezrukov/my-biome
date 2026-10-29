//! Catalog identity and liveness are independent of local package installation.
use crate::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    facts::ParseStatus,
    inventory::DependencyKind,
    model::{AnalysisSnapshot, Location, SourceContent, WorkspaceId},
    options::AnalysisOptions,
    reachability::{Reachability, owner, path, suppressed},
    result::{Evidence, EvidenceGraph, EvidenceId, IssueKind, ProjectIssue},
    scripts::{Activation, ScriptProducer},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogAnalysis {
    pub selections: Vec<crate::inventory::CatalogSelection>,
    /// All definitions retain provider order; findings use the first location for each pair.
    pub entries: Vec<CatalogEntry>,
    pub references: Vec<CatalogUse>,
    pub issues: Vec<ProjectIssue>,
    pub evidence: EvidenceGraph,
    pub gaps: Vec<CoverageGap>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub catalog: String,
    pub package: String,
    pub location: Location,
    pub container: Location,
    pub used: bool,
    pub uncertain: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogProducer {
    Metadata { field: String },
    Script(ScriptProducer),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogUse {
    pub producer: CatalogProducer,
    pub catalog: String,
    pub package: String,
    pub location: Location,
    pub workspace: WorkspaceId,
    pub definite: bool,
}
fn catalog_name(name: &str) -> String {
    if name.is_empty() {
        "default".into()
    } else {
        name.into()
    }
}
pub fn analyze_catalogs(
    snapshot: &AnalysisSnapshot,
    reachability: &Reachability,
    options: &AnalysisOptions,
) -> CatalogAnalysis {
    let mut result = CatalogAnalysis {
        selections: snapshot.inventory.catalog_selections.clone(),
        gaps: snapshot.inventory.catalog_consumer_gaps.clone(),
        ..Default::default()
    };
    for member in reachability
        .membership
        .files
        .iter()
        .filter(|member| member.analyzed)
    {
        let unavailable = snapshot
            .modules
            .get(&member.file)
            .and_then(|module| module.collected.as_ref())
            .is_none_or(|facts| facts.parse_status != ParseStatus::Valid)
            || snapshot
                .sources
                .get(&member.file)
                .is_none_or(|source| matches!(source.content, SourceContent::Unavailable { .. }));
        if unavailable {
            result.gaps.push(CoverageGap {
                producer: "catalog source validity".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::UnavailableInput(
                    "Analyzed source may contain uncaptured catalog commands".into(),
                ),
                location: Some(Location {
                    file: member.file,
                    range: Default::default(),
                }),
            });
        }
    }
    let consumer_uncertain = !result.gaps.is_empty();
    let mut invalid = BTreeSet::new();
    let mut all_invalid = false;
    for selection in &snapshot.inventory.catalog_selections {
        if !selection.valid {
            if selection.field == "catalogs" {
                all_invalid = true;
            } else {
                invalid.insert("default".to_owned());
            }
            result.gaps.push(CoverageGap {
                producer: "catalog provider".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::UnavailableInput(
                    "Selected catalog metadata is unavailable, malformed or unsupported".into(),
                ),
                location: Some(selection.location),
            });
        }
    }
    for workspace in &snapshot.inventory.workspaces {
        for provider in &workspace.catalogs {
            let catalog = catalog_name(&provider.name);
            let malformed = !provider.value.value.is_object()
                || provider
                    .value
                    .members
                    .iter()
                    .any(|(_, v)| !v.value.is_string())
                || workspace
                    .metadata_diagnostics
                    .iter()
                    .any(|d| d.location.file == provider.value.location.file);
            if malformed {
                invalid.insert(catalog.clone());
                result.gaps.push(CoverageGap {
                    producer: "catalog provider".into(),
                    domain: TargetDomain::Unbounded,
                    reason: GapReason::UnavailableInput(
                        "Catalog definitions require a valid mapping of version strings".into(),
                    ),
                    location: Some(provider.value.location),
                });
            }
            for (package, value) in &provider.value.members {
                result.entries.push(CatalogEntry {
                    catalog: catalog.clone(),
                    package: package.clone(),
                    location: value.location,
                    container: provider.container,
                    used: false,
                    uncertain: malformed,
                });
            }
        }
    }
    for declaration in &snapshot.inventory.declarations {
        let Some(name) = declaration.specifier.strip_prefix("catalog:") else {
            continue;
        };
        let root = snapshot
            .inventory
            .workspaces
            .iter()
            .find(|w| w.workspace == declaration.workspace)
            .is_some_and(|w| w.path.as_str() == ".");
        if declaration.kind == DependencyKind::Catalog
            && !matches!(
                declaration.field.as_str(),
                "resolutions" | "pnpm.overrides" | "overrides"
            )
        {
            continue;
        }
        if matches!(declaration.field.as_str(), "pnpm.overrides" | "overrides") && !root {
            continue;
        }
        let package = if matches!(declaration.field.as_str(), "pnpm.overrides" | "overrides") {
            override_package(&declaration.package)
        } else {
            declaration.package.clone()
        };
        result.references.push(CatalogUse {
            producer: CatalogProducer::Metadata {
                field: declaration.field.clone(),
            },
            catalog: catalog_name(name),
            package,
            location: declaration.location,
            workspace: declaration.workspace,
            definite: true,
        });
    }
    for reference in snapshot
        .inventory
        .script_usage
        .catalogs
        .iter()
        .filter(|r| r.mode == options.mode)
    {
        let activation = reachability
            .membership
            .script_producers
            .iter()
            .find(|p| p.producer == reference.producer)
            .map_or(Activation::Dead, |p| p.activation);
        if activation == Activation::Dead {
            continue;
        }
        let workspace = match reference.producer {
            ScriptProducer::Manifest { workspace, .. } => workspace,
            ScriptProducer::Source { file } => owner(snapshot, file),
        };
        result.references.push(CatalogUse {
            producer: CatalogProducer::Script(reference.producer.clone()),
            catalog: catalog_name(reference.catalog_name.as_deref().unwrap_or("")),
            package: reference.package_name.clone(),
            location: reference.location,
            workspace,
            definite: activation == Activation::Definite,
        });
    }
    for reference in result.references.clone() {
        let mut found = false;
        for entry in &mut result.entries {
            if entry.catalog == reference.catalog && entry.package == reference.package {
                found = true;
                entry.used |= reference.definite;
                entry.uncertain |= !reference.definite;
            }
        }
        evidence(
            snapshot,
            &mut result,
            reference.location,
            reference.workspace,
            "catalog reference",
            &format!(
                "{}.{}; definite={}",
                reference.catalog, reference.package, reference.definite
            ),
        );
        if !found && reference.definite && !all_invalid && !invalid.contains(&reference.catalog) {
            issue(
                snapshot,
                options,
                &mut result,
                IssueKind::CatalogReferences,
                &reference.catalog,
                &reference.package,
                reference.location,
                reference.workspace,
            );
        }
    }
    let mut reported = BTreeSet::new();
    for index in 0..result.entries.len() {
        let entry = &mut result.entries[index];
        entry.uncertain |= consumer_uncertain
            || all_invalid
            || invalid.contains(&entry.catalog)
            || matches!(snapshot.coverage, Coverage::Failed(_));
        for gap in snapshot.inventory.script_usage.gaps.iter().filter(|g| {
            g.mode == options.mode
                && g.affects_catalogs
                && reachability
                    .membership
                    .script_producers
                    .iter()
                    .any(|p| p.producer == g.producer && p.activation != Activation::Dead)
        }) {
            entry.uncertain |= match &gap.gap.domain {
                TargetDomain::Unbounded => true,
                TargetDomain::Bounded {
                    files,
                    workspaces,
                    packages,
                } => {
                    packages.contains(&entry.package)
                        || result.references.iter().any(|r| {
                            r.catalog == entry.catalog
                                && r.package == entry.package
                                && (files.contains(&r.location.file)
                                    || workspaces.contains(&r.workspace))
                        })
                }
            };
        }
        if !entry.used
            && !entry.uncertain
            && reported.insert((entry.catalog.clone(), entry.package.clone()))
        {
            let entry = entry.clone();
            issue(
                snapshot,
                options,
                &mut result,
                IssueKind::Catalog,
                &entry.catalog,
                &entry.package,
                entry.location,
                owner(snapshot, entry.location.file),
            );
        }
    }
    result
}
fn override_package(selector: &str) -> String {
    let selector = selector
        .as_bytes()
        .windows(2)
        .position(|pair| pair[1] == b'>' && !matches!(pair[0], b' ' | b'|' | b'@'))
        .map_or(selector, |index| &selector[index + 2..]);
    let end = selector
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '@')
        .map_or(selector.len(), |(index, _)| index);
    crate::dependencies::package_name(&selector[..end])
        .unwrap_or(&selector[..end])
        .to_owned()
}
fn evidence(
    snapshot: &AnalysisSnapshot,
    result: &mut CatalogAnalysis,
    location: Location,
    workspace: WorkspaceId,
    producer: &str,
    symbol: &str,
) -> EvidenceId {
    let id = EvidenceId(format!(
        "{producer}:{}:{}:{symbol}",
        path(snapshot, location.file),
        location.range.start()
    ));
    result.evidence.nodes.insert(
        id.clone(),
        Evidence {
            producer: producer.into(),
            location: Some(location),
            workspace,
            explanation: symbol.into(),
            related: Vec::new(),
        },
    );
    id
}
#[expect(clippy::too_many_arguments)]
fn issue(
    snapshot: &AnalysisSnapshot,
    options: &AnalysisOptions,
    result: &mut CatalogAnalysis,
    kind: IssueKind,
    catalog: &str,
    package: &str,
    location: Location,
    workspace: WorkspaceId,
) {
    let Some(severity) = options.severity(kind) else {
        return;
    };
    if suppressed(snapshot, options, location.file, false) {
        return;
    }
    let symbol = format!("{catalog}.{package}");
    let id = evidence(
        snapshot,
        result,
        location,
        workspace,
        "catalog finding",
        &symbol,
    );
    result.issues.push(ProjectIssue {
        kind,
        severity,
        identity: format!(
            "{kind:?}:{}:{}:{symbol}",
            path(snapshot, location.file),
            location.range.start()
        ),
        symbol: Some(symbol),
        location,
        workspace,
        related_evidence: vec![id],
        eligible_fixes: Vec::new(),
    });
}
