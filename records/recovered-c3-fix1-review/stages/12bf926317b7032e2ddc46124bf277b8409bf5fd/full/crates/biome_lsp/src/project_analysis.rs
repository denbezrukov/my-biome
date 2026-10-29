use crate::{documents::Document, session::Session};
use biome_configuration::{ConfigurationSource, project_analysis::ProjectAnalysisMode};
use biome_diagnostics::Severity;
use biome_line_index::LineIndex;
use biome_lsp_converters::{PositionEncoding, to_proto};
use biome_project_analysis::{
    coverage::GapReason,
    model::{Location, SnapshotRevision, SourceContent},
    options::AnalysisMode,
    result::ProjectAnalysisResult,
};
use biome_service::{
    Workspace,
    project_analysis::ProjectAnalysisRequest,
    projects::ProjectKey,
    workspace::{
        CloseFileParams, ProjectAnalysisState, ProjectAnalysisStatus,
        PullProjectAnalysisStatesParams, PullProjectDiagnosticsParams,
    },
};
use parking_lot::Mutex;
use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::Mutex as AsyncMutex;
use tower_lsp_server::ls_types::{self as lsp, Diagnostic, Uri};

#[derive(Default)]
pub(crate) struct ProjectDiagnostics {
    requests: Mutex<FxHashMap<ProjectKey, (u64, ProjectAnalysisRequest)>>,
    observed: Mutex<FxHashMap<ProjectKey, ProjectAnalysisState>>,
    published: Mutex<FxHashMap<ProjectKey, SnapshotRevision>>,
    uris: Mutex<FxHashMap<ProjectKey, FxHashSet<Uri>>>,
    slots: Mutex<FxHashMap<Uri, Arc<AsyncMutex<UriDiagnostics>>>>,
    jobs: Mutex<FxHashMap<ProjectKey, bool>>,
    sequence: AtomicU64,
    announced: AtomicBool,
    #[cfg(test)]
    pub(crate) hooks: crate::project_analysis_tests::PublicationHooks,
}

#[derive(Default)]
struct UriDiagnostics {
    local_sequence: u64,
    local_document: Option<(i32, Arc<str>)>,
    local: Vec<Diagnostic>,
    projects: FxHashMap<ProjectKey, (SnapshotRevision, Vec<Diagnostic>)>,
}

impl ProjectDiagnostics {
    fn slot(&self, uri: &Uri) -> Arc<AsyncMutex<UriDiagnostics>> {
        self.slots.lock().entry(uri.clone()).or_default().clone()
    }

    pub(crate) fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn request(&self, project: ProjectKey) -> Option<(u64, ProjectAnalysisRequest)> {
        self.requests.lock().get(&project).cloned()
    }
}

pub(crate) fn configured_request(source: &ConfigurationSource) -> Option<ProjectAnalysisRequest> {
    let configuration = source.resolve().project_analysis?;
    if configuration.enabled != Some(true) {
        return None;
    }
    let mode = match configuration.mode.unwrap_or_default() {
        ProjectAnalysisMode::Default => AnalysisMode::Default,
        ProjectAnalysisMode::Production => AnalysisMode::Production,
        ProjectAnalysisMode::Strict => AnalysisMode::Strict,
    };
    Some(ProjectAnalysisRequest::from_configuration_source(
        source, mode,
    ))
}

pub(crate) fn configured_request_with_inline(
    source: &ConfigurationSource,
    inline: Option<biome_configuration::Configuration>,
) -> Option<ProjectAnalysisRequest> {
    let mut source = source.clone();
    if let Some(inline) = inline.filter(|configuration| configuration.project_analysis.is_some()) {
        if let Some(root) = source.root.take() {
            let mut extended = source.extended_configurations.as_slice().to_vec();
            extended.push(biome_configuration::ExtendedConfiguration {
                specifier: None,
                source: root,
            });
            source.extended_configurations = extended.into();
        }
        source.root = Some(biome_configuration::ConfigurationSourceEntry {
            configuration: Some(inline),
            file_path: None,
            file_source: None,
        });
    }
    configured_request(&source)
}

impl Session {
    pub(crate) fn set_project_analysis_request(
        &self,
        project: ProjectKey,
        request: Option<ProjectAnalysisRequest>,
    ) {
        let mut requests = self.project_diagnostics.requests.lock();
        if let Some(request) = request {
            requests.insert(project, (self.project_diagnostics.next_sequence(), request));
        } else {
            requests.remove(&project);
        }
        self.project_diagnostics.published.lock().remove(&project);
    }

    fn project_is_current(&self, project: ProjectKey, revision: &SnapshotRevision) -> bool {
        self.project_diagnostics.request(project).is_some()
            && self.workspace().pull_project_analysis_states(PullProjectAnalysisStatesParams {
                project_keys: vec![project],
            }).is_ok_and(|states| states.iter().any(|state| {
                state.project_key == project && matches!(&state.status, ProjectAnalysisStatus::Completed { revision: current } if current == revision)
            }))
    }

    fn prune_project_slots(&self, slots: &mut UriDiagnostics) {
        slots
            .projects
            .retain(|project, (revision, _)| self.project_is_current(*project, revision));
    }

    async fn send_merged(&self, uri: Uri, slots: &mut UriDiagnostics) {
        self.prune_project_slots(slots);
        let document = self.document(&uri);
        if !slots
            .local_document
            .as_ref()
            .is_some_and(|(version, text)| {
                document.as_ref().is_some_and(|current| {
                    current.version == *version && Arc::ptr_eq(&current.text, text)
                })
            })
        {
            slots.local.clear();
            slots.local_document = None;
        }
        let mut diagnostics = slots.local.clone();
        let mut project_diagnostics: Vec<_> = slots
            .projects
            .values()
            .flat_map(|(_, diagnostics)| diagnostics.clone())
            .collect();
        project_diagnostics
            .sort_by_cached_key(|diagnostic| serde_json::to_string(diagnostic).unwrap_or_default());
        diagnostics.extend(project_diagnostics);
        self.client
            .publish_diagnostics(
                uri.clone(),
                diagnostics,
                document.map(|document| document.version),
            )
            .await;
    }

    pub(crate) async fn publish_local_diagnostics(
        &self,
        uri: Uri,
        document: &Document,
        sequence: u64,
        diagnostics: Vec<Diagnostic>,
    ) {
        #[cfg(test)]
        self.project_diagnostics.hooks.before_local.wait().await;
        let slot = self.project_diagnostics.slot(&uri);
        let mut slots = slot.lock().await;
        if sequence < slots.local_sequence
            || !self.document(&uri).is_some_and(|current| {
                current.version == document.version && Arc::ptr_eq(&current.text, &document.text)
            })
        {
            return;
        }
        slots.local_sequence = sequence;
        slots.local_document = Some((document.version, document.text.clone()));
        slots.local = diagnostics;
        self.send_merged(uri, &mut slots).await;
    }

    /// Reconciles retained states because watch notifications can coalesce across projects.
    pub(crate) async fn reconcile_project_diagnostics(self: &Arc<Self>, schedule: bool) {
        #[cfg(test)]
        self.project_diagnostics.hooks.before_reconcile.wait().await;
        let mut keys: FxHashSet<_> = self
            .project_diagnostics
            .requests
            .lock()
            .keys()
            .copied()
            .collect();
        keys.extend(self.project_diagnostics.observed.lock().keys().copied());
        keys.extend(self.project_diagnostics.uris.lock().keys().copied());
        let Ok(states) =
            self.workspace()
                .pull_project_analysis_states(PullProjectAnalysisStatesParams {
                    project_keys: keys.into_iter().collect(),
                })
        else {
            return;
        };
        for state in states {
            let project = state.project_key;
            let previous = self
                .project_diagnostics
                .observed
                .lock()
                .insert(project, state.clone());
            let changed = previous.as_ref() != Some(&state);
            if matches!(state.status, ProjectAnalysisStatus::Closed) {
                self.set_project_analysis_request(project, None);
                self.forget_project(project);
            }
            let uris = self
                .project_diagnostics
                .uris
                .lock()
                .get(&project)
                .cloned()
                .unwrap_or_default();
            for uri in uris {
                let slot = self.project_diagnostics.slot(&uri);
                let mut slots = slot.lock().await;
                let before = slots.projects.len();
                self.prune_project_slots(&mut slots);
                if slots.projects.len() != before {
                    self.send_merged(uri, &mut slots).await;
                }
            }
            if changed && let ProjectAnalysisStatus::Failed { coverage } = &state.status {
                self.client
                    .log_message(
                        lsp::MessageType::WARNING,
                        format!("Project analysis failed; coverage is incomplete: {coverage:?}"),
                    )
                    .await;
            }
            let needs_result = match &state.status {
                ProjectAnalysisStatus::Dirty => true,
                ProjectAnalysisStatus::Completed { revision } => {
                    self.project_diagnostics.published.lock().get(&project) != Some(revision)
                }
                _ => false,
            };
            if schedule && needs_result && self.project_diagnostics.request(project).is_some() {
                self.schedule_project_analysis(project);
            }
        }
    }

    fn schedule_project_analysis(self: &Arc<Self>, project: ProjectKey) {
        {
            let mut jobs = self.project_diagnostics.jobs.lock();
            if let Some(pending) = jobs.get_mut(&project) {
                *pending = true;
                return;
            }
            jobs.insert(project, false);
        }
        let session = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                #[cfg(test)]
                session
                    .project_diagnostics
                    .hooks
                    .before_analysis
                    .wait()
                    .await;
                if let Some((sequence, request)) = session.project_diagnostics.request(project) {
                    let documents = session.open_documents();
                    let worker = session.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        worker.pull_project_result(project, request)
                    })
                    .await;
                    if let Ok(Some(result)) = result
                        && session
                            .project_diagnostics
                            .request(project)
                            .is_some_and(|(current, _)| current == sequence)
                        && session.project_documents_current(&result, &documents)
                    {
                        session.publish_project_result(project, result).await;
                    }
                }
                let mut jobs = session.project_diagnostics.jobs.lock();
                if jobs.get(&project) == Some(&true) {
                    jobs.insert(project, false);
                } else {
                    jobs.remove(&project);
                    break;
                }
            }
        });
    }

    fn pull_project_result(
        &self,
        project: ProjectKey,
        request: ProjectAnalysisRequest,
    ) -> Option<ProjectAnalysisResult> {
        salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            self.workspace()
                .pull_project_diagnostics(PullProjectDiagnosticsParams {
                    project_key: project,
                    request,
                    selection: Default::default(),
                    selection_revision: None,
                    overrides: Default::default(),
                })
        }))
        .ok()
        .and_then(Result::ok)
    }

    fn project_documents_current(
        &self,
        result: &ProjectAnalysisResult,
        documents: &[(Uri, Document)],
    ) -> bool {
        result.sources.values().all(|source| {
            let Ok(uri) = source.canonical_uri.parse::<Uri>() else { return false; };
            match (documents.iter().find(|(key, _)| *key == uri), self.document(&uri)) {
                (None, None) => true,
                (Some((_, before)), Some(current)) => before.version == current.version && Arc::ptr_eq(&before.text, &current.text)
                    && matches!(&source.content, SourceContent::Available { text, .. } if text.as_str() == current.text.as_ref()),
                _ => false,
            }
        })
    }

    /// Publishes only while the complete service revision is still current.
    pub(crate) async fn publish_project_result(
        &self,
        project: ProjectKey,
        result: ProjectAnalysisResult,
    ) {
        #[cfg(test)]
        self.project_diagnostics.hooks.before_project.wait().await;
        if !self.project_is_current(project, &result.revision) {
            return;
        }
        if !self
            .project_diagnostics
            .announced
            .swap(true, Ordering::Relaxed)
        {
            self.client.log_message(lsp::MessageType::INFO, "Project analysis is experimental and partial. Framework and tool plugins are not supported.").await;
        }
        let mut projected = project_diagnostics(&result, self.position_encoding());
        let mut uris: FxHashSet<_> = projected.keys().cloned().collect();
        {
            let mut known = self.project_diagnostics.uris.lock();
            let previous = known.entry(project).or_default();
            uris.extend(previous.iter().cloned());
            previous.extend(uris.iter().cloned());
        }
        for uri in uris {
            let slot = self.project_diagnostics.slot(&uri);
            let mut slots = slot.lock().await;
            if !self.project_is_current(project, &result.revision) {
                return;
            }
            if let Some(diagnostics) = projected.remove(&uri) {
                slots
                    .projects
                    .insert(project, (result.revision.clone(), diagnostics));
            } else {
                slots.projects.remove(&project);
            }
            self.send_merged(uri, &mut slots).await;
        }
        if self.project_is_current(project, &result.revision) {
            self.project_diagnostics
                .published
                .lock()
                .insert(project, result.revision);
        }
    }

    pub(crate) async fn close_document_diagnostics(
        self: &Arc<Self>,
        uri: Uri,
    ) -> Result<(), biome_service::WorkspaceError> {
        let slot = self.project_diagnostics.slot(&uri);
        let mut slots = slot.lock().await;
        let Some(document) = self.document(&uri) else {
            return Ok(());
        };
        let path = self
            .file_path(&uri)
            .map_err(|_| biome_service::WorkspaceError::db_not_available())?;
        let unchanged = self
            .workspace()
            .fs()
            .read_file_from_path(&path)
            .is_ok_and(|text| text.as_str() == document.text.as_ref());
        self.close_diagnostics(&uri);
        self.remove_document(&uri);
        let closed = self.workspace().close_file(CloseFileParams {
            project_key: document.project_key,
            path,
        });
        slots.local_sequence = self.project_diagnostics.next_sequence();
        slots.local.clear();
        slots.local_document = None;
        let mut completed = Vec::new();
        if unchanged && closed.is_ok() {
            #[cfg(test)]
            self.project_diagnostics
                .hooks
                .before_close_analysis
                .wait()
                .await;
            let mut projects: FxHashSet<_> = slots.projects.keys().copied().collect();
            projects.insert(document.project_key);
            for project in projects {
                if let Some((sequence, request)) = self.project_diagnostics.request(project) {
                    let documents = self.open_documents();
                    let session = self.clone();
                    if let Ok(Some(result)) = tokio::task::spawn_blocking(move || {
                        session.pull_project_result(project, request)
                    })
                    .await
                        && self
                            .project_diagnostics
                            .request(project)
                            .is_some_and(|(current, _)| current == sequence)
                        && self.project_documents_current(&result, &documents)
                        && self.project_is_current(project, &result.revision)
                    {
                        let mut projected = project_diagnostics(&result, self.position_encoding());
                        if let Some(diagnostics) = projected.remove(&uri) {
                            slots
                                .projects
                                .insert(project, (result.revision.clone(), diagnostics));
                        } else {
                            slots.projects.remove(&project);
                        }
                        completed.push((project, result));
                    }
                }
            }
        }
        self.send_merged(uri, &mut slots).await;
        drop(slots);
        for (project, result) in completed {
            self.publish_project_result(project, result).await;
        }
        self.reconcile_project_diagnostics(true).await;
        closed
    }
}

fn source_location(
    result: &ProjectAnalysisResult,
    location: Location,
    encoding: PositionEncoding,
) -> Option<lsp::Location> {
    let source = result.sources.get(&location.file)?;
    let uri = source.canonical_uri.parse().ok()?;
    let range = match &source.content {
        SourceContent::Available { text, .. } => {
            let start = usize::from(location.range.start());
            let end = usize::from(location.range.end());
            if text.get(start..end).is_some() {
                to_proto::range(&LineIndex::new(text), location.range, encoding).unwrap_or_default()
            } else {
                lsp::Range::default()
            }
        }
        SourceContent::Unavailable { .. } => lsp::Range::default(),
    };
    Some(lsp::Location { uri, range })
}

fn severity(severity: Severity) -> lsp::DiagnosticSeverity {
    match severity {
        Severity::Fatal | Severity::Error => lsp::DiagnosticSeverity::ERROR,
        Severity::Warning => lsp::DiagnosticSeverity::WARNING,
        Severity::Information => lsp::DiagnosticSeverity::INFORMATION,
        Severity::Hint => lsp::DiagnosticSeverity::HINT,
    }
}

pub(crate) fn project_diagnostics(
    result: &ProjectAnalysisResult,
    encoding: PositionEncoding,
) -> FxHashMap<Uri, Vec<Diagnostic>> {
    let mut diagnostics: FxHashMap<Uri, Vec<Diagnostic>> = FxHashMap::default();
    let fallback = |workspace| {
        result
            .workspaces
            .get(&workspace)
            .or_else(|| result.workspaces.values().next())
            .map(|workspace| Location {
                file: workspace.manifest,
                range: Default::default(),
            })
    };
    for issue in &result.issues {
        let Some(location) = source_location(result, issue.location, encoding) else {
            continue;
        };
        let kind = serde_json::to_value(issue.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let related = related_information(result, &issue.related_evidence, encoding);
        let mut diagnostic = Diagnostic::new(
            location.range,
            Some(severity(issue.severity)),
            Some(lsp::NumberOrString::String(format!("project/{kind}"))),
            Some("biome".into()),
            format!("{kind}: {}", issue.identity),
            related,
            None,
        );
        diagnostic.data = Some(
            serde_json::json!({"revision": result.revision, "identity": issue.identity, "kind": issue.kind}),
        );
        diagnostics
            .entry(location.uri)
            .or_default()
            .push(diagnostic);
    }
    for hint in &result.hints {
        if let Some(location) = hint
            .location
            .or_else(|| fallback(hint.workspace))
            .and_then(|location| source_location(result, location, encoding))
        {
            diagnostics
                .entry(location.uri)
                .or_default()
                .push(Diagnostic::new(
                    location.range,
                    Some(severity(hint.severity)),
                    Some(lsp::NumberOrString::String("project/hint".into())),
                    Some("biome".into()),
                    hint.message.clone(),
                    related_information(result, &hint.related_evidence, encoding),
                    None,
                ));
        }
    }
    for gap in result.coverage.gaps() {
        let location = gap.location.or_else(|| {
            result.workspaces.values().next().map(|workspace| Location {
                file: workspace.manifest,
                range: Default::default(),
            })
        });
        if let Some(location) =
            location.and_then(|location| source_location(result, location, encoding))
        {
            let (GapReason::Capability(reason)
            | GapReason::DynamicReference(reason)
            | GapReason::UnavailableInput(reason)) = &gap.reason;
            diagnostics
                .entry(location.uri)
                .or_default()
                .push(Diagnostic::new(
                    location.range,
                    Some(lsp::DiagnosticSeverity::WARNING),
                    Some(lsp::NumberOrString::String("project/coverage".into())),
                    Some("biome".into()),
                    format!(
                        "Project analysis is incomplete ({}): {reason}",
                        gap.producer
                    ),
                    None,
                    None,
                ));
        }
    }
    diagnostics
}

fn related_information(
    result: &ProjectAnalysisResult,
    evidence: &[biome_project_analysis::result::EvidenceId],
    encoding: PositionEncoding,
) -> Option<Vec<lsp::DiagnosticRelatedInformation>> {
    let mut related = Vec::new();
    let mut pending = evidence.to_vec();
    let mut visited = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        if let Some(evidence) = result.evidence.nodes.get(&id) {
            pending.extend(evidence.related.iter().cloned());
            let fallback = result
                .workspaces
                .get(&evidence.workspace)
                .map(|workspace| Location {
                    file: workspace.manifest,
                    range: Default::default(),
                });
            if let Some(location) = evidence
                .location
                .or(fallback)
                .and_then(|location| source_location(result, location, encoding))
            {
                related.push(lsp::DiagnosticRelatedInformation {
                    location,
                    message: evidence.explanation.clone(),
                });
            }
        }
    }
    (!related.is_empty()).then_some(related)
}
