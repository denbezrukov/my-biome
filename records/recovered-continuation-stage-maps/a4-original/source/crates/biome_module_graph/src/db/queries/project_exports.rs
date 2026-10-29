//! Export origins are declaration identities, independent of type inference.

use crate::{
    ModuleDb, ProjectDeclarationId, ProjectExportForm, ProjectExportTarget, ProjectParseStatus,
    ProjectResolution, ProjectResolutionContext, ProjectSymbolNamespace, resolve_project_specifier,
};
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectExportResolution {
    Found(ProjectDeclarationId),
    Missing,
    Ambiguous(Vec<ProjectDeclarationId>),
    Unknown(String),
}

#[salsa::interned]
struct ProjectExportRequest<'db> {
    module: Utf8PathBuf,
    symbol: String,
    #[returns(copy)]
    facet: ProjectSymbolNamespace,
    context: ProjectResolutionContext,
}

/// Follows explicit and star exports for one facet and resolution context.
/// Only trusted successful parses can establish a complete symbol table.
/// Unavailable modules and uncertain export shapes cannot establish absence.
pub fn resolve_project_export(
    db: &dyn ModuleDb,
    module: &Utf8Path,
    symbol: &str,
    facet: ProjectSymbolNamespace,
    context: ProjectResolutionContext,
) -> ProjectExportResolution {
    project_export(
        db,
        ProjectExportRequest::new(db, module.to_path_buf(), symbol.to_string(), facet, context),
    )
    .clone()
}

#[salsa::tracked]
fn project_export(db: &dyn ModuleDb, request: ProjectExportRequest<'_>) -> ProjectExportResolution {
    let mut origins = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut pending = vec![(request.module(db).clone(), request.symbol(db).clone())];
    let facet = request.facet(db);
    let mut unknown = None;
    while let Some((path, symbol)) = pending.pop() {
        if !seen.insert((path.clone(), symbol.clone())) {
            continue;
        }
        let Some(module) = db.module_for_path(&path) else {
            unknown = Some(format!("module unavailable: {path}"));
            continue;
        };
        let Some(info) = module.kind(db).as_js_module_info() else {
            unknown = Some(format!("unsupported module: {path}"));
            continue;
        };
        let facts = &info.project_facts;
        if facts.parse_status != ProjectParseStatus::Valid {
            unknown = Some(format!("unverified or invalid parse: {path}"));
            continue;
        }
        let explicit: Vec<_> = facts
            .exports
            .iter()
            .filter(|slot| {
                slot.namespace == facet
                    && slot.name == symbol
                    && slot.form != ProjectExportForm::Star
            })
            .collect();
        let slots = if explicit.is_empty() && symbol != "default" {
            facts
                .exports
                .iter()
                .filter(|slot| slot.namespace == facet && slot.form == ProjectExportForm::Star)
                .collect()
        } else {
            explicit
        };
        for slot in slots {
            match &slot.target {
                ProjectExportTarget::Local(identity) => {
                    origins.insert(identity.clone());
                }
                ProjectExportTarget::Namespace {
                    specifier,
                    identity,
                } => {
                    let mut context = request.context(db).clone();
                    context.syntax = crate::ProjectImportSyntax::Reexport;
                    match resolve_project_specifier(
                        db,
                        path.parent().unwrap_or(&path),
                        specifier,
                        context,
                    ) {
                        ProjectResolution::Resolved { target, .. }
                            if db.module_for_path(&target).is_some_and(|module| {
                                module.kind(db).as_js_module_info().is_some_and(|info| {
                                    info.project_facts.parse_status == ProjectParseStatus::Valid
                                })
                            }) =>
                        {
                            origins.insert(identity.clone());
                        }
                        _ => unknown = Some(format!("unavailable namespace target: {specifier}")),
                    }
                }
                ProjectExportTarget::Unknown(reason) => unknown = Some(reason.clone()),
                ProjectExportTarget::Reexport {
                    specifier,
                    symbol: imported,
                } => {
                    let mut context = request.context(db).clone();
                    context.syntax = crate::ProjectImportSyntax::Reexport;
                    match resolve_project_specifier(
                        db,
                        path.parent().unwrap_or(&path),
                        specifier,
                        context,
                    ) {
                        ProjectResolution::Resolved { target, .. } => pending.push((
                            target,
                            if slot.form == ProjectExportForm::Star {
                                symbol.clone()
                            } else {
                                imported.clone()
                            },
                        )),
                        _ => unknown = Some(format!("unavailable export target: {specifier}")),
                    }
                }
            }
        }
        if facts
            .unknown
            .iter()
            .any(|fact| fact.kind == crate::ProjectUnknownKind::Exports)
        {
            unknown = Some(format!("uncertain exports: {path}"));
        }
    }
    if let Some(reason) = unknown {
        return ProjectExportResolution::Unknown(reason);
    }
    match origins.len() {
        0 => ProjectExportResolution::Missing,
        1 => ProjectExportResolution::Found(origins.pop_first().unwrap()),
        _ => ProjectExportResolution::Ambiguous(origins.into_iter().collect()),
    }
}
