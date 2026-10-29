//! Keeps lexical member paths and declaration relations without evaluating initializers.

use super::*;
use biome_js_syntax::{TsEnumDeclaration, TsModuleDeclaration, TsQualifiedName};

#[derive(Clone, Debug, Eq, PartialEq)]
/// A leaf under an enum or outer namespace; overloads retain their own declaration ranges.
pub struct ProjectMember {
    pub parent: ProjectDeclarationId,
    pub declaration: ProjectDeclarationId,
    pub path: Vec<String>,
    pub tags: Vec<String>,
}

pub(super) fn collect(model: &SemanticModel, path: &Utf8Path) -> Vec<ProjectMember> {
    let mut members = Vec::new();
    for binding in model.all_bindings() {
        let mut parents: Vec<_> = binding
            .syntax()
            .ancestors()
            .skip(1)
            .filter_map(TsModuleDeclaration::cast)
            .filter(|module| {
                module
                    .name()
                    .is_ok_and(|name| !name.range().contains_range(binding.range()))
            })
            .collect();
        parents.reverse();
        if let Some(root) = parents.first()
            && parents.iter().skip(1).all(|module| {
                module
                    .name()
                    .ok()
                    .and_then(|name| model.as_binding_by_range(name.range()))
                    .is_some_and(|binding| binding.is_exported())
            })
            && (binding.is_exported()
                || binding
                    .syntax()
                    .parent()
                    .and_then(biome_js_syntax::TsDeclareFunctionDeclaration::cast)
                    .is_some_and(|function| {
                        function
                            .syntax()
                            .parent()
                            .is_some_and(|parent| JsExport::can_cast(parent.kind()))
                    }))
            && !matches!(
                binding.declaration_kind(),
                JsDeclarationKind::Namespace | JsDeclarationKind::Module
            )
            && let Ok(root_name) = root.name()
        {
            let mut names: Vec<_> = parents
                .iter()
                .skip(1)
                .filter_map(|module| module.name().ok())
                .map(|name| name.to_trimmed_string())
                .collect();
            names.push(
                binding
                    .tree()
                    .name_token()
                    .map(|token| token.text_trimmed().to_string())
                    .unwrap_or_default(),
            );
            members.push(ProjectMember {
                parent: identity(path, root_name.range(), ProjectSymbolNamespace::Value),
                declaration: identity(
                    path,
                    binding.range(),
                    if binding.declaration_kind().declares_value() {
                        ProjectSymbolNamespace::Value
                    } else {
                        ProjectSymbolNamespace::Type
                    },
                ),
                path: names,
                tags: {
                    let mut result = binding.jsdoc().map(|doc| tags(doc)).unwrap_or_default();
                    if let Some(export) = binding.syntax().ancestors().find_map(JsExport::cast) {
                        result.extend(node_tags(export.syntax()));
                    }
                    result.sort();
                    result.dedup();
                    result
                },
            });
        }
        if binding.declaration_kind() == JsDeclarationKind::Enum
            && let Some(enumeration) = binding
                .syntax()
                .ancestors()
                .find_map(TsEnumDeclaration::cast)
        {
            for member in enumeration.members().into_iter().flatten() {
                let Ok(name) = member.name() else {
                    continue;
                };
                let name_text = match &name {
                    biome_js_syntax::AnyTsEnumMemberName::TsLiteralEnumMemberName(name) => {
                        name.value().ok().and_then(|token| {
                            decoded_string(biome_js_syntax::inner_string_text(&token))
                        })
                    }
                    biome_js_syntax::AnyTsEnumMemberName::JsComputedMemberName(name) => name
                        .expression()
                        .ok()
                        .and_then(|value| literal_string(&value)),
                };
                let Some(name_text) = name_text else {
                    continue;
                };
                members.push(ProjectMember {
                    parent: identity(path, binding.range(), ProjectSymbolNamespace::Value),
                    declaration: identity(path, name.range(), ProjectSymbolNamespace::Value),
                    path: vec![name_text],
                    tags: node_tags(member.syntax()),
                });
            }
        }
    }
    members
}

fn declaring_scope(binding: &Binding) -> Option<biome_rowan::TextRange> {
    binding
        .scope()
        .ancestors()
        .find(|scope| {
            scope
                .bindings()
                .any(|candidate| candidate.range() == binding.range())
        })
        .map(|scope| scope.range())
}
fn tags(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| word.strip_prefix('@').map(str::to_string))
        .collect()
}
pub(super) fn node_tags(node: &JsSyntaxNode) -> Vec<String> {
    let Some(token) = node.first_token() else {
        return Vec::new();
    };
    let mut result: Vec<_> = token
        .leading_trivia()
        .pieces()
        .filter(|piece| piece.is_comments())
        .flat_map(|piece| tags(piece.text()))
        .collect();
    if let Some(previous) = token.prev_token() {
        result.extend(
            previous
                .trailing_trivia()
                .pieces()
                .filter(|piece| piece.is_comments())
                .flat_map(|piece| tags(piece.text())),
        );
    }
    result
}

pub(super) fn member_path(node: &JsSyntaxNode) -> Vec<ProjectNamespaceAccess> {
    let mut current = node.clone();
    let mut result = Vec::new();
    while let Some(parent) = current.parent() {
        if parent.kind() == JsSyntaxKind::JS_IDENTIFIER_EXPRESSION
            || parent.kind() == JsSyntaxKind::JS_PARENTHESIZED_EXPRESSION
        {
            current = parent;
            continue;
        }
        let access = if let Some(member) = JsStaticMemberExpression::cast_ref(&parent)
            && member
                .object()
                .is_ok_and(|object| object.syntax() == &current)
        {
            member.member().ok().map(|name| {
                ProjectNamespaceAccess::Known(
                    biome_js_syntax::unescape_js_identifier(&name.to_trimmed_string()).into_owned(),
                )
            })
        } else if let Some(member) = JsComputedMemberExpression::cast_ref(&parent)
            && member
                .object()
                .is_ok_and(|object| object.syntax() == &current)
        {
            Some(
                member
                    .member()
                    .ok()
                    .and_then(|key| literal_string(&key))
                    .map_or(
                        ProjectNamespaceAccess::Computed,
                        ProjectNamespaceAccess::Known,
                    ),
            )
        } else if let Some(member) = TsQualifiedName::cast_ref(&parent)
            && member.left().is_ok_and(|left| left.syntax() == &current)
        {
            member.right().ok().map(|name| {
                ProjectNamespaceAccess::Known(
                    biome_js_syntax::unescape_js_identifier(&name.to_trimmed_string()).into_owned(),
                )
            })
        } else {
            None
        };
        let Some(access) = access else {
            break;
        };
        result.push(access);
        current = parent;
    }
    result
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// The source declaration and a statically proven alias or first merged declaration.
pub struct ProjectDeclarationRelation {
    pub declaration: ProjectDeclarationId,
    pub target: ProjectDeclarationId,
}

pub(super) fn relations(
    model: &SemanticModel,
    path: &Utf8Path,
) -> (
    Vec<ProjectDeclarationRelation>,
    Vec<ProjectDeclarationRelation>,
) {
    let bindings: Vec<_> = model.all_bindings().collect();
    let mut merges = Vec::new();
    let mut aliases = Vec::new();
    for binding in &bindings {
        let kind = binding.declaration_kind();
        if matches!(
            kind,
            JsDeclarationKind::Namespace
                | JsDeclarationKind::Module
                | JsDeclarationKind::Enum
                | JsDeclarationKind::Function
                | JsDeclarationKind::Class
        ) {
            let name = binding.tree().name_token();
            let first = bindings
                .iter()
                .filter(|other| {
                    declaring_scope(other) == declaring_scope(binding)
                        && other.tree().name_token().as_ref().map(|t| t.text_trimmed())
                            == name.as_ref().map(|t| t.text_trimmed())
                        && matches!(
                            other.declaration_kind(),
                            JsDeclarationKind::Namespace
                                | JsDeclarationKind::Module
                                | JsDeclarationKind::Enum
                                | JsDeclarationKind::Function
                                | JsDeclarationKind::Class
                        )
                })
                .min_by_key(|other| other.range().start());
            if let Some(first) = first
                && first.range() != binding.range()
            {
                for namespace in [ProjectSymbolNamespace::Value, ProjectSymbolNamespace::Type] {
                    merges.push(ProjectDeclarationRelation {
                        declaration: identity(path, binding.range(), namespace),
                        target: identity(path, first.range(), namespace),
                    });
                }
            }
        }
        if binding.all_writes().next().is_none()
            && let Some(init) = registration_initializer(binding)
            && let Some(reference) = init.as_js_reference_identifier()
            && let Some(target) = model.binding(&reference)
        {
            aliases.push(ProjectDeclarationRelation {
                declaration: identity(path, binding.range(), ProjectSymbolNamespace::Value),
                target: identity(path, target.range(), ProjectSymbolNamespace::Value),
            });
        }
    }
    (merges, aliases)
}
