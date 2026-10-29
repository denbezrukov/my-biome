use super::MetadataValue;
use biome_deserialize::DeserializationDiagnostic;
use biome_diagnostics::serde::Diagnostic;
use biome_json_value::{JsonArray, JsonObject, JsonValue};
use biome_rowan::{AstNode, TextRange};
use biome_yaml_parser::parse_yaml;
use biome_yaml_syntax::{YamlSyntaxKind as K, YamlSyntaxNode};
use indexmap::IndexMap;

/// Static pnpm/Yarn data, preserving empty providers and unsupported syntax diagnostics.
#[derive(Clone, Debug, Default)]
pub struct WorkspaceMetadata {
    pub fields: IndexMap<String, MetadataValue>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses static mappings and sequences, retaining diagnostics for invalid or unsupported YAML.
pub fn read_workspace_metadata(source: &str) -> WorkspaceMetadata {
    let parsed = parse_yaml(source);
    let mut result = WorkspaceMetadata {
        diagnostics: parsed
            .diagnostics()
            .iter()
            .cloned()
            .map(Diagnostic::new)
            .collect(),
        ..Default::default()
    };
    let documents: Vec<_> = parsed.tree().documents().into_iter().collect();
    if documents.len() != 1 {
        result
            .diagnostics
            .push(Diagnostic::new(DeserializationDiagnostic::new(
                "Workspace data requires one YAML document.",
            )));
    }
    if let Some(node) = documents
        .first()
        .and_then(|d| d.as_yaml_document())
        .and_then(|d| d.node())
    {
        let value = read_node(node.syntax(), &mut result.diagnostics);
        if matches!(value.value, JsonValue::Object(_)) {
            result.fields = value.members;
        } else {
            unsupported(
                &mut result.diagnostics,
                node.range(),
                "Workspace data requires a mapping.",
            );
        }
    }
    if let Some(packages) = result.fields.get("packages") {
        if !matches!(packages.value, JsonValue::Array(_) | JsonValue::Null)
            || packages
                .elements
                .iter()
                .any(|v| v.value.as_string().is_none())
        {
            unsupported(
                &mut result.diagnostics,
                packages.range,
                "Workspace packages must be a sequence of strings.",
            );
        }
    }
    for key in ["catalog", "catalogs", "overrides"] {
        if let Some(value) = result.fields.get(key) {
            if !matches!(value.value, JsonValue::Object(_) | JsonValue::Null) {
                unsupported(
                    &mut result.diagnostics,
                    value.range,
                    "Catalogs and overrides must be mappings.",
                );
            }
            let maps: Vec<_> = if key == "catalogs" {
                value.members.values().collect()
            } else {
                vec![value]
            };
            for map in maps {
                if !matches!(map.value, JsonValue::Object(_) | JsonValue::Null) {
                    unsupported(
                        &mut result.diagnostics,
                        map.range,
                        "Catalog must be a mapping.",
                    );
                }
                for entry in map.members.values() {
                    if entry.value.as_string().is_none() {
                        unsupported(
                            &mut result.diagnostics,
                            entry.range,
                            "Catalog versions must be scalar strings.",
                        );
                    }
                }
            }
        }
    }
    result
}

fn unsupported(diagnostics: &mut Vec<Diagnostic>, range: TextRange, message: &str) {
    diagnostics.push(Diagnostic::new(
        DeserializationDiagnostic::new(message.to_string()).with_range(range),
    ));
}

fn read_node(node: &YamlSyntaxNode, diagnostics: &mut Vec<Diagnostic>) -> MetadataValue {
    let mut result = MetadataValue {
        value: JsonValue::Null,
        range: node.text_trimmed_range(),
        members: IndexMap::new(),
        elements: Vec::new(),
    };
    match node.kind() {
        K::YAML_BLOCK_MAPPING | K::YAML_FLOW_MAPPING => {
            let entries = node.children().find(|n| {
                matches!(
                    n.kind(),
                    K::YAML_BLOCK_MAP_ENTRY_LIST | K::YAML_FLOW_MAP_ENTRY_LIST
                )
            });
            if let Some(entries) = entries {
                for entry in entries.children() {
                    let mut children = entry.children();
                    let Some(key_node) = children.next() else {
                        continue;
                    };
                    let key = read_node(&key_node, diagnostics);
                    let Some(key) = key.value.as_string() else {
                        unsupported(
                            diagnostics,
                            key_node.text_trimmed_range(),
                            "Workspace keys must be scalar strings.",
                        );
                        continue;
                    };
                    let value = children
                        .next()
                        .map(|n| read_node(&n, diagnostics))
                        .unwrap_or_else(|| MetadataValue {
                            value: JsonValue::Null,
                            range: entry.text_trimmed_range(),
                            members: IndexMap::new(),
                            elements: Vec::new(),
                        });
                    if result
                        .members
                        .insert(key.as_str().to_string(), value)
                        .is_some()
                    {
                        unsupported(
                            diagnostics,
                            entry.text_trimmed_range(),
                            "Duplicate workspace metadata key.",
                        );
                    }
                }
            }
            let mut object = JsonObject::default();
            for (key, value) in &result.members {
                object.insert(key.as_str().into(), value.value.clone());
            }
            result.value = JsonValue::Object(object);
        }
        K::YAML_BLOCK_SEQUENCE | K::YAML_FLOW_SEQUENCE => {
            if let Some(entries) = node.children().find(|n| {
                matches!(
                    n.kind(),
                    K::YAML_BLOCK_SEQUENCE_ENTRY_LIST | K::YAML_FLOW_SEQUENCE_ENTRY_LIST
                )
            }) {
                result.elements = entries
                    .children()
                    .map(|n| read_node(&n, diagnostics))
                    .collect();
            }
            let mut array = JsonArray::default();
            array.extend(result.elements.iter().map(|v| v.value.clone()));
            result.value = JsonValue::Array(array);
        }
        K::YAML_PLAIN_SCALAR | K::YAML_SINGLE_QUOTED_SCALAR | K::YAML_DOUBLE_QUOTED_SCALAR => {
            let raw = node.text_trimmed().to_string();
            let value = if node.kind() == K::YAML_SINGLE_QUOTED_SCALAR {
                raw.trim_matches('\'').replace("''", "'")
            } else if node.kind() == K::YAML_DOUBLE_QUOTED_SCALAR {
                let parsed = biome_json_parser::parse_json(&raw, Default::default());
                let (value, errors) = biome_deserialize::json::deserialize_from_json_ast::<String>(
                    &parsed.tree(),
                    "",
                )
                .consume();
                if parsed.has_errors() || !errors.is_empty() {
                    unsupported(diagnostics, result.range, "Unsupported YAML quoted escape.");
                }
                value.unwrap_or_default()
            } else {
                raw
            };
            result.value = if node.kind() == K::YAML_PLAIN_SCALAR {
                match value.as_str() {
                    "null" | "~" => JsonValue::Null,
                    "true" => JsonValue::Bool(true),
                    "false" => JsonValue::Bool(false),
                    _ => value
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .map(JsonValue::Number)
                        .unwrap_or_else(|| JsonValue::String(value.into())),
                }
            } else {
                JsonValue::String(value.into())
            };
        }
        K::YAML_FLOW_IN_BLOCK_NODE
        | K::YAML_BLOCK_IN_BLOCK_NODE
        | K::YAML_FLOW_JSON_NODE
        | K::YAML_FLOW_YAML_NODE
        | K::YAML_BLOCK_SEQUENCE_ENTRY => {
            for child in node.children() {
                if child.kind() == K::YAML_PROPERTY_LIST {
                    if child.children().next().is_some() {
                        unsupported(
                            diagnostics,
                            child.text_trimmed_range(),
                            "YAML tags and anchors are unsupported workspace data.",
                        );
                    }
                } else {
                    result = read_node(&child, diagnostics);
                }
            }
        }
        _ => unsupported(
            diagnostics,
            result.range,
            "Unsupported workspace YAML value.",
        ),
    }
    result
}
