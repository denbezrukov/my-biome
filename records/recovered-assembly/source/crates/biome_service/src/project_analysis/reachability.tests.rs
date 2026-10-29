use super::ProjectAnalysisRequest;
use biome_fs::MemoryFileSystem;
use biome_project_analysis::{
    analyze,
    coverage::Coverage,
    inventory::{ExportProtection, RootRole},
    model::{AnalysisSnapshot, FileId},
    options::{AnalysisMode, AnalysisOptions, ReportSelection},
    reachability::compute_reachability,
    result::ProjectAnalysisResult,
    select_report,
};
use serde_json::json;
use std::sync::Arc;

fn run(
    files: &[(&str, &str)],
    config: serde_json::Value,
    mode: AnalysisMode,
) -> (Arc<AnalysisSnapshot>, ProjectAnalysisResult) {
    let fs = MemoryFileSystem::default();
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
    let result = analyze(
        &snapshot,
        &AnalysisOptions {
            mode,
            ..Default::default()
        },
    );
    (snapshot, result)
}
fn id(snapshot: &AnalysisSnapshot, path: &str) -> FileId {
    *snapshot
        .sources
        .iter()
        .find(|(_, source)| source.logical_path == path)
        .unwrap()
        .0
}
fn issues(result: &ProjectAnalysisResult) -> Vec<&str> {
    result
        .issues
        .iter()
        .map(|issue| result.sources[&issue.location.file].logical_path.as_str())
        .collect()
}
fn config() -> serde_json::Value {
    json!({"entry":["index.ts!"],"project":["**/*.ts","**/*.js","**/*.mts","**/*.cts"]})
}

#[test]
fn default_entries_are_production_roots() {
    let files = [
        (
            "package.json",
            include_str!(
                "../../../biome_configuration/tests/fixtures/default-entries-production/package.json"
            ),
        ),
        (
            "src/index.ts",
            include_str!(
                "../../../biome_configuration/tests/fixtures/default-entries-production/src/index.ts"
            ),
        ),
        (
            "src/live.ts",
            include_str!(
                "../../../biome_configuration/tests/fixtures/default-entries-production/src/live.ts"
            ),
        ),
        ("alternate.ts", "export {};"),
    ];
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let (snapshot, result) = run(&files, json!({}), mode);
        let index = id(&snapshot, "src/index.ts");
        let live = id(&snapshot, "src/live.ts");
        let root = result
            .membership
            .entry_roots
            .iter()
            .find(|root| root.file == index)
            .unwrap();
        assert_eq!(root.producer, "configuration entry");
        assert_eq!(root.role, RootRole::Production);
        assert_eq!(root.exports, ExportProtection::IncludeWhenRequested);
        assert_eq!(root.location, None);
        assert!(
            result
                .membership
                .files
                .iter()
                .any(|file| file.file == live && file.definite)
        );
        let collected = snapshot.modules[&index].collected.as_ref().unwrap();
        assert!(collected.resolutions.iter().any(|resolution|matches!(resolution.outcome, biome_project_analysis::facts::ResolutionOutcome::Resolved {file,..} if file == live)));
        let (snapshot, result) = run(&files, json!({"entry":["alternate.ts"]}), mode);
        assert!(
            !result
                .membership
                .files
                .iter()
                .any(|file| file.file == id(&snapshot, "src/index.ts")
                    || file.file == id(&snapshot, "src/live.ts"))
        );
        assert_eq!(
            result.membership.entry_roots.len(),
            usize::from(mode == AnalysisMode::Default)
        );
    }
}

#[test]
fn bounded_loader_keeps_unrelated_findings() {
    for loader in [
        "import(flag ? './plugins/a.js' : './plugins/b.js');",
        "require(flag ? './plugins/a.js' : './plugins/b.js');",
        "import(flag ? `./plugins/a.js` : `./plugins/b.js`);",
        "require(flag ? `./plugins/a.js` : `./plugins/b.js`);",
    ] {
        let (snapshot, result) = run(
            &[
                ("index.ts", loader),
                ("plugins/a.js", "import './provider.js';"),
                ("plugins/b.js", "export {};"),
                ("plugins/provider.js", "export {};"),
                ("orphan.ts", "export {};"),
            ],
            config(),
            AnalysisMode::Default,
        );
        assert_eq!(issues(&result), ["orphan.ts"]);
        assert!(matches!(result.coverage, Coverage::Incomplete(_)));
        let reachable = compute_reachability(&snapshot, &Default::default());
        assert!(
            reachable
                .possible_files
                .contains(&id(&snapshot, "plugins/provider.js"))
        );
        assert!(
            !reachable
                .definite_files
                .contains(&id(&snapshot, "plugins/a.js"))
        );
    }
}

fn assert_tagged_loader_is_unbounded(loader: &str) {
    let results: Vec<_> = ["`./decoy.js`", "``"]
        .into_iter()
        .map(|template| {
            let source = format!(
                "function tag() {{ return './actual.js'; }} {loader}(flag ? tag{template} : './other.js');"
            );
            let (snapshot, result) = run(
                &[
                    ("index.ts", &source),
                    ("actual.js", "export {};"),
                    ("decoy.js", "export {};"),
                    ("other.js", "export {};"),
                ],
                config(),
                AnalysisMode::Default,
            );
            (snapshot, result)
        })
        .collect();
    assert_eq!(
        results
            .iter()
            .map(|(_, result)| issues(result))
            .collect::<Vec<_>>(),
        vec![Vec::<&str>::new(), Vec::new()],
        "{loader}: nonempty and empty tagged templates can load actual.js"
    );
    for (snapshot, result) in results {
        assert!(matches!(result.coverage, Coverage::Incomplete(_)));
        assert!(result.gaps.iter().any(|gap| gap.active
            && gap.gap.producer == "module collection"
            && matches!(
                gap.gap.domain,
                biome_project_analysis::coverage::TargetDomain::Unbounded
            )));
        assert!(
            snapshot.modules[&id(&snapshot, "index.ts")]
                .collected
                .as_ref()
                .unwrap()
                .imports
                .is_empty()
        );
    }
}

#[test]
fn tagged_dynamic_import_alternatives_remain_unbounded() {
    assert_tagged_loader_is_unbounded("import");
}

#[test]
fn tagged_require_alternatives_remain_unbounded() {
    assert_tagged_loader_is_unbounded("require");
}

#[test]
fn sibling_unbounded_loader_and_selection() {
    let (snapshot, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"workspaces":["packages/*"]}"#,
            ),
            ("packages/a/package.json", r#"{"private":true,"name":"a"}"#),
            ("packages/b/package.json", r#"{"private":true,"name":"b"}"#),
            ("packages/c/package.json", r#"{"private":true,"name":"c"}"#),
            ("index.ts", "import './packages/a/index';"),
            ("packages/a/index.ts", "import(name);"),
            ("packages/b/provider.ts", "import '../c/provider';"),
            ("packages/c/provider.ts", "export const value = 1;"),
        ],
        json!({"entry":["index.ts!"],"project":["**/*.ts"],"workspaces":{"packages/*":{"entry":[],"project":["**/*.ts"]}}}),
        AnalysisMode::Default,
    );
    assert!(matches!(result.coverage, Coverage::Incomplete(_)));
    assert!(result.issues.is_empty());
    assert_eq!(snapshot.inventory.workspaces.len(), 4);
    let a = snapshot
        .inventory
        .files
        .iter()
        .find(|file| file.file == id(&snapshot, "packages/a/index.ts"))
        .unwrap()
        .workspace;
    let c = snapshot
        .inventory
        .files
        .iter()
        .find(|file| file.file == id(&snapshot, "packages/c/provider.ts"))
        .unwrap()
        .workspace;
    let provider = snapshot.modules[&id(&snapshot, "packages/b/provider.ts")]
        .collected
        .as_ref()
        .unwrap();
    assert!(provider.resolutions.iter().any(|resolution|matches!(resolution.outcome,biome_project_analysis::facts::ResolutionOutcome::Resolved {file,..} if file == id(&snapshot,"packages/c/provider.ts"))));
    let workspace = snapshot
        .inventory
        .files
        .iter()
        .find(|file| file.file == id(&snapshot, "packages/b/provider.ts"))
        .unwrap()
        .workspace;
    assert_ne!(workspace, a);
    assert_ne!(workspace, c);
    let selected = select_report(
        &result,
        &ReportSelection {
            workspaces: [workspace].into(),
            ..Default::default()
        },
    );
    assert_eq!(selected.coverage, result.coverage);
    assert_eq!(selected.evidence, result.evidence);
    assert_eq!(selected.sources, result.sources);
    assert!(selected.issues.is_empty());
}

#[test]
fn analysis_membership_excludes_disconnected_candidates() {
    for reached in [false, true] {
        let (snapshot, result) = run(
            &[
                (
                    "index.ts",
                    if reached {
                        "import './a';"
                    } else {
                        "export {};"
                    },
                ),
                ("a.ts", "import './b'; import './missing'; import(name);"),
                ("b.ts", "import './a';"),
                ("orphan.ts", "export {};"),
            ],
            config(),
            AnalysisMode::Default,
        );
        if reached {
            assert!(result.issues.is_empty());
            assert!(matches!(result.coverage, Coverage::Incomplete(_)));
        } else {
            assert_eq!(issues(&result), ["a.ts", "b.ts", "orphan.ts"]);
            assert_eq!(result.coverage, Coverage::Complete);
            assert_eq!(result.membership.files.len(), 1);
            assert_eq!(result.membership.files[0].file, id(&snapshot, "index.ts"));
        }
    }
}

#[test]
fn ambient_declaration_file_is_not_unused() {
    let (snapshot, result) = run(
        &[
            ("globals.d.ts", "declare const APP_VERSION: string;"),
            (
                "marker.d.mts",
                "export {}; declare global { interface Widget {} }",
            ),
            ("common.d.cts", "declare const VALUE: string;"),
            ("dependency.d.ts", "import './dead'; export {};"),
            ("dead.ts", "export const dead = 1;"),
            ("exported.d.ts", "export interface Dead {}"),
            ("invalid.d.ts", "export interface {"),
        ],
        json!({"entry":[],"project":["**/*.ts","**/*.mts","**/*.cts"]}),
        AnalysisMode::Default,
    );
    assert_eq!(
        issues(&result),
        ["dead.ts", "exported.d.ts", "invalid.d.ts"]
    );
    assert!(result.membership.files.is_empty());
    assert!(snapshot.inventory.roots.is_empty());
}

#[test]
fn ignored_consumer_still_keeps_target() {
    let (snapshot, result) = run(
        &[
            ("index.ts", "import './ignored';"),
            ("ignored.ts", "import './provider';"),
            ("provider.ts", "export const unused = 1;"),
            ("orphan.ts", "export {};"),
        ],
        json!({"entry":["index.ts!"],"project":["**/*.ts"],"ignore":["ignored.ts"]}),
        AnalysisMode::Default,
    );
    assert_eq!(issues(&result), ["orphan.ts"]);
    assert!(
        result
            .membership
            .files
            .iter()
            .any(|file| file.file == id(&snapshot, "provider.ts") && file.definite)
    );
}

#[test]
fn reporting_suppression_preserves_distinct_membership_and_global_scope() {
    for policy in ["ignore", "ignoreFiles"] {
        let mut settings = json!({"entry":["index.ts!"],"project":["**/*.ts"],"workspaces":{"packages/*":{"entry":[],"project":["**/*.ts"]}}});
        settings[policy] = json!(["packages/*/hidden.ts"]);
        settings["workspaces"]["packages/*"][policy] = json!(["local.ts"]);
        let (snapshot, result) = run(
            &[
                (
                    "package.json",
                    r#"{"private":true,"workspaces":["packages/*"]}"#,
                ),
                ("packages/b/package.json", r#"{"private":true,"name":"b"}"#),
                ("index.ts", "import './packages/b/hidden';"),
                ("packages/b/hidden.ts", "import './provider';"),
                ("packages/b/provider.ts", "export const unused = 1;"),
                ("packages/b/local.ts", "export {};"),
                ("packages/b/orphan.ts", "export {};"),
            ],
            settings,
            AnalysisMode::Default,
        );
        assert_eq!(issues(&result), ["packages/b/orphan.ts"]);
        let hidden = result
            .membership
            .files
            .iter()
            .find(|file| file.file == id(&snapshot, "packages/b/hidden.ts"))
            .unwrap();
        assert!(hidden.definite);
        assert_eq!(hidden.diagnostic_eligible, policy == "ignoreFiles");
        assert!(
            result
                .membership
                .files
                .iter()
                .any(|file| file.file == id(&snapshot, "packages/b/provider.ts") && file.definite)
        );
    }
}

#[test]
fn possible_loader_producers_and_unsupported_choices_are_conservative() {
    for loader in [
        "import(flag ? './a.js' : './b.js');",
        "require(flag ? './a.js' : './b.js');",
        "import(flag ? './a.js' : name);",
        "import(`./plugins/${name}`);",
        "export interface {",
    ] {
        let (_, result) = run(
            &[
                ("index.ts", loader),
                ("a.js", "import(name);"),
                ("b.js", "export {};"),
                ("orphan.ts", "export {};"),
            ],
            config(),
            AnalysisMode::Default,
        );
        assert!(result.issues.is_empty(), "{loader}");
        assert!(matches!(result.coverage, Coverage::Incomplete(_)));
    }
    let (_, result) = run(
        &[
            ("index.ts", "export {};"),
            ("dead.ts", "import(flag ? './a.js' : './b.js');"),
            ("a.js", "import(name);"),
            ("b.js", "export {};"),
            ("orphan.ts", "export {};"),
        ],
        config(),
        AnalysisMode::Default,
    );
    assert_eq!(issues(&result), ["a.js", "b.js", "dead.ts", "orphan.ts"]);
    assert_eq!(result.coverage, Coverage::Complete);
}

#[test]
fn side_effect_import_keeps_file_only_and_retains_contexts() {
    use biome_project_analysis::facts::{ResolutionOutcome, ResolutionPurpose};
    let (snapshot, result) = run(
        &[
            ("index.ts", "import 'library';"),
            (
                "node_modules/library/package.json",
                r#"{"name":"library","exports":{"types":"./types.d.ts","import":"./runtime.js"}}"#,
            ),
            ("node_modules/library/types.d.ts", "export interface T {}"),
            (
                "node_modules/library/runtime.js",
                "export const unused = 1;",
            ),
        ],
        config(),
        AnalysisMode::Default,
    );
    assert!(result.issues.is_empty());
    let module = snapshot.modules[&id(&snapshot, "index.ts")]
        .collected
        .as_ref()
        .unwrap();
    assert_eq!(module.imports.len(), 1);
    assert!(module.imports[0].bindings.is_empty());
    assert_eq!(module.resolutions.len(), 2);
    for resolution in &module.resolutions {
        let ResolutionOutcome::Resolved { file, .. } = resolution.outcome else {
            panic!("{resolution:?}")
        };
        let expected = if resolution.context.purpose == ResolutionPurpose::Runtime {
            "node_modules/library/runtime.js"
        } else {
            "node_modules/library/types.d.ts"
        };
        assert_eq!(snapshot.sources[&file].logical_path, expected);
        assert!(
            result
                .membership
                .files
                .iter()
                .any(|member| member.file == file
                    && member.definite
                    && !member.diagnostic_eligible)
        );
    }
}

#[test]
fn tsconfig_program_and_declaration_roots_are_distinct_from_candidates() {
    let files = &[
        (
            "tsconfig.json",
            r#"{"files":["program.ts","globals.d.ts"]}"#,
        ),
        ("program.ts", "import './provider';"),
        ("globals.d.ts", "declare const APP_VERSION: string;"),
        ("provider.ts", "export const unused = 1;"),
        ("orphan.ts", "export {};"),
    ];
    let (snapshot, result) = run(
        files,
        json!({"entry":[],"project":["**/*.ts"],"useTsconfigFiles":true}),
        AnalysisMode::Default,
    );
    assert!(result.issues.is_empty());
    assert!(
        !result
            .membership
            .reporting_candidates
            .contains(&id(&snapshot, "orphan.ts"))
    );
    assert!(
        !result
            .membership
            .files
            .iter()
            .any(|file| file.file == id(&snapshot, "orphan.ts"))
    );
    assert_eq!(result.membership.program_roots.len(), 1);
    assert_eq!(
        result.membership.program_roots[0].file,
        id(&snapshot, "program.ts")
    );
    assert!(
        result
            .membership
            .entry_roots
            .iter()
            .any(|root| root.file == id(&snapshot, "globals.d.ts")
                && root.producer == "tsconfig declaration")
    );
    let (snapshot, result) = run(
        files,
        json!({"entry":[],"project":["**/*.ts"],"useTsconfigFiles":false}),
        AnalysisMode::Default,
    );
    assert!(
        result
            .membership
            .reporting_candidates
            .contains(&id(&snapshot, "orphan.ts"))
    );
    assert_eq!(issues(&result), ["orphan.ts", "program.ts", "provider.ts"]);
}

#[test]
fn warm_and_fresh_results_match_and_old_evidence_keeps_its_text() {
    use biome_project_analysis::{model::SourceContent, normalize::normalize_result};
    let fs = MemoryFileSystem::default();
    fs.insert(
        "/project/package.json".into(),
        r#"{"private":true,"type":"module"}"#,
    );
    fs.insert("/project/index.ts".into(), "import './provider';");
    fs.insert("/project/provider.ts".into(), "export const value = 1;");
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(
        MemoryFileSystem::from_files(fs.files.0.clone()),
        "/project",
    );
    let request = ProjectAnalysisRequest::default();
    let first = service.build_project_snapshot(project, &request).unwrap();
    let first_result = analyze(&first, &Default::default());
    fs.insert("/project/index.ts".into(), "export {};");
    let warm = service.build_project_snapshot(project, &request).unwrap();
    let (fresh_service, fresh_project) = crate::test_utils::setup_workspace_and_open_project(
        MemoryFileSystem::from_files(fs.files.0.clone()),
        "/project",
    );
    let fresh = fresh_service
        .build_project_snapshot(fresh_project, &request)
        .unwrap();
    let warm_result = analyze(&warm, &Default::default());
    assert_eq!(issues(&warm_result), ["provider.ts"]);
    assert_eq!(
        normalize_result(&warm_result),
        normalize_result(&analyze(&fresh, &Default::default()))
    );
    assert_ne!(warm.revision, fresh.revision);
    assert!(
        matches!(&first_result.sources[&id(&first,"index.ts")].content, SourceContent::Available {text,..} if text == "import './provider';")
    );
    assert!(first_result.issues.is_empty());
}

#[test]
fn resolution_metadata_uncertainty_retains_consumer_provenance() {
    for reached in [false, true] {
        let (snapshot, result) = run(
            &[
                (
                    "index.ts",
                    if reached {
                        "import './consumer';"
                    } else {
                        "export {};"
                    },
                ),
                ("consumer.ts", "import 'library';"),
                ("orphan.ts", "export {};"),
                (
                    "node_modules/library/package.json",
                    r#"{"name":false,"main":"./index.js"}"#,
                ),
                ("node_modules/library/index.js", "export {};"),
            ],
            config(),
            AnalysisMode::Default,
        );
        assert!(matches!(result.coverage, Coverage::Incomplete(_)));
        let gaps: Vec<_> = result
            .gaps
            .iter()
            .filter(|gap| gap.gap.producer == "resolution metadata")
            .collect();
        assert!(!gaps.is_empty());
        for gap in gaps {
            assert_eq!(gap.active, reached);
            assert!(gap.consumers.contains(&id(&snapshot, "consumer.ts")));
            assert_ne!(gap.gap.location.unwrap().file, id(&snapshot, "consumer.ts"));
        }
        if reached {
            assert!(result.issues.is_empty());
        } else {
            assert_eq!(issues(&result), ["consumer.ts", "orphan.ts"]);
        }
    }
}

#[test]
fn explicit_isolation_retains_boundary_coverage_and_accessible_scope() {
    let fs = MemoryFileSystem::default();
    fs.insert(
        "/project/package.json".into(),
        r#"{"private":true,"workspaces":["packages/*"]}"#,
    );
    fs.insert(
        "/project/packages/a/package.json".into(),
        r#"{"name":"a","private":true}"#,
    );
    fs.insert(
        "/project/packages/b/package.json".into(),
        r#"{"name":"b","private":true}"#,
    );
    fs.insert("/project/packages/a/index.ts".into(), "import(name);");
    fs.insert("/project/packages/b/orphan.ts".into(), "export {};");
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(fs, "/project");
    let request = ProjectAnalysisRequest {
        configuration: serde_json::from_value(
            json!({"entry":[],"workspaces":{"packages/*":{"entry":[],"project":["**/*.ts"]}}}),
        )
        .unwrap(),
        isolation: Some(super::AnalysisBoundary {
            excluded_consumers: vec!["packages/a".into()],
        }),
        ..Default::default()
    };
    let snapshot = service.build_project_snapshot(project, &request).unwrap();
    let result = analyze(&snapshot, &Default::default());
    assert!(result.issues.is_empty());
    assert!(matches!(result.coverage, Coverage::Incomplete(_)));
    assert!(
        result
            .gaps
            .iter()
            .any(|gap| gap.active && gap.gap.producer == "analysis boundary")
    );
    let excluded = snapshot
        .inventory
        .workspaces
        .iter()
        .find(|workspace| workspace.path == "packages/a")
        .unwrap()
        .workspace;
    assert!(!result.context.scope.contains(&excluded));
    assert!(result.workspaces.contains_key(&excluded));
}

#[test]
fn manifest_roots_keep_dependencies_in_all_modes() {
    let files = [
        (
            "package.json",
            r#"{"private":true,"main":"main.ts","types":"types.d.ts","bin":"bin.ts","exports":{"import":"export.ts"}}"#,
        ),
        ("main.ts", "import './provider';"),
        ("types.d.ts", "import './provider'; export {};"),
        ("bin.ts", "import './provider';"),
        ("export.ts", "import './provider';"),
        ("provider.ts", "export const value = 1;"),
        ("orphan.ts", "export {};"),
    ];
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let (snapshot, result) = run(&files, json!({"entry":[]}), mode);
        assert_eq!(issues(&result), ["orphan.ts"]);
        assert_eq!(result.membership.entry_roots.len(), 4);
        assert!(
            result
                .membership
                .entry_roots
                .iter()
                .all(|root| root.producer == "manifest entry"
                    && root.role == RootRole::Production
                    && root.exports == ExportProtection::IncludeWhenRequested
                    && root.location.is_some())
        );
        assert!(
            result
                .membership
                .files
                .iter()
                .any(|file| file.file == id(&snapshot, "provider.ts") && file.definite)
        );
    }
}

#[test]
fn unsupported_reporting_patterns_preserve_graph_membership() {
    for policy in ["ignore", "ignoreFiles"] {
        let files = [
            ("index.ts", "import './provider';"),
            ("provider.ts", "export const value = 1;"),
            ("private.ts", "export {};"),
            ("orphan.ts", "export {};"),
        ];
        let mut unsupported = config();
        unsupported[policy] = json!(["[p]rivate.ts"]);
        let (snapshot, result) = run(&files, unsupported, AnalysisMode::Default);
        assert!(matches!(result.coverage, Coverage::Incomplete(_)));
        assert!(result.issues.is_empty());
        assert_eq!(result.membership.files.len(), 2);
        assert!(
            result
                .membership
                .files
                .iter()
                .all(|file| file.definite && !file.possible)
        );
        let provider = result
            .membership
            .files
            .iter()
            .find(|file| file.file == id(&snapshot, "provider.ts"))
            .unwrap();
        assert_eq!(provider.diagnostic_eligible, policy == "ignoreFiles");
        let mut supported = config();
        supported[policy] = json!(["private.ts"]);
        let (_, control) = run(&files, supported, AnalysisMode::Default);
        assert_eq!(control.coverage, Coverage::Complete);
        assert_eq!(issues(&control), ["orphan.ts"]);
        assert_eq!(
            result.membership.entry_roots,
            control.membership.entry_roots
        );
        assert_eq!(
            result
                .membership
                .files
                .iter()
                .map(|file| (file.file, file.definite, file.possible))
                .collect::<Vec<_>>(),
            control
                .membership
                .files
                .iter()
                .map(|file| (file.file, file.definite, file.possible))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn declared_scope_keeps_manifest_only_workspaces_and_explicit_exclusions() {
    for excluded in [Vec::<&str>::new(), vec!["packages/empty"], vec!["**"]] {
        let (snapshot, result) = run(
            &[
                (
                    "package.json",
                    r#"{"private":true,"workspaces":["packages/*"]}"#,
                ),
                (
                    "packages/empty/package.json",
                    r#"{"private":true,"name":"empty"}"#,
                ),
            ],
            json!({"entry":[],"ignoreWorkspaces":excluded}),
            AnalysisMode::Default,
        );
        assert!(snapshot.inventory.files.is_empty());
        assert!(result.membership.files.is_empty());
        let paths: Vec<_> = result
            .context
            .scope
            .iter()
            .map(|id| result.workspaces[id].path.as_str())
            .collect();
        let expected = match excluded.as_slice() {
            [] => vec![".", "packages/empty"],
            ["**"] => vec![],
            _ => vec!["."],
        };
        assert_eq!(paths, expected);
        let selected = select_report(
            &result,
            &ReportSelection {
                workspaces: result.workspaces.keys().copied().take(1).collect(),
                ..Default::default()
            },
        );
        assert_eq!(selected.context, result.context);
        assert_eq!(selected.workspaces, result.workspaces);
        assert_eq!(selected.coverage, result.coverage);
    }
}
