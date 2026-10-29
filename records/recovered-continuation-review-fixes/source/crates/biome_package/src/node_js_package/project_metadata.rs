use biome_deserialize::{
    Deserializable, DeserializableType, DeserializableTypes, DeserializableValue,
    DeserializationContext, DeserializationDiagnostic, DeserializationVisitor, MapMembers,
    TextRange, json::deserialize_from_json_ast,
};
use biome_diagnostics::serde::Diagnostic;
use biome_json_syntax::JsonRoot;
use biome_json_value::{JsonArray, JsonObject, JsonValue};
use indexmap::IndexMap;

/// A metadata value with its original source range and nested value locations.
/// Invalid values and values beyond the nesting limit remain as `JsonValue::Bogus` with diagnostics.
#[derive(Clone, Debug)]
pub struct MetadataValue {
    pub value: JsonValue,
    pub range: TextRange,
    pub members: IndexMap<String, Self>,
    pub elements: Vec<Self>,
}

impl Deserializable for MetadataValue {
    fn deserialize(
        ctx: &mut dyn DeserializationContext,
        value: &impl DeserializableValue,
        name: &str,
    ) -> Option<Self> {
        Some(Self::read(ctx, value, name, 0))
    }
}

impl MetadataValue {
    fn invalid(ctx: &mut dyn DeserializationContext, range: TextRange, message: &str) -> Self {
        ctx.report(DeserializationDiagnostic::new(message).with_range(range));
        Self {
            value: JsonValue::Bogus,
            range,
            members: IndexMap::new(),
            elements: Vec::new(),
        }
    }

    fn read(
        ctx: &mut dyn DeserializationContext,
        value: &impl DeserializableValue,
        name: &str,
        depth: usize,
    ) -> Self {
        const MAX_METADATA_DEPTH: usize = 64;
        let range = value.range();
        if depth >= MAX_METADATA_DEPTH {
            return Self::invalid(
                ctx,
                range,
                "Project metadata exceeds the nesting limit of 64 values.",
            );
        }
        match value.visitable_type() {
            Some(DeserializableType::Map | DeserializableType::Array) => value
                .deserialize(ctx, MetadataContainerVisitor { depth }, name)
                .unwrap_or_else(|| Self::invalid(ctx, range, "Invalid project metadata value.")),
            Some(_) => match JsonValue::deserialize(ctx, value, name) {
                Some(value) => Self {
                    value,
                    range,
                    members: IndexMap::new(),
                    elements: Vec::new(),
                },
                None => Self::invalid(ctx, range, "Invalid project metadata value."),
            },
            None => Self::invalid(ctx, range, "Invalid project metadata value."),
        }
    }
}

struct MetadataContainerVisitor {
    depth: usize,
}

impl DeserializationVisitor for MetadataContainerVisitor {
    type Output = MetadataValue;
    const EXPECTED_TYPE: DeserializableTypes =
        DeserializableTypes::MAP.union(DeserializableTypes::ARRAY);

    fn visit_map(
        self,
        ctx: &mut dyn DeserializationContext,
        members: &mut MapMembers<'_>,
        range: TextRange,
        _name: &str,
    ) -> Option<Self::Output> {
        let mut fields = IndexMap::new();
        let mut object = JsonObject::default();
        for entry in members {
            let Some((key, value)) = entry else {
                ctx.report(
                    DeserializationDiagnostic::new("Invalid project metadata property.")
                        .with_range(range),
                );
                continue;
            };
            let Some(key_text) = String::deserialize(ctx, &key, "") else {
                continue;
            };
            ctx.enter_property(&key_text, key.range(), value.range());
            let field = MetadataValue::read(ctx, &value, &key_text, self.depth + 1);
            ctx.exit_property();
            object.insert(key_text.as_str().into(), field.value.clone());
            fields.insert(key_text, field);
        }
        Some(MetadataValue {
            value: JsonValue::Object(object),
            range,
            members: fields,
            elements: Vec::new(),
        })
    }

    fn visit_array(
        self,
        ctx: &mut dyn DeserializationContext,
        values: &mut dyn ExactSizeIterator<Item = Option<Box<dyn DeserializableValue>>>,
        range: TextRange,
        _name: &str,
    ) -> Option<Self::Output> {
        let mut elements = Vec::new();
        let mut array = JsonArray::default();
        for (index, value) in values.enumerate() {
            let field = if let Some(value) = value {
                ctx.enter_index(index, value.range());
                let field = MetadataValue::read(ctx, &value, "", self.depth + 1);
                ctx.exit_index();
                field
            } else {
                MetadataValue::invalid(ctx, range, "Invalid project metadata array element.")
            };
            array.push(field.value.clone());
            elements.push(field);
        }
        Some(MetadataValue {
            value: JsonValue::Array(array),
            range,
            members: IndexMap::new(),
            elements,
        })
    }
}

/// A typed tsconfig input that retains the range before path normalization.
#[derive(Clone, Debug)]
pub struct Located<T> {
    pub value: T,
    pub range: TextRange,
}
impl<T: Deserializable> Deserializable for Located<T> {
    fn deserialize(
        ctx: &mut dyn DeserializationContext,
        value: &impl DeserializableValue,
        name: &str,
    ) -> Option<Self> {
        Some(Self {
            value: T::deserialize(ctx, value, name)?,
            range: value.range(),
        })
    }
}

/// Catalog declarations and metadata diagnostics are independent channels.
#[derive(Clone, Debug, Default)]
pub struct CatalogMetadata {
    /// Every candidate is retained; provider precedence belongs to inventory acquisition.
    pub entries: Vec<CatalogDeclaration>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A catalog candidate with its name, JSON property path and original values.
#[derive(Clone, Debug)]
pub struct CatalogDeclaration {
    /// The empty name identifies a default catalog.
    pub name: String,
    /// Distinguishes root catalog/catalogs from legacy workspaces catalog/catalogs.
    pub path: Vec<String>,
    pub value: MetadataValue,
}

/// Raw manifest evidence is independent of the selected resolver types path.
/// All fields, including malformed values, remain available with nested ranges.
#[derive(Clone, Debug, Default)]
pub struct ProjectMetadata {
    pub types: Option<String>,
    pub raw_types: Option<String>,
    pub typings: Option<String>,
    pub module: Option<MetadataValue>,
    pub browser: Option<MetadataValue>,
    pub imports: Option<MetadataValue>,
    pub fields: IndexMap<String, MetadataValue>,
    pub catalogs: CatalogMetadata,
    pub diagnostics: Vec<Diagnostic>,
}

/// Reads static manifest evidence. Parser diagnostics remain owned by the parser result.
pub fn read_project_metadata(root: &JsonRoot) -> ProjectMetadata {
    let (metadata, mut diagnostics) =
        deserialize_from_json_ast::<ProjectMetadata>(root, "package.json").consume();
    if metadata.is_none() && diagnostics.is_empty() {
        diagnostics.push(
            DeserializationDiagnostic::new("Manifest metadata requires a JSON object.")
                .with_range(biome_rowan::AstNode::range(root))
                .into(),
        );
    }
    let mut metadata = metadata.unwrap_or_default();
    metadata.diagnostics = diagnostics.into_iter().map(Diagnostic::new).collect();
    metadata
}

impl Deserializable for ProjectMetadata {
    fn deserialize(
        ctx: &mut dyn DeserializationContext,
        value: &impl DeserializableValue,
        name: &str,
    ) -> Option<Self> {
        let mut recording = MetadataContext {
            parent: ctx,
            diagnostics: Vec::new(),
        };
        let mut metadata = value.deserialize(&mut recording, MetadataVisitor, name)?;
        metadata.diagnostics = recording.diagnostics;
        Some(metadata)
    }
}
struct MetadataVisitor;

struct MetadataContext<'a> {
    parent: &'a mut dyn DeserializationContext,
    diagnostics: Vec<Diagnostic>,
}
impl DeserializationContext for MetadataContext<'_> {
    fn id(&self) -> Option<&str> {
        self.parent.id()
    }
    fn report(&mut self, diagnostic: DeserializationDiagnostic) {
        self.diagnostics.push(Diagnostic::new(diagnostic.clone()));
        self.parent.report(diagnostic);
    }
    fn enter_property(&mut self, name: &str, key_range: TextRange, value_range: TextRange) {
        self.parent.enter_property(name, key_range, value_range);
    }
    fn exit_property(&mut self) {
        self.parent.exit_property();
    }
    fn enter_index(&mut self, index: usize, range: TextRange) {
        self.parent.enter_index(index, range);
    }
    fn exit_index(&mut self) {
        self.parent.exit_index();
    }
}
impl DeserializationVisitor for MetadataVisitor {
    type Output = ProjectMetadata;
    const EXPECTED_TYPE: DeserializableTypes = DeserializableTypes::MAP;
    fn visit_map(
        self,
        ctx: &mut dyn DeserializationContext,
        members: &mut MapMembers<'_>,
        _range: TextRange,
        _name: &str,
    ) -> Option<ProjectMetadata> {
        let mut result = ProjectMetadata::default();
        for (key, value) in members.flatten() {
            let key = String::deserialize(ctx, &key, "")?;
            let Some(field) = MetadataValue::deserialize(ctx, &value, &key) else {
                continue;
            };
            validate_manifest_field(ctx, &key, &field);
            result.fields.insert(key, field);
        }
        let string = |key: &str| {
            result
                .fields
                .get(key)
                .and_then(|v| v.value.as_string())
                .map(|v| v.as_str().to_string())
        };
        result.raw_types = string("types");
        result.typings = string("typings");
        result.types = if result.fields.contains_key("types") {
            result.raw_types.clone()
        } else {
            result.typings.clone()
        };
        result.module = result.fields.get("module").cloned();
        result.browser = result.fields.get("browser").cloned();
        result.imports = result.fields.get("imports").cloned();
        for (prefix, fields) in [
            (Vec::<String>::new(), Some(&result.fields)),
            (
                vec!["workspaces".to_string()],
                result.fields.get("workspaces").map(|v| &v.members),
            ),
        ] {
            let Some(fields) = fields else { continue };
            if let Some(catalog) = fields.get("catalog") {
                collect_catalog(
                    ctx,
                    &mut result.catalogs,
                    "",
                    prefix.iter().cloned().chain(["catalog".into()]).collect(),
                    catalog,
                );
            }
            if let Some(catalogs) = fields.get("catalogs") {
                if !matches!(catalogs.value, JsonValue::Object(_)) {
                    let diagnostic = DeserializationDiagnostic::new(
                        "Named catalogs must be an object of catalog maps.",
                    )
                    .with_range(catalogs.range);
                    result
                        .catalogs
                        .diagnostics
                        .push(Diagnostic::new(diagnostic.clone()));
                    ctx.report(diagnostic);
                }
                for (name, catalog) in &catalogs.members {
                    collect_catalog(
                        ctx,
                        &mut result.catalogs,
                        name,
                        prefix
                            .iter()
                            .cloned()
                            .chain(["catalogs".into(), name.clone()])
                            .collect(),
                        catalog,
                    );
                }
            }
        }
        Some(result)
    }
}

fn expect(
    ctx: &mut dyn DeserializationContext,
    value: &MetadataValue,
    expected: DeserializableTypes,
    name: &str,
) {
    let actual = match &value.value {
        JsonValue::Array(_) => DeserializableType::Array,
        JsonValue::Bool(_) => DeserializableType::Bool,
        JsonValue::Null => DeserializableType::Null,
        JsonValue::Number(_) => DeserializableType::Number,
        JsonValue::Object(_) => DeserializableType::Map,
        JsonValue::String(_) => DeserializableType::Str,
        JsonValue::Bogus => return,
    };
    let actual_types = match actual {
        DeserializableType::Array => DeserializableTypes::ARRAY,
        DeserializableType::Bool => DeserializableTypes::BOOL,
        DeserializableType::Null => DeserializableTypes::NULL,
        DeserializableType::Number => DeserializableTypes::NUMBER,
        DeserializableType::Map => DeserializableTypes::MAP,
        DeserializableType::Str => DeserializableTypes::STR,
    };
    if !expected.contains(actual_types) {
        ctx.report(DeserializationDiagnostic::new_incorrect_type_with_name(
            actual,
            expected,
            name,
            value.range,
        ));
    }
}
fn string_map(ctx: &mut dyn DeserializationContext, value: &MetadataValue, name: &str) {
    expect(ctx, value, DeserializableTypes::MAP, name);
    for field in value.members.values() {
        expect(ctx, field, DeserializableTypes::STR, name);
    }
}
fn string_array(ctx: &mut dyn DeserializationContext, value: &MetadataValue, name: &str) {
    expect(ctx, value, DeserializableTypes::ARRAY, name);
    for field in &value.elements {
        expect(ctx, field, DeserializableTypes::STR, name);
    }
}
fn targets(ctx: &mut dyn DeserializationContext, value: &MetadataValue, name: &str) {
    expect(
        ctx,
        value,
        DeserializableTypes::STR
            .union(DeserializableTypes::MAP)
            .union(DeserializableTypes::ARRAY)
            .union(DeserializableTypes::NULL),
        name,
    );
    for field in value.members.values().chain(value.elements.iter()) {
        targets(ctx, field, name);
    }
}
fn collect_catalog(
    ctx: &mut dyn DeserializationContext,
    catalogs: &mut CatalogMetadata,
    name: &str,
    path: Vec<String>,
    value: &MetadataValue,
) {
    let mut diagnostics = Vec::new();
    if !matches!(value.value, JsonValue::Object(_)) {
        diagnostics.push(
            DeserializationDiagnostic::new("Catalog must be an object of package version strings.")
                .with_range(value.range),
        );
    }
    for field in value.members.values() {
        if !matches!(field.value, JsonValue::String(_)) {
            diagnostics.push(
                DeserializationDiagnostic::new("Catalog versions must be strings.")
                    .with_range(field.range),
            );
        }
    }
    for diagnostic in diagnostics {
        catalogs
            .diagnostics
            .push(Diagnostic::new(diagnostic.clone()));
        ctx.report(diagnostic);
    }
    catalogs.entries.push(CatalogDeclaration {
        name: name.to_string(),
        path,
        value: value.clone(),
    });
}
fn validate_manifest_field(
    ctx: &mut dyn DeserializationContext,
    name: &str,
    value: &MetadataValue,
) {
    match name {
        "name" | "version" | "main" | "types" | "typings" | "module" => {
            expect(ctx, value, DeserializableTypes::STR, name)
        }
        "scripts"
        | "dependencies"
        | "devDependencies"
        | "peerDependencies"
        | "optionalDependencies"
        | "resolutions" => string_map(ctx, value, name),
        "bin" => {
            expect(
                ctx,
                value,
                DeserializableTypes::STR.union(DeserializableTypes::MAP),
                name,
            );
            for field in value.members.values() {
                expect(ctx, field, DeserializableTypes::STR, name);
            }
        }
        "browser" => {
            expect(
                ctx,
                value,
                DeserializableTypes::STR.union(DeserializableTypes::MAP),
                name,
            );
            for field in value.members.values() {
                expect(
                    ctx,
                    field,
                    DeserializableTypes::STR.union(DeserializableTypes::BOOL),
                    name,
                );
            }
        }
        "imports" => {
            expect(ctx, value, DeserializableTypes::MAP, name);
            for field in value.members.values() {
                targets(ctx, field, name);
            }
        }
        "exports" => targets(ctx, value, name),
        "private" => expect(ctx, value, DeserializableTypes::BOOL, name),
        "workspaces" => {
            expect(
                ctx,
                value,
                DeserializableTypes::ARRAY.union(DeserializableTypes::MAP),
                name,
            );
            if matches!(value.value, JsonValue::Array(_)) {
                string_array(ctx, value, name);
            }
            if let Some(packages) = value.members.get("packages") {
                string_array(ctx, packages, "workspaces.packages");
            }
        }
        "peerDependenciesMeta" => {
            expect(ctx, value, DeserializableTypes::MAP, name);
            for peer in value.members.values() {
                expect(ctx, peer, DeserializableTypes::MAP, name);
                if let Some(optional) = peer.members.get("optional") {
                    expect(ctx, optional, DeserializableTypes::BOOL, "optional");
                }
            }
        }
        "publishConfig" => {
            expect(ctx, value, DeserializableTypes::MAP, name);
            for (key, field) in &value.members {
                validate_manifest_field(ctx, key, field);
            }
            if let Some(directory) = value.members.get("directory") {
                expect(ctx, directory, DeserializableTypes::STR, "directory");
            }
        }
        "typesVersions" => {
            expect(ctx, value, DeserializableTypes::MAP, name);
            for version in value.members.values() {
                expect(ctx, version, DeserializableTypes::MAP, name);
                for field in version.members.values() {
                    string_array(ctx, field, name);
                }
            }
        }
        "pnpm" => {
            expect(ctx, value, DeserializableTypes::MAP, name);
            if let Some(overrides) = value.members.get("overrides") {
                string_map(ctx, overrides, "overrides");
            }
        }
        _ => {}
    }
}
