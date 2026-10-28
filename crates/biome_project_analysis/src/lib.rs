//! Issue taxonomy for native project analysis. Declaring a category does not implement its check.

use serde::{Deserialize, Serialize};

/// Machine keys from the pinned reference in `tests/fixtures/compatibility.json`.
/// Unknown keys and alternate casing are rejected during deserialization.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum IssueKind {
    #[serde(rename = "files")]
    Files,
    #[serde(rename = "dependencies")]
    Dependencies,
    #[serde(rename = "devDependencies")]
    DevDependencies,
    #[serde(rename = "optionalPeerDependencies")]
    OptionalPeerDependencies,
    #[serde(rename = "unlisted")]
    Unlisted,
    #[serde(rename = "binaries")]
    Binaries,
    #[serde(rename = "unresolved")]
    Unresolved,
    #[serde(rename = "exports")]
    Exports,
    #[serde(rename = "types")]
    Types,
    #[serde(rename = "nsExports")]
    NsExports,
    #[serde(rename = "nsTypes")]
    NsTypes,
    #[serde(rename = "duplicates")]
    Duplicates,
    #[serde(rename = "enumMembers")]
    EnumMembers,
    #[serde(rename = "namespaceMembers")]
    NamespaceMembers,
    #[serde(rename = "catalog")]
    Catalog,
    #[serde(rename = "catalogReferences")]
    CatalogReferences,
    #[serde(rename = "cycles")]
    Cycles,
}

impl IssueKind {
    /// Every reference category in source declaration order, regardless of implementation status.
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
