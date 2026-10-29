use crate::{
    WorkspaceError,
    workspace::{ProjectAnalysisResult, PullProjectDiagnosticsParams, WorkspaceServerWithDb},
};

impl WorkspaceServerWithDb<'_> {
    pub(crate) fn project_diagnostics(
        &self,
        params: PullProjectDiagnosticsParams,
    ) -> Result<ProjectAnalysisResult, WorkspaceError> {
        #[cfg(all(feature = "module_graph", feature = "lang_js"))]
        {
            for _ in 0..3 {
                let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                    self.try_project_diagnostics(&params)
                }));
                match result {
                    Ok(Ok(Some(report))) => return Ok(report),
                    Ok(Ok(None)) | Err(salsa::Cancelled::PendingWrite) => {}
                    Ok(Err(error)) => return Err(error),
                    Err(other) => std::panic::resume_unwind(Box::new(other)),
                }
            }
        }
        #[cfg(not(all(feature = "module_graph", feature = "lang_js")))]
        let _ = params;
        Err(WorkspaceError::db_not_available())
    }

    #[cfg(all(feature = "module_graph", feature = "lang_js"))]
    fn try_project_diagnostics(
        &self,
        params: &PullProjectDiagnosticsParams,
    ) -> Result<Option<ProjectAnalysisResult>, WorkspaceError> {
        use super::coordinator::SnapshotBuildOutcome;
        let request = self.normalized_project_request(params)?;
        let snapshot = match self.try_build_project_snapshot_now(params.project_key, &request)? {
            SnapshotBuildOutcome::Published(snapshot) => snapshot,
            SnapshotBuildOutcome::Superseded => return Ok(None),
            SnapshotBuildOutcome::Failed(_) => return Err(WorkspaceError::db_not_available()),
        };
        if !params.selection.workspaces.is_empty()
            && params.selection_revision.as_ref() != Some(&snapshot.revision)
        {
            return Err(WorkspaceError::db_not_available());
        }
        let options =
            request.analysis_options_with_overrides(&snapshot.inventory, &params.overrides);
        let Some(slot) = self.db_state.analysis.report_slot(
            params.project_key,
            &snapshot.revision,
            serde_json::to_string(&options).expect("analysis options"),
        ) else {
            return Ok(None);
        };
        let report = slot.get_or_init(|| {
            #[cfg(test)]
            self.db_state
                .analysis
                .analysis_runs
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            std::sync::Arc::new(biome_project_analysis::analyze(&snapshot, &options))
        });
        #[cfg(test)]
        if let Some(barrier) = self.db_state.analysis.after_analysis.lock().take() {
            barrier.wait();
            barrier.wait();
        }
        let selected = biome_project_analysis::select_report(report, &params.selection);
        // Reacquisition checks disk changes and new inventory members as well as service mutations.
        let current_request = self.normalized_project_request(params)?;
        let current = self.try_build_project_snapshot_now(params.project_key, &current_request)?;
        if !matches!(current, SnapshotBuildOutcome::Published(current) if current.revision == snapshot.revision)
            || !self
                .db_state
                .analysis
                .is_current(params.project_key, &snapshot.revision)
        {
            return Ok(None);
        }
        Ok(Some(selected))
    }

    #[cfg(all(feature = "module_graph", feature = "lang_js"))]
    fn normalized_project_request(
        &self,
        params: &PullProjectDiagnosticsParams,
    ) -> Result<super::ProjectAnalysisRequest, WorkspaceError> {
        let mut request = params.request.clone();
        if let Some(scope) = &mut request.virtual_scope {
            let root = self
                .project_get_path(params.project_key)
                .ok_or_else(WorkspaceError::no_project)?;
            let fs = self.analysis_fs();
            let canonical = |path: &camino::Utf8Path| {
                let path = super::paths::canonical(fs.as_ref(), path)
                    .unwrap_or_else(|_| biome_fs::normalize_path(path));
                // Relative MemoryFS paths share a virtual filesystem root, independent of host cwd.
                if path.is_absolute() {
                    path
                } else {
                    camino::Utf8Path::new("/").join(path)
                }
            };
            let scope_root = canonical(&root);
            let supplied = scope.supplied_files.iter().map(|path| {
                if path.is_absolute() {
                    canonical(path)
                } else {
                    canonical(&super::paths::join(&root, path.as_str()))
                }
            });
            let overlays = self.analysis_overlays();
            scope.supplied_files = supplied
                .chain(overlays.keys().map(|path| canonical(path)))
                .filter_map(|path| {
                    path.strip_prefix(&scope_root)
                        .ok()
                        .filter(|path| !path.as_str().is_empty())
                        .map(camino::Utf8Path::to_path_buf)
                })
                .collect();
            scope.supplied_files.sort();
            scope.supplied_files.dedup();
        }
        Ok(request)
    }
}
