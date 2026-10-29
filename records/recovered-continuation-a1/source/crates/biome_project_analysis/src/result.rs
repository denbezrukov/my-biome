use crate::{
    coverage::{CapabilityStatus, Coverage},
    inventory::WorkspaceRecord,
    model::{FileId, Location, SnapshotRevision, SourceRecord, WorkspaceId},
    options::AnalysisMode,
};
use biome_diagnostics::Severity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
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

impl IssueKind {
    pub const ALL: [Self; 17] = [
        Self::Files,
        Self::Dependencies,
        Self::DevDependencies,
        Self::OptionalPeerDependencies,
        Self::Unlisted,
        Self::Binaries,
        Self::Unresolved,
        Self::Exports,
        Self::Types,
        Self::NsExports,
        Self::NsTypes,
        Self::Duplicates,
        Self::EnumMembers,
        Self::NamespaceMembers,
        Self::Catalog,
        Self::CatalogReferences,
        Self::Cycles,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectAnalysisResult {
    pub revision: SnapshotRevision,
    pub context: AnalysisContext,
    pub sources: BTreeMap<FileId, SourceRecord>,
    pub workspaces: BTreeMap<WorkspaceId, WorkspaceRecord>,
    pub coverage: Coverage,
    pub capabilities: BTreeMap<IssueKind, CapabilityStatus>,
    pub issues: Vec<ProjectIssue>,
    pub hints: Vec<AnalysisHint>,
    pub evidence: EvidenceGraph,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisContext {
    pub mode: AnalysisMode,
    pub profile_id: String,
    pub semantic_options_digest: String,
    pub scope: Vec<WorkspaceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectIssue {
    pub kind: IssueKind,
    pub severity: Severity,
    pub identity: String,
    pub symbol: Option<String>,
    pub location: Location,
    pub workspace: WorkspaceId,
    pub related_evidence: Vec<EvidenceId>,
    pub eligible_fixes: Vec<FixKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FixKind {
    Files,
    Dependencies,
    Exports,
    Types,
    Catalog,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisHint {
    pub message: String,
    pub location: Option<Location>,
    pub workspace: WorkspaceId,
    pub related_evidence: Vec<EvidenceId>,
}

/// A producer-qualified logical path and range, with a role suffix when needed.
/// Evidence identities are stable across snapshots and must not embed local numeric IDs.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EvidenceId(pub String);

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceGraph {
    pub nodes: BTreeMap<EvidenceId, Evidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub producer: String,
    pub location: Option<Location>,
    pub workspace: WorkspaceId,
    pub explanation: String,
    pub related: Vec<EvidenceId>,
}
