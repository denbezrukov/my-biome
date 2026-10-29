use crate::model::{FileId, Location, WorkspaceId};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    pub files: Vec<FileRecord>,
    pub workspaces: Vec<WorkspaceRecord>,
    pub roots: Vec<Root>,
    pub declarations: Vec<DependencyDeclaration>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileRecord {
    pub file: FileId,
    pub workspace: WorkspaceId,
    pub role: FileRole,
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
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyDeclaration {
    pub package: String,
    pub workspace: WorkspaceId,
    pub location: Location,
    pub kind: DependencyKind,
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
