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
