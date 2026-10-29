use super::ProjectAnalysisRequest;
use biome_project_analysis::{
    analyze,
    options::{AnalysisMode, AnalysisOptions},
    result::{IssueKind, ProjectAnalysisResult},
};
use serde_json::json;

fn run(
    files: &[(&str, &str)],
    config: serde_json::Value,
    mode: AnalysisMode,
) -> ProjectAnalysisResult {
    let fs = biome_fs::MemoryFileSystem::default();
    fs.insert(
        "/project/package.json".into(),
        r#"{"private":true,"type":"module"}"#,
    );
    for (path, text) in files {
        fs.insert(format!("/project/{path}").into(), *text);
    }
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(fs, "/project");
    let request = ProjectAnalysisRequest {
        configuration: serde_json::from_value(config).unwrap(),
        mode,
        ..Default::default()
    };
    let snapshot = service.build_project_snapshot(project, &request).unwrap();
    analyze(&snapshot, &request.analysis_options(&snapshot.inventory))
}

#[test]
fn b7_configuration_reaches_owning_analysis() {
    let files = [
        ("index.ts", "import './provider';"),
        (
            "provider.ts",
            "export function used() {} used(); export const unused = 1;",
        ),
    ];
    let result = run(
        &files,
        json!({"ignoreExportsUsedInFile":true, "rules":{"exports":"warn"}}),
        AnalysisMode::Default,
    );
    let exports: Vec<_> = result
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::Exports)
        .collect();
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].symbol.as_deref(), Some("unused"));
    assert_eq!(exports[0].severity, biome_diagnostics::Severity::Warning);
}

#[test]
fn b7_source_tag_attachment_and_native_identity() {
    let result = run(
        &[
            ("index.ts", "import './provider';"),
            (
                "provider.ts",
                "// @probe_1\nexport const hidden = 1;\nexport const before = 2; // @public\nexport const after = 3;\nexport const block = 4; /* @public */\nexport const next = 5;\n",
            ),
        ],
        json!({"tags":["-probe_1"]}),
        AnalysisMode::Default,
    );
    let mut names: Vec<_> = result
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::Exports)
        .map(|issue| issue.symbol.as_deref().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["after", "before", "block"]);
}

#[test]
fn b7_default_index_stays_live_in_all_modes() {
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let result = run(
            &[
                ("index.ts", "import {used} from './provider'; used;"),
                ("provider.ts", "export const used = 1;"),
            ],
            json!({}),
            mode,
        );
        assert!(result.issues.is_empty());
        assert_eq!(
            result
                .membership
                .files
                .iter()
                .filter(|file| file.definite)
                .count(),
            2
        );
    }
    assert_eq!(AnalysisOptions::default().mode, AnalysisMode::Default);
}
