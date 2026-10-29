//! Presentation facts captured from effective policy and the authoritative snapshot.
use crate::{
    facts::{
        CollectedModule, DeclarationKind, Export, ExportForm, ExportTarget, ImportOccurrence,
        ImportSyntax, ResolutionOutcome,
    },
    model::{AnalysisSnapshot, DeclarationId, FileId, Location, SymbolNamespace},
    options::AnalysisOptions,
    result::{IssueKind, ProjectAnalysisResult},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectReportMetadata {
    pub enabled_issues: BTreeSet<IssueKind>,
    pub issues: BTreeMap<String, ReportIssueMetadata>,
    pub exports: Vec<ReportExport>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReportIssueMetadata {
    pub symbols: Vec<ReportSymbol>,
    pub parent: Option<String>,
    pub symbol_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReportSymbol {
    pub name: String,
    pub location: Option<Location>,
    pub kind: Option<String>,
    pub specifier: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReportExport {
    pub file: FileId,
    pub name: String,
    pub namespace: SymbolNamespace,
    pub location: Option<Location>,
    pub origin: Option<DeclarationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReportHint {
    Configuration {
        identifier: String,
        description: String,
    },
    Tag {
        identifier: String,
        tag: String,
    },
}

pub(crate) fn collect(
    snapshot: &AnalysisSnapshot,
    options: &AnalysisOptions,
    result: &ProjectAnalysisResult,
) -> ProjectReportMetadata {
    let mut metadata = ProjectReportMetadata {
        enabled_issues: IssueKind::ALL
            .into_iter()
            .filter(|kind| options.includes(*kind))
            .collect(),
        issues: BTreeMap::new(),
        exports: Vec::new(),
    };
    for (file, module) in &snapshot.modules {
        let Some(module) = &module.collected else {
            continue;
        };
        for export in &module.exports {
            metadata.exports.push(ReportExport {
                file: *file,
                name: export.name.clone(),
                namespace: export.namespace,
                location: Some(export.location),
                origin: match export.target {
                    ExportTarget::Local(id) => Some(id),
                    _ => module
                        .export_surfaces
                        .iter()
                        .filter(|surface| {
                            surface.local_context && surface.namespace == export.namespace
                        })
                        .flat_map(|surface| &surface.slots)
                        .find_map(|slot| {
                            if slot.name == export.name
                                && let crate::facts::ExportResolution::Found(id) = slot.resolution
                            {
                                Some(id)
                            } else {
                                None
                            }
                        }),
                },
            });
        }
        for surface in module
            .export_surfaces
            .iter()
            .filter(|surface| surface.local_context)
        {
            for slot in &surface.slots {
                if metadata.exports.iter().any(|export| {
                    export.file == *file
                        && export.name == slot.name
                        && export.namespace == surface.namespace
                }) {
                    continue;
                }
                metadata.exports.push(ReportExport {
                    file: *file,
                    name: slot.name.clone(),
                    namespace: surface.namespace,
                    location: None,
                    origin: if let crate::facts::ExportResolution::Found(id) = slot.resolution {
                        Some(id)
                    } else {
                        None
                    },
                });
            }
        }
    }
    for issue in &result.issues {
        let mut details = ReportIssueMetadata::default();
        let module = snapshot
            .modules
            .get(&issue.location.file)
            .and_then(|module| module.collected.as_ref());
        if issue.kind == IssueKind::Cycles {
            if let Some(cycle) = result.cycles.representatives.iter().find(|cycle| {
                cycle.outgoing.first() == Some(&issue.location)
                    && format!(
                        "cycles:{}",
                        serde_json::to_string(
                            &cycle
                                .path
                                .iter()
                                .map(|file| crate::reachability::path(snapshot, *file))
                                .collect::<Vec<_>>()
                        )
                        .unwrap()
                    ) == issue.identity
            }) {
                for (index, (file, location)) in cycle.path.iter().zip(&cycle.outgoing).enumerate()
                {
                    let edge = snapshot
                        .modules
                        .get(file)
                        .and_then(|module| module.collected.as_ref())
                        .and_then(|module| {
                            module
                                .imports
                                .iter()
                                .find(|import| {
                                    import.specifier_location == *location
                                        || import.bindings.iter().any(|binding| {
                                            binding.declaration.location == *location
                                        })
                                })
                                .map(|import| (module, import))
                        });
                    let presentation = edge.and_then(|(module, import)| {
                        cycle_presentation(
                            module,
                            import,
                            cycle.path[(index + 1) % cycle.path.len()],
                            *location,
                        )
                    });
                    details.symbols.push(ReportSymbol {
                        name: crate::reachability::path(snapshot, *file),
                        location: presentation.map(|(_, location)| location),
                        kind: presentation.map(|(kind, _)| kind.to_owned()),
                        specifier: edge.map(|(_, import)| import.specifier.clone()),
                    });
                }
            }
        } else if issue.kind == IssueKind::Duplicates {
            if let Some(module) = module {
                for export in &module.exports {
                    let ExportTarget::Local(mut id) = export.target else {
                        continue;
                    };
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
                    if id.location == issue.location {
                        details.symbols.push(ReportSymbol {
                            name: export.name.clone(),
                            location: Some(export.location),
                            kind: None,
                            specifier: None,
                        });
                    }
                }
                details.symbols.sort_by(|left, right| {
                    left.location
                        .map(|location| (location.range.start(), location.range.end()))
                        .cmp(
                            &right
                                .location
                                .map(|location| (location.range.start(), location.range.end())),
                        )
                        .then(left.name.cmp(&right.name))
                });
                let mut names = BTreeSet::new();
                details
                    .symbols
                    .retain(|symbol| names.insert(symbol.name.clone()));
            }
        } else {
            let mut name = issue
                .symbol
                .clone()
                .unwrap_or_else(|| crate::reachability::path(snapshot, issue.location.file));
            if matches!(
                issue.kind,
                IssueKind::EnumMembers | IssueKind::NamespaceMembers
            ) && let Some(module) = module
                && let Some((parent, member_name)) = module
                    .members
                    .iter()
                    .filter(|member| member.declaration.location == issue.location)
                    .find_map(|member| {
                        // A leaf can have several retained parent candidates. Match the
                        // structured finding's parent and path, not collection order.
                        let parent = module
                            .declarations
                            .iter()
                            .find(|declaration| declaration.id == member.parent)?;
                        let path = member.path.join(".");
                        (issue.symbol.as_deref()
                            == Some(format!("{}.{path}", parent.name).as_str()))
                        .then_some((parent, path))
                    })
            {
                details.parent = Some(parent.name.clone());
                name = member_name;
            }
            if matches!(
                issue.kind,
                IssueKind::Exports | IssueKind::Types | IssueKind::NsExports | IssueKind::NsTypes
            ) {
                details.symbol_type = module
                    .and_then(|module| {
                        module
                            .declarations
                            .iter()
                            .find(|declaration| declaration.id.location == issue.location)
                    })
                    .and_then(|declaration| match declaration.kind {
                        DeclarationKind::Class => Some("class"),
                        DeclarationKind::Enum => Some("enum"),
                        DeclarationKind::Function => Some("function"),
                        DeclarationKind::Interface => Some("interface"),
                        DeclarationKind::Type => Some("type"),
                        _ => None,
                    })
                    .map(str::to_owned);
            }
            details.symbols.push(ReportSymbol {
                name,
                location: (issue.kind != IssueKind::Files).then_some(issue.location),
                kind: None,
                specifier: None,
            });
        }
        metadata.issues.insert(issue.identity.clone(), details);
    }
    metadata
}

fn occurrence_exports<'a>(
    module: &'a CollectedModule,
    import: &'a ImportOccurrence,
) -> impl Iterator<Item = &'a Export> {
    module.exports.iter().filter(move |export| {
        export.location.file == import.location.file
            && import.location.range.contains_range(export.location.range)
            && match &export.target {
                ExportTarget::Reexport { specifier, .. }
                | ExportTarget::Namespace { specifier, .. } => specifier == &import.specifier,
                _ => false,
            }
    })
}

/// Classifies the selected edge using the pinned reporter's per-target presentation maps.
/// Type-only relations can affect those maps without becoming runtime cycle edges.
fn cycle_presentation(
    module: &CollectedModule,
    selected: &ImportOccurrence,
    target: FileId,
    location: Location,
) -> Option<(&'static str, Location)> {
    if selected.syntax == ImportSyntax::DynamicImport {
        return Some(("dynamicImport", location));
    }
    let (identifier, location) = if selected.syntax == ImportSyntax::Reexport {
        let export = occurrence_exports(module, selected)
            .filter(|export| export.namespace == SymbolNamespace::Value)
            .min_by_key(|export| (export.location.range.start(), export.location.range.end()))?;
        match &export.target {
            ExportTarget::Reexport { symbol, .. } => (
                Some(symbol.as_str()),
                if export.form == ExportForm::NamedReexport {
                    export.location
                } else {
                    location
                },
            ),
            ExportTarget::Namespace { .. } => (Some(export.name.as_str()), location),
            _ => return None,
        }
    } else {
        (
            selected
                .bindings
                .iter()
                .find(|binding| binding.declaration.namespace == SymbolNamespace::Value)
                .map(|binding| binding.imported.as_str()),
            location,
        )
    };
    let Some(identifier) = identifier else {
        // The pinned producer stores no identifier for a bare import.
        return Some(("import", location));
    };
    let mut reexport_star = false;
    let mut reexport_namespace = false;
    let mut reexport_alias = false;
    let mut reexport = false;
    let mut import_namespace = false;
    let mut import_alias = false;
    for import in module.imports.iter().filter(|import| {
        module.resolutions.iter().any(|resolution| {
            resolution.occurrence == import.ordinal
                && matches!(resolution.outcome, ResolutionOutcome::Resolved { file, .. } if file == target)
        })
    }) {
        for export in occurrence_exports(module, import) {
            match &export.target {
                ExportTarget::Reexport { symbol, .. } if symbol == identifier => {
                    reexport_star |= export.form == ExportForm::Star && identifier == "*";
                    reexport_alias |= export.form == ExportForm::NamedReexport && symbol != &export.name;
                    reexport |= export.form == ExportForm::NamedReexport && symbol == &export.name;
                }
                ExportTarget::Namespace { .. } => reexport_namespace |= export.name == identifier,
                _ => {}
            }
        }
        for binding in &import.bindings {
            import_namespace |= binding.imported == "*";
            if binding.imported == identifier {
                import_alias |= module.declarations.iter().any(|declaration| declaration.id == binding.declaration && declaration.name != identifier);
            }
            for export in &module.exports {
                if export.target != ExportTarget::Local(binding.declaration) {
                    continue;
                }
                if binding.imported == "*" {
                    reexport_namespace |= export.name == identifier;
                } else if binding.imported == identifier {
                    reexport_alias |= export.name != identifier;
                    reexport |= export.name == identifier;
                }
            }
        }
    }
    let kind = if reexport_star {
        "reExportStar"
    } else if reexport_namespace {
        "reExportNS"
    } else if reexport_alias {
        "reExportAs"
    } else if reexport {
        "reExport"
    } else if identifier == "*" && import_namespace {
        "importNS"
    } else if import_alias {
        "importAs"
    } else {
        "import"
    };
    Some((kind, location))
}
