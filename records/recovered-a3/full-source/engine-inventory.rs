use crate::{
    coverage::CoverageGap,
    model::{FileId, Location, SourceRecord, WorkspaceId},
};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    pub files: Vec<FileRecord>,
    pub workspaces: Vec<WorkspaceRecord>,
    pub roots: Vec<Root>,
    pub declarations: Vec<DependencyDeclaration>,
    #[serde(default)]
    pub sources: BTreeMap<FileId, SourceRecord>,
    #[serde(default)]
    pub gaps: Vec<CoverageGap>,
    #[serde(default)]
    pub request: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileRecord {
    pub file: FileId,
    pub workspace: WorkspaceId,
    pub role: FileRole,
    #[serde(default)]
    pub aliases: Vec<Utf8PathBuf>,
    #[serde(default)]
    pub program: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileRole {
    ReportingCandidate,
    Context,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    pub workspace: WorkspaceId,
    pub path: Utf8PathBuf,
    pub manifest: FileId,
    pub declaration_locations: Vec<Location>,
    pub metadata_diagnostics: Vec<MetadataDiagnostic>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub metadata: Vec<(String, MetadataValueRecord)>,
    #[serde(default)]
    pub catalogs: Vec<CatalogProvider>,
    #[serde(default)]
    pub dependency_uses: Vec<ManifestDependencyUse>,
    #[serde(default)]
    pub configuration: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MetadataDiagnostic {
    pub message: String,
    pub location: Location,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Root {
    pub file: FileId,
    pub workspace: WorkspaceId,
    pub producer: String,
    pub location: Option<Location>,
    pub role: RootRole,
    pub exports: ExportProtection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RootRole {
    Default,
    Production,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportProtection {
    Protected,
    IncludeWhenRequested,
    Unprotected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyDeclaration {
    pub package: String,
    pub workspace: WorkspaceId,
    pub location: Location,
    pub kind: DependencyKind,
    #[serde(default)]
    pub specifier: String,
    #[serde(default)]
    pub field: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DependencyKind {
    Production,
    Development,
    Peer,
    OptionalPeer,
    Optional,
    Catalog,
}

/// Ordered metadata and ranges belong to the captured source, including inactive branches.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MetadataValueRecord {
    pub value: serde_json::Value,
    pub location: Location,
    pub members: Vec<(String, Self)>,
    pub elements: Vec<Self>,
}

/// The container used by catalog lookup can differ from the selected value's source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogProvider {
    pub name: String,
    pub container: Location,
    pub value: MetadataValueRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManifestDependencyUse {
    pub package: String,
    pub specifier: String,
    pub producer: String,
    pub location: Option<Location>,
}
