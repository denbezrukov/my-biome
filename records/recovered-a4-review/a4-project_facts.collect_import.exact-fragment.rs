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
