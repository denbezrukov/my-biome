use crate::server_test_utils::{Server, ServerFactory};
use biome_fs::{BiomePath, MemoryFileSystem};
use biome_service::workspace::*;
use std::sync::Arc;

/// Exercises the real custom-method registry and its JSON serializer.
#[tokio::test]
async fn project_diagnostics_registry_round_trip() -> anyhow::Result<()> {
    let fs = Arc::new(MemoryFileSystem::default());
    fs.insert(
        "/project/package.json".into(),
        r#"{"private":true,"dependencies":{"unused":"1"}}"#,
    );
    fs.insert("/project/orphan.ts".into(), "export {};");
    let factory = ServerFactory::new_with_fs(fs.clone());
    let (service, _socket) = factory.create().into_inner();
    let mut rpc = Server::new(service);
    rpc.initialize().await?;
    let opened: OpenProjectResult = rpc
        .request(
            "biome/open_project",
            "open",
            OpenProjectParams {
                path: BiomePath::new("/project"),
                open_uninitialized: true,
            },
        )
        .await?
        .unwrap();
    let key = opened.project_key;
    let params = PullProjectDiagnosticsParams {
        project_key: key,
        request: Default::default(),
        selection: Default::default(),
        selection_revision: None,
        overrides: Default::default(),
    };
    let remote: ProjectAnalysisResult = rpc
        .request("biome/pull_project_diagnostics", "pull", &params)
        .await?
        .unwrap();
    let wire = serde_json::to_value(&remote)?;
    assert_eq!(
        remote,
        serde_json::from_value::<ProjectAnalysisResult>(wire)?
    );
    let states: Vec<ProjectAnalysisState> = rpc
        .request(
            "biome/pull_project_analysis_states",
            "state",
            PullProjectAnalysisStatesParams {
                project_keys: vec![key],
            },
        )
        .await?
        .unwrap();
    assert_eq!(
        states[0].status,
        ProjectAnalysisStatus::Completed {
            revision: remote.revision.clone()
        }
    );
    let local = biome_service::workspace::server(fs, None);
    let local_key = local
        .open_project(OpenProjectParams {
            path: BiomePath::new("/project"),
            open_uninitialized: true,
        })?
        .project_key;
    let mut local_params = params.clone();
    local_params.project_key = local_key;
    let local_result = local.pull_project_diagnostics(local_params)?;
    assert_eq!(
        biome_project_analysis::normalize::normalize_result(&local_result),
        biome_project_analysis::normalize::normalize_result(&remote)
    );
    assert_ne!(local_result.revision, remote.revision);
    assert_eq!(
        local.pull_project_analysis_states(PullProjectAnalysisStatesParams {
            project_keys: vec![local_key]
        })?[0]
            .status,
        ProjectAnalysisStatus::Completed {
            revision: local_result.revision
        }
    );
    let mut overridden = params.clone();
    overridden.overrides.exclude = vec![biome_project_analysis::result::IssueKind::Files];
    let overridden: ProjectAnalysisResult = rpc
        .request("biome/pull_project_diagnostics", "override", overridden)
        .await?
        .unwrap();
    assert!(
        !overridden
            .issues
            .iter()
            .any(|issue| issue.kind == biome_project_analysis::result::IssueKind::Files)
    );
    assert!(
        overridden
            .issues
            .iter()
            .any(|issue| issue.kind == biome_project_analysis::result::IssueKind::Dependencies)
    );
    assert_ne!(
        overridden.context.semantic_options_digest,
        remote.context.semantic_options_digest
    );
    let mut empty = params.clone();
    empty.selection.restrict_workspaces = true;
    let selected: ProjectAnalysisResult = rpc
        .request("biome/pull_project_diagnostics", "empty", empty)
        .await?
        .unwrap();
    assert!(!remote.issues.is_empty());
    assert!(selected.issues.is_empty());
    assert_eq!(selected.evidence, remote.evidence);
    let mut selected = params;
    selected
        .selection
        .workspaces
        .extend(remote.workspaces.keys().copied());
    assert!(
        rpc.request::<_, ProjectAnalysisResult>(
            "biome/pull_project_diagnostics",
            "unbound",
            &selected
        )
        .await
        .is_err()
    );
    selected.selection_revision = Some(remote.revision);
    let bound: ProjectAnalysisResult = rpc
        .request("biome/pull_project_diagnostics", "bound", selected)
        .await?
        .unwrap();
    assert_eq!(bound.issues, remote.issues);
    rpc.request::<_, ()>(
        "biome/close_project",
        "close",
        CloseProjectParams { project_key: key },
    )
    .await?;
    let states: Vec<ProjectAnalysisState> = rpc
        .request(
            "biome/pull_project_analysis_states",
            "closed",
            PullProjectAnalysisStatesParams {
                project_keys: vec![key, biome_service::projects::ProjectKey::new()],
            },
        )
        .await?
        .unwrap();
    assert!(
        states
            .iter()
            .all(|state| state.status == ProjectAnalysisStatus::Closed)
    );
    Ok(())
}
