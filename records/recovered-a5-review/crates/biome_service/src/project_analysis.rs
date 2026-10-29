//! Acquires and publishes immutable project inputs without running findings.

pub(crate) mod coordinator;
pub use coordinator::ProjectAnalysisState;
mod inventory;
#[cfg(all(feature = "module_graph", feature = "lang_js"))]
mod owned_facts;
mod paths;
mod pinned_fs;
#[cfg(all(feature = "module_graph", feature = "lang_js"))]
mod snapshot;
#[cfg(all(test, feature = "module_graph", feature = "lang_js"))]
#[path = "project_analysis/snapshot.tests.rs"]
mod snapshot_tests;
mod tsconfig;
pub use inventory::select_workspaces;

use crate::workspace::WorkspaceServerWithDb;
use crate::{LocalWorkspace, WorkspaceError, projects::ProjectKey};
use biome_configuration::project_analysis::ProjectAnalysisConfiguration;
use biome_project_analysis::{inventory::Inventory, options::AnalysisMode};

/// Effective discovery inputs supplied by a service caller; report selection is separate.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectAnalysisRequest {
    pub configuration: ProjectAnalysisConfiguration,
    pub mode: AnalysisMode,
    pub isolation: Option<AnalysisBoundary>,
}

/// Explicitly excluded consumers make discovery incomplete for their possible targets.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisBoundary {
    pub excluded_consumers: Vec<String>,
}

impl LocalWorkspace {
    /// Captures filesystem inputs and coverage gaps without establishing a snapshot revision.
    pub fn discover_inventory(
        &self,
        project: ProjectKey,
        request: &ProjectAnalysisRequest,
    ) -> Result<Inventory, WorkspaceError> {
        inventory::discover(self, project, request)
    }
}

impl WorkspaceServerWithDb<'_> {
    /// Captures filesystem inputs and coverage gaps without establishing a snapshot revision.
    pub fn discover_inventory(
        &self,
        project: ProjectKey,
        request: &ProjectAnalysisRequest,
    ) -> Result<Inventory, WorkspaceError> {
        inventory::discover(self, project, request)
    }
}

// Watch paths use the same identity rules as captured acquisition inputs.
pub(crate) fn canonical_watch_path(
    fs: &dyn biome_fs::FileSystem,
    path: &camino::Utf8Path,
) -> camino::Utf8PathBuf {
    paths::canonical(fs, path).unwrap_or_else(|_| biome_fs::normalize_path(path))
}
