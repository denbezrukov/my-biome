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
                .filter(|binding| range.contains_range(binding.range()) && binding.is_imported())
            {
                let imported = imported_name(&binding);
                for declaration in self
                    .declarations
                    .iter()
                    .filter(|declaration| declaration.id.range == binding.range())
                {
                    bindings.push(ProjectImportBinding {
                        declaration: declaration.id.clone(),
                        imported: imported.clone(),
                    });
                }
            }
        } else if syntax == ProjectImportSyntax::Require
            && let Some(declarator) = import
                .syntax()
                .ancestors()
                .find_map(JsVariableDeclarator::cast)
            && let Ok(id) = declarator.id()
        {
            for declaration in self
                .declarations
                .iter()
                .filter(|declaration| id.range().contains_range(declaration.id.range))
            {
                let binding = model.as_binding_by_range(declaration.id.range)?;
                let imported = if let Some(property) = binding
                    .syntax()
                    .ancestors()
                    .find_map(biome_js_syntax::JsObjectBindingPatternProperty::cast)
                {
                    property
                        .member()
                        .ok()?
                        .to_trimmed_string()
                        .trim_matches(['\'', '"'])
                        .to_string()
                } else if id.range() != declaration.id.range {
                    declaration.name.clone()
                } else if let Some(member) = import
                    .syntax()
                    .parent()
                    .and_then(|node| AnyJsExpression::cast_ref(&node))
                    .and_then(|expression| member_expression(&expression))
                {
                    member.1
                } else {
                    "*".into()
                };
                bindings.push(ProjectImportBinding {
                    declaration: declaration.id.clone(),
                    imported,
                });
            }
        }
        self.imports.push(
            specifier.into(),
            ProjectImportOccurrence {
                range,
                specifier_range: import.module_name_token()?.text_trimmed_range(),
                syntax,
                type_only,
                optional: false,
                bindings,
            },
        );
        Some(())
    }

    fn collect_export(
        &mut self,
        export: &JsExport,
        model: &SemanticModel,
        path: &Utf8Path,
    ) -> Option<()> {
        if export
            .syntax()
            .ancestors()
            .any(|node| node.kind() == JsSyntaxKind::TS_MODULE_BLOCK)
        {
            return None;
        }
        let range = export.range();
        self.has_module_marker = true;
        let before = self.exports.len();
        match export.export_clause().ok()? {
            AnyJsExportClause::JsExportNamedClause(clause) => {
                for specifier in clause.specifiers().into_iter().flatten() {
                    let name = specifier.exported_name().ok()?.to_string();
                    let local = specifier.local_name().ok()?;
                    for namespace in [ProjectSymbolNamespace::Value, ProjectSymbolNamespace::Type] {
                        if namespace == ProjectSymbolNamespace::Value
                            && specifier.exports_only_types()
                        {
                            continue;
                        }
                        if let Some(binding) = binding_for_name(
                            model,
                            local.syntax(),
                            &local.to_trimmed_string(),
                            namespace,
                        ) {
                            self.export_local(
                                name.clone(),
                                namespace,
                                specifier.range(),
                                ProjectExportForm::Specifier,
                                identity(path, binding.range(), namespace),
                                model,
                            );
                        }
                    }
                }
            }
            AnyJsExportClause::JsExportNamedFromClause(clause) => {
                let source = clause
                    .source()
                    .ok()?
                    .as_js_module_source()?
                    .inner_string_text()
                    .ok()?
                    .to_string();
                for specifier in clause.specifiers().into_iter().flatten() {
                    let symbol = specifier
                        .source_name()
                        .ok()?
                        .inner_string_text()
                        .ok()?
                        .to_string();
                    let name = specifier
                        .export_as()
                        .and_then(|alias| alias.exported_name().ok())
                        .and_then(|name| name.inner_string_text().ok())
                        .map_or_else(|| symbol.clone(), |name| name.to_string());
                    for namespace in [ProjectSymbolNamespace::Value, ProjectSymbolNamespace::Type] {
                        if namespace == ProjectSymbolNamespace::Value
                            && (clause.type_token().is_some() || specifier.type_token().is_some())
                        {
                            continue;
                        }
                        self.exports.push(ProjectExportSlot {
                            name: name.clone(),
                            namespace,
                            range: specifier.range(),
                            form: ProjectExportForm::NamedReexport,
                            target: ProjectExportTarget::Reexport {
                                specifier: source.clone(),
                                symbol: symbol.clone(),
                            },
                        });
                    }
                }
            }
            AnyJsExportClause::JsExportFromClause(clause) => {
                let source = clause
                    .source()
                    .ok()?
                    .as_js_module_source()?
                    .inner_string_text()
                    .ok()?
                    .to_string();
                let name = clause
                    .export_as()
                    .and_then(|alias| alias.exported_name().ok())
                    .and_then(|name| name.inner_string_text().ok())
                    .map(|name| name.to_string());
                for namespace in [ProjectSymbolNamespace::Value, ProjectSymbolNamespace::Type] {
                    if namespace == ProjectSymbolNamespace::Value && clause.type_token().is_some() {
                        continue;
                    }
                    let (name, form, target) = match &name {
                        Some(name) => (
                            name.clone(),
                            ProjectExportForm::Namespace,
                            ProjectExportTarget::Namespace {
                                specifier: source.clone(),
                                identity: identity(path, range, namespace),
                            },
                        ),
                        None => (
                            "*".into(),
                            ProjectExportForm::Star,
                            ProjectExportTarget::Reexport {
                                specifier: source.clone(),
                                symbol: "*".into(),
                            },
                        ),
                    };
                    self.exports.push(ProjectExportSlot {
                        name,
                        namespace,
                        range,
                        form,
                        target,
                    });
                }
            }
            AnyJsExportClause::JsExportDefaultExpressionClause(clause) => {
                let expression = clause.expression().ok()?;
                let id = expression
                    .as_js_reference_identifier()
                    .and_then(|reference| model.binding(&reference))
                    .map_or_else(
                        || identity(path, expression.range(), ProjectSymbolNamespace::Value),
                        |binding| identity(path, binding.range(), ProjectSymbolNamespace::Value),
                    );
                self.export_local(
                    "default".into(),
                    ProjectSymbolNamespace::Value,
                    range,
                    ProjectExportForm::DefaultExpression,
                    id,
                    model,
                );
                if let Some(reference) = expression.as_js_reference_identifier()
                    && let Some(binding) = binding_for_name(
                        model,
                        reference.syntax(),
                        &reference.to_trimmed_string(),
                        ProjectSymbolNamespace::Type,
                    )
                {
                    self.export_local(
                        "default".into(),
                        ProjectSymbolNamespace::Type,
                        range,
                        ProjectExportForm::DefaultExpression,
                        identity(path, binding.range(), ProjectSymbolNamespace::Type),
                        model,
                    );
                }
            }
            AnyJsExportClause::JsExportDefaultDeclarationClause(clause) => {
                let declaration = clause.declaration().ok()?;
                let bindings: Vec<_> = self
                    .declarations
                    .iter()
                    .filter(|binding| {
                        declaration.range().contains_range(binding.id.range)
                            && model
                                .as_binding_by_range(binding.id.range)
                                .is_some_and(|binding| {
                                    binding
                                        .syntax()
                                        .parent()
                                        .is_some_and(|parent| parent == *declaration.syntax())
                                })
                    })
                    .cloned()
                    .collect();
                if bindings.is_empty() {
                    for namespace in [ProjectSymbolNamespace::Value, ProjectSymbolNamespace::Type] {
                        if namespace == ProjectSymbolNamespace::Type
                            && !biome_js_syntax::AnyJsClass::can_cast(declaration.syntax().kind())
                        {
                            continue;
                        }
                        self.exports.push(ProjectExportSlot {
                            name: "default".into(),
                            namespace,
                            range,
                            form: ProjectExportForm::DefaultDeclaration,
                            target: ProjectExportTarget::Local(identity(
                                path,
                                declaration.range(),
                                namespace,
                            )),
                        });
                    }
                }
                for binding in bindings {
                    self.export_local(
                        "default".into(),
                        binding.id.namespace,
                        range,
                        ProjectExportForm::DefaultDeclaration,
                        binding.id,
                        model,
                    );
                }
            }
            AnyJsExportClause::AnyJsDeclarationClause(_)
            | AnyJsExportClause::TsExportDeclareClause(_) => {
                let bindings: Vec<_> = self
                    .declarations
                    .iter()
                    .filter(|declaration| {
                        model
                            .as_binding_by_range(declaration.id.range)
                            .is_some_and(|binding| {
                                binding
                                    .syntax()
                                    .ancestors()
                                    .find_map(JsExport::cast)
                                    .is_some_and(|owner| owner == *export)
                                    && binding.is_exported()
                            })
                    })
                    .cloned()
                    .collect();
                for binding in bindings {
                    self.export_local(
                        binding.name,
                        binding.id.namespace,
                        range,
                        ProjectExportForm::Declaration,
                        binding.id,
                        model,
                    );
                }
            }
            AnyJsExportClause::TsExportAssignmentClause(_)
            | AnyJsExportClause::TsExportAsNamespaceClause(_) => {
                self.unknown(
                    range,
                    ProjectUnknownKind::Exports,
                    "TypeScript export assignment or global namespace export",
                );
                self.exports.push(ProjectExportSlot {
                    name: "*".into(),
                    namespace: ProjectSymbolNamespace::Value,
                    range,
                    form: ProjectExportForm::CommonJs,
                    target: ProjectExportTarget::Unknown("TypeScript namespace export".into()),
                });
            }
        }
        self.has_static_exports |= self.exports.len() != before;
        Some(())
    }

    fn export_local(
        &mut self,
        name: String,
        namespace: ProjectSymbolNamespace,
        range: TextRange,
        form: ProjectExportForm,
        id: ProjectDeclarationId,
        model: &SemanticModel,
    ) {
        let target = model
            .as_binding_by_range(id.range)
            .filter(|binding| binding.is_imported())
            .and_then(|binding| {
                let import = binding.syntax().ancestors().find_map(JsImport::cast)?;
                let specifier = import.source_text().ok()?.to_string();
                let symbol = imported_name(&binding);
                Some(if symbol == "*" {
                    ProjectExportTarget::Namespace {
                        specifier,
                        identity: id.clone(),
                    }
                } else {
                    ProjectExportTarget::Reexport { specifier, symbol }
                })
            })
            .unwrap_or(ProjectExportTarget::Local(id));
        self.exports.push(ProjectExportSlot {
            name,
            namespace,
            range,
            form,
            target,
        });
    }

    fn collect_call(
        &mut self,
        call: &JsCallExpression,
        model: &SemanticModel,
        path: &Utf8Path,
    ) -> Option<()> {
        let callee = call.callee().ok()?;
        let arguments = call.arguments().ok()?;
        let [first, second] = arguments.get_arguments_by_index([0, 1]);
        if let Some((object, member)) = member_expression(&callee) {
            if member == "define" {
                let provenance = registry_provenance(&object, model, 0);
                let declaration = second
                    .as_ref()
                    .and_then(|argument| argument.as_any_js_expression())
                    .and_then(|expression| registration_identity(expression, model, path));
                self.registrations.push(ProjectRegistration {
                    range: call.range(),
                    declaration,
                    provenance,
                });
                if provenance == ProjectRegistryProvenance::Unknown {
                    self.unknown(
                        call.range(),
                        ProjectUnknownKind::Registry,
                        "unproven custom-element registry",
                    );
                }
            }
            let syntax = if member == "resolve" && global_named(&object, "require", model) {
                Some(ProjectImportSyntax::RequireResolve)
            } else if member == "resolve"
                && object.syntax().kind() == JsSyntaxKind::JS_IMPORT_META_EXPRESSION
            {
                Some(ProjectImportSyntax::ImportMetaResolve)
            } else {
                None
            };
            if let Some(syntax) = syntax {
                self.entry_argument(
                    first
                        .as_ref()
                        .and_then(|argument| argument.as_any_js_expression()),
                    call.range(),
                    syntax,
                    false,
                );
            }
        }
        if let Some((package, method)) = node_import(&callee, model) {
            if package == "module" && method == "register" {
                let first = first
                    .as_ref()
                    .and_then(|argument| argument.as_any_js_expression());
                let literal = first.and_then(literal_string);
                let supported = literal.as_deref().is_some_and(|specifier| {
                    !specifier.starts_with("data:")
                        && (!biome_resolver::is_relative_specifier(specifier)
                            || second
                                .as_ref()
                                .and_then(|argument| argument.as_any_js_expression())
                                .is_some_and(import_meta_url))
                });
                if supported {
                    self.entry_argument(first, call.range(), ProjectImportSyntax::NodeEntry, false);
                } else {
                    self.unknown(
                        call.range(),
                        ProjectUnknownKind::Entry,
                        "unsupported Node register entry",
                    );
                }
            } else if package == "child_process"
                && matches!(
                    method.as_str(),
                    "fork" | "spawn" | "spawnSync" | "execFile" | "execFileSync"
                )
            {
                if let Some(expression) = first
                    .as_ref()
                    .and_then(|argument| argument.as_any_js_expression())
                {
                    self.path_entry(
                        expression,
                        call.range(),
                        ProjectImportSyntax::NodeEntry,
                        model,
                    );
                }
            } else if package == "child_process" && matches!(method.as_str(), "exec" | "execSync") {
                self.unknown(
                    call.range(),
                    ProjectUnknownKind::Entry,
                    "shell entry requires script analysis",
                );
            }
        }
        Some(())
    }

    fn collect_new(
        &mut self,
        new: &JsNewExpression,
        model: &SemanticModel,
        _path: &Utf8Path,
    ) -> Option<()> {
        let callee = new.callee().ok()?;
        if global_named(&callee, "URL", model) {
            let args = new.arguments()?;
            let [first, second] = args.get_arguments_by_index([0, 1]);
            if second
                .as_ref()
                .and_then(|argument| argument.as_any_js_expression())
                .is_some_and(import_meta_url)
            {
                self.entry_argument(
                    first
                        .as_ref()
                        .and_then(|argument| argument.as_any_js_expression()),
                    new.range(),
                    ProjectImportSyntax::Url,
                    true,
                );
            } else {
                self.unknown(
                    new.range(),
                    ProjectUnknownKind::Entry,
                    "URL with unsupported base",
                );
            }
        } else if node_import(&callee, model)
            .is_some_and(|(package, method)| package == "worker_threads" && method == "Worker")
            || global_named(&callee, "Worker", model)
        {
            let first = new.arguments()?.get_arguments_by_index([0])[0].clone()?;
            if let Some(expression) = first.as_any_js_expression() {
                if let Some(url) = JsNewExpression::cast_ref(expression.syntax())
                    && url
                        .callee()
                        .is_ok_and(|callee| global_named(&callee, "URL", model))
                {
                    return Some(());
                }
                if callee
                    .as_js_reference_identifier()
                    .is_some_and(|reference| reference.to_trimmed_string() != "Worker")
                {
                    self.unknown(
                        new.range(),
                        ProjectUnknownKind::Entry,
                        "aliased Worker entry",
                    );
                } else {
                    self.path_entry(expression, new.range(), ProjectImportSyntax::Worker, model);
                }
            }
        }
        Some(())
    }

    fn path_entry(
        &mut self,
        expression: &AnyJsExpression,
        range: TextRange,
        syntax: ProjectImportSyntax,
        model: &SemanticModel,
    ) {
        if let Some(specifier) = dirname_path(expression, model) {
            self.imports.push(
                Text::from(specifier),
                ProjectImportOccurrence {
                    range,
                    specifier_range: expression.range(),
                    syntax,
                    type_only: false,
                    optional: false,
                    bindings: Vec::new(),
                },
            );
        } else {
            self.unknown(
                range,
                ProjectUnknownKind::Entry,
                "unsupported Node path entry",
            );
        }
    }

    fn entry_argument(
        &mut self,
        expression: Option<&AnyJsExpression>,
        range: TextRange,
        syntax: ProjectImportSyntax,
        optional: bool,
    ) {
        if let Some(expression) = expression
            && let Some(specifier) = literal_string(expression)
        {
            self.imports.push(
                Text::from(specifier),
                ProjectImportOccurrence {
                    range,
                    specifier_range: expression.range(),
                    syntax,
                    type_only: false,
                    optional,
                    bindings: Vec::new(),
                },
            );
        } else {
            self.unknown(range, ProjectUnknownKind::Entry, "computed entry specifier");
        }
    }

    fn collect_assignment(
        &mut self,
        assignment: &JsAssignmentExpression,
        model: &SemanticModel,
        path: &Utf8Path,
    ) -> Option<()> {
        let left = assignment.left().ok()?;
        let right = assignment.right().ok()?;
        if let Some(identifier) = biome_js_syntax::JsIdentifierAssignment::cast_ref(left.syntax())
            && identifier
                .name_token()
                .is_ok_and(|token| matches!(token.text_trimmed(), "exports" | "module"))
            && model.binding(&identifier).is_none()
        {
            self.unknown(
                assignment.range(),
                ProjectUnknownKind::Exports,
                "reassigned CommonJS object",
            );
            return None;
        }
        let (object, member) =
            if let Some(member) = JsStaticMemberAssignment::cast_ref(left.syntax()) {
                (
                    member.object().ok()?,
                    Some(member.member().ok()?.to_trimmed_string()),
                )
            } else {
                let member = JsComputedMemberAssignment::cast_ref(left.syntax())?;
                (
                    member.object().ok()?,
                    literal_string(&member.member().ok()?),
                )
            };
        let whole = global_named(&object, "module", model) && member.as_deref() == Some("exports");
        let exports = global_named(&object, "exports", model)
            || member_expression(&object).is_some_and(|(object, member)| {
                member == "exports" && global_named(&object, "module", model)
            });
        if !whole && !exports {
            if commonjs_object(&object, model, 0)
                || (member.is_none() && global_named(&object, "module", model))
            {
                self.unknown(
                    assignment.range(),
                    ProjectUnknownKind::Exports,
                    "nested CommonJS mutation",
                );
            }
            return None;
        }
        if assignment
            .operator_token()
            .is_ok_and(|token| token.text_trimmed() != "=")
        {
            self.unknown(
                assignment.range(),
                ProjectUnknownKind::Exports,
                "compound CommonJS mutation",
            );
            return None;
        }
        if whole {
            if let Some(object) = JsObjectExpression::cast_ref(right.syntax()) {
                for member in object.members() {
                    let Ok(member) = member else {
                        continue;
                    };
                    if let Some(property) = JsPropertyObjectMember::cast_ref(member.syntax()) {
                        let name = property.name().ok()?;
                        if name.syntax().kind() == JsSyntaxKind::JS_COMPUTED_MEMBER_NAME {
                            self.unknown(
                                property.range(),
                                ProjectUnknownKind::Exports,
                                "computed CommonJS object member",
                            );
                            continue;
                        }
                        let name = name.to_trimmed_string();
                        self.commonjs_export(
                            name.trim_matches(['\'', '"']).into(),
                            property.value().ok()?,
                            property.range(),
                            model,
                            path,
                        );
                    } else if let Some(property) =
                        JsShorthandPropertyObjectMember::cast_ref(member.syntax())
                    {
                        let name = property.name().ok()?;
                        let id = model.binding(&name).map(|binding| {
                            identity(path, binding.range(), ProjectSymbolNamespace::Value)
                        });
                        self.exports.push(ProjectExportSlot {
                            name: name.to_trimmed_string(),
                            namespace: ProjectSymbolNamespace::Value,
                            range: property.range(),
                            form: ProjectExportForm::CommonJs,
                            target: id.map_or(
                                ProjectExportTarget::Unknown(
                                    "unresolved CommonJS shorthand".into(),
                                ),
                                ProjectExportTarget::Local,
                            ),
                        });
                    } else {
                        self.unknown(
                            member.range(),
                            ProjectUnknownKind::Exports,
                            "opaque CommonJS object member",
                        );
                    }
                }
            } else {
                self.commonjs_export("default".into(), right, assignment.range(), model, path);
                self.unknown(
                    assignment.range(),
                    ProjectUnknownKind::Exports,
                    "CommonJS object escape",
                );
            }
        } else if let Some(member) = member {
            self.commonjs_export(member, right, assignment.range(), model, path);
        } else {
            self.unknown(
                assignment.range(),
                ProjectUnknownKind::Exports,
                "computed CommonJS export",
            );
        }
        Some(())
    }

    fn collect_commonjs_reference(
        &mut self,
        reference: &AnyJsIdentifierReference,
        model: &SemanticModel,
    ) {
        let Some(expression) = reference.syntax().parent().and_then(AnyJsExpression::cast) else {
            return;
        };
        let is_exports = global_named(&expression, "exports", model);
        let is_module = global_named(&expression, "module", model);
        if !is_exports && !is_module {
            return;
        }
        let member = expression
            .syntax()
            .parent()
            .and_then(AnyJsExpression::cast)
            .and_then(|expression| member_expression(&expression));
        if is_module && member.as_ref().is_none_or(|(_, name)| name != "exports") {
            return;
        }
        if expression.syntax().ancestors().any(|node| {
            matches!(
                node.kind(),
                JsSyntaxKind::JS_PRE_UPDATE_EXPRESSION | JsSyntaxKind::JS_POST_UPDATE_EXPRESSION
            ) || biome_js_syntax::JsUnaryExpression::cast_ref(&node).is_some_and(|unary| {
                unary
                    .operator_token()
                    .is_ok_and(|token| token.text_trimmed() == "delete")
            })
        }) {
            self.unknown(
                reference.range(),
                ProjectUnknownKind::Exports,
                "opaque CommonJS mutation",
            );
            return;
        }
        if expression
            .syntax()
            .ancestors()
            .find_map(JsAssignmentExpression::cast)
            .is_some_and(|assignment| {
                assignment
                    .left()
                    .is_ok_and(|left| left.range().contains_range(reference.range()))
            })
        {
            return;
        }
        if is_exports && member.is_some() {
            return;
        }
        self.unknown(
            reference.range(),
            ProjectUnknownKind::Exports,
            "CommonJS object escape or opaque mutation",
        );
    }

    fn commonjs_export(
        &mut self,
        name: String,
        expression: AnyJsExpression,
        range: TextRange,
        model: &SemanticModel,
        path: &Utf8Path,
    ) {
        let id = expression
            .as_js_reference_identifier()
            .and_then(|reference| model.binding(&reference))
            .map_or_else(
                || identity(path, expression.range(), ProjectSymbolNamespace::Value),
                |binding| identity(path, binding.range(), ProjectSymbolNamespace::Value),
            );
        self.exports.push(ProjectExportSlot {
            name,
            namespace: ProjectSymbolNamespace::Value,
            range,
            form: ProjectExportForm::CommonJs,
            target: ProjectExportTarget::Local(id),
        });
    }
}

fn identity(
    path: &Utf8Path,
    range: TextRange,
    namespace: ProjectSymbolNamespace,
) -> ProjectDeclarationId {
    ProjectDeclarationId {
        module: path.to_path_buf(),
        range,
        namespace,
    }
}

fn imported_name(binding: &Binding) -> String {
    if binding
        .syntax()
        .ancestors()
        .any(|node| JsNamespaceImportSpecifier::can_cast(node.kind()))
    {
        return "*".into();
    }
    binding
        .syntax()
        .ancestors()
        .find_map(AnyJsNamedImportSpecifier::cast)
        .map_or_else(
            || "default".into(),
            |specifier| {
                specifier.imported_name().map_or_else(
                    || binding.syntax().text_trimmed().to_string(),
                    |token| token.text_trimmed().trim_matches(['\'', '"']).to_string(),
                )
            },
        )
}

fn binding_for_name(
    model: &SemanticModel,
    node: &JsSyntaxNode,
    name: &str,
    namespace: ProjectSymbolNamespace,
) -> Option<Binding> {
    let mut scope = model.scope(node);
    loop {
        if let Some(reference) = scope.get_binding_reference(name) {
            let id = match (namespace, reference) {
                (
                    ProjectSymbolNamespace::Value,
                    TsBindingReference::ValueType(id)
                    | TsBindingReference::TypeAndValueType(id)
                    | TsBindingReference::NamespaceAndValueType(id),
                ) => Some(id),
                (
                    ProjectSymbolNamespace::Type,
                    TsBindingReference::Type(id)
                    | TsBindingReference::TypeAndValueType(id)
                    | TsBindingReference::NamespaceAndValueType(id),
                ) => Some(id),
                (ProjectSymbolNamespace::Value, TsBindingReference::Merged { value_ty, .. }) => {
                    value_ty
                }
                (
                    ProjectSymbolNamespace::Type,
                    TsBindingReference::Merged {
                        ty, namespace_ty, ..
                    },
                ) => namespace_ty.or(ty),
                _ => None,
            }?;
            return model.binding_by_id(id);
        }
        scope = scope.parent()?;
    }
}

fn namespace_access(node: &JsSyntaxNode) -> ProjectNamespaceAccess {
    if let Some(qualified) = node
        .parent()
        .and_then(biome_js_syntax::TsQualifiedName::cast)
        && qualified.left().is_ok_and(|left| left.syntax() == node)
    {
        return qualified
            .right()
            .map_or(ProjectNamespaceAccess::Computed, |name| {
                ProjectNamespaceAccess::Known(name.to_trimmed_string())
            });
    }
    let expression = node.parent();
    let parent = expression.as_ref().and_then(|node| node.parent());
    if let Some(member) = parent.as_ref().and_then(JsStaticMemberExpression::cast_ref)
        && member
            .object()
            .is_ok_and(|object| Some(object.syntax()) == expression.as_ref())
    {
        return member
            .member()
            .map_or(ProjectNamespaceAccess::Computed, |name| {
                ProjectNamespaceAccess::Known(name.to_trimmed_string())
            });
    }
    if let Some(member) = parent
        .as_ref()
        .and_then(JsComputedMemberExpression::cast_ref)
        && member
            .object()
            .is_ok_and(|object| Some(object.syntax()) == expression.as_ref())
    {
        return member
            .member()
            .ok()
            .and_then(|key| literal_string(&key))
            .map_or(
                ProjectNamespaceAccess::Computed,
                ProjectNamespaceAccess::Known,
            );
    }
    ProjectNamespaceAccess::Escape
}

fn literal_string(expression: &AnyJsExpression) -> Option<String> {
    let value = expression.as_static_value()?;
    matches!(
        value,
        biome_js_syntax::static_value::StaticValue::String(_)
            | biome_js_syntax::static_value::StaticValue::EmptyString(_)
    )
    .then(|| value.text().to_string())
}

fn member_expression(expression: &AnyJsExpression) -> Option<(AnyJsExpression, String)> {
    if let Some(member) = JsStaticMemberExpression::cast_ref(expression.syntax()) {
        return Some((
            member.object().ok()?,
            member.member().ok()?.to_trimmed_string(),
        ));
    }
    let member = JsComputedMemberExpression::cast_ref(expression.syntax())?;
    Some((
        member.object().ok()?,
        literal_string(&member.member().ok()?)?,
    ))
}

fn global_named(expression: &AnyJsExpression, name: &str, model: &SemanticModel) -> bool {
    expression
        .as_js_reference_identifier()
        .is_some_and(|reference| {
            reference
                .value_token()
                .is_ok_and(|token| token.text_trimmed() == name)
                && binding_for_name(
                    model,
                    reference.syntax(),
                    name,
                    ProjectSymbolNamespace::Value,
                )
                .is_none()
        })
}

fn import_meta_url(expression: &AnyJsExpression) -> bool {
    member_expression(expression).is_some_and(|(object, name)| {
        name == "url" && object.syntax().kind() == JsSyntaxKind::JS_IMPORT_META_EXPRESSION
    })
}

fn initializer(binding: &Binding) -> Option<AnyJsExpression> {
    binding
        .syntax()
        .ancestors()
        .find_map(JsVariableDeclarator::cast)?
        .initializer()?
        .expression()
        .ok()
}

fn registry_provenance(
    expression: &AnyJsExpression,
    model: &SemanticModel,
    depth: usize,
) -> ProjectRegistryProvenance {
    use ProjectRegistryProvenance as P;
    if depth > 16 {
        return P::Unknown;
    }
    let expression = expression.clone().omit_parentheses();
    if global_named(&expression, "customElements", model) {
        return P::Global;
    }
    if let Some((object, member)) = member_expression(&expression)
        && member == "customElements"
    {
        if ["window", "globalThis", "self"]
            .iter()
            .any(|name| global_named(&object, name, model))
        {
            return P::Host;
        }
        if shadow_root(&object, model, depth + 1) {
            return P::ShadowRoot;
        }
    }
    if let Some(new) = JsNewExpression::cast_ref(expression.syntax())
        && new
            .callee()
            .is_ok_and(|callee| global_named(&callee, "CustomElementRegistry", model))
    {
        return P::Constructor;
    }
    if let Some(reference) = expression.as_js_reference_identifier()
        && let Some(binding) = model.binding(&reference)
    {
        if binding.all_writes().next().is_some() {
            return P::Unknown;
        }
        if let Some(init) = initializer(&binding)
            && registry_provenance(&init, model, depth + 1) != P::Unknown
        {
            return P::Alias;
        }
    }
    if let Some(assertion) = biome_js_syntax::TsAsExpression::cast_ref(expression.syntax())
        && let Ok(inner) = assertion.expression()
    {
        return registry_provenance(&inner, model, depth + 1);
    }
    P::Unknown
}

fn shadow_root(expression: &AnyJsExpression, model: &SemanticModel, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    if let Some(reference) = expression.as_js_reference_identifier()
        && let Some(binding) = model.binding(&reference)
        && binding.all_writes().next().is_none()
        && let Some(init) = initializer(&binding)
    {
        return shadow_root(&init, model, depth + 1);
    }
    if let Some(assertion) = biome_js_syntax::TsAsExpression::cast_ref(expression.syntax())
        && let Ok(inner) = assertion.expression()
    {
        return shadow_root(&inner, model, depth + 1);
    }
    let Some(call) = JsCallExpression::cast_ref(expression.syntax()) else {
        return false;
    };
    let Some((host, method)) = call
        .callee()
        .ok()
        .and_then(|callee| member_expression(&callee))
    else {
        return false;
    };
    method == "attachShadow"
        && member_expression(&host).is_some_and(|(document, property)| {
            matches!(property.as_str(), "body" | "documentElement")
                && global_named(&document, "document", model)
        })
}

fn registration_identity(
    expression: &AnyJsExpression,
    model: &SemanticModel,
    path: &Utf8Path,
) -> Option<ProjectDeclarationId> {
    if let Some(reference) = expression.as_js_reference_identifier() {
        let mut binding = model.binding(&reference)?;
        let mut seen = std::collections::BTreeSet::new();
        while seen.insert(binding.range()) && binding.all_writes().next().is_none() {
            let Some(next) = initializer(&binding)
                .and_then(|expression| expression.as_js_reference_identifier())
                .and_then(|reference| model.binding(&reference))
            else {
                break;
            };
            binding = next;
        }
        return Some(identity(
            path,
            binding.range(),
            ProjectSymbolNamespace::Value,
        ));
    }
    if expression.syntax().kind() != JsSyntaxKind::JS_THIS_EXPRESSION {
        return None;
    }
    let mut static_block = false;
    for ancestor in expression.syntax().ancestors().skip(1) {
        if biome_js_syntax::AnyJsFunction::can_cast(ancestor.kind())
            || matches!(
                ancestor.kind(),
                JsSyntaxKind::JS_METHOD_CLASS_MEMBER | JsSyntaxKind::JS_METHOD_OBJECT_MEMBER
            )
        {
            return None;
        }
        static_block |=
            ancestor.kind() == JsSyntaxKind::JS_STATIC_INITIALIZATION_BLOCK_CLASS_MEMBER;
        if let Some(class) = biome_js_syntax::AnyJsClass::cast_ref(&ancestor) {
            return static_block.then(|| {
                identity(
                    path,
                    class.id().map_or_else(|| class.range(), |id| id.range()),
                    ProjectSymbolNamespace::Value,
                )
            });
        }
    }
    None
}

fn node_import(expression: &AnyJsExpression, model: &SemanticModel) -> Option<(String, String)> {
    if let Some(reference) = expression.as_js_reference_identifier() {
        let binding = model.binding(&reference)?;
        if binding.all_writes().next().is_some() || binding.tree().is_type_only() {
            return None;
        }
        let (source, symbol) =
            if let Some(import) = binding.syntax().ancestors().find_map(JsImport::cast) {
                (
                    import.source_text().ok()?.to_string(),
                    imported_name(&binding),
                )
            } else {
                let init = initializer(&binding)?;
                let call = JsCallExpression::cast_ref(init.syntax())?;
                if !global_named(&call.callee().ok()?, "require", model) {
                    return None;
                }
                let source = AnyJsImportLike::JsCallExpression(call)
                    .inner_string_text()?
                    .to_string();
                let declarator = binding
                    .syntax()
                    .ancestors()
                    .find_map(JsVariableDeclarator::cast)?;
                let symbol = if declarator.id().ok()?.range() == binding.range() {
                    "*".into()
                } else if let Some(property) = binding
                    .syntax()
                    .ancestors()
                    .find_map(biome_js_syntax::JsObjectBindingPatternProperty::cast)
                {
                    property.member().ok()?.to_trimmed_string()
                } else {
                    binding.syntax().text_trimmed().to_string()
                };
                (source, symbol)
            };
        let package = source.strip_prefix("node:").unwrap_or(&source);
        return matches!(
            package,
            "module" | "worker_threads" | "child_process" | "path"
        )
        .then(|| (package.into(), symbol));
    }
    let (object, member) = member_expression(expression)?;
    let (package, imported) = node_import(&object, model)?;
    matches!(imported.as_str(), "*" | "default").then_some((package, member))
}

fn dirname_path(expression: &AnyJsExpression, model: &SemanticModel) -> Option<String> {
    let call = JsCallExpression::cast_ref(expression.syntax())?;
    let callee = call.callee().ok()?;
    let (package, method) = node_import(&callee, model)?;
    if package != "path" || !matches!(method.as_str(), "join" | "resolve") {
        return None;
    }
    if let Some(reference) = callee.as_js_reference_identifier()
        && reference.to_trimmed_string() != method
    {
        return None;
    }
    let arguments = call.arguments().ok()?;
    let mut arguments = arguments.args().into_iter();
    if !global_named(
        arguments.next()?.ok()?.as_any_js_expression()?,
        "__dirname",
        model,
    ) {
        return None;
    }
    let mut parts = Vec::new();
    for argument in arguments {
        let argument = argument.ok()?;
        let value = literal_string(argument.as_any_js_expression()?)?;
        if value.starts_with('/') {
            return None;
        }
        parts.push(value);
    }
    (!parts.is_empty()).then(|| format!("./{}", parts.join("/")))
}

fn commonjs_object(expression: &AnyJsExpression, model: &SemanticModel, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    if global_named(expression, "exports", model) {
        return true;
    }
    member_expression(expression).is_some_and(|(object, member)| {
        (member == "exports" && global_named(&object, "module", model))
            || commonjs_object(&object, model, depth + 1)
    })
}
