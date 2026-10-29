use super::{
    ProjectAnalysisRequest,
    coordinator::{ProjectAnalysisState, SnapshotBuildOutcome},
    inventory::{self, Capture},
    owned_facts, paths,
    pinned_fs::PinnedFs,
};
use crate::{
    LocalWorkspace, WorkspaceError,
    projects::ProjectKey,
    scanner::{IndexTrigger, WorkspaceScannerBridge},
    workspace::WorkspaceServerWithDb,
};
use biome_configuration::project_analysis::WorkspaceAnalysisConfiguration;
use biome_fs::{BiomePath, FileSystem};
use biome_module_graph::{
    ModuleDb, ModuleInfoKind, ProjectResolutionContext, ProjectResolutionPurpose,
    project_dependencies,
};
use biome_project_analysis::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    model::{AnalysisSnapshot, Location, SnapshotRevision},
};
use biome_resolver::SourceMapping;
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

impl LocalWorkspace {
    /// Captures completed project inputs. A concurrent mutation returns an unavailable-database error.
    pub fn build_project_snapshot(
        &self,
        project: ProjectKey,
        request: &ProjectAnalysisRequest,
    ) -> Result<Arc<AnalysisSnapshot>, WorkspaceError> {
        self.as_workspace().build_project_snapshot(project, request)
    }
    /// Returns retained state for each requested key, including unknown or closed projects.
    pub fn project_analysis_states(&self, projects: &[ProjectKey]) -> Vec<ProjectAnalysisState> {
        self.as_workspace().project_analysis_states(projects)
    }
}
impl WorkspaceServerWithDb<'_> {
    pub fn project_analysis_states(&self, projects: &[ProjectKey]) -> Vec<ProjectAnalysisState> {
        self.db_state.analysis.observe(projects);
        projects
            .iter()
            .map(|project| self.db_state.analysis.state(*project))
            .collect()
    }
    pub fn build_project_snapshot(
        &self,
        project: ProjectKey,
        request: &ProjectAnalysisRequest,
    ) -> Result<Arc<AnalysisSnapshot>, WorkspaceError> {
        match self.try_build_project_snapshot(project, request)? {
            SnapshotBuildOutcome::Published(snapshot) => Ok(snapshot),
            SnapshotBuildOutcome::Superseded => Err(WorkspaceError::db_not_available()),
            SnapshotBuildOutcome::Failed(coverage) => {
                debug_assert!(matches!(coverage, Coverage::Failed(_)));
                Err(WorkspaceError::db_not_available())
            }
        }
    }
    pub(crate) fn try_build_project_snapshot(
        &self,
        project: ProjectKey,
        request: &ProjectAnalysisRequest,
    ) -> Result<SnapshotBuildOutcome, WorkspaceError> {
        let coordinator = &self.db_state.analysis;
        let Some(token) = coordinator.begin_request(project, digest(request)) else {
            return Ok(SnapshotBuildOutcome::Superseded);
        };
        let Some(mut generation) = coordinator.wait_until_settled(project, token) else {
            return Ok(SnapshotBuildOutcome::Superseded);
        };
        let result: Result<SnapshotBuildOutcome, WorkspaceError> = (|| {
            let root = self
                .project_get_path(project)
                .ok_or_else(WorkspaceError::no_project)?;
            let base_fs = self.analysis_fs();
            let overlays = self
                .analysis_overlays()
                .into_iter()
                .map(|(path, text)| {
                    (
                        paths::canonical(base_fs.as_ref(), &path).unwrap_or(path),
                        text,
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let overlay_paths: Vec<_> = overlays.keys().cloned().collect();
            let pinned = Arc::new(PinnedFs::new(self.analysis_fs(), overlays));
            let (acquisition, key) = self.analysis_acquisition(project, pinned.clone())?;
            let discovery =
                inventory::discover_with_context(&acquisition, key, request, &overlay_paths)?;
            let root = paths::canonical(pinned.as_ref(), &root).unwrap_or(root);
            let discovered_inventory = discovery.inventory.clone();
            let config_digest = digest(&(
                request,
                &discovery.mappings,
                &discovery.inventory,
                self.analysis_settings_identity(project)?,
            ));
            if let Some(cached) = coordinator.cached(project, token, &config_digest)
                && pinned.unchanged()
            {
                return Ok(coordinator.publish(project, token, generation, cached));
            }
            let Some(current_generation) = coordinator.reacquire(project, token, generation) else {
                return Ok(SnapshotBuildOutcome::Superseded);
            };
            generation = current_generation;
            let contexts = contexts(
                &root,
                &discovery.inventory,
                request,
                &discovery.mappings,
                &config_digest,
            );
            let mut capture = Capture {
                fs: pinned.as_ref(),
                root: root.clone(),
                paths: discovery
                    .inventory
                    .sources
                    .iter()
                    .map(|(file, source)| (root.join(&source.logical_path), *file))
                    .collect(),
                inventory: discovery.inventory,
            };
            capture.inventory.reporting_policies = reporting_policies(request, &capture.inventory);
            let mut required: BTreeSet<_> = capture
                .inventory
                .files
                .iter()
                .map(|file| root.join(&capture.inventory.sources[&file.file].logical_path))
                .collect();
            let mut pending = required.clone();
            pending.extend(
                capture
                    .paths
                    .keys()
                    .filter(|path| !paths::is_source(path))
                    .cloned(),
            );
            // Indexing uses the package location; captured identity may follow a file symlink.
            let mut attempted = BTreeMap::new();
            let mut modules = BTreeMap::new();
            for pass in 0..128 {
                if pass == 127 {
                    let coverage = Coverage::Failed(vec![gap(
                        "acquisition",
                        "Dependency acquisition did not reach a fixed point.",
                    )]);
                    return Ok(coordinator.fail(project, token, generation, coverage));
                }
                // Parser source selection reads ProjectLayout, so every batch must
                // index governing manifests before sources, including new dependencies.
                let mut batch = std::mem::take(&mut pending);
                let manifests: Vec<_> = batch
                    .iter()
                    .filter(|path| paths::is_source(path))
                    .flat_map(|path| path.parent().into_iter().flat_map(Utf8Path::ancestors))
                    .map(|directory| directory.join("package.json"))
                    .filter(|path| pinned.path_is_file(path))
                    .collect();
                batch.extend(manifests);
                let (manifests, sources): (Vec<_>, Vec<_>) = batch
                    .into_iter()
                    .partition(|path| BiomePath::new(path.clone()).is_manifest());
                let workspace = acquisition.as_workspace();
                {
                    let epoch = workspace.db_state.scanner_epoch();
                    let scanner = acquisition.with_db_state(epoch.view());
                    for path in manifests.into_iter().chain(sources) {
                        if attempted.contains_key(&path) {
                            continue;
                        }
                        let file = capture.capture(&path);
                        attempted.insert(path.clone(), file);
                        if let Err(error) =
                            scanner.index_file(key, path.clone(), IndexTrigger::InitialScan)
                        {
                            capture.gap(
                                "indexing",
                                error.to_string(),
                                Some(Location {
                                    file,
                                    range: Default::default(),
                                }),
                            );
                        }
                    }
                }
                let mut collected = Vec::new();
                {
                    let db = workspace.get_db();
                    for (path, &file) in &attempted {
                        if paths::is_source(path) {
                            match db.get_parsed_source(path) {
                                Some(parsed) if parsed.has_errors(&*db) => capture.gap(
                                    "parser",
                                    format!("Parse failed for {path}"),
                                    Some(Location {
                                        file,
                                        range: Default::default(),
                                    }),
                                ),
                                None => capture.gap(
                                    "parser",
                                    format!(
                                        "Input is unreadable, oversized, or unsupported: {path}"
                                    ),
                                    Some(Location {
                                        file,
                                        range: Default::default(),
                                    }),
                                ),
                                _ => {}
                            }
                        }
                        if required.contains(path) && !paths::is_source(path) {
                            capture.inventory.gaps.push(CoverageGap {
                                producer: "module format".into(),
                                domain: TargetDomain::Unbounded,
                                reason: GapReason::Capability(format!(
                                    "Unsupported module input {path}"
                                )),
                                location: Some(Location {
                                    file,
                                    range: Default::default(),
                                }),
                            });
                        }
                        let Some(module) = db.module_for_path(path) else {
                            continue;
                        };
                        let ModuleInfoKind::Js(info) = module.kind(&*db) else {
                            continue;
                        };
                        let context = contexts
                            .iter()
                            .filter(|(base, _)| path.starts_with(base))
                            .max_by_key(|(base, _)| base.components().count())
                            .map_or(&contexts[0].1, |(_, context)| context);
                        let resolved = project_dependencies(&*db, path, info, context);
                        for target in resolved.targets.iter() {
                            let target = paths::canonical(pinned.as_ref(), target)
                                .unwrap_or_else(|_| target.clone());
                            required.insert(target.clone());
                            let target_file = capture.capture(&target);
                            if !capture
                                .inventory
                                .files
                                .iter()
                                .any(|record| record.file == target_file)
                                && let Some(owner) = capture
                                    .inventory
                                    .workspaces
                                    .iter()
                                    .filter(|workspace| {
                                        target.starts_with(root.join(&workspace.path))
                                    })
                                    .max_by_key(|workspace| workspace.path.components().count())
                                    .or_else(|| capture.inventory.workspaces.first())
                            {
                                capture.inventory.files.push(
                                    biome_project_analysis::inventory::FileRecord {
                                        file: target_file,
                                        workspace: owner.workspace,
                                        role: biome_project_analysis::inventory::FileRole::Context,
                                        aliases: Vec::new(),
                                        program: false,
                                    },
                                );
                            }
                            if !attempted.contains_key(&target) {
                                pending.insert(target);
                            }
                        }
                        collected.push((file, info.project_facts.clone(), resolved.occurrences));
                    }
                }
                if !pending.is_empty() {
                    continue;
                }
                for (file, facts, resolutions) in collected {
                    modules.insert(
                        file,
                        owned_facts::convert(&mut capture, file, &facts, &resolutions),
                    );
                }
                {
                    let db = workspace.get_db();
                    for target in super::scripts::discover(
                        &mut capture,
                        &modules,
                        &*db,
                        &contexts,
                        request.mode,
                    ) {
                        required.insert(target.clone());
                        if !attempted.contains_key(&target) {
                            pending.insert(target);
                        }
                    }
                }
                {
                    let db = workspace.get_db();
                    for target in super::dependencies::discover(&mut capture, &modules, &*db) {
                        required.insert(target.clone());
                        if !attempted.contains_key(&target) {
                            pending.insert(target);
                        }
                    }
                }
                if !pending.is_empty() {
                    continue;
                }
                {
                    let db = workspace.get_db();
                    let all_contexts: Vec<_> = contexts
                        .iter()
                        .flat_map(|(_, contexts)| contexts.iter().cloned())
                        .collect();
                    for (path, file) in &attempted {
                        if let Some(module) = modules
                            .get_mut(file)
                            .and_then(|module| module.collected.as_mut())
                        {
                            let local_contexts = contexts
                                .iter()
                                .filter(|(base, _)| path.starts_with(base))
                                .max_by_key(|(base, _)| base.components().count())
                                .map_or(&contexts[0].1, |(_, contexts)| contexts);
                            module.export_surfaces = owned_facts::export_surfaces(
                                &mut capture,
                                &*db,
                                path,
                                &all_contexts,
                                local_contexts,
                            );
                        }
                    }
                }
                break;
            }
            let can_reconcile = request
                .isolation
                .as_ref()
                .is_none_or(|boundary| boundary.excluded_consumers.is_empty())
                && !capture
                    .inventory
                    .gaps
                    .iter()
                    .any(|gap| matches!(gap.reason, GapReason::UnavailableInput(_)));
            let (watch_gaps, recovered) =
                coordinator.watch_gaps(project, generation, &root, &capture.paths, can_reconcile);
            capture.inventory.gaps.extend(watch_gaps);
            capture
                .inventory
                .gaps
                .sort_by_cached_key(|gap| serde_json::to_string(gap).unwrap());
            capture.inventory.gaps.dedup();
            let coverage = if capture.inventory.gaps.is_empty() {
                Coverage::Complete
            } else {
                Coverage::Incomplete(capture.inventory.gaps.clone())
            };
            let input_digest = digest(&(&capture.inventory, &modules, &coverage));
            let snapshot = Arc::new(AnalysisSnapshot {
                revision: SnapshotRevision {
                    session_id: format!("{}:{project}", coordinator.session_id),
                    project_root: root.to_string(),
                    epoch: generation,
                    config_digest,
                    input_digest,
                },
                sources: capture.inventory.sources.clone(),
                inventory: capture.inventory,
                modules,
                coverage,
            });
            // Re-enumeration detects new glob members; recorded misses and bytes cover resolver inputs.
            let (validation, validation_key) = self.analysis_acquisition(
                project,
                Arc::new(PinnedFs::new(self.analysis_fs(), pinned.overlays.clone())),
            )?;
            let current = inventory::discover_with_context(
                &validation,
                validation_key,
                request,
                &overlay_paths,
            )?;
            if !pinned.unchanged()
                || current.inventory != discovered_inventory
                || coordinator.generation() != generation
            {
                return Ok(SnapshotBuildOutcome::Superseded);
            }
            #[cfg(test)]
            let publication_barrier = coordinator.before_publish.lock().take();
            #[cfg(test)]
            if let Some(barrier) = publication_barrier {
                barrier.wait();
                barrier.wait();
            }
            let outcome =
                coordinator.publish_reconciled(project, token, generation, snapshot, &recovered);
            if let SnapshotBuildOutcome::Published(snapshot) = &outcome {
                coordinator.remember_inputs(project, token, snapshot.clone(), pinned);
            }
            Ok(outcome)
        })();
        if let Err(error) = &result {
            let coverage = Coverage::Failed(vec![gap("acquisition", &error.to_string())]);
            coordinator.fail(project, token, generation, coverage);
        }
        result
    }
}
fn gap(producer: &str, reason: &str) -> CoverageGap {
    CoverageGap {
        producer: producer.into(),
        domain: TargetDomain::Unbounded,
        reason: GapReason::UnavailableInput(reason.into()),
        location: None,
    }
}
fn digest(value: &impl serde::Serialize) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn contexts(
    root: &Utf8Path,
    inventory: &biome_project_analysis::inventory::Inventory,
    request: &ProjectAnalysisRequest,
    pairs: &[(Utf8PathBuf, Vec<(Utf8PathBuf, Utf8PathBuf)>)],
    revision: &str,
) -> Vec<(Utf8PathBuf, Vec<ProjectResolutionContext>)> {
    let mut result = Vec::new();
    for workspace in &inventory.workspaces {
        let base = root.join(&workspace.path);
        let settings: WorkspaceAnalysisConfiguration =
            serde_json::from_str(&workspace.configuration_json).unwrap();
        let mut mappings = Vec::new();
        for (anchor, aliases) in [
            (base.as_path(), settings.paths.as_ref()),
            (root, request.configuration.paths.as_ref()),
        ]
        .into_iter()
        .filter_map(|(anchor, aliases)| aliases.map(|aliases| (anchor, aliases)))
        {
            for (from, candidates) in aliases {
                for to in &candidates.0 {
                    mappings.push(SourceMapping {
                        from: from.clone(),
                        to: paths::join(anchor, to),
                    });
                }
            }
        }
        if let Some((_, pairs)) = pairs.iter().find(|(path, _)| path == &base) {
            for (output, source) in pairs {
                if output != source {
                    mappings.push(SourceMapping {
                        from: format!("{output}/*"),
                        to: source.join("*"),
                    });
                }
            }
        }
        for mapping in request.configuration.source_mappings.iter().flatten() {
            mappings.push(SourceMapping {
                from: format!("{}/*", paths::join(&base, &mapping.output)),
                to: paths::join(&base, &mapping.source).join("*"),
            });
        }
        let mapping_revision = digest(
            &mappings
                .iter()
                .map(|mapping| (&mapping.from, &mapping.to))
                .collect::<Vec<_>>(),
        );
        result.push((
            base,
            [
                ProjectResolutionPurpose::Runtime,
                ProjectResolutionPurpose::Declaration,
            ]
            .into_iter()
            .map(|purpose| ProjectResolutionContext {
                purpose,
                config_revision: revision.into(),
                mapping_revision: mapping_revision.clone(),
                source_mappings: mappings.clone(),
                ..Default::default()
            })
            .collect(),
        ));
    }
    if result.is_empty() {
        result.push((
            root.into(),
            vec![ProjectResolutionContext {
                config_revision: revision.into(),
                ..Default::default()
            }],
        ));
    }
    result
}

fn reporting_policies(
    request: &ProjectAnalysisRequest,
    inventory: &biome_project_analysis::inventory::Inventory,
) -> Vec<biome_project_analysis::inventory::ReportingPolicy> {
    use biome_project_analysis::inventory::ReportingPolicy;
    let mut policies = vec![ReportingPolicy {
        base: ".".into(),
        workspace: None,
        ignore: request
            .configuration
            .ignore
            .as_ref()
            .map(|patterns| patterns.0.clone())
            .unwrap_or_default(),
        ignore_files: request
            .configuration
            .ignore_files
            .as_ref()
            .map(|patterns| patterns.0.clone())
            .unwrap_or_default(),
    }];
    for workspace in &inventory.workspaces {
        let settings: WorkspaceAnalysisConfiguration =
            serde_json::from_str(&workspace.configuration_json).unwrap();
        policies.push(ReportingPolicy {
            base: workspace.path.clone(),
            workspace: Some(workspace.workspace),
            ignore: settings
                .ignore
                .map(|patterns| patterns.0)
                .unwrap_or_default(),
            ignore_files: settings
                .ignore_files
                .map(|patterns| patterns.0)
                .unwrap_or_default(),
        });
    }
    policies
}
