use crate::{projects::ProjectKey, workspace::ServiceNotification};
use biome_project_analysis::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    model::{AnalysisSnapshot, FileId, SnapshotRevision},
    result::ProjectAnalysisResult,
};
use camino::{Utf8Path, Utf8PathBuf};
use parking_lot::{Condvar, Mutex};
#[cfg(any(not(target_arch = "wasm32"), test))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, OnceLock},
};
use tokio::sync::watch;

/// Retained state is authoritative; service notifications only prompt reconciliation.
#[derive(Clone, Debug)]
pub enum ProjectAnalysisState {
    Dirty {
        generation: u64,
    },
    Completed {
        generation: u64,
        snapshot: Arc<AnalysisSnapshot>,
    },
    Failed {
        generation: u64,
        coverage: Coverage,
    },
    Closed {
        generation: u64,
    },
}

pub(crate) enum SnapshotBuildOutcome {
    Published(Arc<AnalysisSnapshot>),
    Superseded,
    Failed(Coverage),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RequestToken(u64);

pub(crate) struct ProjectAnalysisCoordinator {
    pub(crate) session_id: Option<String>,
    inner: Mutex<State>,
    settled: Condvar,
    #[cfg(test)]
    pub(crate) analysis_runs: AtomicU64,
    #[cfg(test)]
    pub(crate) after_analysis: Mutex<Option<Arc<std::sync::Barrier>>>,
    #[cfg(test)]
    pub(crate) before_publish: Mutex<Option<Arc<std::sync::Barrier>>>,
    #[cfg(test)]
    pub(crate) settlement_started: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    #[cfg(test)]
    pub(crate) after_settlement: Mutex<Option<Arc<std::sync::Barrier>>>,
}

#[derive(Default)]
struct State {
    generation: u64,
    pending: usize,
    gaps: Vec<WatchGap>,
    projects: HashMap<ProjectKey, ProjectAnalysisState>,
    requests: HashMap<ProjectKey, (String, RequestToken)>,
    next_request: u64,
    interested: HashSet<ProjectKey>,
    cached_inputs: HashMap<ProjectKey, (Arc<AnalysisSnapshot>, Arc<super::pinned_fs::PinnedFs>)>,
    notification: Option<watch::Sender<ServiceNotification>>,
    reports: HashMap<ProjectKey, ReportCache>,
}

#[derive(Default)]
struct ReportCache {
    revision: Option<SnapshotRevision>,
    options: HashMap<String, ReportSlot>,
}

pub(crate) type ReportSlot = Arc<OnceLock<Arc<ProjectAnalysisResult>>>;

struct WatchGap {
    project: Option<ProjectKey>,
    path: Option<Utf8PathBuf>,
    generation: u64,
    persistent: bool,
    reconciled: HashSet<ProjectKey>,
    gap: CoverageGap,
}

impl Default for ProjectAnalysisCoordinator {
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        static NEXT: AtomicU64 = AtomicU64::new(0);
        #[cfg(not(target_arch = "wasm32"))]
        let session_id = Some(format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(target_arch = "wasm32")]
        let session_id = session_id_from_entropy(getrandom::getrandom);
        Self {
            session_id,
            inner: Mutex::new(State::default()),
            settled: Condvar::new(),
            #[cfg(test)]
            analysis_runs: AtomicU64::new(0),
            #[cfg(test)]
            after_analysis: Mutex::default(),
            #[cfg(test)]
            before_publish: Mutex::default(),
            #[cfg(test)]
            settlement_started: Mutex::default(),
            #[cfg(test)]
            after_settlement: Mutex::default(),
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn session_id_from_entropy(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Option<String> {
    let mut nonce = [0; 16];
    fill(&mut nonce).ok()?;
    Some(format!("{:032x}", u128::from_be_bytes(nonce)))
}

impl ProjectAnalysisCoordinator {
    pub(crate) fn subscribe(&self, notification: watch::Sender<ServiceNotification>) {
        self.inner.lock().notification = Some(notification);
    }

    pub(crate) fn open(&self, project: ProjectKey) {
        let mut state = self.inner.lock();
        let generation = state.generation;
        state
            .projects
            .insert(project, ProjectAnalysisState::Dirty { generation });
        Self::wake(&state);
    }

    pub(crate) fn close(&self, project: ProjectKey) {
        let mut state = self.inner.lock();
        state.generation += 1;
        let generation = state.generation;
        state
            .projects
            .insert(project, ProjectAnalysisState::Closed { generation });
        state.reports.remove(&project);
        state.cached_inputs.remove(&project);
        state.requests.remove(&project);
        Self::wake(&state);
        self.settled.notify_all();
    }

    /// State consumers pull once when registering, then reconcile on later wakeups.
    pub(crate) fn observe(&self, projects: &[ProjectKey]) {
        self.inner
            .lock()
            .interested
            .extend(projects.iter().copied());
    }

    /// Consecutive identical requests share a token; a replacement invalidates
    /// every earlier token, even if a later request restores the same identity.
    pub(crate) fn begin_request(
        &self,
        project: ProjectKey,
        identity: String,
    ) -> Option<RequestToken> {
        let mut state = self.inner.lock();
        let first = state.interested.insert(project);
        if first {
            Self::wake(&state);
        }
        if matches!(
            state.projects.get(&project),
            None | Some(ProjectAnalysisState::Closed { .. })
        ) {
            return None;
        }
        if let Some((current, token)) = state.requests.get(&project) {
            if *current == identity {
                return Some(*token);
            }
            state.generation += 1;
            let generation = state.generation;
            state
                .projects
                .insert(project, ProjectAnalysisState::Dirty { generation });
            state.cached_inputs.remove(&project);
            Self::wake(&state);
        }
        state.next_request += 1;
        let token = RequestToken(state.next_request);
        state.requests.insert(project, (identity, token));
        Some(token)
    }

    /// Observed external changes invalidate a completion before replacement acquisition.
    pub(crate) fn reacquire(
        &self,
        project: ProjectKey,
        request: RequestToken,
        generation: u64,
    ) -> Option<u64> {
        let mut state = self.inner.lock();
        if state.pending != 0
            || !Self::current_request(&state, project, request)
            || state.generation != generation
            || matches!(
                state.projects.get(&project),
                None | Some(ProjectAnalysisState::Closed { .. })
            )
        {
            return None;
        }
        if matches!(
            state.projects.get(&project),
            Some(ProjectAnalysisState::Completed { .. })
        ) {
            state.generation += 1;
            let generation = state.generation;
            state.cached_inputs.clear();
            for status in state.projects.values_mut() {
                if !matches!(status, ProjectAnalysisState::Closed { .. }) {
                    *status = ProjectAnalysisState::Dirty { generation };
                }
            }
            Self::wake(&state);
        }
        Some(state.generation)
    }

    pub(crate) fn cached(
        &self,
        project: ProjectKey,
        request: RequestToken,
        config_digest: &str,
    ) -> Option<Arc<AnalysisSnapshot>> {
        let (snapshot, inputs) = {
            let state = self.inner.lock();
            if !Self::current_request(&state, project, request) {
                return None;
            }
            state.cached_inputs.get(&project)?.clone()
        };
        (snapshot.revision.config_digest == config_digest && inputs.unchanged()).then_some(snapshot)
    }
    pub(super) fn remember_inputs(
        &self,
        project: ProjectKey,
        request: RequestToken,
        snapshot: Arc<AnalysisSnapshot>,
        inputs: Arc<super::pinned_fs::PinnedFs>,
    ) {
        let mut state = self.inner.lock();
        if Self::current_request(&state, project, request)
            && matches!(state.projects.get(&project), Some(ProjectAnalysisState::Completed { snapshot: current, .. }) if current.revision == snapshot.revision)
        {
            state.cached_inputs.insert(project, (snapshot, inputs));
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.inner.lock().generation
    }

    /// Observes settlement without waiting for a pending mutation to release its guard.
    pub(crate) fn settled_generation(
        &self,
        project: ProjectKey,
        request: RequestToken,
    ) -> Option<u64> {
        let state = self.inner.lock();
        (state.pending == 0 && Self::current_request(&state, project, request))
            .then_some(state.generation)
    }

    pub(crate) fn wait_until_settled(
        &self,
        project: ProjectKey,
        request: RequestToken,
    ) -> Option<u64> {
        #[cfg(test)]
        let barrier = self.after_settlement.lock().take();
        let mut state = self.inner.lock();
        #[cfg(test)]
        if state.pending != 0
            && let Some(sender) = self.settlement_started.lock().as_ref()
        {
            sender.send(()).unwrap();
        }
        while state.pending != 0 {
            self.settled.wait(&mut state);
        }
        let generation =
            Self::current_request(&state, project, request).then_some(state.generation);
        drop(state);
        #[cfg(test)]
        if let Some(barrier) = barrier {
            barrier.wait();
            barrier.wait();
        }
        generation
    }

    /// Nested operations remain pending until every guard, including epoch finalization, ends.
    pub(crate) fn mutation(&self) -> Mutation<'_> {
        let mut state = self.inner.lock();
        state.pending += 1;
        state.generation += 1;
        let generation = state.generation;
        state.cached_inputs.clear();
        for status in state.projects.values_mut() {
            if !matches!(status, ProjectAnalysisState::Closed { .. }) {
                *status = ProjectAnalysisState::Dirty { generation };
            }
        }
        Self::wake(&state);
        Mutation(self)
    }

    pub(crate) fn publish(
        &self,
        project: ProjectKey,
        request: RequestToken,
        generation: u64,
        snapshot: Arc<AnalysisSnapshot>,
    ) -> SnapshotBuildOutcome {
        self.publish_reconciled(project, request, generation, snapshot, &[])
    }

    pub(crate) fn publish_reconciled(
        &self,
        project: ProjectKey,
        request: RequestToken,
        generation: u64,
        mut snapshot: Arc<AnalysisSnapshot>,
        recovered: &[usize],
    ) -> SnapshotBuildOutcome {
        let mut state = self.inner.lock();
        if state.pending != 0
            || !Self::current_request(&state, project, request)
            || state.generation != generation
            || matches!(
                state.projects.get(&project),
                None | Some(ProjectAnalysisState::Closed { .. })
            )
        {
            return SnapshotBuildOutcome::Superseded;
        }
        // Recovery and publication share request/generation checks and the lock. Superseded
        // acquisitions cannot retire either older debt or a newly recorded error.
        for index in recovered {
            if let Some(entry) = state.gaps.get_mut(*index)
                && !entry.persistent
                && entry.generation <= generation
            {
                entry.reconciled.insert(project);
            }
        }
        if let Some(ProjectAnalysisState::Completed {
            snapshot: cached, ..
        }) = state.projects.get(&project)
        {
            if cached.revision == snapshot.revision {
                return SnapshotBuildOutcome::Published(cached.clone());
            }
            state.generation += 1;
            let next = state.generation;
            for (key, status) in &mut state.projects {
                if *key != project && !matches!(status, ProjectAnalysisState::Closed { .. }) {
                    *status = ProjectAnalysisState::Dirty { generation: next };
                }
            }
            state.cached_inputs.clear();
            Arc::make_mut(&mut snapshot).revision.epoch = state.generation;
        }
        let generation = state.generation;
        state.projects.insert(
            project,
            ProjectAnalysisState::Completed {
                generation,
                snapshot: snapshot.clone(),
            },
        );
        Self::wake(&state);
        SnapshotBuildOutcome::Published(snapshot)
    }

    pub(crate) fn fail(
        &self,
        project: ProjectKey,
        request: RequestToken,
        generation: u64,
        coverage: Coverage,
    ) -> SnapshotBuildOutcome {
        let mut state = self.inner.lock();
        if state.pending != 0
            || !Self::current_request(&state, project, request)
            || state.generation != generation
            || matches!(
                state.projects.get(&project),
                None | Some(ProjectAnalysisState::Closed { .. })
            )
        {
            return SnapshotBuildOutcome::Superseded;
        }
        state.projects.insert(
            project,
            ProjectAnalysisState::Failed {
                generation,
                coverage: coverage.clone(),
            },
        );
        Self::wake(&state);
        SnapshotBuildOutcome::Failed(coverage)
    }

    pub(crate) fn state(&self, project: ProjectKey) -> ProjectAnalysisState {
        let state = self.inner.lock();
        state
            .projects
            .get(&project)
            .cloned()
            .unwrap_or(ProjectAnalysisState::Closed {
                generation: state.generation,
            })
    }

    pub(crate) fn reconcile(
        &self,
        projects: &[ProjectKey],
    ) -> Vec<crate::workspace::ProjectAnalysisState> {
        use crate::workspace::{ProjectAnalysisState as WireState, ProjectAnalysisStatus};
        let mut state = self.inner.lock();
        state.interested.extend(projects.iter().copied());
        projects
            .iter()
            .map(|project_key| {
                let (generation, status) = match state.projects.get(project_key) {
                    Some(ProjectAnalysisState::Dirty { generation }) => {
                        (*generation, ProjectAnalysisStatus::Dirty)
                    }
                    Some(ProjectAnalysisState::Completed {
                        generation,
                        snapshot,
                    }) => (
                        *generation,
                        ProjectAnalysisStatus::Completed {
                            revision: snapshot.revision.clone(),
                        },
                    ),
                    Some(ProjectAnalysisState::Failed {
                        generation,
                        coverage,
                    }) => (
                        *generation,
                        ProjectAnalysisStatus::Failed {
                            coverage: coverage.clone(),
                        },
                    ),
                    Some(ProjectAnalysisState::Closed { generation }) => {
                        (*generation, ProjectAnalysisStatus::Closed)
                    }
                    None => (state.generation, ProjectAnalysisStatus::Closed),
                };
                WireState {
                    project_key: *project_key,
                    generation,
                    status,
                }
            })
            .collect()
    }

    pub(crate) fn report_slot(
        &self,
        project: ProjectKey,
        revision: &SnapshotRevision,
        options: String,
    ) -> Option<ReportSlot> {
        let mut state = self.inner.lock();
        if state.pending != 0
            || !matches!(state.projects.get(&project), Some(ProjectAnalysisState::Completed { snapshot, .. }) if &snapshot.revision == revision)
        {
            return None;
        }
        let cache = state.reports.entry(project).or_default();
        if cache.revision.as_ref() != Some(revision) {
            cache.revision = Some(revision.clone());
            cache.options.clear();
        }
        Some(cache.options.entry(options).or_default().clone())
    }

    pub(crate) fn is_current(&self, project: ProjectKey, revision: &SnapshotRevision) -> bool {
        let state = self.inner.lock();
        state.pending == 0
            && matches!(state.projects.get(&project), Some(ProjectAnalysisState::Completed { snapshot, .. }) if &snapshot.revision == revision)
    }

    pub(crate) fn record_watch_input_error(
        &self,
        project: ProjectKey,
        path: &Utf8Path,
        reason: String,
    ) {
        self.record_watch_gap(Some(project), Some(path), false, reason);
    }

    pub(crate) fn record_watch_failure(
        &self,
        project: Option<ProjectKey>,
        path: Option<&Utf8Path>,
        reason: String,
    ) {
        self.record_watch_gap(project, path, true, reason);
    }

    fn record_watch_gap(
        &self,
        project: Option<ProjectKey>,
        path: Option<&Utf8Path>,
        persistent: bool,
        reason: String,
    ) {
        let _mutation = self.mutation();
        let mut state = self.inner.lock();
        let generation = state.generation;
        state.gaps.push(WatchGap {
            project,
            path: path.map(Utf8Path::to_path_buf),
            generation,
            persistent,
            reconciled: HashSet::new(),
            gap: CoverageGap {
                producer: "watcher".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::UnavailableInput(reason),
                location: None,
            },
        });
    }

    /// A fresh full-scope acquisition can reconcile input operations, but cannot
    /// repair failed watch registration or a stopped watcher. History already
    /// reconciled for this project never becomes active coverage again.
    pub(crate) fn watch_gaps(
        &self,
        project: ProjectKey,
        generation: u64,
        root: &Utf8Path,
        inputs: &BTreeMap<Utf8PathBuf, FileId>,
        can_reconcile: bool,
    ) -> (Vec<CoverageGap>, Vec<usize>) {
        let state = self.inner.lock();
        let mut gaps = Vec::new();
        let mut recovered = Vec::new();
        for (index, entry) in state.gaps.iter().enumerate() {
            if entry.reconciled.contains(&project) {
                continue;
            }
            let covered_path = entry.path.as_ref().is_some_and(|path| {
                path.starts_with(root)
                    || root.starts_with(path)
                    || inputs.keys().any(|input| input.starts_with(path))
            });
            let relevant = entry.project == Some(project)
                || covered_path
                || (entry.project.is_none() && entry.path.is_none());
            if !relevant {
                continue;
            }
            if can_reconcile && covered_path && !entry.persistent && entry.generation <= generation
            {
                recovered.push(index);
            } else {
                gaps.push(entry.gap.clone());
            }
        }
        (gaps, recovered)
    }
    fn current_request(state: &State, project: ProjectKey, request: RequestToken) -> bool {
        state
            .requests
            .get(&project)
            .is_some_and(|(_, current)| *current == request)
    }

    fn wake(state: &State) {
        if let Some(notification) = &state.notification {
            for (project_key, status) in &state.projects {
                if !state.interested.contains(project_key) {
                    continue;
                }
                let revision = if let ProjectAnalysisState::Completed { snapshot, .. } = status {
                    Some(Arc::new(snapshot.revision.clone()))
                } else {
                    None
                };
                notification.send_replace(ServiceNotification::ProjectAnalysisReady {
                    project_key: *project_key,
                    generation: state.generation,
                    revision,
                });
            }
        }
    }
}

pub(crate) struct Mutation<'a>(&'a ProjectAnalysisCoordinator);
impl Drop for Mutation<'_> {
    fn drop(&mut self) {
        let mut state = self.0.inner.lock();
        state.pending -= 1;
        Self::finish(&self.0.settled, &state);
    }
}
impl Mutation<'_> {
    fn finish(settled: &Condvar, state: &State) {
        if state.pending == 0 {
            settled.notify_all();
        }
        ProjectAnalysisCoordinator::wake(state);
    }
}
