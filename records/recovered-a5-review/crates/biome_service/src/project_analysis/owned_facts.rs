use super::inventory::Capture;
use biome_module_graph as graph;
use biome_project_analysis::{
    coverage::{CoverageGap, GapReason, TargetDomain},
    facts as owned,
    model::{DeclarationId, FileId, Location, ModuleFacts, SymbolNamespace},
};
use biome_resolver::ResolutionEvent;
use biome_rowan::TextRange;

fn location(file: FileId, range: TextRange) -> Location {
    Location { file, range }
}
fn declaration(capture: &mut Capture, id: &graph::ProjectDeclarationId) -> DeclarationId {
    DeclarationId {
        location: location(capture.capture(&id.module), id.range),
        namespace: namespace(id.namespace),
    }
}
fn namespace(value: graph::ProjectSymbolNamespace) -> SymbolNamespace {
    match value {
        graph::ProjectSymbolNamespace::Value => SymbolNamespace::Value,
        graph::ProjectSymbolNamespace::Type => SymbolNamespace::Type,
    }
}

pub(super) fn context(value: &graph::ProjectResolutionContext) -> owned::ResolutionContext {
    owned::ResolutionContext {
        profile: profile(value.profile),
        purpose: purpose(value.purpose),
        syntax: syntax(value.syntax),
        config_revision: value.config_revision.clone(),
        mapping_revision: value.mapping_revision.clone(),
        additional_conditions: value.additional_conditions.clone(),
        source_mappings: value.source_mappings.iter().map(mapping).collect(),
    }
}
fn mapping(value: &biome_resolver::SourceMapping) -> owned::SourceMapping {
    owned::SourceMapping {
        from: value.from.clone(),
        to: value.to.clone(),
    }
}

pub(super) fn convert(
    capture: &mut Capture,
    file: FileId,
    facts: &graph::ProjectModuleFacts,
    resolutions: &[graph::ProjectResolvedOccurrence],
) -> ModuleFacts {
    let collected = owned::CollectedModule {
        parse_status: parse_status(facts.parse_status),
        declaration_file: facts.declaration_file.map(declaration_file),
        has_static_exports: facts.has_static_exports,
        has_module_marker: facts.has_module_marker,
        declarations: facts
            .declarations
            .iter()
            .map(|value| owned::NamedDeclaration {
                id: declaration(capture, &value.id),
                name: value.name.clone(),
                tags: value.tags.clone(),
            })
            .collect(),
        uses: facts
            .uses
            .iter()
            .map(|value| owned::Use {
                declaration: declaration(capture, &value.declaration),
                location: location(file, value.range),
                kind: use_kind(&value.kind),
                write: value.write,
                namespace_access: value.namespace_access.as_ref().map(|access| match access {
                    graph::ProjectNamespaceAccess::Known(name) => {
                        owned::NamespaceAccess::Known(name.clone())
                    }
                    graph::ProjectNamespaceAccess::Computed => owned::NamespaceAccess::Computed,
                    graph::ProjectNamespaceAccess::Escape => owned::NamespaceAccess::Escape,
                }),
            })
            .collect(),
        imports: facts
            .imports
            .named_iter()
            .enumerate()
            .map(|(ordinal, (specifier, value))| owned::ImportOccurrence {
                ordinal,
                specifier: specifier.to_string(),
                location: location(file, value.range),
                specifier_location: location(file, value.specifier_range),
                syntax: syntax(value.syntax),
                type_only: value.type_only,
                optional: value.optional,
                bindings: value
                    .bindings
                    .iter()
                    .map(|binding| owned::ImportBinding {
                        declaration: declaration(capture, &binding.declaration),
                        imported: binding.imported.clone(),
                    })
                    .collect(),
                type_usage: value.type_usage.as_ref().map(|usage| owned::ImportTypeUse {
                    kind: use_kind(&usage.kind),
                    namespace: namespace(usage.namespace),
                    qualifier: usage.qualifier.clone(),
                    location: location(file, usage.range),
                }),
            })
            .collect(),
        exports: facts
            .exports
            .iter()
            .map(|value| owned::Export {
                name: value.name.clone(),
                namespace: namespace(value.namespace),
                location: location(file, value.range),
                form: export_form(value.form),
                target: match &value.target {
                    graph::ProjectExportTarget::Local(id) => {
                        owned::ExportTarget::Local(declaration(capture, id))
                    }
                    graph::ProjectExportTarget::Reexport { specifier, symbol } => {
                        owned::ExportTarget::Reexport {
                            specifier: specifier.clone(),
                            symbol: symbol.clone(),
                        }
                    }
                    graph::ProjectExportTarget::Namespace {
                        specifier,
                        identity,
                    } => owned::ExportTarget::Namespace {
                        specifier: specifier.clone(),
                        identity: declaration(capture, identity),
                    },
                    graph::ProjectExportTarget::Unknown(reason) => {
                        owned::ExportTarget::Unknown(reason.clone())
                    }
                },
            })
            .collect(),
        registrations: facts
            .registrations
            .iter()
            .map(|value| owned::Registration {
                location: location(file, value.range),
                declaration: value
                    .declaration
                    .as_ref()
                    .map(|id| declaration(capture, id)),
                provenance: provenance(value.provenance),
            })
            .collect(),
        unknown: facts
            .unknown
            .iter()
            .map(|value| {
                let location = location(file, value.range);
                capture.inventory.gaps.push(CoverageGap {
                    producer: "module collection".into(),
                    domain: TargetDomain::Unbounded,
                    reason: GapReason::DynamicReference(value.reason.clone()),
                    location: Some(location),
                });
                owned::UnknownFact {
                    location,
                    kind: unknown_kind(&value.kind),
                    reason: value.reason.clone(),
                }
            })
            .collect(),
        resolutions: resolutions
            .iter()
            .map(|value| {
                let (outcome, trace) = match &value.resolution {
                    graph::ProjectResolution::Resolved {
                        target,
                        kind,
                        trace,
                    } => (
                        owned::ResolutionOutcome::Resolved {
                            file: capture.capture(target),
                            kind: format!("{kind:?}"),
                        },
                        trace,
                    ),
                    graph::ProjectResolution::Unresolved { error, trace } => (
                        owned::ResolutionOutcome::Unresolved {
                            error: format!("{error:?}"),
                        },
                        trace,
                    ),
                    graph::ProjectResolution::Builtin { name, trace } => (
                        owned::ResolutionOutcome::Builtin { name: name.clone() },
                        trace,
                    ),
                    graph::ProjectResolution::Unknown { reason, trace } => {
                        capture.gap(
                            "module resolution",
                            reason.clone(),
                            Some(location(
                                file,
                                facts
                                    .imports
                                    .named_iter()
                                    .nth(value.occurrence)
                                    .unwrap()
                                    .1
                                    .range,
                            )),
                        );
                        (
                            owned::ResolutionOutcome::Unknown {
                                reason: reason.clone(),
                            },
                            trace,
                        )
                    }
                };
                owned::ResolvedOccurrence {
                    occurrence: value.occurrence,
                    context: context(&value.context),
                    outcome,
                    trace: trace
                        .events
                        .iter()
                        .map(|event| match event {
                            ResolutionEvent::Manifest { path, quality } => {
                                let manifest = capture.capture(path);
                                if !quality.known || quality.has_errors() {
                                    capture.gap(
                                        "resolution metadata",
                                        format!("Incomplete manifest {path}"),
                                        Some(location(manifest, TextRange::default())),
                                    );
                                }
                                owned::ResolutionEvent::Manifest {
                                    file: manifest,
                                    known: quality.known,
                                    diagnostics: quality
                                        .diagnostics
                                        .iter()
                                        .map(|diagnostic| owned::ManifestDiagnostic {
                                            severity: format!("{:?}", diagnostic.severity),
                                            message: diagnostic.message.clone(),
                                            location: diagnostic
                                                .range
                                                .map(|range| location(manifest, range)),
                                        })
                                        .collect(),
                                }
                            }
                            ResolutionEvent::Package { path, name } => {
                                owned::ResolutionEvent::Package {
                                    path: path.clone(),
                                    name: name.clone(),
                                }
                            }
                            ResolutionEvent::Condition(value) => {
                                owned::ResolutionEvent::Condition(value.clone())
                            }
                            ResolutionEvent::Target(path) => {
                                owned::ResolutionEvent::Target(path.clone())
                            }
                            ResolutionEvent::Mapping(value) => {
                                owned::ResolutionEvent::Mapping(mapping(value))
                            }
                            ResolutionEvent::Failure(error) => {
                                owned::ResolutionEvent::Failure(format!("{error:?}"))
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
    };
    ModuleFacts {
        collected: Some(collected),
        ..Default::default()
    }
}

fn parse_status(value: graph::ProjectParseStatus) -> owned::ParseStatus {
    match value {
        graph::ProjectParseStatus::Valid => owned::ParseStatus::Valid,
        graph::ProjectParseStatus::Invalid => owned::ParseStatus::Invalid,
        graph::ProjectParseStatus::Unknown => owned::ParseStatus::Unknown,
    }
}

fn declaration_file(value: graph::ProjectDeclarationFileKind) -> owned::DeclarationFileKind {
    match value {
        graph::ProjectDeclarationFileKind::TypeScript => owned::DeclarationFileKind::TypeScript,
        graph::ProjectDeclarationFileKind::Module => owned::DeclarationFileKind::Module,
        graph::ProjectDeclarationFileKind::CommonJs => owned::DeclarationFileKind::CommonJs,
    }
}

fn use_kind(value: &graph::ProjectUseKind) -> owned::UseKind {
    match value {
        graph::ProjectUseKind::Runtime => owned::UseKind::Runtime,
        graph::ProjectUseKind::Type => owned::UseKind::Type,
        graph::ProjectUseKind::TypeQuery => owned::UseKind::TypeQuery,
        graph::ProjectUseKind::RuntimeTypeof => owned::UseKind::RuntimeTypeof,
    }
}

fn syntax(value: graph::ProjectImportSyntax) -> owned::ImportSyntax {
    match value {
        graph::ProjectImportSyntax::Import => owned::ImportSyntax::Import,
        graph::ProjectImportSyntax::Require => owned::ImportSyntax::Require,
        graph::ProjectImportSyntax::DynamicImport => owned::ImportSyntax::DynamicImport,
        graph::ProjectImportSyntax::ImportType => owned::ImportSyntax::ImportType,
        graph::ProjectImportSyntax::Reexport => owned::ImportSyntax::Reexport,
        graph::ProjectImportSyntax::RequireResolve => owned::ImportSyntax::RequireResolve,
        graph::ProjectImportSyntax::ImportMetaResolve => owned::ImportSyntax::ImportMetaResolve,
        graph::ProjectImportSyntax::Url => owned::ImportSyntax::Url,
        graph::ProjectImportSyntax::Worker => owned::ImportSyntax::Worker,
        graph::ProjectImportSyntax::NodeEntry => owned::ImportSyntax::NodeEntry,
    }
}

fn export_form(value: graph::ProjectExportForm) -> owned::ExportForm {
    match value {
        graph::ProjectExportForm::Declaration => owned::ExportForm::Declaration,
        graph::ProjectExportForm::Specifier => owned::ExportForm::Specifier,
        graph::ProjectExportForm::DefaultDeclaration => owned::ExportForm::DefaultDeclaration,
        graph::ProjectExportForm::DefaultExpression => owned::ExportForm::DefaultExpression,
        graph::ProjectExportForm::NamedReexport => owned::ExportForm::NamedReexport,
        graph::ProjectExportForm::Star => owned::ExportForm::Star,
        graph::ProjectExportForm::Namespace => owned::ExportForm::Namespace,
        graph::ProjectExportForm::CommonJs => owned::ExportForm::CommonJs,
    }
}

fn provenance(value: graph::ProjectRegistryProvenance) -> owned::RegistryProvenance {
    match value {
        graph::ProjectRegistryProvenance::Global => owned::RegistryProvenance::Global,
        graph::ProjectRegistryProvenance::Host => owned::RegistryProvenance::Host,
        graph::ProjectRegistryProvenance::Constructor => owned::RegistryProvenance::Constructor,
        graph::ProjectRegistryProvenance::ShadowRoot => owned::RegistryProvenance::ShadowRoot,
        graph::ProjectRegistryProvenance::Alias => owned::RegistryProvenance::Alias,
        graph::ProjectRegistryProvenance::Unknown => owned::RegistryProvenance::Unknown,
    }
}

fn unknown_kind(value: &graph::ProjectUnknownKind) -> owned::UnknownKind {
    match value {
        graph::ProjectUnknownKind::Declaration => owned::UnknownKind::Declaration,
        graph::ProjectUnknownKind::ModuleSpecifier => owned::UnknownKind::ModuleSpecifier,
        graph::ProjectUnknownKind::Exports => owned::UnknownKind::Exports,
        graph::ProjectUnknownKind::Registry => owned::UnknownKind::Registry,
        graph::ProjectUnknownKind::Entry => owned::UnknownKind::Entry,
    }
}

fn profile(value: graph::ProjectResolutionProfile) -> owned::ResolutionProfile {
    match value {
        graph::ProjectResolutionProfile::Knip => owned::ResolutionProfile::Knip,
        graph::ProjectResolutionProfile::Node => owned::ResolutionProfile::Node,
    }
}

fn purpose(value: graph::ProjectResolutionPurpose) -> owned::ResolutionPurpose {
    match value {
        graph::ProjectResolutionPurpose::Runtime => owned::ResolutionPurpose::Runtime,
        graph::ProjectResolutionPurpose::Declaration => owned::ResolutionPurpose::Declaration,
    }
}
