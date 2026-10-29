//! Acquires owned project inputs without running findings or publishing snapshots.

mod inventory;
mod paths;
mod tsconfig;
pub use inventory::select_workspaces;

use crate::workspace::WorkspaceServerWithDb;
use crate::{LocalWorkspace, WorkspaceError, projects::ProjectKey};
use biome_configuration::project_analysis::ProjectAnalysisConfiguration;
use biome_project_analysis::{inventory::Inventory, options::AnalysisMode};

/// Effective discovery inputs supplied by a service caller; report selection is separate.
#[derive(Clone, Debug, Default)]
pub struct ProjectAnalysisRequest {
    pub configuration: ProjectAnalysisConfiguration,
    pub mode: AnalysisMode,
    pub isolation: Option<AnalysisBoundary>,
}

/// Explicitly excluded consumers make discovery incomplete for their possible targets.
#[derive(Clone, Debug, Default)]
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
