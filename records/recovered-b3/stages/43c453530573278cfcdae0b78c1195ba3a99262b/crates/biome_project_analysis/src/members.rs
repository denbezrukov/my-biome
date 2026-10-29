//! Member checks use owned declaration relations and the shared source membership.
use crate::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    exports::{self, ExportAnalysis},
    facts::{
        CollectedModule, DeclarationKind, ExportForm, ExportResolution, ExportTarget,
        NamespaceAccess, ParseStatus, UseKind, UseRole,
    },
    model::{AnalysisSnapshot, DeclarationId, Location},
    options::AnalysisOptions,
    reachability::{self, Reachability},
    result::{Evidence, EvidenceGraph, EvidenceId, IssueKind, ProjectIssue},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemberAnalysis {
    pub issues: Vec<ProjectIssue>,
    pub evidence: EvidenceGraph,
    pub gaps: Vec<CoverageGap>,
    pub used: Vec<DeclarationId>,
    pub uncertainty: Vec<MemberUncertainty>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// A computed access affects members beneath a known prefix without selecting any leaf.
pub struct MemberUncertainty {
    pub parent: DeclarationId,
    pub location: Location,
    pub path: Vec<String>,
}

fn canonical(module: &CollectedModule, id: DeclarationId) -> DeclarationId {
    module
        .merges
        .iter()
        .find(|relation| relation.declaration == id)
        .map_or(id, |relation| relation.target)
}
fn allowed_tags(tags: &[String], options: &AnalysisOptions) -> bool {
    !tags
        .iter()
        .any(|tag| tag == "public" || options.excluded_tags.contains(tag))
        && (options.included_tags.is_empty()
            || tags.iter().any(|tag| options.included_tags.contains(tag)))
}

/// Uses shared membership and export evidence; reporting filters do not remove member uncertainty.
pub fn analyze_members(
    snapshot: &AnalysisSnapshot,
    exports: &ExportAnalysis,
    reachability: &Reachability,
    options: &AnalysisOptions,
) -> MemberAnalysis {
    let mut result = MemberAnalysis::default();
    for pattern in &options.ignore_members {
        if let Err(error) = pattern.parse::<biome_glob::NormalizedGlob>() {
            result.gaps.push(CoverageGap {
                producer: "member reporting policy".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::Capability(format!(
                    "Unsupported member pattern {pattern}: {error}"
                )),
                location: None,
            });
        }
    }
    let mut accesses: Vec<(DeclarationId, Vec<NamespaceAccess>, Location, bool)> = Vec::new();
    for usage in &exports.member_uses {
        let mut path = usage.path.clone();
        if path.is_empty() {
            path.push(usage.access.clone());
        }
        if usage.namespace_import {
            let mut target = usage.target;
            while !path.is_empty() {
                let NamespaceAccess::Known(name) = &path[0] else {
                    break;
                };
                let Some(slot) = exports::surface(
                    snapshot,
                    target,
                    usage.declaration.namespace,
                    &usage.context,
                )
                .and_then(|surface| surface.slots.iter().find(|slot| slot.name == *name)) else {
                    break;
                };
                let ExportResolution::Found(origin) = slot.resolution else {
                    break;
                };
                path.remove(0);
                if let Some(next) = slot.namespace_target {
                    target = next;
                } else {
                    accesses.push((
                        origin,
                        path,
                        usage.location,
                        usage.kind == UseKind::RuntimeTypeof,
                    ));
                    break;
                }
            }
        } else {
            accesses.push((
                usage.declaration,
                path,
                usage.location,
                usage.kind == UseKind::RuntimeTypeof,
            ));
        }
    }
    for file in &reachability.definite_files {
        let Some(module) = exports::get_module(snapshot, *file) else {
            continue;
        };
        for usage in &module.uses {
            if usage.write || usage.role == UseRole::Export {
                continue;
            }
            let mut id = usage.declaration;
            let mut seen = BTreeSet::new();
            while seen.insert(id.location.range) {
                let Some(alias) = module
                    .aliases
                    .iter()
                    .find(|alias| alias.declaration.location == id.location)
                else {
                    break;
                };
                id = alias.target;
            }
            if usage.namespace_access == Some(NamespaceAccess::Enumerate) {
                accesses.push((id, vec![NamespaceAccess::Enumerate], usage.location, false));
            } else if !usage.member_path.is_empty() {
                accesses.push((id, usage.member_path.clone(), usage.location, false));
            }
            if module
                .members
                .iter()
                .any(|member| member.declaration.location == id.location)
                && !result.used.contains(&id)
            {
                result.used.push(id);
            }
        }
    }
    for membership in &reachability.membership.files {
        if !membership.definite
            || membership.possible
            || matches!(snapshot.coverage, Coverage::Failed(_))
        {
            continue;
        }
        let eligible = membership.diagnostic_eligible
            && !exports::reporting_uncertain(snapshot, reachability, membership.file);
        let Some(module) = exports::get_module(snapshot, membership.file) else {
            continue;
        };
        if module.parse_status != ParseStatus::Valid {
            continue;
        }
        let mut parents = BTreeSet::new();
        for member in &module.members {
            parents.insert(canonical(module, member.parent).location.range);
        }
        for parent_range in parents {
            let Some(parent) = module
                .declarations
                .iter()
                .filter(|declaration| declaration.id.location.range == parent_range)
                .min_by_key(|declaration| {
                    declaration.id.namespace == crate::model::SymbolNamespace::Type
                })
            else {
                continue;
            };
            let parent_id = parent.id;
            let public = module.export_surfaces.iter().flat_map(|surface| &surface.slots).any(|slot| {
                matches!(slot.resolution, ExportResolution::Found(id) if id.location == parent_id.location)
                    && slot.slots.iter().any(|key| exports.protected_slots.contains(key))
            });
            if public
                || !exports
                    .consumed
                    .iter()
                    .any(|id| canonical(module, *id).location == parent_id.location)
                || exports.gaps.iter().any(|gap| match &gap.domain {
                    TargetDomain::Unbounded => true,
                    TargetDomain::Bounded {
                        files, workspaces, ..
                    } => {
                        files.contains(&membership.file)
                            || workspaces.contains(&exports::workspace(snapshot, membership.file))
                    }
                })
            {
                continue;
            }
            let parent_accesses: Vec<_> = accesses
                .iter()
                .filter(|(id, _, _, _)| canonical(module, *id).location == parent_id.location)
                .collect();
            let has_known = parent_accesses
                .iter()
                .any(|(_, path, _, _)| matches!(path.first(), Some(NamespaceAccess::Known(_))));
            let whole = parent_accesses.iter().any(|(_, path, _, runtime_typeof)| {
                if *runtime_typeof {
                    return false;
                }
                matches!(path.first(), Some(NamespaceAccess::Enumerate))
                    || ((!has_known && parent.kind == DeclarationKind::Enum)
                        && (path.is_empty()
                            || matches!(path.first(), Some(NamespaceAccess::Escape))))
            });
            if whole || parent_accesses.is_empty() {
                continue;
            }
            let kind = if parent.kind == DeclarationKind::Enum {
                IssueKind::EnumMembers
            } else {
                IssueKind::NamespaceMembers
            };
            let mut leaves: BTreeMap<Vec<String>, Vec<_>> = BTreeMap::new();
            for member in module
                .members
                .iter()
                .filter(|member| canonical(module, member.parent).location == parent_id.location)
            {
                leaves.entry(member.path.clone()).or_default().push(member);
            }
            for (path, declarations) in leaves {
                let member = declarations
                    .iter()
                    .min_by_key(|member| member.declaration.location.range.start())
                    .unwrap();
                let tags: Vec<_> = declarations
                    .iter()
                    .flat_map(|member| member.tags.iter().cloned())
                    .collect();
                let ignored = reachability::matches_patterns(
                    &options.ignore_members,
                    camino::Utf8Path::new(&path.join(".")),
                );
                if path.iter().any(String::is_empty)
                    || declarations.iter().any(|member| {
                        result
                            .used
                            .iter()
                            .any(|id| id.location == member.declaration.location)
                    })
                {
                    continue;
                }
                let mut used = false;
                for (_, access, location, _) in &parent_accesses {
                    let mut prefix = Vec::new();
                    let mut uncertain = false;
                    for part in access {
                        match part {
                            NamespaceAccess::Known(name)
                                if !(parent.kind == DeclarationKind::Enum
                                    && canonical_number(name)) =>
                            {
                                prefix.push(name.clone())
                            }
                            NamespaceAccess::Computed | NamespaceAccess::Known(_) => {
                                uncertain = true;
                                break;
                            }
                            _ => break,
                        }
                    }
                    if uncertain && path.starts_with(&prefix) {
                        let uncertainty = MemberUncertainty {
                            parent: parent_id,
                            location: *location,
                            path: prefix,
                        };
                        if !result.uncertainty.contains(&uncertainty) {
                            result.uncertainty.push(uncertainty);
                            result.gaps.push(CoverageGap {
                                producer: "member consumption".into(),
                                domain: TargetDomain::Bounded {
                                    files: vec![membership.file],
                                    workspaces: Vec::new(),
                                    packages: Vec::new(),
                                },
                                reason: GapReason::DynamicReference(
                                    "Computed member does not select a declaration".into(),
                                ),
                                location: Some(*location),
                            });
                            evidence(
                                &mut result,
                                snapshot,
                                *location,
                                "uncertain member",
                                &parent.name,
                            );
                        }
                        used = true;
                    } else if !prefix.is_empty()
                        && (path.starts_with(&prefix) || prefix.starts_with(&path))
                    {
                        used = true;
                        if !result.used.contains(&member.declaration) {
                            result.used.push(member.declaration);
                        }
                    }
                }
                if !used
                    && eligible
                    && !ignored
                    && allowed_tags(&parent.tags, options)
                    && allowed_tags(&tags, options)
                {
                    issue(
                        &mut result,
                        snapshot,
                        options,
                        kind,
                        member.declaration.location,
                        format!("{}.{}", parent.name, path.join(".")),
                    );
                }
            }
        }
        if eligible {
            duplicates(snapshot, module, exports, options, &mut result);
        }
    }
    result
}
fn canonical_number(name: &str) -> bool {
    name.parse::<f64>()
        .is_ok_and(|number| number.is_finite() && number.to_string() == name)
}
fn evidence(
    result: &mut MemberAnalysis,
    snapshot: &AnalysisSnapshot,
    location: Location,
    producer: &str,
    symbol: &str,
) -> EvidenceId {
    let id = EvidenceId(format!(
        "member:{producer}:{}:{}:{symbol}",
        reachability::path(snapshot, location.file),
        u32::from(location.range.start())
    ));
    result.evidence.nodes.insert(
        id.clone(),
        Evidence {
            producer: producer.into(),
            location: Some(location),
            workspace: exports::workspace(snapshot, location.file),
            explanation: symbol.into(),
            related: Vec::new(),
        },
    );
    id
}
fn issue(
    result: &mut MemberAnalysis,
    snapshot: &AnalysisSnapshot,
    options: &AnalysisOptions,
    kind: IssueKind,
    location: Location,
    symbol: String,
) {
    let Some(severity) = options.severity(kind) else {
        return;
    };
    let identity = format!(
        "{kind:?}:{}:{}:{symbol}",
        reachability::path(snapshot, location.file),
        u32::from(location.range.start())
    );
    if result.issues.iter().any(|issue| issue.identity == identity) {
        return;
    }
    let related = evidence(result, snapshot, location, "member analysis", &symbol);
    result.issues.push(ProjectIssue {
        kind,
        severity,
        identity,
        location,
        symbol: Some(symbol),
        workspace: exports::workspace(snapshot, location.file),
        related_evidence: vec![related],
        eligible_fixes: Vec::new(),
    });
}
fn duplicates(
    snapshot: &AnalysisSnapshot,
    module: &CollectedModule,
    exports: &ExportAnalysis,
    options: &AnalysisOptions,
    result: &mut MemberAnalysis,
) {
    let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for export in &module.exports {
        if !matches!(
            export.form,
            ExportForm::Declaration
                | ExportForm::DefaultDeclaration
                | ExportForm::DefaultExpression
        ) || export.tags.iter().any(|tag| tag == "alias")
        {
            continue;
        }
        let ExportTarget::Local(mut id) = export.target else {
            continue;
        };
        if exports
            .protected_slots
            .iter()
            .any(|key| key.file == export.location.file && key.name == export.name)
        {
            continue;
        }
        let Some(declaration) = module
            .declarations
            .iter()
            .find(|d| d.id.location == id.location)
        else {
            continue;
        };
        if !allowed_tags(&declaration.tags, options) {
            continue;
        }
        let mut seen = BTreeSet::new();
        while seen.insert(id.location.range) {
            let Some(alias) = module
                .aliases
                .iter()
                .find(|alias| alias.declaration.location == id.location)
            else {
                break;
            };
            id = alias.target;
        }
        if !module
            .export_surfaces
            .iter()
            .filter(|surface| surface.local_context)
            .flat_map(|surface| &surface.slots)
            .any(|slot| {
                slot.name == export.name && matches!(slot.resolution, ExportResolution::Found(_))
            })
        {
            continue;
        }
        groups
            .entry(canonical(module, id).location.range)
            .or_default()
            .push(export);
    }
    for group in groups.values() {
        let names: BTreeSet<_> = group.iter().map(|export| &export.name).collect();
        if names.len() < 2 {
            continue;
        }
        let first = group
            .iter()
            .min_by_key(|export| export.location.range.start())
            .unwrap();
        let location = if let ExportTarget::Local(id) = first.target {
            id.location
        } else {
            first.location
        };
        issue(
            result,
            snapshot,
            options,
            IssueKind::Duplicates,
            location,
            names.into_iter().cloned().collect::<Vec<_>>().join("/"),
        );
    }
}
