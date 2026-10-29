use crate::{
    coverage::{Coverage, TargetDomain},
    inventory::Inventory,
};
use biome_text_size::TextRange;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct FileId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub u32);

/// Freshness is scoped to a service session and project, including close/reopen epochs.
/// Digests are lowercase SHA-256 hex strings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotRevision {
    pub session_id: String,
    pub project_root: String,
    pub epoch: u64,
    pub config_digest: String,
    pub input_digest: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Location {
    pub file: FileId,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct DeclarationId {
    pub location: Location,
    pub namespace: SymbolNamespace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolNamespace {
    Value,
    Type,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSnapshot {
    pub revision: SnapshotRevision,
    pub inventory: Inventory,
    pub modules: BTreeMap<FileId, ModuleFacts>,
    pub sources: BTreeMap<FileId, SourceRecord>,
    pub coverage: Coverage,
}

/// Text and paths belong to the snapshot; consumers must not reread the current disk.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRecord {
    pub logical_path: Utf8PathBuf,
    pub canonical_uri: String,
    pub content: SourceContent,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SourceContent {
    Available {
        digest: String,
        text: String,
    },
    Unavailable {
        reason: String,
        fallback: Utf8PathBuf,
    },
}

/// Adapter facts retain syntax identities and occurrences, without database references.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModuleFacts {
    pub edges: Vec<ModuleEdge>,
    pub declarations: Vec<DeclarationId>,
    pub exports: Vec<ExportSlot>,
    pub uses: Vec<SymbolUse>,
    pub tags: Vec<Tag>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModuleEdge {
    pub location: Location,
    pub specifier: String,
    pub kind: EdgeKind,
    pub target: EdgeTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKind {
    Runtime,
    Type,
    Dynamic,
    CommonJs,
    ReExport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum EdgeTarget {
    Resolved {
        file: FileId,
    },
    External {
        package: String,
    },
    Failed {
        reason: String,
    },
    Unknown {
        domain: TargetDomain,
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportSlot {
    pub name: String,
    pub declaration: DeclarationId,
    pub location: Location,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolUse {
    pub declaration: DeclarationId,
    pub location: Location,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub location: Location,
}
