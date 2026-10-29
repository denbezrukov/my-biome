use super::ProjectAnalysisRequest;
use biome_fs::MemoryFileSystem;
use biome_project_analysis::{
    analyze,
    model::AnalysisSnapshot,
    options::{AnalysisMode, AnalysisOptions},
    result::IssueKind,
};
use serde_json::json;
use std::sync::Arc;

fn snapshot(files: &[(&str, &str)], mode: AnalysisMode, entries: &[&str]) -> Arc<AnalysisSnapshot> {
    let fs = MemoryFileSystem::default();
    fs.insert(
        "/project/package.json".into(),
        r#"{"private":true,"type":"module"}"#,
    );
    for (path, text) in files {
        fs.insert(format!("/project/{path}").into(), *text);
    }
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(fs, "/project");
    service
        .build_project_snapshot(
            project,
            &ProjectAnalysisRequest {
                configuration: serde_json::from_value(
                    json!({"entry":entries,"project":["**/*.js!", "**/*.cjs!"]}),
                )
                .unwrap(),
                mode,
                ..Default::default()
            },
        )
        .unwrap()
}

#[test]
fn source_exec_discovers_and_activates_node_target() {
    let s = snapshot(
        &[
            (
                "index.js",
                "import { exec } from 'node:child_process'; exec('node hidden/app.js');",
            ),
            ("hidden/app.js", "export const value = 1;"),
        ],
        AnalysisMode::Default,
        &["index.js"],
    );
    let result = analyze(&s, &AnalysisOptions::default());
    let target = s
        .sources
        .iter()
        .find(|(_, s)| s.logical_path == "hidden/app.js")
        .unwrap()
        .0;
    assert!(
        result
            .membership
            .files
            .iter()
            .any(|m| &m.file == target && m.definite)
    );
    assert!(
        !result
            .coverage
            .gaps()
            .iter()
            .any(|g| format!("{:?}", g.reason).contains("shell entry"))
    );
}

#[test]
fn production_start_node_targets_and_explicit_overlap() {
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        for explicit in [false, true] {
            let entries: &[&str] = if explicit {
                &["app.js!", "setup.cjs!"]
            } else {
                &[]
            };
            let s = snapshot(
                &[
                    (
                        "package.json",
                        r#"{"private":true,"scripts":{"start":"node --require ./setup.cjs app.js","build":"node build.js"}}"#,
                    ),
                    ("app.js", "export const app = 1;"),
                    ("setup.cjs", "exports.setup = 1;"),
                    ("build.js", "export const build = 1;"),
                ],
                mode,
                entries,
            );
            let result = analyze(
                &s,
                &AnalysisOptions {
                    mode,
                    include_entry_exports: true,
                    ..Default::default()
                },
            );
            for path in ["app.js", "setup.cjs"] {
                assert!(
                    !result.issues.iter().any(|i| i.kind == IssueKind::Files
                        && result.sources[&i.location.file].logical_path == path),
                    "{mode:?} {path}"
                );
                assert_eq!(
                    result.issues.iter().any(|i| i.kind == IssueKind::Exports
                        && result.sources[&i.location.file].logical_path == path),
                    explicit,
                    "{mode:?} {path}"
                );
            }
        }
    }
}

#[test]
fn node_child_process_binding() {
    for command in [
        "import {spawn as launch} from 'node:child_process'; launch('node', ['app.js']);",
        "import * as cp from 'child_process'; cp.execFile('node', ['app.js']);",
        "const {execSync: launch}=require('node:child_process'); launch('node app.js');",
        "import {fork} from 'child_process'; fork('./app.js');",
        "import {Worker as W} from 'node:worker_threads'; new W('./app.js');",
    ] {
        let s = snapshot(
            &[("index.js", command), ("app.js", "export const app=1;")],
            AnalysisMode::Default,
            &["index.js"],
        );
        let result = analyze(&s, &Default::default());
        let file = *s
            .sources
            .iter()
            .find(|(_, s)| s.logical_path == "app.js")
            .unwrap()
            .0;
        assert!(
            result
                .membership
                .files
                .iter()
                .any(|m| m.file == file && m.definite),
            "{command}"
        );
        assert!(
            !result
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::Exports && i.location.file == file),
            "{command}"
        );
    }
    for command in [
        "import {exec} from 'node:child_process'; function f(exec){exec('node app.js');}",
        "const cp={exec(x){}}; cp.exec('node app.js');",
    ] {
        let s = snapshot(
            &[("index.js", command), ("app.js", "export const app=1;")],
            AnalysisMode::Default,
            &["index.js"],
        );
        assert!(s.inventory.script_usage.targets.is_empty(), "{command}");
    }
}
#[test]
fn dead_child_process_does_not_root_target() {
    use biome_project_analysis::scripts::{Activation, ScriptProducer};
    for active in [false, true] {
        let entry = if active {
            "import './producer.js';"
        } else {
            "export {};"
        };
        let s = snapshot(
            &[
                ("index.js", entry),
                (
                    "producer.js",
                    "import {fork,exec,spawn} from 'node:child_process'; fork('./app.js'); exec('pnpm dlx sherif@catalog:tools'); spawn(executable, []);",
                ),
                ("app.js", "export const app=1;"),
                ("idle.js", "export const idle=1;"),
            ],
            AnalysisMode::Default,
            &["index.js"],
        );
        let result = analyze(&s, &Default::default());
        let producer = *s
            .sources
            .iter()
            .find(|(_, s)| s.logical_path == "producer.js")
            .unwrap()
            .0;
        assert!(
            s.inventory
                .script_usage
                .targets
                .iter()
                .any(|t| t.producer == ScriptProducer::Source { file: producer })
        );
        assert_eq!(s.inventory.script_usage.catalogs.len(), 1);
        assert_eq!(
            result
                .membership
                .script_producers
                .iter()
                .find(|p| p.producer == ScriptProducer::Source { file: producer })
                .unwrap()
                .activation,
            if active {
                Activation::Definite
            } else {
                Activation::Dead
            }
        );
        assert_eq!(
            result.issues.iter().any(|i| i.kind == IssueKind::Files
                && result.sources[&i.location.file].logical_path == "idle.js"),
            !active
        );
        assert_eq!(result.coverage.gaps().is_empty(), !active);
    }
}
#[test]
fn installed_bin_provider_and_metadata_gap_keep_source_identity() {
    for reachable in [false, true] {
        let s = snapshot(
            &[
                (
                    "package.json",
                    r#"{"type":"module","dependencies":{"boxen-cli":"*"}}"#,
                ),
                (
                    "index.js",
                    if reachable {
                        "import './producer.js';"
                    } else {
                        "export {};"
                    },
                ),
                (
                    "producer.js",
                    "import {exec} from 'child_process'; exec('boxen text');",
                ),
                ("idle.js", "export const idle=1;"),
                (
                    "node_modules/boxen-cli/package.json",
                    r#"{"name":"boxen-cli","bin":{"boxen":"cli.js"}}"#,
                ),
                ("node_modules/boxen-cli/cli.js", "export {};"),
            ],
            AnalysisMode::Default,
            &["index.js"],
        );
        assert!(
            s.inventory
                .script_usage
                .packages
                .iter()
                .any(|p| p.package_name == "boxen-cli")
        );
        assert!(
            !s.inventory
                .script_usage
                .packages
                .iter()
                .any(|p| p.package_name == "boxen")
        );
        let result = analyze(&s, &Default::default());
        assert_eq!(result.coverage.gaps().is_empty(), !reachable);
    }
}

#[test]
fn script_discovery_fixed_point_is_immutable_and_matches_fresh() {
    use biome_project_analysis::normalize::normalize_snapshot;
    let fs = MemoryFileSystem::default();
    let files = [
        (
            "/project/package.json",
            r#"{"scripts":{"start":"node hidden/first.js; node $(touch MARKER)"}}"#,
        ),
        (
            "/project/hidden/first.js",
            "import {exec} from 'child_process'; exec('node hidden/second.js');",
        ),
        ("/project/hidden/second.js", "import './third.js';"),
        ("/project/hidden/third.js", "export const value=1;"),
    ];
    for (path, text) in files {
        fs.insert(path.into(), text);
    }
    let bytes = || {
        fs.files
            .0
            .read()
            .iter()
            .map(|(p, b)| (p.clone(), b.lock().clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let before = bytes();
    let environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    let cwd = std::env::current_dir().unwrap();
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(
        MemoryFileSystem::from_files(fs.files.0.clone()),
        "/project",
    );
    let request = ProjectAnalysisRequest {
        configuration: serde_json::from_value(json!({"entry":[],"project":[]})).unwrap(),
        ..Default::default()
    };
    let first = service.build_project_snapshot(project, &request).unwrap();
    for path in ["hidden/first.js", "hidden/second.js", "hidden/third.js"] {
        assert!(first.sources.values().any(|s| s.logical_path == path));
    }
    assert!(!first.inventory.script_usage.gaps.is_empty());
    let second = service.build_project_snapshot(project, &request).unwrap();
    let (fresh, project) = crate::test_utils::setup_workspace_and_open_project(
        MemoryFileSystem::from_files(fs.files.0.clone()),
        "/project",
    );
    let fresh = fresh.build_project_snapshot(project, &request).unwrap();
    assert_eq!(normalize_snapshot(&first), normalize_snapshot(&second));
    assert_eq!(normalize_snapshot(&first), normalize_snapshot(&fresh));
    assert_eq!(before, bytes());
    assert!(environment == std::env::vars_os().collect());
    assert_eq!(cwd, std::env::current_dir().unwrap());
}
#[test]
fn source_argv_literal_is_not_shell_text_and_cwd_is_static() {
    let s = snapshot(
        &[
            (
                "index.js",
                "import {spawn,execFileSync} from 'child_process'; spawn('node', ['semi;colon.js'], {cwd:'nested'}); execFileSync('node', ['app.js'], {cwd:'nested'});",
            ),
            ("nested/semi;colon.js", "export const value=1;"),
            ("nested/app.js", "export const value=2;"),
        ],
        AnalysisMode::Default,
        &["index.js"],
    );
    let result = analyze(&s, &Default::default());
    assert_eq!(s.inventory.script_usage.targets.len(), 2);
    for target in &s.inventory.script_usage.targets {
        assert_eq!(target.cwd.as_str(), "nested");
    }
    assert!(result.coverage.gaps().is_empty());
}

#[test]
fn possible_and_ignored_source_producers_preserve_activation() {
    use biome_project_analysis::{
        coverage::{CoverageGap, GapReason, TargetDomain},
        model::Location,
        scripts::{Activation, ScriptProducer},
    };
    let s = snapshot(
        &[
            ("index.js", "export {};"),
            (
                "producer.js",
                "import {fork} from 'child_process'; fork('./app.js');",
            ),
            ("app.js", "export const app=1;"),
            ("idle.js", "export const idle=1;"),
        ],
        AnalysisMode::Default,
        &["index.js"],
    );
    let id = |name: &str| {
        *s.sources
            .iter()
            .find(|(_, s)| s.logical_path == name)
            .unwrap()
            .0
    };
    let mut possible = (*s).clone();
    possible.inventory.gaps.push(CoverageGap {
        producer: "test bounded source uncertainty".into(),
        domain: TargetDomain::Bounded {
            files: vec![id("producer.js")],
            workspaces: vec![],
            packages: vec![],
        },
        reason: GapReason::DynamicReference("bounded producer".into()),
        location: Some(Location {
            file: id("index.js"),
            range: Default::default(),
        }),
    });
    let options = AnalysisOptions {
        ignore: vec!["producer.js".into()],
        ..Default::default()
    };
    let result = analyze(&possible, &options);
    assert_eq!(
        result
            .membership
            .script_producers
            .iter()
            .find(|p| p.producer
                == ScriptProducer::Source {
                    file: id("producer.js")
                })
            .unwrap()
            .activation,
        Activation::Possible
    );
    assert!(
        result
            .membership
            .files
            .iter()
            .any(|m| m.file == id("app.js") && m.possible && !m.definite)
    );
    assert!(
        result
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::Files && i.location.file == id("idle.js"))
    );
}
#[test]
fn manifest_located_provider_gap_is_gated_by_source_file() {
    let s = snapshot(
        &[
            ("package.json", r#"{"dependencies":{"broken":"*"}}"#),
            ("index.js", "export {};"),
            (
                "producer.js",
                "import {exec} from 'child_process'; exec('boxen');",
            ),
            (
                "node_modules/broken/package.json",
                r#"{"name":false,"bin":{"boxen":"cli.js"}}"#,
            ),
        ],
        AnalysisMode::Default,
        &["index.js"],
    );
    let producer = *s
        .sources
        .iter()
        .find(|(_, s)| s.logical_path == "producer.js")
        .unwrap()
        .0;
    assert!(s.inventory.script_usage.gaps.iter().any(|g| g.producer
        == biome_project_analysis::scripts::ScriptProducer::Source { file: producer }
        && g.gap.location.is_some_and(
            |l| s.sources[&l.file].logical_path == "node_modules/broken/package.json"
        )));
    let result = analyze(&s, &Default::default());
    assert!(result.coverage.gaps().is_empty());
}

#[test]
fn binary_providers_follow_workspace_ownership_and_strict_scope() {
    for mode in [AnalysisMode::Default, AnalysisMode::Strict] {
        let s = snapshot(
            &[
                (
                    "package.json",
                    r#"{"private":true,"workspaces":["packages/*"],"dependencies":{"root-tool":"*"}}"#,
                ),
                (
                    "packages/child/package.json",
                    r#"{"name":"child","scripts":{"start":"boxen"}}"#,
                ),
                (
                    "packages/sibling/package.json",
                    r#"{"name":"sibling","dependencies":{"sibling-tool":"*"}}"#,
                ),
                (
                    "node_modules/root-tool/package.json",
                    r#"{"name":"root-tool","bin":{"boxen":"index.js"}}"#,
                ),
                (
                    "node_modules/sibling-tool/package.json",
                    r#"{"name":"sibling-tool","bin":{"boxen":"index.js"}}"#,
                ),
            ],
            mode,
            &[],
        );
        let packages: Vec<_> = s
            .inventory
            .script_usage
            .packages
            .iter()
            .map(|p| p.package_name.as_str())
            .collect();
        if mode == AnalysisMode::Default {
            assert_eq!(packages, ["root-tool"]);
        } else {
            assert!(packages.is_empty());
        }
    }
}

#[test]
fn unsupported_bun_command_adapter_is_visible() {
    let s = snapshot(
        &[
            ("index.js", "Bun.spawn(['node','app.js']);"),
            ("app.js", "export const app=1;"),
        ],
        AnalysisMode::Default,
        &["index.js"],
    );
    assert!(!analyze(&s, &Default::default()).coverage.gaps().is_empty());
}

#[test]
fn retained_node_start_comparison_has_named_native_correction() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../xtask/project_analysis/fixtures/scripts");
    let comparison: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(base.join("comparison.json")).unwrap())
            .unwrap();
    let original: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(base.join("original-node-start-observations.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(original.as_array().unwrap().len(), 15);
    assert_eq!(comparison["correction"], "production-start-node-targets");
    let paths = [
        "package.json",
        "app.js",
        "setup.cjs",
        "build.js",
        "checks.js",
        "before.js",
        "idle.js",
    ];
    let files: Vec<_> = paths
        .iter()
        .map(|path| {
            (
                *path,
                std::fs::read_to_string(base.join("production-start-node-targets").join(path))
                    .unwrap(),
            )
        })
        .collect();
    let borrowed: Vec<_> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();
    for (name, mode) in [
        ("default", AnalysisMode::Default),
        ("production", AnalysisMode::Production),
        ("strict", AnalysisMode::Strict),
    ] {
        let s = snapshot(&borrowed, mode, &[]);
        let result = analyze(
            &s,
            &AnalysisOptions {
                mode,
                include_entry_exports: true,
                ..Default::default()
            },
        );
        let actual: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Files)
            .map(|i| result.sources[&i.location.file].logical_path.as_str())
            .collect();
        assert_eq!(json!(actual), comparison["native"][name]["files"]);
        assert!(!result.issues.iter().any(|i| i.kind == IssueKind::Exports));
    }
}
#[test]
fn worker_url_targets_have_source_protection() {
    let s = snapshot(
        &[
            (
                "index.js",
                "import {Worker} from 'worker_threads'; new Worker(new URL('./app.js',import.meta.url));",
            ),
            ("app.js", "export const app=1;"),
        ],
        AnalysisMode::Default,
        &["index.js"],
    );
    let result = analyze(&s, &Default::default());
    assert!(!result.issues.iter().any(|i| i.kind == IssueKind::Exports
        && result.sources[&i.location.file].logical_path == "app.js"));
}

#[test]
fn installed_binary_alias_retains_requested_and_actual_package_identity() {
    let s = snapshot(
        &[
            (
                "package.json",
                r#"{"dependencies":{"alias":"npm:real-tool@1"},"scripts":{"start":"boxen"}}"#,
            ),
            (
                "node_modules/alias/package.json",
                r#"{"name":"real-tool","bin":{"boxen":"cli.js"}}"#,
            ),
        ],
        AnalysisMode::Default,
        &[],
    );
    assert!(
        s.inventory
            .script_usage
            .packages
            .iter()
            .any(|p| p.package_name == "alias")
    );
    assert!(
        s.inventory.script_usage.binaries[0]
            .providers
            .iter()
            .any(|p| p.package_name.as_deref() == Some("real-tool"))
    );
}

#[test]
fn node_builtin_preloads_are_not_dependency_packages() {
    let s = snapshot(
        &[
            (
                "package.json",
                r#"{"scripts":{"start":"node --require fs --import node:path app.js"}}"#,
            ),
            ("app.js", "export const app=1;"),
        ],
        AnalysisMode::Default,
        &[],
    );
    assert!(s.inventory.script_usage.packages.is_empty());
}

#[test]
fn invalid_provider_json_is_not_known_metadata() {
    let s = snapshot(
        &[
            (
                "package.json",
                r#"{"dependencies":{"boxen-cli":"*"},"scripts":{"start":"boxen"}}"#,
            ),
            (
                "node_modules/boxen-cli/package.json",
                r#"{"name":"boxen-cli" "bin":{"boxen":"cli.js"}}"#,
            ),
        ],
        AnalysisMode::Default,
        &[],
    );
    assert!(s.inventory.script_usage.packages.is_empty());
    assert!(
        s.inventory
            .binary_providers
            .iter()
            .filter(|p| p.manifest.is_some())
            .all(|p| !p.metadata_known)
    );
}

#[test]
fn supplemental_command_observations_keep_native_evidence_and_limits_separate() {
    use biome_project_analysis::{
        facts::ResolutionOutcome,
        scripts::{Activation, PackageUseKind, ScriptProducer},
    };
    use std::collections::BTreeSet;
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../xtask/project_analysis/fixtures/scripts");
    let read = |name: &str| {
        serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(base.join(name)).unwrap(),
        )
        .unwrap()
    };
    let fixtures = read("original-command-fixtures.json");
    let comparisons = read("command-comparisons.json");
    assert_eq!(
        read("original-command-observations.json")
            .as_array()
            .unwrap()
            .len(),
        14
    );
    for case in comparisons["cases"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let fixture = fixtures
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["case"] == name)
            .unwrap();
        let fs = MemoryFileSystem::default();
        for file in fixture["files"].as_array().unwrap() {
            if file["type"] == "file" {
                fs.insert(
                    format!("/project/{}", file["path"].as_str().unwrap()).into(),
                    file["content_utf8"].as_str().unwrap(),
                );
            }
        }
        let (service, project) =
            crate::test_utils::setup_workspace_and_open_project(fs, "/project");
        let mode: AnalysisMode = serde_json::from_value(case["mode"].clone()).unwrap();
        let s = service
            .build_project_snapshot(
                project,
                &ProjectAnalysisRequest {
                    configuration: serde_json::from_value(case["nativeConfiguration"].clone())
                        .unwrap(),
                    mode,
                    ..Default::default()
                },
            )
            .unwrap();
        let result = analyze(
            &s,
            &AnalysisOptions {
                mode,
                include_entry_exports: true,
                ..Default::default()
            },
        );
        let files: BTreeSet<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Files)
            .map(|i| result.sources[&i.location.file].logical_path.as_str())
            .collect();
        let exports: BTreeSet<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Exports)
            .map(|i| {
                format!(
                    "{}:{}",
                    result.sources[&i.location.file].logical_path,
                    i.symbol.as_ref().unwrap()
                )
            })
            .collect();
        let targets: BTreeSet<_> = s
            .inventory
            .script_usage
            .targets
            .iter()
            .flat_map(|t| &t.resolutions)
            .filter_map(|r| {
                if let ResolutionOutcome::Resolved { file, .. } = r.outcome {
                    Some(s.sources[&file].logical_path.as_str())
                } else {
                    None
                }
            })
            .collect();
        let packages: BTreeSet<_> = s
            .inventory
            .script_usage
            .packages
            .iter()
            .map(|p| p.package_name.as_str())
            .collect();
        assert_eq!(json!(files), case["nativeFiles"], "{name}");
        assert_eq!(json!(exports), case["nativeExports"], "{name}");
        assert_eq!(json!(targets), case["nativeTargets"], "{name}");
        assert_eq!(json!(packages), case["nativePackages"], "{name}");
        assert_eq!(
            if result.coverage.gaps().is_empty() {
                "complete"
            } else {
                "incomplete"
            },
            case["nativeCoverage"].as_str().unwrap(),
            "{name}"
        );
        if name.starts_with("02") {
            assert!(s.inventory.script_usage.binaries.iter().any(|b| {
                b.binary == "boxen"
                    && b.optional
                    && b.providers
                        .iter()
                        .any(|p| p.package_name.as_deref() == Some("boxen-cli") && p.available)
            }));
        }
        if name.starts_with("03") {
            assert!(
                s.inventory
                    .script_usage
                    .packages
                    .iter()
                    .any(|p| p.kind == PackageUseKind::OnDemand
                        && p.package_name == "boxen-cli"
                        && p.optional)
            );
        }
        if name.starts_with("04") {
            assert!(
                s.inventory
                    .script_usage
                    .catalogs
                    .iter()
                    .any(|c| c.package_name == "sherif"
                        && c.catalog_name.as_deref() == Some("tools"))
            );
        }
        if name.starts_with("07") || name.starts_with("08") || name.starts_with("09") {
            assert!(s.inventory.script_usage.targets.iter().all(
                |t| matches!(&t.producer,ScriptProducer::Manifest {script,..} if script=="start")
                    && t.cwd.as_str() == "child"
            ));
        }
        if name.starts_with("13") || name.starts_with("14") {
            assert!(result.membership.script_producers.iter().any(|p| matches!(
                p.producer,
                ScriptProducer::Source { .. }
            ) && p.activation
                == if name.starts_with("13") {
                    Activation::Dead
                } else {
                    Activation::Definite
                }));
        }
        for kind in [
            IssueKind::Dependencies,
            IssueKind::Catalog,
            IssueKind::CatalogReferences,
        ] {
            assert_eq!(
                result.capabilities[&kind],
                biome_project_analysis::coverage::CapabilityStatus::Unsupported
            );
        }
    }
}

#[test]
fn b4_review_pipeline_roots_the_original_cwd_target() {
    let s = snapshot(
        &[
            (
                "package.json",
                r#"{"scripts":{"start":"cd child | node app.js"}}"#,
            ),
            ("app.js", "export const root = 1;"),
            ("child/app.js", "export const child = 1;"),
        ],
        AnalysisMode::Default,
        &[],
    );
    let result = analyze(&s, &AnalysisOptions::default());
    assert!(result.coverage.gaps().is_empty());
    let unused: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.kind == IssueKind::Files)
        .map(|i| result.sources[&i.location.file].logical_path.as_str())
        .collect();
    assert_eq!(unused, ["child/app.js"]);
}

#[test]
fn b4_review_spread_options_gap_is_source_gated() {
    for command in [
        "spawn('node', ['app.js'], ...[{cwd:'nested'}]);",
        "spawn('node', ...[['app.js'], {cwd:'nested'}]);",
        "new Worker('./app.js', ...[{eval:true}]);",
    ] {
        for live in [false, true] {
            let producer = format!(
                "import {{spawn}} from 'node:child_process'; import {{Worker}} from 'node:worker_threads'; {command}"
            );
            let s = snapshot(
                &[
                    (
                        "index.js",
                        if live { "import './producer.js';" } else { "" },
                    ),
                    ("producer.js", &producer),
                    ("app.js", "export const root = 1;"),
                    ("nested/app.js", "export const nested = 1;"),
                ],
                AnalysisMode::Default,
                &["index.js"],
            );
            let source = *s
                .sources
                .iter()
                .find(|(_, source)| source.logical_path == "producer.js")
                .unwrap()
                .0;
            assert!(
                s.inventory.script_usage.gaps.iter().any(|g| g.producer
                    == biome_project_analysis::scripts::ScriptProducer::Source { file: source }),
                "{command}"
            );
            let result = analyze(&s, &AnalysisOptions::default());
            assert_eq!(!result.coverage.gaps().is_empty(), live, "{command}");
            assert_eq!(
                result.issues.iter().any(|i| i.kind == IssueKind::Files
                    && result.sources[&i.location.file].logical_path == "nested/app.js"),
                !live,
                "{command}"
            );
        }
    }
}
