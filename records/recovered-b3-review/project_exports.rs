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
    .resolution
    .clone()
}

#[salsa::tracked]
fn project_export(db: &dyn ModuleDb, request: ProjectExportRequest<'_>) -> ProjectExportTrace {
    let mut origins = BTreeSet::new();
    let mut selected_slots = BTreeSet::new();
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
            .filter(|slot| slot.namespace == facet && slot.name == symbol && !is_star(slot))
            .collect();
        let slots = if explicit.is_empty() {
            facts
                .exports
                .iter()
                .filter(|slot| {
                    slot.namespace == facet
                        && is_star(slot)
                        && (symbol != "default" || slot.form == ProjectExportForm::CommonJs)
                })
                .collect()
        } else {
            explicit
        };
        for slot in slots {
            selected_slots.insert(ProjectExportSlotId {
                module: path.clone(),
                name: symbol.clone(),
                namespace: facet,
            });
            match &slot.target {
                ProjectExportTarget::Local(identity) => {
                    origins.insert(
                        facts
                            .merges
                            .iter()
                            .find(|relation| relation.declaration == *identity)
                            .map_or_else(|| identity.clone(), |relation| relation.target.clone()),
                    );
                }
                ProjectExportTarget::Namespace {
                    specifier,
                    identity,
                } => {
                    let mut context = request.context(db).clone();
                    context.syntax = if slot.form == ProjectExportForm::CommonJs {
                        crate::ProjectImportSyntax::Require
                    } else {
                        crate::ProjectImportSyntax::Reexport
                    };
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
                    context.syntax = if slot.form == ProjectExportForm::CommonJs {
                        crate::ProjectImportSyntax::Require
                    } else {
                        crate::ProjectImportSyntax::Reexport
                    };
                    match resolve_project_specifier(
                        db,
                        path.parent().unwrap_or(&path),
                        specifier,
                        context,
                    ) {
                        ProjectResolution::Resolved { target, .. } => pending.push((
                            target,
                            if is_star(slot) {
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
    let candidates: Vec<_> = origins.into_iter().collect();
    let resolution = if let Some(reason) = unknown {
        ProjectExportResolution::Unknown(reason)
    } else {
        match candidates.as_slice() {
            [] => ProjectExportResolution::Missing,
            [origin] => ProjectExportResolution::Found(origin.clone()),
            _ => ProjectExportResolution::Ambiguous(candidates.clone()),
        }
    };
    ProjectExportTrace {
        resolution,
        candidates,
        slots: selected_slots.into_iter().collect(),
    }
}

/// A selected module/name/facet on the authoritative export traversal.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProjectExportSlotId {
    pub module: Utf8PathBuf,
    pub name: String,
    pub namespace: ProjectSymbolNamespace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectExportTrace {
    pub resolution: ProjectExportResolution,
    pub candidates: Vec<ProjectDeclarationId>,
    pub slots: Vec<ProjectExportSlotId>,
}

/// Includes selected forwarding slots and candidate origins even when resolution is uncertain.
pub fn resolve_project_export_trace(
    db: &dyn ModuleDb,
    module: &Utf8Path,
    symbol: &str,
    facet: ProjectSymbolNamespace,
    context: ProjectResolutionContext,
) -> ProjectExportTrace {
    project_export(
        db,
        ProjectExportRequest::new(db, module.to_path_buf(), symbol.to_string(), facet, context),
    )
    .clone()
}

/// Enumerates possible exported names; lookup still decides precedence and ambiguity.
/// The boolean is false if a reachable star surface is unavailable or opaque.
pub fn project_export_names(
    db: &dyn ModuleDb,
    module: &Utf8Path,
    facet: ProjectSymbolNamespace,
    context: &ProjectResolutionContext,
) -> (BTreeSet<String>, bool) {
    let mut names = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut pending = vec![(module.to_path_buf(), false)];
    let mut complete = true;
    while let Some((path, star)) = pending.pop() {
        if !seen.insert((path.clone(), star)) {
            continue;
        }
        let Some(module) = db.module_for_path(&path) else {
            complete = false;
            continue;
        };
        let Some(info) = module.kind(db).as_js_module_info() else {
            complete = false;
            continue;
        };
        let facts = &info.project_facts;
        if facts.parse_status != ProjectParseStatus::Valid {
            complete = false;
            continue;
        }
        complete &= !facts
            .unknown
            .iter()
            .any(|fact| fact.kind == crate::ProjectUnknownKind::Exports);
        for slot in &facts.exports {
            if slot.namespace != facet {
                continue;
            }
            if !is_star(slot) {
                if !star || slot.name != "default" {
                    names.insert(slot.name.clone());
                }
            } else if let ProjectExportTarget::Reexport { specifier, .. } = &slot.target {
                let mut context = context.clone();
                context.syntax = if slot.form == ProjectExportForm::CommonJs {
                    crate::ProjectImportSyntax::Require
                } else {
                    crate::ProjectImportSyntax::Reexport
                };
                match resolve_project_specifier(
                    db,
                    path.parent().unwrap_or(&path),
                    specifier,
                    context,
                ) {
                    ProjectResolution::Resolved { target, .. } => {
                        pending.push((target, star || slot.form == ProjectExportForm::Star))
                    }
                    _ => complete = false,
                }
            }
        }
    }
    (names, complete)
}

fn is_star(slot: &crate::ProjectExportSlot) -> bool {
    slot.form == ProjectExportForm::Star
        || (slot.form == ProjectExportForm::CommonJs
            && matches!(&slot.target,ProjectExportTarget::Reexport { symbol, .. } if symbol == "*"))
}
