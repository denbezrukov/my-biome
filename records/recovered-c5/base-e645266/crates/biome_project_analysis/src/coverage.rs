use crate::model::{FileId, Location, WorkspaceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum CapabilityStatus {
    Supported,
    Untested,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "status", content = "gaps", rename_all = "camelCase")]
pub enum Coverage {
    Complete,
    Incomplete(Vec<CoverageGap>),
    Failed(Vec<CoverageGap>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CoverageGap {
    pub producer: String,
    pub domain: TargetDomain,
    pub reason: GapReason,
    pub location: Option<Location>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TargetDomain {
    Bounded {
        files: Vec<FileId>,
        workspaces: Vec<WorkspaceId>,
        packages: Vec<String>,
    },
    Unbounded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", content = "reason", rename_all = "camelCase")]
pub enum GapReason {
    Capability(String),
    DynamicReference(String),
    UnavailableInput(String),
}

impl Coverage {
    pub fn gaps(&self) -> &[CoverageGap] {
        match self {
            Self::Complete => &[],
            Self::Incomplete(gaps) | Self::Failed(gaps) => gaps,
        }
    }
}
