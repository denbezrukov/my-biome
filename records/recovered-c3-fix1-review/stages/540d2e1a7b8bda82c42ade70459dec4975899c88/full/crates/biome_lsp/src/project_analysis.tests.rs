use crate::server_test_utils::{Server, ServerFactory, ServerNotification, client_handler};
use biome_fs::MemoryFileSystem;
use futures::{StreamExt, channel::mpsc};
use std::sync::Arc;
use std::time::Duration;
use tower_lsp_server::ls_types as lsp;

#[tokio::test]
async fn project_diagnostics_include_closed_manifest() {
    let fs = Arc::new(MemoryFileSystem::default());
    fs.insert(
        "/project/biome.json".into(),
        r#"{"projectAnalysis":{"enabled":true,"entry":[],"project":[]}}"#,
    );
    fs.insert(
        "/project/package.json".into(),
        "{\n  \"private\": true,\n  \"dependencies\": {\"unused\": \"1\"}\n}\n",
    );
    let factory = ServerFactory::new_with_fs(fs);
    let (service, socket) = factory.create().into_inner();
    let mut server = Server::new(service);
    let (sender, mut notifications) = mpsc::channel(32);
    let (stream, sink) = socket.split();
    let client = tokio::spawn(client_handler(stream, sink, sender));
    server
        .initialize_with_root("file:///project".parse().unwrap())
        .await
        .unwrap();
    server.initialized().await.unwrap();
    let published = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(notification) = notifications.next().await {
            if let ServerNotification::PublishDiagnostics(params) = notification
                && params.uri.as_str() == "file:///project/package.json"
                && params
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains("unused"))
            {
                return Some(params);
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    assert!(
        published.is_some(),
        "project dependency diagnostics must publish without an open document"
    );
    let published = published.unwrap();
    assert_eq!(published.version, None);
    let issue = published
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("unused"))
        .unwrap();
    assert_eq!(issue.range.start.line, 2);
    assert_eq!(issue.severity, Some(lsp::DiagnosticSeverity::ERROR));
    client.abort();
}

#[derive(Default)]
pub(crate) struct PublicationHooks {
    pub(crate) before_local: PublicationBarrier,
    pub(crate) before_reconcile: PublicationBarrier,
    pub(crate) before_project: PublicationBarrier,
    pub(crate) before_analysis: PublicationBarrier,
    pub(crate) before_close_analysis: PublicationBarrier,
}

#[derive(Default)]
pub(crate) struct PublicationBarrier(parking_lot::Mutex<Option<Arc<tokio::sync::Barrier>>>);

impl PublicationBarrier {
    fn pause(&self) -> Arc<tokio::sync::Barrier> {
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        *self.0.lock() = Some(barrier.clone());
        barrier
    }

    pub(crate) async fn wait(&self) {
        let barrier = self.0.lock().take();
        if let Some(barrier) = barrier {
            barrier.wait().await;
            barrier.wait().await;
        }
    }
}

use crate::{documents::Document, session::Session};
use biome_project_analysis::{
    model::{Location, SourceContent},
    result::{Evidence, EvidenceId, IssueKind, ProjectAnalysisResult},
};
use biome_service::{
    Workspace, project_analysis::ProjectAnalysisRequest, projects::ProjectKey, workspace::*,
};

struct Harness {
    fs: Arc<MemoryFileSystem>,
    factory: ServerFactory,
    session: Arc<Session>,
    _server: Server,
    notifications: mpsc::Receiver<ServerNotification>,
    client: tokio::task::JoinHandle<anyhow::Result<()>>,
    subscription: Arc<tokio::sync::Barrier>,
}
impl Harness {
    async fn new() -> Self {
        let fs = Arc::new(MemoryFileSystem::default());
        let factory = ServerFactory::new_with_fs(fs.clone());
        let (service, socket) = factory.create().into_inner();
        let session = service.inner().session.clone();
        let subscription = session.project_diagnostics.hooks.before_reconcile.pause();
        let (sender, notifications) = mpsc::channel(128);
        let (stream, sink) = socket.split();
        let client = tokio::spawn(client_handler(stream, sink, sender));
        let mut server = Server::new(service);
        server
            .initialize_with_root("file:///project".parse().unwrap())
            .await
            .unwrap();
        subscription.wait().await;
        Self {
            fs,
            factory,
            session,
            _server: server,
            notifications,
            client,
            subscription,
        }
    }
    fn project(&self, root: &str) -> ProjectKey {
        self.fs.insert(
            format!("{root}/package.json").into(),
            "{\n\"private\":true,\n\"dependencies\":{\"unused\":\"1\"}\n}",
        );
        let project = self
            .session
            .workspace()
            .open_project(OpenProjectParams {
                path: biome_fs::BiomePath::new(root),
                open_uninitialized: true,
            })
            .unwrap()
            .project_key;
        self.session
            .set_project_analysis_request(project, Some(ProjectAnalysisRequest::default()));
        project
    }
    fn pull(&self, project: ProjectKey) -> ProjectAnalysisResult {
        self.session
            .workspace()
            .pull_project_diagnostics(PullProjectDiagnosticsParams {
                project_key: project,
                request: Default::default(),
                selection: Default::default(),
                selection_revision: None,
                overrides: Default::default(),
            })
            .unwrap()
    }
    fn open(&self, project: ProjectKey, path: &str, text: &str) -> lsp::Uri {
        self.session
            .workspace()
            .open_file(OpenFileParams {
                project_key: project,
                path: biome_fs::BiomePath::new(path),
                content: FileContent::FromClient {
                    content: text.into(),
                    version: 7,
                },
                document_file_source: None,
                persist_node_cache: false,
                inline_config: None,
                editor_features: None,
            })
            .unwrap();
        let uri: lsp::Uri = format!("file://{path}").parse().unwrap();
        self.session
            .insert_document(uri.clone(), Document::new(project, 7, text));
        uri
    }
    async fn publication(&mut self, uri: &str) -> lsp::PublishDiagnosticsParams {
        tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(notification) = self.notifications.next().await {
                if let ServerNotification::PublishDiagnostics(params) = notification
                    && params.uri.as_str() == uri
                {
                    return params;
                }
            }
            panic!("publication channel closed")
        })
        .await
        .expect("expected publication")
    }
    async fn fence(&self) {
        self.session.client.configuration(vec![]).await.unwrap();
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.client.abort();
    }
}

#[tokio::test]
async fn project_diagnostics_clear_deleted_and_resolved() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    let result = test.pull(project);
    test.session.publish_project_result(project, result).await;
    assert!(
        !test
            .publication("file:///project/package.json")
            .await
            .diagnostics
            .is_empty()
    );
    test.fs
        .insert("/project/package.json".into(), "{\"private\":true}");
    let resolved = test.pull(project);
    test.session.publish_project_result(project, resolved).await;
    let cleared_manifest_diagnostics = test
        .publication("file:///project/package.json")
        .await
        .diagnostics;
    assert!(cleared_manifest_diagnostics.is_empty());
    test.fs.insert(
        "/project/package.json".into(),
        "{\"private\":true,\"dependencies\":{\"unused\":\"1\"}}",
    );
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    assert!(
        !test
            .publication("file:///project/package.json")
            .await
            .diagnostics
            .is_empty()
    );
    test.fs
        .remove(camino::Utf8Path::new("/project/package.json"));
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    assert!(
        test.publication("file:///project/package.json")
            .await
            .diagnostics
            .is_empty()
    );
    test.fs.insert(
        "/project/package.json".into(),
        "{\"private\":true,\"dependencies\":{\"unused\":\"1\"}}",
    );
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication("file:///project/package.json").await;
    test.session.set_project_analysis_request(project, None);
    test.session.reconcile_project_diagnostics(false).await;
    assert!(
        test.publication("file:///project/package.json")
            .await
            .diagnostics
            .is_empty()
    );
    test.session
        .set_project_analysis_request(project, Some(Default::default()));
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication("file:///project/package.json").await;
    test.session
        .workspace()
        .close_project(CloseProjectParams {
            project_key: project,
        })
        .unwrap();
    test.session.reconcile_project_diagnostics(false).await;
    assert!(
        test.publication("file:///project/package.json")
            .await
            .diagnostics
            .is_empty()
    );
}

#[tokio::test]
async fn project_revision_rejects_stale_publish() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    test.fs.insert("/project/index.js".into(), "export {};\n");
    let uri = test.open(project, "/project/index.js", "export {};\n");
    let old = test.pull(project);
    let barrier = test
        .session
        .project_diagnostics
        .hooks
        .before_project
        .pause();
    let session = test.session.clone();
    let stale = tokio::spawn(async move {
        session.publish_project_result(project, old).await;
    });
    barrier.wait().await;
    test.fs.insert(
        "/project/package.json".into(),
        "{\"private\":true,\"dependencies\":{\"new-unused\":\"1\"}}",
    );
    let current = test.pull(project);
    let current_revision = current.revision.clone();
    test.session.publish_project_result(project, current).await;
    let published = test.publication("file:///project/package.json").await;
    let published_revision: biome_project_analysis::model::SnapshotRevision =
        serde_json::from_value(published.diagnostics[0].data.as_ref().unwrap()["revision"].clone())
            .unwrap();
    assert_eq!(published_revision, current_revision);
    assert_eq!(test.session.document(&uri).unwrap().version, 7);
    test.fence().await;
    while test.notifications.try_recv().is_ok() {}
    barrier.wait().await;
    stale.await.unwrap();
    test.fence().await;
    assert!(test.notifications.try_recv().is_err());
}

#[tokio::test]
async fn cross_file_evidence_uses_target_line_index() {
    let test = Harness::new().await;
    let project = test.project("/project");
    test.fs.insert(
        "/project/related.js".into(),
        "\n\n\"𐐀\"; export const value = 1;\n",
    );
    let mut result = test.pull(project);
    let file = *result
        .sources
        .iter()
        .find(|(_, source)| source.logical_path.as_str() == "related.js")
        .unwrap()
        .0;
    let id = EvidenceId("related-evidence".into());
    let issue = result
        .issues
        .iter_mut()
        .find(|issue| issue.kind == IssueKind::Dependencies)
        .unwrap();
    issue.related_evidence = vec![id.clone()];
    result.evidence.nodes.insert(
        id,
        Evidence {
            producer: "test".into(),
            location: Some(Location {
                file,
                range: biome_text_size::TextRange::new(24.into(), 29.into()),
            }),
            workspace: issue.workspace,
            explanation: "Related declaration".into(),
            related: vec![],
        },
    );
    test.fs
        .insert("/project/related.js".into(), "different live text");
    for (encoding, column) in [
        (biome_lsp_converters::PositionEncoding::Utf8, 22),
        (
            biome_lsp_converters::PositionEncoding::Wide(biome_line_index::WideEncoding::Utf16),
            20,
        ),
        (
            biome_lsp_converters::PositionEncoding::Wide(biome_line_index::WideEncoding::Utf32),
            19,
        ),
    ] {
        let projected = crate::project_analysis::project_diagnostics(&result, encoding);
        let diagnostic = projected
            .values()
            .flatten()
            .find(|diagnostic| diagnostic.message.starts_with("dependencies:"))
            .unwrap();
        let related = &diagnostic.related_information.as_ref().unwrap()[0];
        assert_eq!(related.location.uri.as_str(), "file:///project/related.js");
        assert_eq!(related.location.range.start, lsp::Position::new(2, column));
    }
    result.sources.get_mut(&file).unwrap().content = SourceContent::Unavailable {
        reason: "unreadable".into(),
        fallback: "related.js".into(),
    };
    let projected = crate::project_analysis::project_diagnostics(
        &result,
        biome_lsp_converters::PositionEncoding::Utf8,
    );
    let diagnostic = projected
        .values()
        .flatten()
        .find(|diagnostic| diagnostic.message.starts_with("dependencies:"))
        .unwrap();
    assert_eq!(
        diagnostic.related_information.as_ref().unwrap()[0]
            .location
            .range,
        lsp::Range::default()
    );
}

#[tokio::test]
async fn document_close_preserves_project_findings() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    let text = "{\n\"private\":true,\n\"dependencies\":{\"unused\":\"1\"}\n}";
    let uri = test.open(project, "/project/package.json", text);
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication(uri.as_str()).await;
    let document = test.session.document(&uri).unwrap();
    test.session
        .publish_local_diagnostics(
            uri.clone(),
            &document,
            test.session.project_diagnostics.next_sequence(),
            vec![lsp::Diagnostic::new_simple(
                Default::default(),
                "local".into(),
            )],
        )
        .await;
    test.publication(uri.as_str()).await;
    test.session
        .close_document_diagnostics(uri.clone())
        .await
        .unwrap();
    let closed = test.publication(uri.as_str()).await;
    assert_eq!(closed.version, None);
    assert!(
        closed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unused"))
    );
    assert!(
        closed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message != "local")
    );
    let warm = test.pull(project);
    let fresh = biome_service::workspace::server(test.fs.clone(), None);
    let key = fresh
        .open_project(OpenProjectParams {
            path: biome_fs::BiomePath::new("/project"),
            open_uninitialized: true,
        })
        .unwrap()
        .project_key;
    let fresh = fresh
        .pull_project_diagnostics(PullProjectDiagnosticsParams {
            project_key: key,
            request: Default::default(),
            selection: Default::default(),
            selection_revision: None,
            overrides: Default::default(),
        })
        .unwrap();
    assert_eq!(
        biome_project_analysis::normalize::normalize_result(&warm),
        biome_project_analysis::normalize::normalize_result(&fresh)
    );
}

#[tokio::test]
async fn document_close_discards_overlay_before_reanalysis() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    let uri = test.open(
        project,
        "/project/package.json",
        "{\"private\":true,\"dependencies\":{\"overlay-only\":\"1\"}}",
    );
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication(uri.as_str()).await;
    let barrier = test
        .session
        .project_diagnostics
        .hooks
        .before_analysis
        .pause();
    test.session
        .close_document_diagnostics(uri.clone())
        .await
        .unwrap();
    assert!(test.publication(uri.as_str()).await.diagnostics.is_empty());
    barrier.wait().await;
    let result = test.pull(project);
    test.session.publish_project_result(project, result).await;
    let fresh = test.publication(uri.as_str()).await;
    assert!(
        fresh
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unused"))
    );
    assert!(
        !fresh
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("overlay-only"))
    );
    barrier.wait().await;
}

#[tokio::test]
async fn interleaved_local_clear_keeps_project_findings() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    let text = "{\n\"private\":true,\n\"dependencies\":{\"unused\":\"1\"}\n}";
    let uri = test.open(project, "/project/package.json", text);
    let document = test.session.document(&uri).unwrap();
    let barrier = test.session.project_diagnostics.hooks.before_local.pause();
    let session = test.session.clone();
    let target = uri.clone();
    let local = tokio::spawn(async move {
        session
            .publish_local_diagnostics(
                target,
                &document,
                session.project_diagnostics.next_sequence(),
                vec![],
            )
            .await;
    });
    barrier.wait().await;
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication(uri.as_str()).await;
    barrier.wait().await;
    local.await.unwrap();
    assert!(
        test.publication(uri.as_str())
            .await
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unused"))
    );
}

#[tokio::test]
async fn coalesced_multi_project_completion() {
    let mut test = Harness::new().await;
    let a = test.project("/a");
    let b = test.project("/b");
    let a_result = test.pull(a);
    let b_result = test.pull(b);
    test.factory
        .service_tx
        .send_replace(ServiceNotification::IndexUpdated);
    test.subscription.wait().await;
    let mut seen = std::collections::BTreeSet::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        while seen.len() < 2 {
            if let Some(ServerNotification::PublishDiagnostics(params)) =
                test.notifications.next().await
                && !params.diagnostics.is_empty()
            {
                seen.insert(params.uri.to_string());
            }
        }
    })
    .await
    .unwrap();
    assert!(seen.contains("file:///a/package.json"));
    assert!(seen.contains("file:///b/package.json"));
    let states = test
        .session
        .workspace()
        .pull_project_analysis_states(PullProjectAnalysisStatesParams {
            project_keys: vec![a, b],
        })
        .unwrap();
    assert!(states.iter().any(|state| state.status
        == ProjectAnalysisStatus::Completed {
            revision: a_result.revision.clone()
        }));
    assert!(states.iter().any(|state| state.status
        == ProjectAnalysisStatus::Completed {
            revision: b_result.revision.clone()
        }));
    test.session
        .workspace()
        .close_project(CloseProjectParams { project_key: a })
        .unwrap();
    assert!(
        test.publication("file:///a/package.json")
            .await
            .diagnostics
            .is_empty()
    );
}

#[test]
fn project_analysis_configured_mode_and_opt_in() {
    for (enabled, mode, expected) in [
        (
            true,
            "production",
            Some(biome_project_analysis::options::AnalysisMode::Production),
        ),
        (
            true,
            "strict",
            Some(biome_project_analysis::options::AnalysisMode::Strict),
        ),
        (false, "default", None),
    ] {
        let source = biome_configuration::ConfigurationSource {
            root: Some(biome_configuration::ConfigurationSourceEntry {
                configuration: Some(
                    serde_json::from_value(
                        serde_json::json!({"projectAnalysis":{"enabled":enabled,"mode":mode}}),
                    )
                    .unwrap(),
                ),
                file_path: None,
                file_source: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            crate::project_analysis::configured_request(&source).map(|request| request.mode),
            expected
        );
    }
}

#[tokio::test]
#[ignore = "requires a current CLI in BIOME_PROJECT_ANALYSIS_CLI; mandatory separate equivalence gate"]
async fn overlay_changes_match_cli_snapshot() {
    let binary = std::env::var_os("BIOME_PROJECT_ANALYSIS_CLI")
        .expect("set BIOME_PROJECT_ANALYSIS_CLI to the current Cargo-built executable");
    let root = std::env::temp_dir().join(format!("biome-lsp-project-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let mut test = Harness::new().await;
    let root = camino::Utf8PathBuf::from_path_buf(root).unwrap();
    let configuration = r#"{"projectAnalysis":{"enabled":true,"mode":"strict","entry":["index.js"],"project":["**/*.js"]}}"#;
    let manifest = r#"{"name":"fixture","private":true,"dependencies":{"unused":"1"}}"#;
    let overlay = "console.log('overlay');\n";
    for (path, text) in [
        ("biome.json", configuration),
        ("package.json", manifest),
        ("index.js", overlay),
    ] {
        std::fs::write(root.join(path), text).unwrap();
        test.fs.insert(root.join(path), text);
    }
    test.fs.insert(root.join("index.js"), "import 'unused';\n");
    let project = test
        .session
        .workspace()
        .open_project(OpenProjectParams {
            path: root.as_path().into(),
            open_uninitialized: true,
        })
        .unwrap()
        .project_key;
    let source = biome_service::configuration::load_configuration(
        test.fs.as_ref(),
        biome_configuration::ConfigurationPathHint::FromLsp(root.clone()),
    )
    .unwrap();
    let request = crate::project_analysis::configured_request(&source.source).unwrap();
    test.session
        .set_project_analysis_request(project, Some(request.clone()));
    test.open(project, root.join("index.js").as_str(), overlay);
    let result = test
        .session
        .workspace()
        .pull_project_diagnostics(PullProjectDiagnosticsParams {
            project_key: project,
            request,
            selection: Default::default(),
            selection_revision: None,
            overrides: Default::default(),
        })
        .unwrap();
    assert_eq!(
        result.context.mode,
        biome_project_analysis::options::AnalysisMode::Strict
    );
    assert!(
        result
            .sources
            .values()
            .any(|source| source.logical_path.as_str() == "index.js"
                && matches!(&source.content,SourceContent::Available {text,..} if text==overlay))
    );
    let current_revision = result.revision.clone();
    test.session.publish_project_result(project, result).await;
    let published = test
        .publication(&format!("file://{root}/package.json"))
        .await;
    let output = std::process::Command::new(binary)
        .args(["analyze", "--reporter=json", "--colors=off"])
        .current_dir(&root)
        .env_remove("BIOME_CONFIG_PATH")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut cli_issues: Vec<_> = cli["diagnostics"].as_array().unwrap().iter().filter(|diagnostic| diagnostic["category"]=="project").map(|diagnostic| serde_json::json!({
        "category":diagnostic["category"], "message":diagnostic["message"], "severity":diagnostic["severity"], "path":diagnostic["location"]["path"],
        "start":[diagnostic["location"]["start"]["line"].as_u64().unwrap()-1,diagnostic["location"]["start"]["column"].as_u64().unwrap()-1],
        "end":[diagnostic["location"]["end"]["line"].as_u64().unwrap()-1,diagnostic["location"]["end"]["column"].as_u64().unwrap()-1],
    })).collect();
    let mut lsp_issues: Vec<_> = published.diagnostics.iter().filter(|diagnostic| diagnostic.data.is_some()).map(|diagnostic| {
        let published_revision: biome_project_analysis::model::SnapshotRevision = serde_json::from_value(diagnostic.data.as_ref().unwrap()["revision"].clone()).unwrap();
        assert_eq!(published_revision,current_revision);
        let data = diagnostic.data.as_ref().unwrap();
        let kind = data["kind"].as_str().unwrap();
        let identity = data["identity"].as_str().unwrap();
        assert_eq!(diagnostic.code, Some(lsp::NumberOrString::String(format!("project/{kind}"))));
        assert_eq!(diagnostic.message, format!("{kind}: {identity}"));
        let severity = match diagnostic.severity.unwrap() {
            lsp::DiagnosticSeverity::ERROR => "error",
            lsp::DiagnosticSeverity::WARNING => "warning",
            lsp::DiagnosticSeverity::INFORMATION => "info",
            lsp::DiagnosticSeverity::HINT => "hint",
            other => panic!("unexpected severity {other:?}"),
        };
        let path = published.uri.to_file_path().unwrap();
        let path = path.strip_prefix(root.as_std_path()).unwrap().to_str().unwrap();
        serde_json::json!({"category":"project","message":diagnostic.message,"severity":severity,"path":path,"start":[diagnostic.range.start.line,diagnostic.range.start.character],"end":[diagnostic.range.end.line,diagnostic.range.end.character]})
    }).collect();
    cli_issues.sort_by_key(serde_json::Value::to_string);
    lsp_issues.sort_by_key(serde_json::Value::to_string);
    assert!(!cli_issues.is_empty());
    assert_eq!(lsp_issues, cli_issues);
}

#[tokio::test]
async fn watcher_stopped_supersedes_completion() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication("file:///project/package.json").await;
    test.open(project, "/project/new.js", "export {};\n");
    test.factory
        .service_tx
        .send_replace(ServiceNotification::WatcherStopped);
    test.subscription.wait().await;
    assert!(
        test.publication("file:///project/package.json")
            .await
            .diagnostics
            .is_empty()
    );
}

#[tokio::test]
async fn manifest_change_during_unchanged_close_uses_current_revision() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    test.fs.insert("/project/index.js".into(), "export {};\n");
    let uri = test.open(project, "/project/index.js", "export {};\n");
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication("file:///project/package.json").await;
    let barrier = test
        .session
        .project_diagnostics
        .hooks
        .before_close_analysis
        .pause();
    let session = test.session.clone();
    let close = tokio::spawn(async move {
        session.close_document_diagnostics(uri).await.unwrap();
    });
    barrier.wait().await;
    test.fs.insert(
        "/project/package.json".into(),
        "{\"private\":true,\"dependencies\":{\"current-dependency\":\"1\"}}",
    );
    test.session
        .workspace()
        .update_settings(UpdateSettingsParams {
            project_key: project,
            configuration: Default::default(),
            workspace_directory: None,
            extended_configurations: Default::default(),
            module_graph_resolution_kind: Default::default(),
        })
        .unwrap();
    barrier.wait().await;
    close.await.unwrap();
    let published = test.publication("file:///project/package.json").await;
    assert!(
        published
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("current-dependency"))
    );
    assert!(
        !published
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unused"))
    );
    let current = test.pull(project);
    let published_revision: biome_project_analysis::model::SnapshotRevision =
        serde_json::from_value(published.diagnostics[0].data.as_ref().unwrap()["revision"].clone())
            .unwrap();
    let current_revision = current.revision;
    assert_eq!(published_revision, current_revision);
}

#[tokio::test]
async fn project_analysis_does_not_block_formatting() {
    let test = Harness::new().await;
    let project = test.project("/project");
    test.open(project, "/project/index.js", "const value={a:1};\n");
    let barrier = test
        .session
        .project_diagnostics
        .hooks
        .before_analysis
        .pause();
    test.session.reconcile_project_diagnostics(true).await;
    barrier.wait().await;
    let session = test.session.clone();
    let formatting = tokio::task::spawn_blocking(move || {
        session
            .workspace_for_request()
            .format_file(FormatFileParams {
                project_key: project,
                path: biome_fs::BiomePath::new("/project/index.js"),
                inline_config: None,
            })
    });
    let output = tokio::time::timeout(Duration::from_secs(3), formatting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(output.as_code().contains("a: 1"));
    barrier.wait().await;
}

#[tokio::test]
async fn project_capability_fallback() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    test.fs
        .insert("/project/index.js".into(), "import(process.env.MODULE);\n");
    let result = test.pull(project);
    assert!(!result.coverage.gaps().is_empty());
    test.session.publish_project_result(project, result).await;
    let notification = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(ServerNotification::PublishDiagnostics(params)) =
            test.notifications.next().await
        {
            if params.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == Some(lsp::NumberOrString::String("project/coverage".into()))
            }) {
                return params;
            }
        }
        panic!("missing coverage publication")
    })
    .await
    .unwrap();
    assert_eq!(notification.version, None);
    assert!(
        notification
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("incomplete"))
    );
}

#[tokio::test]
async fn project_close_keeps_other_contributors_for_shared_uri() {
    let mut test = Harness::new().await;
    let a = test.project("/project");
    let b = test.project("/project/packages/b");
    test.fs.insert(
        "/project/package.json".into(),
        r#"{"private":true,"workspaces":["packages/b"]}"#,
    );
    let text = "{\n\"private\":true,\n\"dependencies\":{\"unused\":\"1\"}\n}";
    let uri = test.open(b, "/project/packages/b/package.json", text);
    let a_result = test.pull(a);
    let b_result = test.pull(b);
    assert!(
        a_result
            .issues
            .iter()
            .any(|issue| a_result.sources[&issue.location.file].canonical_uri == uri.as_str())
    );
    test.session.publish_project_result(a, a_result).await;
    test.publication(uri.as_str()).await;
    test.session.publish_project_result(b, b_result).await;
    assert_eq!(test.publication(uri.as_str()).await.diagnostics.len(), 2);
    let doc = test.session.document(&uri).unwrap();
    test.session
        .publish_local_diagnostics(
            uri.clone(),
            &doc,
            test.session.project_diagnostics.next_sequence(),
            vec![lsp::Diagnostic::new_simple(
                Default::default(),
                "local".into(),
            )],
        )
        .await;
    test.publication(uri.as_str()).await;
    test.session
        .workspace()
        .close_project(CloseProjectParams { project_key: a })
        .unwrap();
    test.session.publish_project_result(b, test.pull(b)).await;
    test.session.reconcile_project_diagnostics(false).await;
    let published = test.publication(uri.as_str()).await;
    assert_eq!(published.diagnostics.len(), 2);
    assert!(
        published
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "local")
    );
    assert!(
        published
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unused"))
    );
}

#[tokio::test]
async fn missing_document_close_withdraws_project_findings() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    let text = "{\n\"private\":true,\n\"dependencies\":{\"unused\":\"1\"}\n}";
    let uri = test.open(project, "/project/package.json", text);
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication(uri.as_str()).await;
    test.fs
        .remove(camino::Utf8Path::new("/project/package.json"));
    let barrier = test
        .session
        .project_diagnostics
        .hooks
        .before_analysis
        .pause();
    test.session
        .close_document_diagnostics(uri.clone())
        .await
        .unwrap();
    assert!(test.publication(uri.as_str()).await.diagnostics.is_empty());
    barrier.wait().await;
    barrier.wait().await;
}

#[tokio::test]
async fn local_feature_disabled_clear_keeps_project_findings() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    let text = "{\n\"private\":true,\n\"dependencies\":{\"unused\":\"1\"}\n}";
    let uri = test.open(project, "/project/package.json", text);
    test.session
        .workspace()
        .update_settings(UpdateSettingsParams {
            project_key: project,
            workspace_directory: None,
            configuration: serde_json::from_value(
                serde_json::json!({"linter":{"enabled":false},"assist":{"enabled":false}}),
            )
            .unwrap(),
            extended_configurations: vec![],
            module_graph_resolution_kind: Default::default(),
        })
        .unwrap();
    test.session
        .publish_project_result(project, test.pull(project))
        .await;
    test.publication(uri.as_str()).await;
    test.session.update_diagnostics(uri.clone()).await.unwrap();
    let published = test.publication(uri.as_str()).await;
    assert_eq!(published.version, Some(7));
    assert!(
        published
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unused"))
    );
}

#[tokio::test]
async fn project_close_reanalyzes_other_dirty_projects() {
    let mut test = Harness::new().await;
    let a = test.project("/a");
    let b = test.project("/b");
    let old_b = test.pull(b).revision;
    test.subscription.wait().await;
    let mut seen = std::collections::BTreeSet::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        while seen.len() < 2 {
            if let Some(ServerNotification::PublishDiagnostics(params)) =
                test.notifications.next().await
                && !params.diagnostics.is_empty()
            {
                seen.insert(params.uri.to_string());
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        seen,
        std::collections::BTreeSet::from([
            "file:///a/package.json".to_string(),
            "file:///b/package.json".to_string()
        ])
    );
    let analysis = test
        .session
        .project_diagnostics
        .hooks
        .before_analysis
        .pause();
    test.session
        .workspace()
        .close_project(CloseProjectParams { project_key: a })
        .unwrap();
    loop {
        if test
            .publication("file:///b/package.json")
            .await
            .diagnostics
            .is_empty()
        {
            break;
        }
    }
    analysis.wait().await;
    analysis.wait().await;
    let published = test.publication("file:///b/package.json").await;
    assert!(!published.diagnostics.is_empty());
    let revision: biome_project_analysis::model::SnapshotRevision =
        serde_json::from_value(published.diagnostics[0].data.as_ref().unwrap()["revision"].clone())
            .unwrap();
    assert_ne!(revision, old_b);
    let states = test
        .session
        .workspace()
        .pull_project_analysis_states(PullProjectAnalysisStatesParams {
            project_keys: vec![b],
        })
        .unwrap();
    assert_eq!(
        states[0].status,
        ProjectAnalysisStatus::Completed { revision }
    );
}

#[tokio::test]
async fn completion_at_observed_dirty_generation() {
    let mut test = Harness::new().await;
    let project = test.project("/project");
    test.session.reconcile_project_diagnostics(false).await;
    let before = test
        .session
        .workspace()
        .pull_project_analysis_states(PullProjectAnalysisStatesParams {
            project_keys: vec![project],
        })
        .unwrap();
    assert_eq!(before[0].status, ProjectAnalysisStatus::Dirty);
    let result = test.pull(project);
    let after = test
        .session
        .workspace()
        .pull_project_analysis_states(PullProjectAnalysisStatesParams {
            project_keys: vec![project],
        })
        .unwrap();
    assert_eq!(before[0].generation, after[0].generation);
    assert_eq!(
        after[0].status,
        ProjectAnalysisStatus::Completed {
            revision: result.revision
        }
    );
    test.subscription.wait().await;
    assert!(
        !test
            .publication("file:///project/package.json")
            .await
            .diagnostics
            .is_empty()
    );
}
