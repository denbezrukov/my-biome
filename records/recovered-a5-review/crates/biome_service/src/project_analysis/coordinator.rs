use crate::{projects::ProjectKey, workspace::ServiceNotification};
use biome_project_analysis::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    model::{AnalysisSnapshot, FileId},
};
use camino::{Utf8Path, Utf8PathBuf};
use parking_lot::{Condvar, Mutex};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
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
    Closed,
}

pub(crate) enum SnapshotBuildOutcome {
    Published(Arc<AnalysisSnapshot>),
    Superseded,
    Failed(Coverage),
}

pub(crate) struct ProjectAnalysisCoordinator {
    pub(crate) session_id: String,
    inner: Mutex<State>,
    settled: Condvar,
    #[cfg(test)]
    pub(crate) before_publish: Mutex<Option<Arc<std::sync::Barrier>>>,
}

#[derive(Default)]
struct State {
    generation: u64,
    pending: usize,
    gaps: Vec<WatchGap>,
    projects: HashMap<ProjectKey, ProjectAnalysisState>,
    requests: HashMap<ProjectKey, String>,
    interested: HashSet<ProjectKey>,
    cached_inputs: HashMap<ProjectKey, (Arc<AnalysisSnapshot>, Arc<super::pinned_fs::PinnedFs>)>,
    notification: Option<watch::Sender<ServiceNotification>>,
}

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
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self {
            session_id: format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            inner: Mutex::new(State::default()),
            settled: Condvar::new(),
            #[cfg(test)]
            before_publish: Mutex::default(),
        }
    }
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
        state.projects.insert(project, ProjectAnalysisState::Closed);
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

    pub(crate) fn begin_request(&self, project: ProjectKey, identity: String) {
        let mut state = self.inner.lock();
        let first = state.interested.insert(project);
        if first {
            Self::wake(&state);
        }
        if matches!(
            state.projects.get(&project),
            None | Some(ProjectAnalysisState::Closed)
        ) {
            return;
        }
        if state
            .requests
            .get(&project)
            .is_some_and(|current| *current != identity)
        {
            state.generation += 1;
            let generation = state.generation;
            state
                .projects
                .insert(project, ProjectAnalysisState::Dirty { generation });
            state.cached_inputs.remove(&project);
            Self::wake(&state);
        }
        state.requests.insert(project, identity);
    }

    /// Observed external changes invalidate a completion before replacement acquisition.
    pub(crate) fn reacquire(&self, project: ProjectKey, generation: u64) -> Option<u64> {
        let mut state = self.inner.lock();
        if state.pending != 0
            || state.generation != generation
            || matches!(
                state.projects.get(&project),
                None | Some(ProjectAnalysisState::Closed)
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
                if !matches!(status, ProjectAnalysisState::Closed) {
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
        config_digest: &str,
    ) -> Option<Arc<AnalysisSnapshot>> {
        let (snapshot, inputs) = self.inner.lock().cached_inputs.get(&project)?.clone();
        (snapshot.revision.config_digest == config_digest && inputs.unchanged()).then_some(snapshot)
    }
    pub(super) fn remember_inputs(
        &self,
        project: ProjectKey,
        snapshot: Arc<AnalysisSnapshot>,
        inputs: Arc<super::pinned_fs::PinnedFs>,
    ) {
        let mut state = self.inner.lock();
        if matches!(state.projects.get(&project), Some(ProjectAnalysisState::Completed { snapshot: current, .. }) if current.revision == snapshot.revision)
        {
            state.cached_inputs.insert(project, (snapshot, inputs));
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.inner.lock().generation
    }

    pub(crate) fn wait_until_settled(&self) -> u64 {
        let mut state = self.inner.lock();
        while state.pending != 0 {
            self.settled.wait(&mut state);
        }
        state.generation
    }

    /// Nested operations remain pending until every guard, including epoch finalization, ends.
    pub(crate) fn mutation(&self) -> Mutation<'_> {
        let mut state = self.inner.lock();
        state.pending += 1;
        state.generation += 1;
        let generation = state.generation;
        state.cached_inputs.clear();
        for status in state.projects.values_mut() {
            if !matches!(status, ProjectAnalysisState::Closed) {
                *status = ProjectAnalysisState::Dirty { generation };
            }
        }
        Self::wake(&state);
        Mutation(self)
    }

    pub(crate) fn publish(
        &self,
        project: ProjectKey,
        generation: u64,
        snapshot: Arc<AnalysisSnapshot>,
    ) -> SnapshotBuildOutcome {
        self.publish_reconciled(project, generation, snapshot, &[])
    }

    pub(crate) fn publish_reconciled(
        &self,
        project: ProjectKey,
        generation: u64,
        mut snapshot: Arc<AnalysisSnapshot>,
        recovered: &[usize],
    ) -> SnapshotBuildOutcome {
        let mut state = self.inner.lock();
        if state.pending != 0
            || state.generation != generation
            || matches!(
                state.projects.get(&project),
                None | Some(ProjectAnalysisState::Closed)
            )
        {
            return SnapshotBuildOutcome::Superseded;
        }
        // Recovery and publication share the generation check and lock. Superseded
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
                if *key != project && !matches!(status, ProjectAnalysisState::Closed) {
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
        generation: u64,
        coverage: Coverage,
    ) -> SnapshotBuildOutcome {
        let mut state = self.inner.lock();
        if state.pending != 0
            || state.generation != generation
            || matches!(
                state.projects.get(&project),
                None | Some(ProjectAnalysisState::Closed)
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
        self.inner
            .lock()
            .projects
            .get(&project)
            .cloned()
            .unwrap_or(ProjectAnalysisState::Closed)
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
