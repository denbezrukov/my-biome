use biome_deserialize::{
    Deserializable, DeserializableType, DeserializableValidator, DeserializableValue,
    DeserializationContext, TextRange,
};
use biome_deserialize_macros::{Deserializable, Merge};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_ENTRIES: &[&str] = &[
    "{index,cli,main}.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!",
    "src/{index,cli,main}.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!",
];
pub const DEFAULT_PROJECT: &[&str] = &["**/*.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!"];

/// A string or list of strings, normalized to a list during deserialization.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AnalysisStrings(
    #[cfg_attr(feature = "schema", schemars(with = "StringOrStrings"))] pub Vec<String>,
);

#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
enum StringOrStrings {
    String(String),
    Strings(Vec<String>),
}

impl<'de> Deserialize<'de> for AnalysisStrings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(match StringOrStrings::deserialize(deserializer)? {
            StringOrStrings::String(s) => vec![s],
            StringOrStrings::Strings(s) => s,
        }))
    }
}
impl biome_deserialize::Merge for AnalysisStrings {
    fn merge_with(&mut self, other: Self) {
        *self = other;
    }
}
impl Deserializable for AnalysisStrings {
    fn deserialize(
        ctx: &mut dyn DeserializationContext,
        value: &impl DeserializableValue,
        name: &str,
    ) -> Option<Self> {
        if value.visitable_type()? == DeserializableType::Str {
            <String as Deserializable>::deserialize(ctx, value, name).map(|s| Self(vec![s]))
        } else {
            <Vec<String> as Deserializable>::deserialize(ctx, value, name).map(Self)
        }
    }
}

/// Selects the declared analysis mode. Parsing this setting does not run analysis.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Deserializable, Merge,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ProjectAnalysisMode {
    #[default]
    Default,
    Production,
    Strict,
}

/// Separates glob negation from the trailing production marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPattern {
    pub pattern: String,
    pub negated: bool,
    pub production: bool,
}
impl ProjectPattern {
    pub fn new(value: &str) -> Self {
        let negated = value.starts_with('!');
        let value = if negated { &value[1..] } else { value };
        let production = value.ends_with('!');
        let pattern = if production {
            &value[..value.len() - 1]
        } else {
            value
        }
        .to_string();
        Self {
            pattern,
            negated,
            production,
        }
    }
    pub fn applies_to(&self, mode: ProjectAnalysisMode) -> bool {
        mode == ProjectAnalysisMode::Default || self.production
    }
}

/// Declarative project analysis inputs. Native analysis support is tracked separately from schema support.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Deserializable, Merge)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ProjectAnalysisConfiguration {
    /// Enables project analysis inputs. Defaults to false; native analysis integration is pending.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Optional schema URL for migrated static configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "$schema")]
    pub schema: Option<String>,
    /// Entry globs. Explicit values replace the default production entry globs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<AnalysisStrings>,
    /// Candidate globs. Explicit values replace the default production project glob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<AnalysisStrings>,
    /// Static import aliases, with one or more targets per alias.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<IndexMap<String, AnalysisStrings>>,
    /// Suppresses all findings for matching paths without excluding graph inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<AnalysisStrings>,
    /// Suppresses unused-file findings only; matching files remain graph inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_files: Option<AnalysisStrings>,
    /// Executable names or patterns exempted from binary findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_binaries: Option<AnalysisStrings>,
    /// Accepts known global executable names. Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_global_binaries: Option<bool>,
    /// Dependency names or patterns exempted from dependency findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_dependencies: Option<AnalysisStrings>,
    /// Member names or patterns exempted from member findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_members: Option<AnalysisStrings>,
    /// Import specifiers or patterns exempted from unresolved findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_unresolved: Option<AnalysisStrings>,
    /// Credits local export uses for all or selected symbol kinds. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_exports_used_in_file: Option<LocalExportUses>,
    /// Suppresses specified issue kinds for each path glob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_issues: Option<BTreeMap<String, Vec<ProjectIssueKind>>>,
    /// Includes ordinary entry exports in findings. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_entry_exports: Option<bool>,
    /// Per-workspace static settings, keyed by workspace path or glob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<IndexMap<String, WorkspaceAnalysisConfiguration>>,
    /// Explicit source and output directory pairs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_mappings: Option<Vec<SourceMapping>>,
    /// Explicit dependency uses with a workspace and a reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_uses: Option<Vec<DependencyUse>>,
    /// Entry paths whose exported API is public.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_entries: Option<AnalysisStrings>,
    /// Analysis mode. Defaults to default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ProjectAnalysisMode>,
    /// Per-category severity: error, warn or off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<BTreeMap<ProjectIssueKind, ProjectIssueSeverity>>,
    /// Issue categories selected for reporting after analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<ProjectIssueKind>>,
    /// Issue categories excluded from reporting after analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<ProjectIssueKind>>,
    /// Ordered workspace discovery exclusions; prefix ! re-includes a workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_workspaces: Option<AnalysisStrings>,
    /// Export tag selection, with + to include and - to exclude.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<AnalysisStrings>,
    /// Static runtime cycle policies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycles: Option<ProjectCycleConfiguration>,
    /// Counts configuration hints as errors. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treat_config_hints_as_errors: Option<bool>,
    /// Counts tag hints as errors. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treat_tag_hints_as_errors: Option<bool>,
    /// Uses tsconfig files/include/exclude and references as declared program inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_tsconfig_files: Option<bool>,
    /// Explicit tsconfig paths; configuration execution is unsupported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tsconfig: Option<AnalysisStrings>,
}

/// Workspace settings supported by the pinned static compatibility profile.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Deserializable, Merge)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct WorkspaceAnalysisConfiguration {
    /// Entry globs. Explicit values replace the default production entry globs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<AnalysisStrings>,
    /// Candidate globs. Explicit values replace the default production project glob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<AnalysisStrings>,
    /// Static import aliases, with one or more targets per alias.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<IndexMap<String, AnalysisStrings>>,
    /// Suppresses all findings for matching paths without excluding graph inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<AnalysisStrings>,
    /// Suppresses unused-file findings only; matching files remain graph inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_files: Option<AnalysisStrings>,
    /// Executable names or patterns exempted from binary findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_binaries: Option<AnalysisStrings>,
    /// Accepts known global executable names. Defaults to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_global_binaries: Option<bool>,
    /// Dependency names or patterns exempted from dependency findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_dependencies: Option<AnalysisStrings>,
    /// Member names or patterns exempted from member findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_members: Option<AnalysisStrings>,
    /// Import specifiers or patterns exempted from unresolved findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_unresolved: Option<AnalysisStrings>,
    /// Credits local export uses for all or selected symbol kinds. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_exports_used_in_file: Option<LocalExportUses>,
    /// Suppresses specified issue kinds for each path glob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_issues: Option<BTreeMap<String, Vec<ProjectIssueKind>>>,
    /// Includes ordinary entry exports in findings. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_entry_exports: Option<bool>,
}

impl ProjectAnalysisConfiguration {
    pub fn entry_patterns(&self) -> Vec<String> {
        self.entry.as_ref().map_or_else(
            || DEFAULT_ENTRIES.iter().map(|v| (*v).into()).collect(),
            |v| v.0.clone(),
        )
    }
    pub fn project_patterns(&self) -> Vec<String> {
        self.project.as_ref().map_or_else(
            || DEFAULT_PROJECT.iter().map(|v| (*v).into()).collect(),
            |v| v.0.clone(),
        )
    }
    pub fn normalized_entries(&self) -> Vec<ProjectPattern> {
        self.entry_patterns()
            .iter()
            .map(|p| ProjectPattern::new(p))
            .collect()
    }
}

impl WorkspaceAnalysisConfiguration {
    pub fn entry_patterns(&self) -> Vec<String> {
        self.entry.as_ref().map_or_else(
            || DEFAULT_ENTRIES.iter().map(|v| (*v).into()).collect(),
            |v| v.0.clone(),
        )
    }
    pub fn project_patterns(&self) -> Vec<String> {
        self.project.as_ref().map_or_else(
            || DEFAULT_PROJECT.iter().map(|v| (*v).into()).collect(),
            |v| v.0.clone(),
        )
    }
    pub fn normalized_entries(&self) -> Vec<ProjectPattern> {
        self.entry_patterns()
            .iter()
            .map(|p| ProjectPattern::new(p))
            .collect()
    }
}

/// Maps generated output back to its source directory.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Deserializable, Merge)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMapping {
    /// Source directory relative to the workspace.
    #[deserializable(required, validate = "biome_deserialize::non_empty")]
    pub source: String,
    /// Output directory relative to the workspace.
    #[deserializable(required, validate = "biome_deserialize::non_empty")]
    pub output: String,
}

/// Credits a dependency used outside statically collected references.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Deserializable)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[deserializable(with_validator)]
pub struct DependencyUse {
    /// Package credited by this declaration.
    #[deserializable(required, validate = "biome_deserialize::non_empty")]
    pub package: String,
    /// Workspace owning the declaration.
    #[deserializable(required, validate = "biome_deserialize::non_empty")]
    pub workspace: String,
    /// Why the package is used outside statically collected references.
    #[deserializable(required, validate = "biome_deserialize::non_empty")]
    pub reason: String,
    #[serde(skip)]
    #[deserializable(skip)]
    pub location: Option<ConfigurationLocation>,
}
/// Source identity and byte range of a declaration before configuration merging.
/// The source is unavailable when the deserialization caller supplies no identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigurationLocation {
    pub source: Option<String>,
    pub range: TextRange,
}

impl DeserializableValidator for DependencyUse {
    fn validate(
        &mut self,
        ctx: &mut dyn DeserializationContext,
        _name: &str,
        range: TextRange,
    ) -> bool {
        self.location = Some(ConfigurationLocation {
            source: ctx.id().filter(|id| !id.is_empty()).map(str::to_owned),
            range,
        });
        true
    }
}

/// Runtime cycle policies; dynamic imports are excluded by default.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Deserializable, Merge)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ProjectCycleConfiguration {
    /// Includes runtime cycles through dynamic import expressions.
    pub dynamic_imports: Option<bool>,
    /// Allowed root-relative cycle paths, without the repeated closing vertex.
    pub allow: Option<Vec<Vec<String>>>,
}

/// Local export use credit, either for every symbol kind or selected kinds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Merge)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum LocalExportUses {
    All(bool),
    Kinds(BTreeMap<LocalSymbolKind, bool>),
}
impl Deserializable for LocalExportUses {
    fn deserialize(
        ctx: &mut dyn DeserializationContext,
        value: &impl DeserializableValue,
        name: &str,
    ) -> Option<Self> {
        if value.visitable_type()? == DeserializableType::Bool {
            <bool as Deserializable>::deserialize(ctx, value, name).map(Self::All)
        } else {
            <BTreeMap<LocalSymbolKind, bool> as Deserializable>::deserialize(ctx, value, name)
                .map(Self::Kinds)
        }
    }
}

/// Symbol kinds accepted by local export use policies.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserializable,
    Merge,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum LocalSymbolKind {
    Class,
    Enum,
    Function,
    Interface,
    Member,
    Namespace,
    Type,
    Variable,
}

/// Severity for a project issue category.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserializable,
    Merge,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ProjectIssueSeverity {
    Error,
    Warn,
    Off,
}

/// The 17 issue category keys in the pinned compatibility profile.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserializable,
    Merge,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ProjectIssueKind {
    Files,
    Dependencies,
    DevDependencies,
    OptionalPeerDependencies,
    Unlisted,
    Binaries,
    Unresolved,
    Exports,
    Types,
    NsExports,
    NsTypes,
    Duplicates,
    EnumMembers,
    NamespaceMembers,
    Catalog,
    CatalogReferences,
    Cycles,
}
