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
