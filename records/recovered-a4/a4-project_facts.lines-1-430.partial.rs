//! Owned syntax evidence for project analysis, independent of inferred types.

use biome_js_semantic::{Binding, JsDeclarationKind, SemanticModel, TsBindingReference};
use biome_js_syntax::{
    AnyJsExportClause, AnyJsExpression, AnyJsIdentifierReference, AnyJsImportLike,
    AnyJsNamedImportSpecifier, JsAssignmentExpression, JsCallExpression,
    JsComputedMemberAssignment, JsComputedMemberExpression, JsExport, JsImport,
    JsNamespaceImportSpecifier, JsNewExpression, JsObjectExpression, JsPropertyObjectMember,
    JsShorthandPropertyObjectMember, JsStaticMemberAssignment, JsStaticMemberExpression,
    JsSyntaxKind, JsSyntaxNode, JsVariableDeclarator,
};
use biome_rowan::{AstNode, Text};
use camino::Utf8Path;

use crate::ImportPathMap;
use biome_rowan::TextRange;
use camino::Utf8PathBuf;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProjectSymbolNamespace {
    Value,
    Type,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectDeclarationId {
    pub module: Utf8PathBuf,
    pub range: TextRange,
    pub namespace: ProjectSymbolNamespace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectDeclaration {
    pub id: ProjectDeclarationId,
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectParseStatus {
    Valid,
    Invalid,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectNamespaceAccess {
    Known(String),
    Computed,
    Escape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectUseKind {
    Runtime,
    Type,
    TypeQuery,
    RuntimeTypeof,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectUse {
    pub kind: ProjectUseKind,
    pub declaration: ProjectDeclarationId,
    pub range: TextRange,
    pub write: bool,
    pub namespace_access: Option<ProjectNamespaceAccess>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectImportBinding {
    pub declaration: ProjectDeclarationId,
    pub imported: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ProjectImportSyntax {
    #[default]
    Import,
    Require,
    DynamicImport,
    Reexport,
    RequireResolve,
    ImportMetaResolve,
    Url,
    Worker,
    NodeEntry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectImportOccurrence {
    pub range: TextRange,
    pub specifier_range: TextRange,
    pub syntax: ProjectImportSyntax,
    pub type_only: bool,
    pub optional: bool,
    pub bindings: Vec<ProjectImportBinding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectExportForm {
    Declaration,
    Specifier,
    DefaultDeclaration,
    DefaultExpression,
    NamedReexport,
    Star,
    Namespace,
    CommonJs,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectExportTarget {
    Local(ProjectDeclarationId),
    Reexport {
        specifier: String,
        symbol: String,
    },
    Namespace {
        specifier: String,
        identity: ProjectDeclarationId,
    },
    Unknown(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectExportSlot {
    pub name: String,
    pub namespace: ProjectSymbolNamespace,
    pub range: TextRange,
    pub form: ProjectExportForm,
    pub target: ProjectExportTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRegistryProvenance {
    Global,
    Host,
    Constructor,
    ShadowRoot,
    Alias,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRegistration {
    pub range: TextRange,
    pub declaration: Option<ProjectDeclarationId>,
    pub provenance: ProjectRegistryProvenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectUnknownKind {
    Declaration,
    ModuleSpecifier,
    Exports,
    Registry,
    Entry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectUnknownFact {
    pub range: TextRange,
    pub kind: ProjectUnknownKind,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectDeclarationFileKind {
    TypeScript,
    Module,
    CommonJs,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectModuleFacts {
    pub declaration_file: Option<ProjectDeclarationFileKind>,
    pub parse_status: ProjectParseStatus,
    pub has_static_exports: bool,
    pub has_module_marker: bool,
    pub declarations: Vec<ProjectDeclaration>,
    pub uses: Vec<ProjectUse>,
    pub imports: ImportPathMap<ProjectImportOccurrence>,
    pub exports: Vec<ProjectExportSlot>,
    pub registrations: Vec<ProjectRegistration>,
    pub unknown: Vec<ProjectUnknownFact>,
}

impl ProjectModuleFacts {
    /// A recovered or unverified parse cannot establish the ambient exemption.
    pub fn is_ambient_declaration_file(&self) -> bool {
        self.declaration_file.is_some()
            && self.parse_status == ProjectParseStatus::Valid
            && !self.has_static_exports
    }
}

impl ProjectModuleFacts {
    pub(crate) fn from_semantic(
        model: &SemanticModel,
        path: &Utf8Path,
        status: ProjectParseStatus,
    ) -> Self {
        let mut facts = Self {
            declaration_file: if path.as_str().ends_with(".d.ts") {
                Some(ProjectDeclarationFileKind::TypeScript)
            } else if path.as_str().ends_with(".d.mts") {
                Some(ProjectDeclarationFileKind::Module)
            } else if path.as_str().ends_with(".d.cts") {
                Some(ProjectDeclarationFileKind::CommonJs)
            } else {
                None
            },
            parse_status: status,
            ..Self::default()
        };
        for binding in model.all_bindings() {
            let kind = binding.declaration_kind();
            let type_only = binding.tree().is_type_only();
            if kind == JsDeclarationKind::Unknown {
                facts.unknown(
                    binding.range(),
                    ProjectUnknownKind::Declaration,
                    "unknown declaration kind",
                );
            }
            for namespace in [ProjectSymbolNamespace::Value, ProjectSymbolNamespace::Type] {
                let declares = match namespace {
                    ProjectSymbolNamespace::Value => kind.declares_value() && !type_only,
                    ProjectSymbolNamespace::Type => {
                        kind.declares_type() || kind.declares_namespace() || type_only
                    }
                };
                if declares {
                    facts.declarations.push(ProjectDeclaration {
                        id: identity(path, binding.range(), namespace),
                        name: binding
                            .tree()
                            .name_token()
                            .map(|token| token.text_trimmed().to_string())
                            .unwrap_or_default(),
                        tags: binding
                            .jsdoc()
                            .map(|doc| {
                                doc.split_whitespace()
                                    .filter_map(|word| word.strip_prefix('@').map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    });
                }
            }
            let namespace_import = binding
                .syntax()
                .ancestors()
                .any(|node| JsNamespaceImportSpecifier::can_cast(node.kind()));
            for reference in binding.all_references() {
                let node = reference.syntax();
                let type_use = AnyJsIdentifierReference::cast_ref(&node)
                    .is_some_and(|reference| reference.is_only_type())
                    && !node
                        .ancestors()
                        .any(|ancestor| ancestor.kind() == JsSyntaxKind::TS_TYPEOF_TYPE);
                let namespace = if type_use || type_only {
                    ProjectSymbolNamespace::Type
                } else {
                    ProjectSymbolNamespace::Value
                };
                facts.uses.push(ProjectUse {
                    kind: if node
                        .ancestors()
                        .any(|ancestor| ancestor.kind() == JsSyntaxKind::TS_TYPEOF_TYPE)
                    {
                        ProjectUseKind::TypeQuery
                    } else if node.ancestors().any(|ancestor| {
                        biome_js_syntax::JsUnaryExpression::cast_ref(&ancestor).is_some_and(
                            |unary| {
                                unary
                                    .operator_token()
                                    .is_ok_and(|token| token.text_trimmed() == "typeof")
                            },
                        )
                    }) {
                        ProjectUseKind::RuntimeTypeof
                    } else if type_use || type_only {
                        ProjectUseKind::Type
                    } else {
                        ProjectUseKind::Runtime
                    },
                    declaration: identity(path, binding.range(), namespace),
                    range: node.text_trimmed_range(),
                    write: reference.is_write(),
                    namespace_access: namespace_import.then(|| namespace_access(&node)),
                });
            }
        }
        facts.uses.sort_by_key(|usage| usage.range);
        facts
    }

    pub(crate) fn finish(&mut self) {
        for slot in &self.exports {
            let id = match &slot.target {
                ProjectExportTarget::Local(id)
                | ProjectExportTarget::Namespace { identity: id, .. } => id,
                _ => continue,
            };
            if !self
                .declarations
                .iter()
                .any(|declaration| declaration.id == *id)
            {
                self.declarations.push(ProjectDeclaration {
                    id: id.clone(),
                    name: slot.name.clone(),
                    tags: Vec::new(),
                });
            }
        }
    }

    pub(crate) fn enter_node(
        &mut self,
        node: &JsSyntaxNode,
        model: &SemanticModel,
        path: &Utf8Path,
    ) {
        if let Some(reference) = AnyJsIdentifierReference::cast_ref(node) {
            self.collect_commonjs_reference(&reference, model);
        }
        if let Some(export) = JsExport::cast_ref(node) {
            self.collect_export(&export, model, path);
        }
        if let Some(import) = AnyJsImportLike::cast_ref(node) {
            self.collect_import(&import, model);
        }
        if let Some(call) = JsCallExpression::cast_ref(node) {
            self.collect_call(&call, model, path);
        }
        if let Some(new) = JsNewExpression::cast_ref(node) {
            self.collect_new(&new, model, path);
        }
        if let Some(assignment) = JsAssignmentExpression::cast_ref(node) {
            self.collect_assignment(&assignment, model, path);
        }
    }

    fn unknown(&mut self, range: TextRange, kind: ProjectUnknownKind, reason: &str) {
        self.unknown.push(ProjectUnknownFact {
            range,
            kind,
            reason: reason.into(),
        });
    }

    fn collect_import(&mut self, import: &AnyJsImportLike, model: &SemanticModel) -> Option<()> {
        if import.is_in_ts_module_declaration() {
            return None;
        }
        let (syntax, range, type_only) = match import {
            AnyJsImportLike::JsModuleSource(source) => {
                let statement = source.syntax().ancestors().find(|node| {
                    JsImport::can_cast(node.kind())
                        || JsExport::can_cast(node.kind())
                        || biome_js_syntax::TsImportEqualsDeclaration::can_cast(node.kind())
                });
                if let Some(import_equals) = statement
                    .as_ref()
                    .and_then(biome_js_syntax::TsImportEqualsDeclaration::cast_ref)
                {
                    let range = import_equals.range();
                    let id_range = import_equals.id().ok()?.range();
                    let bindings = self
                        .declarations
                        .iter()
                        .filter(|declaration| declaration.id.range == id_range)
                        .map(|declaration| ProjectImportBinding {
                            declaration: declaration.id.clone(),
                            imported: "*".into(),
                        })
                        .collect();
                    self.imports.push(
                        source.inner_string_text().ok()?.into(),
                        ProjectImportOccurrence {
                            range,
                            specifier_range: source.range(),
                            syntax: ProjectImportSyntax::Require,
                            type_only: import_equals.type_token().is_some(),
                            optional: false,
                            bindings,
                        },
                    );
                    return Some(());
                }
                let is_export = statement
                    .as_ref()
                    .is_some_and(|node| JsExport::can_cast(node.kind()));
                (
                    if is_export {
                        ProjectImportSyntax::Reexport
                    } else {
                        ProjectImportSyntax::Import
                    },
                    statement.map_or_else(|| source.range(), |node| node.text_trimmed_range()),
                    source.imports_only_types(),
                )
            }
            AnyJsImportLike::JsCallExpression(call) => {
                if !global_named(&call.callee().ok()?, "require", model) {
                    return None;
                }
                (ProjectImportSyntax::Require, call.range(), false)
            }
            AnyJsImportLike::JsImportCallExpression(call) => {
                (ProjectImportSyntax::DynamicImport, call.range(), false)
            }
        };
        let Some(specifier) = import.inner_string_text() else {
            self.unknown(
                range,
                ProjectUnknownKind::ModuleSpecifier,
                "computed module specifier",
            );
            return None;
        };
        let mut bindings = Vec::new();
        if syntax == ProjectImportSyntax::Import {
            for binding in model
                .all_bindings()
