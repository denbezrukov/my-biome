                {
                    bindings.push(ProjectImportBinding {
                        declaration: declaration.id.clone(),
                        imported: imported.clone(),
                    });
                }
            }
        } else if syntax == ProjectImportSyntax::Require {
            if let Some(declarator) = import
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
        }
        self.imports.push(
            specifier.into(),
            ProjectImportOccurrence {
                range,
                specifier_range: import.module_name_token()?.text_trimmed_range(),
