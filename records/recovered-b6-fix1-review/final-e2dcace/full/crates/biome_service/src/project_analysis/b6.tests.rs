use super::*;
use biome_project_analysis::{
    analyze, model::AnalysisSnapshot, options::AnalysisOptions, result::IssueKind,
};
use serde_json::json;
use std::sync::Arc;
fn acquire(files: &[(&str, &str)], configuration: serde_json::Value) -> Arc<AnalysisSnapshot> {
    let fs = biome_fs::MemoryFileSystem::default();
    for (path, text) in files {
        fs.insert(format!("/project/{path}").into(), *text);
    }
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(fs, "/project");
    service
        .build_project_snapshot(
            project,
            &ProjectAnalysisRequest {
                configuration: serde_json::from_value(configuration).unwrap(),
                ..Default::default()
            },
        )
        .unwrap()
}
#[test]
fn b6_catalog_provider_precedence() {
    let root = r#"{"private":true,"catalog":{"root":"1"},"catalogs":{"tools":{"root-named":"2"}},"workspaces":{"catalog":{"legacy":"3"},"catalogs":{"tools":{"legacy-named":"4"}}}}"#;
    let base = [
        ("package.json", root),
        (
            ".yarnrc.yml",
            "catalog:\n  yarn: '1'\ncatalogs:\n  tools:\n    yarn-named: '1'\n",
        ),
    ];
    for (pnpm, expected) in [
        (Some("catalog: {}\n"), vec!["tools.root-named"]),
        (None, vec!["default.yarn", "tools.yarn-named"]),
    ] {
        let mut files = base.to_vec();
        if let Some(yaml) = pnpm {
            files.push(("pnpm-workspace.yaml", yaml));
        }
        let snapshot = acquire(&files, json!({"entry":[]}));
        let result = analyze(&snapshot, &AnalysisOptions::default());
        let mut names: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Catalog)
            .map(|i| i.symbol.as_deref().unwrap())
            .collect();
        names.sort_unstable();
        assert_eq!(names, expected);
        for issue in result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Catalog)
        {
            let source = &result.sources[&issue.location.file];
            let SourceContent::Available { text, .. } = &source.content else {
                panic!()
            };
            assert!(
                !text[usize::from(issue.location.range.start())
                    ..usize::from(issue.location.range.end())]
                    .is_empty()
            );
            if pnpm.is_some() {
                assert_eq!(source.logical_path.as_str(), "package.json");
            }
        }
        assert!(!snapshot.inventory.catalog_selections.is_empty());
    }
}
#[test]
fn b6_yaml_metadata_never_silently_skips() {
    for yaml in [
        "catalog: invalid\n",
        "catalog:\n  x: '1'\n  x: '2'\n",
        "catalogs:\n  tools: &tools\n    '@scope/x': '1'\n  other: *tools\n",
        "catalog:\n  x: [bad]\n",
    ] {
        let snapshot = acquire(
            &[
                (
                    "package.json",
                    r#"{"private":true,"dependencies":{"missing":"catalog:"}}"#,
                ),
                ("pnpm-workspace.yaml", yaml),
            ],
            json!({"entry":[]}),
        );
        let result = analyze(&snapshot, &AnalysisOptions::default());
        assert!(
            !matches!(
                result.coverage,
                biome_project_analysis::coverage::Coverage::Complete
            ),
            "{yaml}"
        );
        assert!(
            !result
                .issues
                .iter()
                .any(|i| matches!(i.kind, IssueKind::Catalog | IssueKind::CatalogReferences)),
            "{yaml}: {:?}",
            result.issues
        );
    }
    let snapshot = acquire(
        &[
            (
                "package.json",
                r#"{"private":true,"dependencies":{"@scope/x":"catalog:"}}"#,
            ),
            (
                "pnpm-workspace.yaml",
                "catalog: { '@scope/x': '1', unused: '2' }\n",
            ),
        ],
        json!({"entry":[]}),
    );
    let result = analyze(&snapshot, &AnalysisOptions::default());
    assert_eq!(
        result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Catalog)
            .count(),
        1
    );
}
#[test]
fn b6_semantic_entry_order_is_preserved() {
    let files = [
        ("package.json", r#"{"private":true}"#),
        ("a.ts", "import './b'; import './c';"),
        ("b.ts", "import './c'; import './a';"),
        ("c.ts", "import './a'; import './b';"),
    ];
    let options = AnalysisOptions {
        enabled_issues: [IssueKind::Cycles].into(),
        ..Default::default()
    };
    for (entries, first) in [
        (json!(["a.ts", "c.ts"]), "a.ts"),
        (json!(["c.ts", "a.ts"]), "c.ts"),
    ] {
        let snapshot = acquire(&files, json!({"entry":entries}));
        let result = analyze(&snapshot, &options);
        assert_eq!(result.cycles.representatives.len(), 3);
        assert_eq!(
            result.sources[&result.cycles.representatives[0].path[0]]
                .logical_path
                .as_str(),
            first
        );
    }
}

#[test]
fn b6_closed_corpus_native_expectations_and_retained_oracle() {
    use biome_project_analysis::options::AnalysisMode;
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../xtask/project_analysis/resolution-catalog-cycles/original-fixtures.json"
    ))
    .unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../xtask/project_analysis/resolution-catalog-cycles/native-expectations.json"
    ))
    .unwrap();
    let originals: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../xtask/project_analysis/resolution-catalog-cycles/original-observations.json"
    ))
    .unwrap();
    assert_eq!(fixtures.as_array().unwrap().len(), 24);
    assert_eq!(originals.as_array().unwrap().len(), 24);
    for (fixture, expected) in fixtures
        .as_array()
        .unwrap()
        .iter()
        .zip(expected.as_array().unwrap())
    {
        let id = fixture["id"].as_str().unwrap();
        assert_eq!(id, expected["case"]);
        let original = originals
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap();
        for stream in ["stdout", "stderr"] {
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(original[stream].as_str().unwrap().as_bytes())
                ),
                original[format!("{stream}_sha256")]
            );
        }
        let files: Vec<_> = fixture["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["path"].as_str().unwrap(),
                    f["content_utf8"].as_str().unwrap(),
                )
            })
            .collect();
        for file in fixture["files"].as_array().unwrap() {
            let text = file["content_utf8"].as_str().unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(text.as_bytes())),
                file["sha256"]
            );
            assert_eq!(text.len() as u64, file["bytes"].as_u64().unwrap());
        }
        let config: serde_json::Value =
            serde_json::from_str(files.iter().find(|(p, _)| *p == "knip.json").unwrap().1).unwrap();
        let mut configuration = json!({"entry":config["entry"],"project":config["project"]});
        if let Some(workspaces) = config["workspaces"].as_object() {
            configuration["workspaces"] = serde_json::Value::Object(
                workspaces
                    .iter()
                    .map(|(name, w)| {
                        (
                            name.clone(),
                            json!({"entry":w["entry"],"project":w["project"]}),
                        )
                    })
                    .collect(),
            );
        }
        let mode = if expected["mode"] == "production" {
            AnalysisMode::Production
        } else {
            AnalysisMode::Default
        };
        let fs = biome_fs::MemoryFileSystem::default();
        for (path, text) in &files {
            fs.insert(format!("/project/{path}").into(), *text);
        }
        let (service, project) =
            crate::test_utils::setup_workspace_and_open_project(fs, "/project");
        let request = ProjectAnalysisRequest {
            configuration: serde_json::from_value(configuration).unwrap(),
            mode,
            ..Default::default()
        };
        let snapshot = service.build_project_snapshot(project, &request).unwrap();
        let mut options = AnalysisOptions {
            mode,
            ..Default::default()
        };
        options.enabled_issues.insert(IssueKind::Cycles);
        options.cycles.dynamic_imports = config["cycles"]["dynamicImports"]
            .as_bool()
            .unwrap_or(false);
        if config["cycles"]["allow"].is_array() {
            options.cycles.allowed_paths =
                serde_json::from_value(config["cycles"]["allow"].clone()).unwrap();
        }
        let result = analyze(&snapshot, &options);
        if expected["cycles"].is_array() {
            let cycles: Vec<Vec<_>> = result
                .cycles
                .representatives
                .iter()
                .map(|c| {
                    c.path
                        .iter()
                        .map(|f| result.sources[f].logical_path.to_string())
                        .collect()
                })
                .filter(|path: &Vec<String>| {
                    result.issues.iter().any(|i| {
                        i.kind == IssueKind::Cycles
                            && i.symbol.as_deref() == Some(path.join(" -> ").as_str())
                    })
                })
                .collect();
            assert_eq!(json!(cycles), expected["cycles"], "{id}");
            let oracle_cycles: Vec<_> = original["json"]["issues"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|row| row["cycles"].as_array().unwrap())
                .collect();
            let oracle_paths: BTreeSet<Vec<_>> = oracle_cycles
                .iter()
                .map(|cycle| {
                    cycle
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|m| m["name"].as_str().unwrap().to_owned())
                        .collect()
                })
                .collect();
            assert_eq!(
                cycles.iter().cloned().collect::<BTreeSet<_>>(),
                oracle_paths,
                "{id}"
            );
            for cycle in &result.cycles.representatives {
                let paths: Vec<_> = cycle
                    .path
                    .iter()
                    .map(|f| result.sources[f].logical_path.as_str())
                    .collect();
                if let Some(original) = oracle_cycles.iter().find(|c| {
                    c.as_array()
                        .unwrap()
                        .iter()
                        .map(|m| m["name"].as_str().unwrap())
                        .collect::<Vec<_>>()
                        == paths
                }) {
                    for (location, member) in
                        cycle.outgoing.iter().zip(original.as_array().unwrap())
                    {
                        assert_eq!(
                            u32::from(location.range.start()) as u64,
                            member["pos"].as_u64().unwrap(),
                            "{id}"
                        );
                    }
                }
            }
            assert!(
                result
                    .issues
                    .iter()
                    .filter(|i| i.kind == IssueKind::Cycles)
                    .all(|i| i.severity == biome_diagnostics::Severity::Warning)
            );
        } else {
            let catalogs: BTreeSet<_> = result
                .issues
                .iter()
                .filter(|i| matches!(i.kind, IssueKind::Catalog | IssueKind::CatalogReferences))
                .map(|i| {
                    (
                        serde_json::to_value(i.kind)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_owned(),
                        i.symbol.clone().unwrap(),
                        result.sources[&i.location.file].logical_path.to_string(),
                    )
                })
                .collect();
            assert_eq!(json!(catalogs), expected["catalogs"], "{id}");
            if expected["incomplete"] != true {
                let oracle_pairs: BTreeSet<_> = original["json"]["issues"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|row| {
                        ["catalog", "catalogReferences"]
                            .into_iter()
                            .flat_map(move |kind| {
                                row[kind]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .map(move |issue| {
                                        (
                                            kind.to_owned(),
                                            format!(
                                                "{}.{}",
                                                issue["namespace"].as_str().unwrap(),
                                                issue["name"].as_str().unwrap()
                                            ),
                                        )
                                    })
                            })
                    })
                    .collect();
                let native_pairs: BTreeSet<_> = catalogs
                    .iter()
                    .filter(|(kind, _, _)| {
                        id != "catalog-script-source-dead" || kind != "catalogReferences"
                    })
                    .map(|(kind, name, _)| (kind.clone(), name.clone()))
                    .collect();
                assert_eq!(native_pairs, oracle_pairs, "{id}");
            } else if id == "catalog-yaml-duplicate" {
                assert_eq!(original["exit_code"], 2);
                assert!(original["json"].is_null());
                assert!(!original["stdout"].as_str().unwrap().is_empty());
            }
            if expected["incomplete"] == true {
                assert!(
                    !matches!(
                        result.coverage,
                        biome_project_analysis::coverage::Coverage::Complete
                    ),
                    "{id}"
                );
            }
            for issue in result
                .issues
                .iter()
                .filter(|i| matches!(i.kind, IssueKind::Catalog | IssueKind::CatalogReferences))
            {
                let SourceContent::Available { text, .. } =
                    &result.sources[&issue.location.file].content
                else {
                    panic!()
                };
                assert!(
                    !text[usize::from(issue.location.range.start())
                        ..usize::from(issue.location.range.end())]
                        .is_empty(),
                    "{id}"
                );
            }
        }
        let warm = service.build_project_snapshot(project, &request).unwrap();
        assert_eq!(
            biome_project_analysis::normalize::normalize_result(&result),
            biome_project_analysis::normalize::normalize_result(&analyze(&warm, &options)),
            "{id}"
        );
        assert!(
            result
                .capabilities
                .values()
                .all(|s| *s == biome_project_analysis::coverage::CapabilityStatus::Supported)
        );
    }
}

#[test]
fn b6_known_catalog_requests_and_unknown_commands_keep_separate_coverage() {
    for tool in [false, true] {
        for (entry, reachable) in [("", false), ("import './dead';", true)] {
            let manifest = if tool {
                r#"{"private":true,"scripts":{"start":"pnpm dlx keep@catalog:"}}"#
            } else {
                r#"{"private":true,"dependencies":{"keep":"catalog:"}}"#
            };
            let snapshot = acquire(
                &[
                    ("package.json", manifest),
                    (
                        "pnpm-workspace.yaml",
                        "catalog:\n  keep: '1'\n  unused: '2'\n",
                    ),
                    ("entry.ts", entry),
                    (
                        "dead.ts",
                        "import {execSync} from 'node:child_process'; execSync(command);",
                    ),
                ],
                json!({"entry":["entry.ts!"],"project":["**/*.ts!"]}),
            );
            let result = analyze(&snapshot, &AnalysisOptions::default());
            assert_eq!(
                result.issues.iter().any(|i| i.kind == IssueKind::Catalog
                    && i.symbol.as_deref() == Some("default.unused")),
                !tool && !reachable
            );
            let dead = snapshot
                .inventory
                .files
                .iter()
                .find(|f| snapshot.sources[&f.file].logical_path.as_str() == "dead.ts")
                .unwrap()
                .file;
            let activation = result
                .membership
                .script_producers
                .iter()
                .find(|p| {
                    p.producer
                        == biome_project_analysis::scripts::ScriptProducer::Source { file: dead }
                })
                .unwrap()
                .activation;
            assert_eq!(
                activation,
                if reachable {
                    biome_project_analysis::scripts::Activation::Definite
                } else if tool {
                    biome_project_analysis::scripts::Activation::Possible
                } else {
                    biome_project_analysis::scripts::Activation::Dead
                }
            );
            if tool || reachable {
                assert!(!matches!(
                    result.coverage,
                    biome_project_analysis::coverage::Coverage::Complete
                ));
            }
            assert_eq!(
                snapshot
                    .inventory
                    .script_usage
                    .gaps
                    .iter()
                    .any(|g| !g.affects_catalogs),
                tool
            );
            assert!(
                snapshot
                    .inventory
                    .script_usage
                    .gaps
                    .iter()
                    .any(|g| g.affects_catalogs)
            );
        }
    }
}
#[test]
fn b6_source_resolution_final_outcomes_and_ranges() {
    let source = "import 'node:fs'; import 'absent'; import './missing?raw'; import '#missing'; import '@local/value'; import './dist/value.js'; import 'conditions'; import './asset.css';";
    let snapshot = acquire(
        &[
            (
                "package.json",
                r#"{"private":true,"dependencies":{"conditions":"1"}}"#,
            ),
            ("entry.ts", source),
            ("src/value.ts", "export {};"),
            (
                "node_modules/conditions/package.json",
                r#"{"name":"conditions","exports":{"custom":"./value.js"}}"#,
            ),
            ("node_modules/conditions/value.js", "export {};"),
        ],
        json!({"entry":["entry.ts!"],"paths":{"@local/*":["./src/*"]},"sourceMappings":[{"output":"dist","source":"src"}]}),
    );
    let result = analyze(&snapshot, &AnalysisOptions::default());
    let unresolved: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.kind == IssueKind::Unresolved)
        .collect();
    assert_eq!(
        unresolved
            .iter()
            .map(|i| i.symbol.as_deref().unwrap())
            .collect::<BTreeSet<_>>(),
        ["./missing?raw", "#missing"].into()
    );
    for issue in unresolved {
        let raw = &source
            [usize::from(issue.location.range.start())..usize::from(issue.location.range.end())];
        assert_eq!(raw.trim_matches('\''), issue.symbol.as_deref().unwrap());
    }
    assert!(
        result
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::Unlisted && i.symbol.as_deref() == Some("absent"))
    );
    assert!(
        result
            .resolution
            .requests
            .iter()
            .any(|r| r.specifier == "conditions" && r.package_category.is_some())
    );
}

#[test]
fn b6_incremental_results_equal_fresh_after_source_and_catalog_changes() {
    let fs = Arc::new(biome_fs::MemoryFileSystem::default());
    fs.insert(
        "/project/package.json".into(),
        r#"{"private":true,"dependencies":{"keep":"catalog:"}}"#,
    );
    fs.insert(
        "/project/pnpm-workspace.yaml".into(),
        "catalog:\n  keep: '1'\n",
    );
    fs.insert("/project/entry.ts".into(), "import './a';");
    fs.insert("/project/a.ts".into(), "import './b'; import './missing';");
    fs.insert("/project/b.ts".into(), "import './a';");
    let (warm, project) =
        crate::test_utils::setup_workspace_and_open_project(fs.clone(), "/project");
    let request = ProjectAnalysisRequest {
        configuration: serde_json::from_value(json!({"entry":["entry.ts!"]})).unwrap(),
        ..Default::default()
    };
    let options = AnalysisOptions {
        enabled_issues: [IssueKind::Cycles].into(),
        ..Default::default()
    };
    for step in 0..3 {
        if step == 1 {
            fs.insert("/project/b.ts".into(), "export {};");
            fs.insert("/project/package.json".into(), r#"{"private":true}"#);
        }
        if step == 2 {
            fs.insert("/project/missing.ts".into(), "export {};");
        }
        let warm_snapshot = warm.build_project_snapshot(project, &request).unwrap();
        let (fresh, fresh_project) =
            crate::test_utils::setup_workspace_and_open_project(fs.clone(), "/project");
        let fresh_snapshot = fresh
            .build_project_snapshot(fresh_project, &request)
            .unwrap();
        let warm_result = analyze(&warm_snapshot, &options);
        let fresh_result = analyze(&fresh_snapshot, &options);
        assert_ne!(
            warm_result.revision.session_id,
            fresh_result.revision.session_id
        );
        assert_eq!(
            biome_project_analysis::normalize::normalize_result(&warm_result),
            biome_project_analysis::normalize::normalize_result(&fresh_result)
        );
        assert_eq!(
            warm_result
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::Cycles),
            step == 0
        );
        assert_eq!(
            warm_result
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::Catalog),
            step > 0
        );
        assert_eq!(
            warm_result
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::Unresolved),
            step < 2
        );
    }
}

#[test]
fn b6_script_only_context_traverses_and_credits_without_source_diagnostics() {
    let snapshot = acquire(
        &[
            (
                "package.json",
                r#"{"private":true,"scripts":{"start":"node hidden.ts"},"dependencies":{"pkg":"1"},"catalog":{"hidden":"1"}}"#,
            ),
            (
                "hidden.ts",
                "import './provider'; import './project'; import './hidden-missing'; import {execSync} from 'node:child_process'; execSync('pnpm dlx hidden@catalog:'); execSync('pnpm dlx missing@catalog:');",
            ),
            (
                "provider.ts",
                "import 'pkg'; import 'absent'; import './hidden';",
            ),
            ("project.ts", "import './candidate-missing';"),
            (
                "node_modules/pkg/package.json",
                r#"{"name":"pkg","main":"index.js"}"#,
            ),
            ("node_modules/pkg/index.js", "export {};"),
        ],
        json!({"entry":[],"project":["project.ts"]}),
    );
    let options = AnalysisOptions {
        enabled_issues: [IssueKind::Cycles].into(),
        ..Default::default()
    };
    let result = analyze(&snapshot, &options);
    assert!(!result.dependencies.declarations[0].used.is_empty());
    let source_findings: Vec<_> = result
        .issues
        .iter()
        .filter(|i| {
            matches!(
                i.kind,
                IssueKind::Unlisted
                    | IssueKind::Unresolved
                    | IssueKind::Cycles
                    | IssueKind::CatalogReferences
            )
        })
        .collect();
    assert_eq!(source_findings.len(), 1);
    assert_eq!(source_findings[0].kind, IssueKind::Unresolved);
    assert_eq!(
        source_findings[0].symbol.as_deref(),
        Some("./candidate-missing")
    );
    assert!(
        result
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::Catalog && i.symbol.as_deref() == Some("default.hidden"))
    );
    for path in ["hidden.ts", "provider.ts"] {
        let file = result
            .membership
            .files
            .iter()
            .find(|m| result.sources[&m.file].logical_path.as_str() == path)
            .unwrap();
        assert!(file.definite && !file.analyzed && !file.diagnostic_eligible);
    }
    assert!(
        result
            .membership
            .script_producers
            .iter()
            .filter(|p| matches!(
                p.producer,
                biome_project_analysis::scripts::ScriptProducer::Source { .. }
            ))
            .all(|p| p.activation == biome_project_analysis::scripts::Activation::Dead)
    );
}

#[test]
fn b6_invalid_source_catalog_uncertainty_is_membership_gated() {
    for (live, candidate) in [(false, true), (true, true), (true, false)] {
        let snapshot = acquire(
            &[
                ("package.json", r#"{"private":true,"catalog":{"keep":"1"}}"#),
                (
                    "entry.ts",
                    if live {
                        "import './broken';"
                    } else {
                        "export {};"
                    },
                ),
                ("broken.ts", "export const =;"),
            ],
            json!({"entry":["entry.ts"],"project":if candidate {json!(["*.ts"])} else {json!(["entry.ts"])}}),
        );
        let result = analyze(&snapshot, &AnalysisOptions::default());
        assert_eq!(
            result.issues.iter().any(|i| i.kind == IssueKind::Catalog),
            !(live && candidate)
        );
        assert_eq!(
            result
                .catalogs
                .gaps
                .iter()
                .any(|g| g.producer == "catalog source validity"),
            live && candidate
        );
    }
}

fn assert_catalog_consumer_manifest_uncertainty(broken: Vec<u8>) {
    let fs = Arc::new(biome_fs::MemoryFileSystem::default());
    fs.insert("/project/package.json".into(), r#"{"private":true,"workspaces":["packages/*"],"catalog":{"keep":"1","unused":"2"},"dependencies":{"missing":"catalog:"}}"#);
    fs.insert("/project/packages/app/package.json".into(), broken.clone());
    fs.insert("/project/unrelated/package.json".into(), broken);
    let (warm, project) =
        crate::test_utils::setup_workspace_and_open_project(fs.clone(), "/project");
    let request = ProjectAnalysisRequest {
        configuration: serde_json::from_value(json!({"entry":[],"project":[]})).unwrap(),
        ..Default::default()
    };
    for step in 0..3 {
        if step == 1 {
            fs.insert(
                "/project/packages/app/package.json".into(),
                r#"{"private":true,"dependencies":{"keep":"catalog:"}}"#,
            );
        }
        if step == 2 {
            fs.insert(
                "/project/packages/app/package.json".into(),
                r#"{"private":true}"#,
            );
        }
        let snapshot = warm.build_project_snapshot(project, &request).unwrap();
        let result = analyze(&snapshot, &AnalysisOptions::default());
        assert!(result.membership.files.is_empty());
        let unused: BTreeSet<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Catalog)
            .map(|i| i.symbol.as_deref().unwrap())
            .collect();
        let expected: BTreeSet<_> = match step {
            0 => [].into(),
            1 => ["default.unused"].into(),
            _ => ["default.keep", "default.unused"].into(),
        };
        assert_eq!(unused, expected, "consumer recovery step {step}");
        let missing: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::CatalogReferences)
            .collect();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].symbol.as_deref(), Some("default.missing"));
        let gaps: Vec<_> = result
            .catalogs
            .gaps
            .iter()
            .filter(|g| g.producer == "catalog consumer metadata")
            .collect();
        assert_eq!(gaps.len(), usize::from(step == 0));
        if let Some(gap) = gaps.first() {
            assert_eq!(
                result.sources[&gap.location.unwrap().file]
                    .logical_path
                    .as_str(),
                "packages/app/package.json"
            );
            assert!(matches!(
                gap.domain,
                biome_project_analysis::coverage::TargetDomain::Unbounded
            ));
        }
        assert!(
            !result.coverage.gaps().is_empty(),
            "unrelated invalid input remains incomplete even after consumer recovery"
        );
        let (fresh, fresh_project) =
            crate::test_utils::setup_workspace_and_open_project(fs.clone(), "/project");
        let fresh_snapshot = fresh
            .build_project_snapshot(fresh_project, &request)
            .unwrap();
        assert_eq!(
            biome_project_analysis::normalize::normalize_result(&result),
            biome_project_analysis::normalize::normalize_result(&analyze(
                &fresh_snapshot,
                &AnalysisOptions::default()
            ))
        );
    }
}

#[test]
fn b6_review1_invalid_consumer_manifest_retains_catalog_uncertainty() {
    assert_catalog_consumer_manifest_uncertainty(
        br#"{"dependencies":{"keep":"catalog:"}"#.to_vec(),
    );
}

#[test]
fn b6_review1_unavailable_consumer_manifest_retains_catalog_uncertainty() {
    assert_catalog_consumer_manifest_uncertainty(vec![255]);
}

#[test]
fn b6_review1_unavailable_yaml_preserves_unknown_selection_and_recovers() {
    for fallback in [false, true] {
        let fs = Arc::new(biome_fs::MemoryFileSystem::default());
        let manifest = if fallback {
            r#"{"private":true,"dependencies":{"missing":"catalog:"},"catalog":{"fallback":"1"}}"#
        } else {
            r#"{"private":true,"dependencies":{"missing":"catalog:"}}"#
        };
        fs.insert("/project/package.json".into(), manifest);
        fs.insert("/project/pnpm-workspace.yaml".into(), vec![255]);
        let (warm, project) =
            crate::test_utils::setup_workspace_and_open_project(fs.clone(), "/project");
        let request = ProjectAnalysisRequest {
            configuration: serde_json::from_value(json!({"entry":[],"project":[]})).unwrap(),
            ..Default::default()
        };
        for step in 0..3 {
            if step == 1 {
                fs.insert(
                    "/project/pnpm-workspace.yaml".into(),
                    "catalog: { missing: '1', yaml-unused: '2' }\n",
                );
            }
            if step == 2 {
                fs.insert("/project/pnpm-workspace.yaml".into(), "packages: []\n");
            }
            let snapshot = warm.build_project_snapshot(project, &request).unwrap();
            let result = analyze(&snapshot, &AnalysisOptions::default());
            let missing = result
                .issues
                .iter()
                .filter(|i| i.kind == IssueKind::CatalogReferences)
                .count();
            assert_eq!(
                missing,
                usize::from(step == 2),
                "fallback={fallback} step={step}"
            );
            let unused: Vec<_> = result
                .issues
                .iter()
                .filter(|i| i.kind == IssueKind::Catalog)
                .collect();
            assert_eq!(
                unused.len(),
                usize::from(step == 1 || step == 2 && fallback)
            );
            if step == 0 {
                assert!(
                    result.catalogs.entries.is_empty(),
                    "unknown winning YAML must not select a fallback definition"
                );
                assert_eq!(result.catalogs.selections.len(), 2);
                for selection in &result.catalogs.selections {
                    assert!(!selection.valid);
                    assert_eq!(
                        result.sources[&selection.location.file]
                            .logical_path
                            .as_str(),
                        "pnpm-workspace.yaml"
                    );
                    assert_eq!(selection.location.file, selection.container.file);
                    assert!(matches!(
                        result.sources[&selection.location.file].content,
                        SourceContent::Unavailable { .. }
                    ));
                }
                assert!(!result.catalogs.gaps.is_empty());
            } else {
                assert!(result.catalogs.selections.iter().all(|s| s.valid));
                assert!(result.catalogs.gaps.is_empty());
                if let Some(issue) = unused.first() {
                    assert_eq!(
                        result.sources[&issue.location.file].logical_path.as_str(),
                        if step == 1 {
                            "pnpm-workspace.yaml"
                        } else {
                            "package.json"
                        }
                    );
                }
            }
            let (fresh, fresh_project) =
                crate::test_utils::setup_workspace_and_open_project(fs.clone(), "/project");
            let fresh_snapshot = fresh
                .build_project_snapshot(fresh_project, &request)
                .unwrap();
            assert_eq!(
                biome_project_analysis::normalize::normalize_result(&result),
                biome_project_analysis::normalize::normalize_result(&analyze(
                    &fresh_snapshot,
                    &AnalysisOptions::default()
                ))
            );
        }
    }
}

#[test]
fn b6_review1_only_pnpm_provider_overrides_consume_catalogs() {
    let yaml = "catalog: { keep: '1', root: '1', nested: '1' }\noverrides: { keep: 'catalog:', missing: 'catalog:' }\n";
    for pnpm in [false, true] {
        let snapshot = acquire(
            &[
                (
                    "package.json",
                    r#"{"private":true,"workspaces":["packages/*"],"pnpm":{"overrides":{"root":"catalog:","root-missing":"catalog:"}}}"#,
                ),
                (
                    if pnpm {
                        "pnpm-workspace.yaml"
                    } else {
                        ".yarnrc.yml"
                    },
                    yaml,
                ),
                (
                    "packages/app/package.json",
                    r#"{"private":true,"pnpm":{"overrides":{"nested":"catalog:","nested-missing":"catalog:"}}}"#,
                ),
            ],
            json!({"entry":[],"project":[]}),
        );
        let result = analyze(&snapshot, &AnalysisOptions::default());
        let unused: BTreeSet<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Catalog)
            .map(|i| i.symbol.as_deref().unwrap())
            .collect();
        assert_eq!(
            unused,
            if pnpm {
                ["default.nested"].into()
            } else {
                ["default.keep", "default.nested"].into()
            }
        );
        let missing: BTreeSet<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::CatalogReferences)
            .map(|i| i.symbol.as_deref().unwrap())
            .collect();
        assert_eq!(
            missing,
            if pnpm {
                ["default.missing", "default.root-missing"].into()
            } else {
                ["default.root-missing"].into()
            }
        );
        assert!(
            result
                .catalogs
                .references
                .iter()
                .all(|r| result.sources[&r.location.file].logical_path.as_str()
                    != "packages/app/package.json")
        );
        assert_eq!(result.catalogs.references.iter().filter(|r| matches!(&r.producer, biome_project_analysis::catalogs::CatalogProducer::Metadata { field } if field == "overrides")).count(), if pnpm { 2 } else { 0 });
    }
}
