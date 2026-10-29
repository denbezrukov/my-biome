use super::ProjectAnalysisRequest;
use crate::{
    Workspace,
    db::DbState,
    projects::ProjectKey,
    workspace::{NoopQueryProvider, OpenProjectParams, PullProjectDiagnosticsParams, ServiceNotification, WorkspaceServer},
};
use biome_fs::{BiomePath, MemoryFileSystem};
use biome_project_analysis::{
    normalize::{normalize_result, normalize_snapshot},
    options::AnalysisMode,
    result::ProjectAnalysisResult,
};
use camino::Utf8Path;
use std::sync::{Arc, atomic::Ordering};

struct Service {
    server: WorkspaceServer,
    db: DbState,
    project: ProjectKey,
}

impl Service {
    fn new(fs: &MemoryFileSystem, owned: bool) -> Self {
        let fs = Arc::new(MemoryFileSystem::from_files(fs.files.0.clone()));
        let db = if owned { DbState::lsp(fs.clone()) } else { DbState::new(fs.clone()) };
        let (watcher, _) = crossbeam::channel::unbounded();
        let (notifications, _) = tokio::sync::watch::channel(ServiceNotification::IndexUpdated);
        let server = WorkspaceServer::new(fs, watcher, notifications, Arc::new(NoopQueryProvider {}), None);
        let project = server.with_db_state(&db).open_project(OpenProjectParams {
            path: BiomePath::new("/project"), open_uninitialized: true,
        }).unwrap().project_key;
        Self { server, db, project }
    }

    fn result(&self, request: &ProjectAnalysisRequest) -> ProjectAnalysisResult {
        self.server.with_db_state(&self.db).pull_project_diagnostics(PullProjectDiagnosticsParams {
            project_key: self.project, request: request.clone(), overrides: Default::default(),
            selection: Default::default(), selection_revision: None,
        }).unwrap()
    }

    fn equivalent(&self, fs: &MemoryFileSystem, owned: bool, request: &ProjectAnalysisRequest) {
        let fresh = Self::new(fs, owned);
        let warm_result = self.result(request);
        let fresh_result = fresh.result(request);
        assert_eq!(normalize_result(&warm_result), normalize_result(&fresh_result));
        let warm_snapshot = self.server.with_db_state(&self.db).build_project_snapshot(self.project, request).unwrap();
        let fresh_snapshot = fresh.server.with_db_state(&fresh.db).build_project_snapshot(fresh.project, request).unwrap();
        assert_eq!(normalize_snapshot(&warm_snapshot), normalize_snapshot(&fresh_snapshot));
        assert_ne!(warm_result.revision.session_id, fresh_result.revision.session_id);
    }
}

#[test]
fn watch_project_mutations_equal_fresh() {
    for owned in [false, true] {
        let fs = MemoryFileSystem::default();
        fs.insert("/project/package.json".into(), r#"{"private":true,"workspaces":["packages/*"],"exports":"./index.ts"}"#);
        fs.insert("/project/index.ts".into(), "export { value } from './missing';");
        let service = Service::new(&fs, owned);
        let mut request = ProjectAnalysisRequest::default();
        service.equivalent(&fs, owned, &request);
        for (path, content) in [
            ("missing.ts", Some("export const value = 1;")),
            ("missing.ts", Some("export const value = 2; export const unused = 3;")),
            ("created/member.ts", Some("export const member = 1;")),
            ("created/member.ts", None),
            ("renamed/member.ts", Some("export const member = 1;")),
            ("renamed/member.ts", None),
            ("packages/lib/package.json", Some(r#"{"name":"lib","exports":"./index.ts"}"#)),
            ("packages/lib/index.ts", Some("export const library = 1;")),
            ("node_modules/dep/package.json", Some(r#"{"name":"dep","exports":"./index.js"}"#)),
            ("node_modules/dep/index.js", Some("export const dependency = 1;")),
            ("index.ts", Some("export { dependency } from 'dep';")),
            ("node_modules/dep/package.json", Some(r#"{"name":"dep","exports":"./other.js"}"#)),
            ("node_modules/dep/other.js", Some("export const dependency = 2;")),
            ("tsconfig.json", Some(r#"{"compilerOptions":{"rootDir":"src","outDir":"dist"}}"#)),
            ("pnpm-workspace.yaml", Some("packages:\n  - packages/*\ncatalog:\n  dep: '^1.0.0'\n")),
            ("package.json", Some(r#"{"private":true,"workspaces":["packages/*"],"dependencies":{"dep":"catalog:"}}"#)),
            ("pnpm-workspace.yaml", Some("packages:\n  - packages/*\ncatalog:\n  dep: '^2.0.0'\n")),
            ("packages/lib/index.ts", None),
            ("packages/lib/package.json", None),
        ] {
            let path = Utf8Path::new("/project").join(path);
            if let Some(content) = content { fs.insert(path, content); } else { fs.remove(&path); }
            service.equivalent(&fs, owned, &request);
        }
        let old_config_digest = service.result(&request).revision.config_digest;
        request.configuration = serde_json::from_value(serde_json::json!({"entry":["index.ts!"],"project":["**/*.ts!"],"ignore":["missing.ts"]})).unwrap();
        request.mode = AnalysisMode::Production;
        let new_config_digest = service.result(&request).revision.config_digest;
        assert_ne!(old_config_digest, new_config_digest);
        service.equivalent(&fs, owned, &request);
    }
}

#[test]
fn cache_key_includes_mode_and_config() {
    let fs = MemoryFileSystem::default();
    fs.insert("/project/package.json".into(), r#"{"private":true,"devDependencies":{"unused":"1"}}"#);
    fs.insert("/project/index.ts".into(), "export const value = 1;");
    for owned in [false, true] {
        let service = Service::new(&fs, owned);
        let mut request = ProjectAnalysisRequest::default();
        let first = service.result(&request);
        assert_eq!(first, service.result(&request));
        assert_eq!(service.db.analysis.analysis_runs.load(Ordering::Relaxed), 1);
        for mode in [AnalysisMode::Production, AnalysisMode::Strict, AnalysisMode::Default] {
            request.mode = mode;
            let result = service.result(&request);
            assert_eq!(result.context.mode, mode);
            service.equivalent(&fs, owned, &request);
        }
        let old_config_digest = service.result(&request).revision.config_digest;
        request.configuration.ignore = Some(biome_configuration::project_analysis::AnalysisStrings(vec!["index.ts".to_string()]));
        let new_config_digest = service.result(&request).revision.config_digest;
        assert_ne!(old_config_digest, new_config_digest);
        service.equivalent(&fs, owned, &request);
    }
}

#[test]
fn watch_recovers_from_event_loss() {
    use biome_project_analysis::coverage::Coverage;
    for owned in [false, true] {
        let fs = MemoryFileSystem::default();
        fs.insert("/project/package.json".into(), r#"{"private":true}"#);
        fs.insert("/project/index.ts".into(), "export const value = 1;");
        let service = Service::new(&fs, owned);
        let request = ProjectAnalysisRequest::default();
        let before = service.result(&request);
        service.db.analysis.record_watch_input_error(service.project, Utf8Path::new("/project/index.ts"), "event batch lost".into());
        let mut isolated = request.clone();
        isolated.isolation = Some(super::AnalysisBoundary { excluded_consumers: vec!["unobserved".into()] });
        let incomplete = service.result(&isolated);
        assert!(matches!(incomplete.coverage, Coverage::Incomplete(_)));
        assert!(incomplete.coverage.gaps().iter().any(|gap| gap.producer == "watcher"));
        let recovered = service.result(&request);
        assert_eq!(normalize_result(&before), normalize_result(&recovered));
        service.equivalent(&fs, owned, &request);
        service.db.analysis.record_watch_failure(None, None, "watch channel stopped".into());
        let failed_watch = service.result(&request);
        assert!(matches!(failed_watch.coverage, Coverage::Incomplete(_)));
        assert!(failed_watch.coverage.gaps().iter().any(|gap| gap.producer == "watcher"));
        assert!(matches!(service.result(&request).coverage, Coverage::Incomplete(_)));
    }
}
