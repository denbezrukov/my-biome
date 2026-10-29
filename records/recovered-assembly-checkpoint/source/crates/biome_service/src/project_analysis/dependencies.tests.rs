use super::ProjectAnalysisRequest;
use biome_fs::MemoryFileSystem;
use biome_project_analysis::{
    analyze,
    model::AnalysisSnapshot,
    options::{AnalysisMode, AnalysisOptions},
    result::{IssueKind, ProjectAnalysisResult},
};
use serde_json::{Value, json};
use std::sync::Arc;

fn run(
    files: &[(&str, &str)],
    config: Value,
    mode: AnalysisMode,
) -> (Arc<AnalysisSnapshot>, ProjectAnalysisResult) {
    let fs = MemoryFileSystem::default();
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
fn packages(result: &ProjectAnalysisResult) -> Vec<(String, String, String)> {
    let mut issues: Vec<_> = result
        .issues
        .iter()
        .filter(|i| {
            matches!(
                i.kind,
                IssueKind::Dependencies
                    | IssueKind::DevDependencies
                    | IssueKind::OptionalPeerDependencies
                    | IssueKind::Unlisted
                    | IssueKind::Binaries
            )
        })
        .map(|i| {
            (
                serde_json::to_value(i.kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
                result.sources[&i.location.file].logical_path.to_string(),
                i.symbol.clone().unwrap(),
            )
        })
        .collect();
    issues.sort();
    issues
}
fn expected(case: &str, mode: AnalysisMode) -> Vec<(String, String, String)> {
    let strict = mode == AnalysisMode::Strict;
    let values: Vec<(&str, &str, &str)> = match case {
        "owner-both" | "imports-child-only-map" => vec![(
            "dependencies",
            "package.json",
            if case == "owner-both" {
                "probe-owner"
            } else {
                "probe-map"
            },
        )],
        "owner-ancestor-only" if strict => vec![
            ("dependencies", "package.json", "probe-owner"),
            ("unlisted", "packages/child/entry.ts", "probe-owner"),
        ],
        "ordinary-type-versus-runtime" if strict => vec![
            ("dependencies", "package.json", "probe-type"),
            ("unlisted", "entry.ts", "probe-runtime"),
        ],
        "private-declaration-tsconfig" | "private-declaration-manifest" if strict => {
            vec![("dependencies", "package.json", "probe-type")]
        }
        "public-declaration-placement" if strict => vec![
            ("unlisted", "types/public.d.ts", "probe-dev"),
            ("unlisted", "types/public.d.ts", "@types/probe-untyped"),
        ],
        "missing-category-contrast" => vec![
            ("unlisted", "entry.ts", "probe-installed"),
            ("unlisted", "entry.ts", "probe-missing"),
        ],
        "publication-overrides" => vec![("unlisted", "types/public.d.ts", "probe-public")],
        "publication-absent-output" => vec![("dependencies", "package.json", "probe-public")],
        "publication-versioned-and-null-surface" => vec![
            ("unlisted", "features/visible.d.ts", "probe-visible"),
            ("unlisted", "versions/old.d.ts", "probe-versioned"),
        ],
        _ => vec![],
    };
    let mut result: Vec<_> = values
        .into_iter()
        .map(|(k, p, n)| (k.into(), p.into(), n.into()))
        .collect();
    result.sort();
    result
}
#[test]
fn dependencies_closed_33_source_corpus() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../xtask/project_analysis/dependencies/corpus.json"
    ))
    .unwrap();
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../../xtask/project_analysis/dependencies/original-fixtures.json"
    ))
    .unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 33);
    let mut differences = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let mode: AnalysisMode = serde_json::from_value(case["mode"].clone()).unwrap();
        let fixture = fixtures
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == name)
            .unwrap();
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
        let mut config: Value =
            serde_json::from_str(files.iter().find(|(p, _)| *p == "knip.json").unwrap().1).unwrap();
        config.as_object_mut().unwrap().retain(|key, _| {
            matches!(
                key.as_str(),
                "entry" | "project" | "workspaces" | "useTsconfigFiles"
            )
        });
        if let Some(workspaces) = config.get_mut("workspaces").and_then(Value::as_object_mut) {
            for workspace in workspaces.values_mut() {
                workspace.as_object_mut().unwrap().retain(|key, _| {
                    matches!(key.as_str(), "entry" | "project" | "useTsconfigFiles")
                });
            }
        }
        let (_, result) = run(&files, config, mode);
        let native = packages(&result);
        let native_expected = if name == "publication-absent-output" {
            Vec::new()
        } else {
            expected(name, mode)
        };
        if native != native_expected {
            differences.push(format!(
                "{name}/{mode:?}: {native:?}; expected {:?}",
                native_expected
            ));
        }
        if name == "publication-absent-output" {
            assert!(!result.coverage.gaps().is_empty());
            assert!(result.dependencies.uses.is_empty());
        }
        let mut observed = Vec::new();
        for file in case["observed_issues"].as_array().unwrap() {
            for kind in [
                "dependencies",
                "devDependencies",
                "optionalPeerDependencies",
                "unlisted",
                "binaries",
            ] {
                for issue in file[kind].as_array().into_iter().flatten() {
                    observed.push((
                        kind.to_string(),
                        file["file"].as_str().unwrap().to_string(),
                        issue["name"].as_str().unwrap().to_string(),
                    ));
                }
            }
        }
        observed.sort();
        assert_eq!(
            observed,
            expected(name, mode),
            "reference projection {name}"
        );
        if name == "publication-directory" && result.coverage.gaps().is_empty() {
            differences.push("directory publication lacks unsupported coverage".into());
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
#[test]
fn dependencies_public_output_ignores_author_paths() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"name":"public-lib","private":false,"types":"dist/index.d.ts","devDependencies":{"@types/untyped":"*"}}"#,
            ),
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"paths":{"untyped":["./src/shim.ts"]}}}"#,
            ),
            ("dist/index.d.ts", "export type { Shape } from 'untyped';"),
            ("src/shim.ts", "export interface Shape {}"),
            (
                "node_modules/untyped/package.json",
                r#"{"name":"untyped","main":"index.js"}"#,
            ),
            ("node_modules/untyped/index.js", "exports.value=1;"),
            (
                "node_modules/@types/untyped/package.json",
                r#"{"name":"@types/untyped","types":"index.d.ts"}"#,
            ),
            (
                "node_modules/@types/untyped/index.d.ts",
                "export interface Shape {}",
            ),
        ],
        json!({"entry":[],"project":["**/*.ts!"]}),
        AnalysisMode::Strict,
    );
    assert_eq!(
        packages(&result),
        vec![(
            "unlisted".into(),
            "dist/index.d.ts".into(),
            "@types/untyped".into()
        )]
    );
}
#[test]
fn dependencies_manifest_import_branches_and_controls() {
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let (_, result) = run(
            &[
                (
                    "package.json",
                    r##"{"private":true,"dependencies":{"first":"*","second":"*","unused":"*"},"imports":{"#collection":{"node":"first/get","default":"second/*"},"ignored":"unused","#relative":"./local.ts","#wild":"./parts/*.ts"}}"##,
                ),
                ("local.ts", "export {};"),
                ("parts/a.ts", "export {};"),
            ],
            json!({"entry":[],"project":[]}),
            mode,
        );
        assert_eq!(
            packages(&result),
            vec![(
                "dependencies".into(),
                "package.json".into(),
                "unused".into()
            )]
        );
        assert_eq!(
            result
                .dependencies
                .uses
                .iter()
                .filter(|u| u.producer
                    == biome_project_analysis::dependencies::DependencyProducer::Manifest)
                .count(),
            2
        );
    }
}
#[test]
fn dependencies_bundled_types_obsolete_companion_and_required_peers() {
    let files = [
        (
            "package.json",
            r#"{"private":true,"dependencies":{"host":"*","peer":"*"},"devDependencies":{"@types/host":"*"}}"#,
        ),
        ("index.ts", "import 'host';"),
        (
            "node_modules/host/package.json",
            r#"{"name":"host","main":"index.js","types":"index.d.ts","peerDependencies":{"peer":"*"}}"#,
        ),
        ("node_modules/host/index.js", "export {};"),
        ("node_modules/host/index.d.ts", "export interface Shape {}"),
    ];
    let (_, result) = run(
        &files,
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Default,
    );
    assert_eq!(
        packages(&result),
        vec![(
            "devDependencies".into(),
            "package.json".into(),
            "@types/host".into()
        )]
    );
    let mut untyped = files;
    untyped[2].1 = r#"{"name":"host","main":"index.js","peerDependencies":{"peer":"*"}}"#;
    let (_, result) = run(
        &untyped,
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty());
}
#[test]
fn dependencies_npm_nested_installed_alias_identity() {
    let (snapshot, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"workspaces":["packages/*"],"dependencies":{"alias":"npm:outer@1"}}"#,
            ),
            (
                "packages/child/package.json",
                r#"{"name":"child","private":true,"dependencies":{"alias":"npm:inner@2"}}"#,
            ),
            ("packages/child/index.ts", "import 'alias';"),
            (
                "node_modules/alias/package.json",
                r#"{"name":"outer","main":"index.js"}"#,
            ),
            ("node_modules/alias/index.js", "export {};"),
            (
                "packages/child/node_modules/alias/package.json",
                r#"{"name":"inner","main":"index.js"}"#,
            ),
            ("packages/child/node_modules/alias/index.js", "export {};"),
        ],
        json!({"entry":[],"project":[],"workspaces":{"packages/child":{"entry":["index.ts!"],"project":["*.ts!"]}}}),
        AnalysisMode::Default,
    );
    assert_eq!(
        packages(&result),
        vec![("dependencies".into(), "package.json".into(), "alias".into())]
    );
    let used = result
        .dependencies
        .declarations
        .iter()
        .find(|d| !d.used.is_empty())
        .unwrap();
    assert_eq!(used.declaration.specifier, "npm:inner@2");
    assert_eq!(
        snapshot.sources[&used.declaration.location.file].logical_path,
        "packages/child/package.json"
    );
    let usage = result
        .dependencies
        .uses
        .iter()
        .find(|u| u.package == "alias")
        .unwrap();
    assert_eq!(usage.workspace, used.declaration.workspace);
    assert!(usage.resolutions.iter().flat_map(|r|&r.trace).any(|event|matches!(event,biome_project_analysis::facts::ResolutionEvent::Package{name:Some(name),..} if name=="inner")));
}
#[test]
fn dependencies_pnp_is_explicit_unsupported() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"dependencies":{"package":"*"}}"#,
            ),
            (".pnp.cjs", "throw new Error('must never execute');"),
            ("index.ts", "import 'package';"),
        ],
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Default,
    );
    assert!(
        result
            .coverage
            .gaps()
            .iter()
            .any(|g| format!("{:?}", g.reason).contains("Yarn PnP"))
    );
}
#[test]
fn dependencies_on_demand_binary_is_not_local_unlisted() {
    let (snapshot, result) = run(
        &[(
            "package.json",
            r#"{"private":true,"scripts":{"start":"npx absent-tool"}}"#,
        )],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty());
    assert!(
        snapshot
            .inventory
            .script_usage
            .binaries
            .iter()
            .any(|b| b.binary == "absent-tool" && b.optional)
    );
    assert!(
        !result
            .dependencies
            .uses
            .iter()
            .any(|u| u.package == "absent-tool")
    );
    let (_, result) = run(
        &[(
            "package.json",
            r#"{"private":true,"scripts":{"start":"npx absent-tool@1"}}"#,
        )],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty());
    assert!(
        result
            .dependencies
            .uses
            .iter()
            .any(|u| u.package == "absent-tool" && u.optional)
    );
}
#[cfg(unix)]
#[test]
fn dependencies_pnpm_and_yarn_node_modules_symlinks() {
    for topology in ["pnpm", "yarn"] {
        let mut fixture = biome_fs::TemporaryFs::new("biome-b5-package-topology");
        fixture.create_file("project/package.json",r#"{"private":true,"workspaces":["packages/*"],"dependencies":{"alias":"npm:actual@1"}}"#);
        fixture.create_file("project/index.ts", "import 'alias';");
        let provider = if topology == "pnpm" {
            "project/node_modules/.pnpm/actual@1/node_modules/actual"
        } else {
            "project/packages/actual"
        };
        let manifest = fixture.create_file(
            &format!("{provider}/package.json"),
            r#"{"name":"actual","private":true,"main":"index.js"}"#,
        );
        fixture.create_file(&format!("{provider}/index.js"), "export const value=1;");
        let root = fixture.working_directory.join("project");
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::os::unix::fs::symlink(manifest.parent().unwrap(), root.join("node_modules/alias"))
            .unwrap();
        let (service, project) =
            crate::test_utils::setup_workspace_and_open_project(fixture.create_os(), root.as_str());
        let request = ProjectAnalysisRequest {
            configuration: serde_json::from_value(
                json!({"entry":["index.ts!"],"project":["*.ts!"]}),
            )
            .unwrap(),
            ..Default::default()
        };
        let snapshot = service.build_project_snapshot(project, &request).unwrap();
        let result = analyze(&snapshot, &Default::default());
        assert!(
            packages(&result).is_empty(),
            "{topology}: {:?}",
            packages(&result)
        );
        let usage = result
            .dependencies
            .uses
            .iter()
            .find(|u| u.package == "alias")
            .unwrap();
        assert!(usage.resolutions.iter().flat_map(|r|&r.trace).any(|event|matches!(event,biome_project_analysis::facts::ResolutionEvent::Package{name:Some(name),..} if name=="actual")));
        let captured = snapshot
            .sources
            .iter()
            .find(|(_, s)| s.canonical_uri == url::Url::from_file_path(&manifest).unwrap().as_str())
            .unwrap();
        assert!(usage.resolutions.iter().flat_map(|r|&r.trace).any(|event|matches!(event,biome_project_analysis::facts::ResolutionEvent::Manifest{file,..} if file==captured.0)));
        let (fresh, project) =
            crate::test_utils::setup_workspace_and_open_project(fixture.create_os(), root.as_str());
        let fresh = fresh.build_project_snapshot(project, &request).unwrap();
        assert_eq!(
            biome_project_analysis::normalize::normalize_result(&result),
            biome_project_analysis::normalize::normalize_result(&analyze(
                &fresh,
                &Default::default()
            ))
        );
    }
}
#[test]
fn dependencies_public_output_and_provider_edits_warm_equal_fresh() {
    use biome_project_analysis::normalize::{normalize_result, normalize_snapshot};
    let fs = MemoryFileSystem::default();
    for (path, text) in [
        (
            "package.json",
            r#"{"private":false,"types":"dist/index.d.ts","dependencies":{"used":"*","other":"*"}}"#,
        ),
        ("dist/index.d.ts", "export type {T} from 'used';"),
        (
            "node_modules/used/package.json",
            r#"{"name":"used","types":"index.d.ts"}"#,
        ),
        ("node_modules/used/index.d.ts", "export interface T {}"),
        (
            "node_modules/other/package.json",
            r#"{"name":"other","types":"index.d.ts"}"#,
        ),
        ("node_modules/other/index.d.ts", "export interface T {}"),
    ] {
        fs.insert(format!("/project/{path}").into(), text);
    }
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(
        MemoryFileSystem::from_files(fs.files.0.clone()),
        "/project",
    );
    let request = ProjectAnalysisRequest {
        configuration: serde_json::from_value(json!({"entry":[],"project":["**/*.ts!"]})).unwrap(),
        mode: AnalysisMode::Strict,
        ..Default::default()
    };
    let options = AnalysisOptions {
        mode: AnalysisMode::Strict,
        ..Default::default()
    };
    let first = service.build_project_snapshot(project, &request).unwrap();
    assert_eq!(
        packages(&analyze(&first, &options)),
        vec![("dependencies".into(), "package.json".into(), "other".into())]
    );
    for (path, text) in [
        ("dist/index.d.ts", "\n\nexport type {T} from 'other';"),
        (
            "node_modules/other/package.json",
            r#"{"name":"renamed","types":"index.d.ts"}"#,
        ),
    ] {
        fs.insert(format!("/project/{path}").into(), text);
        let warm = service.build_project_snapshot(project, &request).unwrap();
        let (fresh, project) = crate::test_utils::setup_workspace_and_open_project(
            MemoryFileSystem::from_files(fs.files.0.clone()),
            "/project",
        );
        let fresh = fresh.build_project_snapshot(project, &request).unwrap();
        assert_eq!(normalize_snapshot(&warm), normalize_snapshot(&fresh));
        let result = analyze(&warm, &options);
        assert_eq!(
            normalize_result(&result),
            normalize_result(&analyze(&fresh, &options))
        );
        assert_eq!(
            packages(&result),
            vec![("dependencies".into(), "package.json".into(), "used".into())]
        );
        let usage=result.dependencies.uses.iter().find(|u|u.package=="other" && matches!(u.producer,biome_project_analysis::dependencies::DependencyProducer::PublicOutput{..})).unwrap();
        assert_eq!(u32::from(usage.location.range.start()), 23);
        assert_ne!(first.revision.input_digest, warm.revision.input_digest);
    }
}
#[test]
fn dependencies_peer_retention_is_transitive_and_not_direct_optional_credit() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"dependencies":{"host":"*","peer":"*","leaf":"*"},"peerDependencies":{"optional":"*"},"peerDependenciesMeta":{"optional":{"optional":true}}}"#,
            ),
            ("index.ts", "import 'host';"),
            (
                "node_modules/host/package.json",
                r#"{"name":"host","main":"index.js","peerDependencies":{"peer":"*","optional":"*"}}"#,
            ),
            ("node_modules/host/index.js", "export {};"),
            (
                "node_modules/peer/package.json",
                r#"{"name":"peer","main":"index.js","peerDependencies":{"leaf":"*"}}"#,
            ),
            ("node_modules/peer/index.js", "export {};"),
            (
                "node_modules/leaf/package.json",
                r#"{"name":"leaf","main":"index.js","peerDependencies":{"host":"*"}}"#,
            ),
            ("node_modules/leaf/index.js", "export {};"),
        ],
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty(), "{:?}", packages(&result));
    let optional = result
        .dependencies
        .declarations
        .iter()
        .find(|d| d.declaration.package == "optional")
        .unwrap();
    assert!(optional.used.is_empty());
    assert!(!optional.retained.is_empty());
}
#[test]
fn dependencies_strict_internal_type_import_credits_direct_dev() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"workspaces":["packages/*"],"devDependencies":{"child":"workspace:*"}}"#,
            ),
            ("index.ts", "import type { Shape } from 'child';"),
            (
                "packages/child/package.json",
                r#"{"name":"child","private":true,"types":"index.d.ts"}"#,
            ),
            ("packages/child/index.d.ts", "export interface Shape {}"),
            (
                "node_modules/child/package.json",
                r#"{"name":"child","private":true,"types":"../../packages/child/index.d.ts"}"#,
            ),
        ],
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Strict,
    );
    assert!(packages(&result).is_empty());
    assert!(
        result
            .dependencies
            .declarations
            .iter()
            .any(|d| d.declaration.package == "child" && !d.used.is_empty())
    );
}
#[test]
fn dependencies_public_type_metadata_entry_variants() {
    for field in [
        json!({"main":"dist/index.js"}),
        json!({"typings":"dist/index.d.ts"}),
        json!({"exports":{".":{"types":"./dist/index.d.ts","default":"./ignored.js"}}}),
        json!({"types":"missing.d.ts","publishConfig":{"types":"dist/index.d.ts"}}),
    ] {
        let mut manifest = json!({"name":"public","private":false});
        manifest
            .as_object_mut()
            .unwrap()
            .extend(field.as_object().unwrap().clone());
        let manifest = manifest.to_string();
        let (_, result) = run(
            &[
                ("package.json", &manifest),
                ("dist/index.d.ts", "export type {Shape} from 'external';"),
                ("ignored.js", "export {};"),
            ],
            json!({"entry":[],"project":[]}),
            AnalysisMode::Strict,
        );
        assert_eq!(
            packages(&result),
            vec![(
                "unlisted".into(),
                "dist/index.d.ts".into(),
                "external".into()
            )],
            "{field}"
        );
        let usage = result
            .dependencies
            .uses
            .iter()
            .find(|u| {
                matches!(
                    u.producer,
                    biome_project_analysis::dependencies::DependencyProducer::PublicOutput { .. }
                )
            })
            .unwrap();
        assert_eq!(u32::from(usage.location.range.start()), 25);
        assert_eq!(
            usage.resolutions[0].context.syntax,
            biome_project_analysis::facts::ImportSyntax::Reexport
        );
    }
}
#[test]
fn dependencies_public_output_uncertainty_remains_visible() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":false,"publishConfig":{"types":"dist/index.d.ts"},"dependencies":{"uncertain":"*"}}"#,
            ),
            ("dist/index.d.ts", "export type T = import(name).Shape;"),
        ],
        json!({"entry":[],"project":["**/*.ts!"]}),
        AnalysisMode::Strict,
    );
    assert!(!result.coverage.gaps().is_empty());
    assert!(
        !packages(&result)
            .iter()
            .any(|(_, _, name)| name == "uncertain")
    );
}
#[test]
fn dependencies_source_command_activation_retains_provider_identity() {
    for (source, definite, possible) in [
        ("export {};", false, false),
        ("import './command.js';", true, false),
        ("import(flag ? './command.js' : './other.js');", false, true),
    ] {
        let (snapshot, result) = run(
            &[
                (
                    "package.json",
                    r#"{"private":true,"dependencies":{"alias":"npm:actual-cli@1"}}"#,
                ),
                ("index.js", source),
                (
                    "command.js",
                    "import {exec} from 'node:child_process'; exec('tool');",
                ),
                ("other.js", "export {};"),
                (
                    "node_modules/alias/package.json",
                    r#"{"name":"actual-cli","bin":{"tool":"cli.js"}}"#,
                ),
                ("node_modules/alias/cli.js", "export {};"),
            ],
            json!({"entry":["index.js!"],"project":["*.js!"]}),
            AnalysisMode::Default,
        );
        assert_eq!(
            packages(&result)
                .iter()
                .any(|(kind, _, _)| kind == "dependencies"),
            !definite && !possible
        );
        let declaration = &result.dependencies.declarations[0];
        assert_eq!(!declaration.used.is_empty(), definite);
        if possible {
            assert!(!declaration.affecting_gaps.is_empty());
        }
        let uses: Vec<_> = result
            .dependencies
            .uses
            .iter()
            .filter(|u| u.package == "alias")
            .collect();
        assert_eq!(uses.is_empty(), !definite && !possible);
        if let Some(usage) = uses.first() {
            assert!(matches!(
                usage.producer,
                biome_project_analysis::dependencies::DependencyProducer::Script(
                    biome_project_analysis::scripts::ScriptProducer::Source { .. }
                )
            ));
            let provider = usage.binary_providers.iter().find(|p| p.available).unwrap();
            assert_eq!(provider.requested_package.as_deref(), Some("alias"));
            assert_eq!(provider.package_name.as_deref(), Some("actual-cli"));
            assert_eq!(
                snapshot.sources[&provider.manifest.unwrap()].logical_path,
                "node_modules/alias/package.json"
            );
            assert!(!result.coverage.gaps().is_empty());
        }
    }
}
#[test]
fn dependencies_runtime_use_credits_installed_definitely_typed_companion() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"dependencies":{"untyped":"*"},"devDependencies":{"@types/untyped":"*"}}"#,
            ),
            ("index.ts", "import 'untyped';"),
            (
                "node_modules/untyped/package.json",
                r#"{"name":"untyped","main":"index.js"}"#,
            ),
            ("node_modules/untyped/index.js", "export {};"),
            (
                "node_modules/@types/untyped/package.json",
                r#"{"name":"@types/untyped","types":"index.d.ts"}"#,
            ),
            (
                "node_modules/@types/untyped/index.d.ts",
                "export interface T {}",
            ),
        ],
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty(), "{:?}", packages(&result));
}

#[test]
fn dependencies_public_resolution_uncertainty_is_not_hidden_by_author_mapping() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"name":"public","private":false,"publishConfig":{"types":"dist/index.d.ts"},"dependencies":{"other":"*"}}"#,
            ),
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"paths":{"broken":["./src/shim.ts"]}}}"#,
            ),
            ("dist/index.d.ts", "export type {Shape} from 'broken';"),
            ("src/shim.ts", "export interface Shape {}"),
            (
                "node_modules/broken/package.json",
                r#"{"name":false,"types":"index.d.ts"}"#,
            ),
            (
                "node_modules/broken/index.d.ts",
                "export interface Shape {}",
            ),
        ],
        json!({"entry":[],"project":["**/*.ts!"]}),
        AnalysisMode::Strict,
    );
    assert!(!result.dependencies.gaps.is_empty());
    assert!(
        !packages(&result)
            .iter()
            .any(|(kind, _, _)| kind == "dependencies")
    );
}

#[test]
fn dependencies_public_exports_specific_target_masks_wildcard_output() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"name":"public","private":false,"exports":{"./feature/*":{"types":"./features/*.d.ts"},"./feature/special":{"types":"./alternate/special.d.ts"}}}"#,
            ),
            (
                "features/special.d.ts",
                "export type {Shape} from 'masked';",
            ),
            (
                "features/visible.d.ts",
                "export type {Shape} from 'visible';",
            ),
            (
                "alternate/special.d.ts",
                "export type {Shape} from 'selected';",
            ),
        ],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Strict,
    );
    assert_eq!(
        packages(&result),
        vec![
            (
                "unlisted".into(),
                "alternate/special.d.ts".into(),
                "selected".into()
            ),
            (
                "unlisted".into(),
                "features/visible.d.ts".into(),
                "visible".into()
            )
        ]
    );
}

#[test]
fn dependencies_manifest_entry_transitive_dependencies() {
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        for manifest_root in [true, false] {
            let manifest = if manifest_root {
                r#"{"private":true,"types":"types.d.ts","dependencies":{"pkg":"*"}}"#
            } else {
                r#"{"private":true,"dependencies":{"pkg":"*"}}"#
            };
            let mut files = vec![
                ("package.json", manifest),
                ("types.d.ts", "import './provider'; export {};"),
                ("provider.ts", "import 'pkg';"),
                (
                    "node_modules/pkg/package.json",
                    r#"{"name":"pkg","main":"index.js"}"#,
                ),
                ("node_modules/pkg/index.js", "export {};"),
            ];
            if !manifest_root {
                files.push(("tsconfig.json", r#"{"files":["types.d.ts"]}"#));
            }
            let (snapshot, result) = run(&files, json!({"entry":[],"project":[]}), mode);
            assert!(packages(&result).is_empty(), "{mode:?} {manifest_root}");
            assert!(!result.dependencies.declarations[0].used.is_empty());
            let usage = result
                .dependencies
                .uses
                .iter()
                .find(|u| u.package == "pkg")
                .unwrap();
            assert_eq!(
                snapshot.sources[&usage.location.file].logical_path,
                "provider.ts"
            );
            assert!(usage.definite);
        }
    }
}

#[test]
fn dependencies_resolved_peer_host_exception_is_not_missing_or_strict() {
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        for installed in [false, true] {
            let mut files = vec![
                (
                    "package.json",
                    r#"{"private":true,"dependencies":{"host":"*"}}"#,
                ),
                ("index.ts", "import 'peer';"),
                (
                    "node_modules/host/package.json",
                    r#"{"name":"host","main":"index.js","peerDependencies":{"peer":"*"}}"#,
                ),
                ("node_modules/host/index.js", "export {};"),
            ];
            if installed {
                files.extend([
                    (
                        "node_modules/peer/package.json",
                        r#"{"name":"peer","main":"index.js"}"#,
                    ),
                    ("node_modules/peer/index.js", "export {};"),
                ]);
            }
            let (_, result) = run(
                &files,
                json!({"entry":["index.ts!"],"project":["*.ts!"]}),
                mode,
            );
            let unlisted = packages(&result)
                .iter()
                .any(|(kind, _, name)| kind == "unlisted" && name == "peer");
            assert_eq!(
                unlisted,
                !installed || mode == AnalysisMode::Strict,
                "{mode:?} installed={installed}"
            );
            assert!(
                packages(&result)
                    .iter()
                    .any(|(kind, _, name)| kind == "dependencies" && name == "host")
            );
        }
    }
}

#[test]
fn dependencies_ordinary_type_only_can_use_declared_companion_without_installation() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"devDependencies":{"@types/legacy":"*"}}"#,
            ),
            ("index.ts", "import type {Shape} from 'legacy';"),
        ],
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty(), "{:?}", packages(&result));
}
#[test]
fn dependencies_runtime_and_ambient_type_exceptions() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"engines":{"custom-runtime":"*"},"devDependencies":{"@types/node":"*","@types/bun":"*","@types/jest":"*"}}"#,
            ),
            ("index.ts", "import 'custom-runtime'; import 'deno';"),
        ],
        json!({"entry":["index.ts!"],"project":["*.ts!"]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty());
}
#[test]
fn dependencies_public_provider_types_versions_are_bundled_metadata() {
    let (snapshot, result) = run(
        &[
            (
                "package.json",
                r#"{"private":false,"types":"public.d.ts","dependencies":{"@types/typed":"*"},"devDependencies":{"typed":"*"}}"#,
            ),
            ("public.d.ts", "export type {Shape} from 'typed';"),
            (
                "node_modules/typed/package.json",
                r#"{"name":"typed","main":"index.js","typesVersions":{"*":{"*":["types/*"]}}}"#,
            ),
            ("node_modules/typed/index.js", "export {};"),
        ],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Strict,
    );
    assert_eq!(snapshot.inventory.public_requirements[0].package, "typed");
    assert!(packages(&result).is_empty());
}

#[test]
fn dependencies_public_transitive_output_credits_required_peer() {
    let (snapshot, result) = run(
        &[
            (
                "package.json",
                r#"{"name":"public","private":false,"types":"index.d.ts","peerDependencies":{"external":"*"}}"#,
            ),
            ("index.d.ts", "export type {Shape} from './nested.js';"),
            (
                "nested.d.ts",
                "export type {Shape} from 'external'; export type {Shape as Again} from './index.js';",
            ),
        ],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Strict,
    );
    assert!(packages(&result).is_empty());
    assert!(!result.dependencies.declarations[0].used.is_empty());
    let usage = result
        .dependencies
        .uses
        .iter()
        .find(|u| u.package == "external")
        .unwrap();
    assert!(matches!(
        usage.producer,
        biome_project_analysis::dependencies::DependencyProducer::PublicOutput { .. }
    ));
    assert_eq!(
        snapshot.sources[&usage.location.file].logical_path,
        "nested.d.ts"
    );
}
#[test]
fn dependencies_missing_binary_has_manifest_evidence() {
    let (_, result) = run(
        &[(
            "package.json",
            r#"{"private":true,"scripts":{"start":"missing-command"}}"#,
        )],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Default,
    );
    assert_eq!(
        packages(&result),
        vec![(
            "binaries".into(),
            "package.json".into(),
            "missing-command".into()
        )]
    );
    assert!(!result.issues[0].related_evidence.is_empty());
}

#[test]
fn dependencies_binary_report_suppression_preserves_provider_credit() {
    for (config, expected) in [
        (json!({"entry":[],"project":[]}), vec!["local-tool"]),
        (
            json!({"entry":[],"project":[],"ignoreGlobalBinaries":false}),
            vec!["echo", "local-tool"],
        ),
        (
            json!({"entry":[],"project":[],"ignoreBinaries":["local-tool"]}),
            vec![],
        ),
    ] {
        let (snapshot, result) = run(
            &[(
                "package.json",
                r#"{"private":true,"scripts":{"start":"echo done; local-tool"}}"#,
            )],
            config,
            AnalysisMode::Default,
        );
        let names: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::Binaries)
            .map(|i| i.symbol.as_deref().unwrap())
            .collect();
        assert_eq!(names, expected);
        let echo = snapshot
            .inventory
            .script_usage
            .binaries
            .iter()
            .find(|b| b.binary == "echo")
            .unwrap();
        assert_eq!(
            echo.report_suppression
                .contains(&biome_project_analysis::scripts::BinaryReportSuppression::Global),
            !expected.contains(&"echo")
        );
        assert!(!echo.optional);
        let local = snapshot
            .inventory
            .script_usage
            .binaries
            .iter()
            .find(|b| b.binary == "local-tool")
            .unwrap();
        assert_eq!(
            local.report_suppression.contains(
                &biome_project_analysis::scripts::BinaryReportSuppression::ExplicitIgnore
            ),
            expected.is_empty()
        );
        assert!(!result.coverage.gaps().is_empty());
    }
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"scripts":{"start":"local-tool"},"dependencies":{"alias":"npm:actual@1"}}"#,
            ),
            (
                "node_modules/alias/package.json",
                r#"{"name":"actual","bin":{"local-tool":"index.js"}}"#,
            ),
            ("node_modules/alias/index.js", "export {};"),
        ],
        json!({"entry":[],"project":[],"ignoreBinaries":["local-tool"]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty());
    assert!(!result.dependencies.declarations[0].used.is_empty());
    assert!(
        result
            .dependencies
            .uses
            .iter()
            .any(|u| u.package == "alias")
    );
}

#[test]
fn dependencies_review_unavailable_public_outputs_suppress_unused() {
    use crate::{Workspace, workspace::UpdateSettingsParams};
    use biome_fs::BiomePath;
    fn configure(service: &crate::LocalWorkspace, project: crate::projects::ProjectKey) {
        service
            .update_settings(UpdateSettingsParams {
                project_key: project,
                configuration: serde_json::from_value(json!({"files":{"maxSize":1024}})).unwrap(),
                workspace_directory: Some(BiomePath::new("/project")),
                extended_configurations: Vec::new(),
                module_graph_resolution_kind: Default::default(),
            })
            .unwrap();
    }
    for transitive in [false, true] {
        let fs = MemoryFileSystem::default();
        fs.insert("/project/package.json".into(), r#"{"private":false,"publishConfig":{"types":"dist/index.d.ts"},"dependencies":{"uncertain":"*"}}"#);
        let oversized = format!(
            "{}\nexport type {{Shape}} from 'uncertain';",
            "/* padding */\n".repeat(200)
        );
        let unavailable = if transitive {
            "dist/nested.d.ts"
        } else {
            "dist/index.d.ts"
        };
        if transitive {
            fs.insert(
                "/project/dist/index.d.ts".into(),
                "export type {Shape} from './nested.js';",
            );
        }
        fs.insert(format!("/project/{unavailable}").into(), oversized.as_str());
        let (service, project) = crate::test_utils::setup_workspace_and_open_project(
            MemoryFileSystem::from_files(fs.files.0.clone()),
            "/project",
        );
        configure(&service, project);
        let request = ProjectAnalysisRequest {
            configuration: serde_json::from_value(json!({"entry":[],"project":[]})).unwrap(),
            ..Default::default()
        };
        let snapshot = service.build_project_snapshot(project, &request).unwrap();
        let result = analyze(&snapshot, &Default::default());
        assert!(
            packages(&result).is_empty(),
            "transitive={transitive}: {:?}",
            packages(&result)
        );
        assert!(
            !result.dependencies.declarations[0]
                .affecting_gaps
                .is_empty()
        );
        assert!(result.dependencies.declarations[0].used.is_empty());
        let gap = result
            .dependencies
            .gaps
            .iter()
            .find(|gap| {
                gap.location
                    .is_some_and(|l| snapshot.sources[&l.file].logical_path == unavailable)
            })
            .unwrap();
        assert!(matches!(
            gap.reason,
            biome_project_analysis::coverage::GapReason::UnavailableInput(_)
        ));
        assert!(result.membership.entry_roots.is_empty());
        assert!(!result.membership.files.iter().any(|f| f.definite));
        assert!(
            result
                .dependencies
                .evidence
                .nodes
                .values()
                .any(|e| e.location == gap.location)
        );
        fs.insert(
            format!("/project/{unavailable}").into(),
            "export type {Shape} from 'uncertain';",
        );
        let warm = service.build_project_snapshot(project, &request).unwrap();
        let (fresh, fresh_project) = crate::test_utils::setup_workspace_and_open_project(
            MemoryFileSystem::from_files(fs.files.0.clone()),
            "/project",
        );
        configure(&fresh, fresh_project);
        let fresh = fresh
            .build_project_snapshot(fresh_project, &request)
            .unwrap();
        let warm_result = analyze(&warm, &Default::default());
        assert_eq!(
            biome_project_analysis::normalize::normalize_snapshot(&warm),
            biome_project_analysis::normalize::normalize_snapshot(&fresh)
        );
        assert_eq!(
            biome_project_analysis::normalize::normalize_result(&warm_result),
            biome_project_analysis::normalize::normalize_result(&analyze(
                &fresh,
                &Default::default()
            ))
        );
        assert!(warm_result.dependencies.gaps.is_empty());
        assert!(!warm_result.dependencies.declarations[0].used.is_empty());
        let usage = warm_result
            .dependencies
            .uses
            .iter()
            .find(|u| u.package == "uncertain")
            .unwrap();
        assert_eq!(warm.sources[&usage.location.file].logical_path, unavailable);
        assert_eq!(u32::from(usage.location.range.start()), 25);
    }
}

#[test]
fn dependencies_review_public_uncertainty_affects_ancestor_declarations() {
    for mode in [AnalysisMode::Default, AnalysisMode::Production] {
        for output in [
            "export type T = import(name).Shape;",
            "export type {Shape} from 'broken';",
        ] {
            let (snapshot, result) = run(
                &[
                    (
                        "package.json",
                        r#"{"private":true,"workspaces":["packages/*"],"dependencies":{"uncertain":"*"}}"#,
                    ),
                    (
                        "packages/child/package.json",
                        r#"{"name":"child","publishConfig":{"types":"dist/index.d.ts"}}"#,
                    ),
                    ("packages/child/dist/index.d.ts", output),
                    (
                        "node_modules/broken/package.json",
                        r#"{"name":false,"types":"index.d.ts"}"#,
                    ),
                    (
                        "node_modules/broken/index.d.ts",
                        "export interface Shape {}",
                    ),
                ],
                json!({"entry":[],"project":[],"workspaces":{"packages/child":{"entry":[],"project":[]}}}),
                mode,
            );
            assert!(
                !packages(&result)
                    .iter()
                    .any(|(kind, _, _)| kind == "dependencies"),
                "{mode:?} {output}: {:?}",
                packages(&result)
            );
            assert!(result.dependencies.declarations[0].used.is_empty());
            assert!(
                !result.dependencies.declarations[0]
                    .affecting_gaps
                    .is_empty()
            );
            assert!(result.membership.entry_roots.is_empty());
            assert!(!result.membership.files.iter().any(|f| f.definite));
            assert!(
                result.dependencies.gaps.iter().any(|g| g
                    .location
                    .is_some_and(|l| snapshot.sources[&l.file].logical_path
                        == "packages/child/dist/index.d.ts"))
            );
        }
    }
}

#[test]
fn dependencies_review_inherited_peer_chain_uses_consumer_installation() {
    for mode in [AnalysisMode::Default, AnalysisMode::Production] {
        for nested in [false, true] {
            let mut files = vec![
                (
                    "package.json",
                    r#"{"private":true,"workspaces":["packages/*"],"dependencies":{"host":"*","peer":"npm:outer@1","leaf":"*","wrong":"*"}}"#,
                ),
                (
                    "packages/child/package.json",
                    r#"{"name":"child","private":true}"#,
                ),
                ("packages/child/src/index.ts", "import 'host';"),
                (
                    "node_modules/host/package.json",
                    r#"{"name":"host","main":"index.js","peerDependencies":{"peer":"*"}}"#,
                ),
                ("node_modules/host/index.js", "export {};"),
                (
                    "node_modules/peer/package.json",
                    r#"{"name":"outer","main":"index.js","peerDependencies":{"leaf":"*"}}"#,
                ),
                ("node_modules/peer/index.js", "export {};"),
                (
                    "node_modules/leaf/package.json",
                    r#"{"name":"leaf","main":"index.js","peerDependencies":{"host":"*"}}"#,
                ),
                ("node_modules/leaf/index.js", "export {};"),
            ];
            if nested {
                files[5].1 =
                    r#"{"name":"outer","main":"index.js","peerDependencies":{"wrong":"*"}}"#;
                files.extend([
                    (
                        "packages/child/src/node_modules/peer/package.json",
                        r#"{"name":"inner","main":"index.js","peerDependencies":{"leaf":"*"}}"#,
                    ),
                    (
                        "packages/child/src/node_modules/peer/index.js",
                        "export {};",
                    ),
                ]);
            }
            let (snapshot, result) = run(
                &files,
                json!({"entry":[],"project":[],"workspaces":{"packages/child":{"entry":["src/index.ts!"],"project":["src/*.ts!"]}}}),
                mode,
            );
            assert_eq!(
                packages(&result),
                vec![("dependencies".into(), "package.json".into(), "wrong".into())],
                "{mode:?} nested={nested}"
            );
            let leaf = result
                .dependencies
                .declarations
                .iter()
                .find(|d| d.declaration.package == "leaf")
                .unwrap();
            assert!(leaf.used.is_empty());
            assert!(!leaf.retained.is_empty());
            let usage = result
                .dependencies
                .uses
                .iter()
                .find(|u| u.package == "peer")
                .unwrap();
            assert_ne!(usage.workspace, leaf.declaration.workspace);
            assert_eq!(
                snapshot.sources[&usage.location.file].logical_path,
                "packages/child/src/index.ts"
            );
            assert!(usage.resolutions.iter().flat_map(|r|&r.trace).any(|e|matches!(e,biome_project_analysis::facts::ResolutionEvent::Package{name:Some(name),..} if name==if nested {"inner"} else {"outer"})));
        }
    }
}

#[test]
fn dependencies_review_extensionless_types_versions_select_only_declarations() {
    let (snapshot, result) = run(
        &[
            (
                "package.json",
                r#"{"name":"public","publishConfig":{"typesVersions":{"*":{"*":["versions/*"]}}},"dependencies":{"required":"*"}}"#,
            ),
            (
                "versions/old.d.ts",
                "export type {Shape} from 'required'; export type {Other} from 'unlisted';",
            ),
            ("versions/runtime.js", "import 'runtime-only';"),
            ("versions/source.ts", "import 'source-only';"),
        ],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Strict,
    );
    assert_eq!(
        packages(&result),
        vec![(
            "unlisted".into(),
            "versions/old.d.ts".into(),
            "unlisted".into()
        )]
    );
    assert_eq!(snapshot.inventory.public_outputs.len(), 1);
    assert_eq!(
        snapshot.sources[&snapshot.inventory.public_outputs[0].file].logical_path,
        "versions/old.d.ts"
    );
    assert!(result.membership.entry_roots.is_empty());
    assert!(!result.membership.files.iter().any(|f| f.definite));
}

#[test]
fn dependencies_review_alternate_export_alias_keeps_public_output() {
    let (snapshot, result) = run(
        &[
            (
                "package.json",
                r#"{"name":"public","publishConfig":{"exports":{"./a/*":{"types":"./types/*.d.ts"},"./a/special":null,"./z/*":{"types":"./types/*.d.ts"}}}}"#,
            ),
            ("types/special.d.ts", "export type {Shape} from 'external';"),
        ],
        json!({"entry":[],"project":[]}),
        AnalysisMode::Strict,
    );
    assert_eq!(
        packages(&result),
        vec![(
            "unlisted".into(),
            "types/special.d.ts".into(),
            "external".into()
        )]
    );
    assert!(!snapshot.inventory.public_outputs.is_empty());
    assert!(result.membership.entry_roots.is_empty());
    assert!(!result.membership.files.iter().any(|f| f.definite));
}

#[test]
fn dependencies_review_peer_expansion_keeps_distinct_source_consumers() {
    let (_, result) = run(
        &[
            (
                "package.json",
                r#"{"private":true,"dependencies":{"host":"*","peer":"*","first":"*","second":"*"}}"#,
            ),
            ("a/index.ts", "import 'host';"),
            ("b/index.ts", "import 'host';"),
            (
                "node_modules/host/package.json",
                r#"{"name":"host","main":"index.js","peerDependencies":{"peer":"*"}}"#,
            ),
            ("node_modules/host/index.js", "export {};"),
            (
                "a/node_modules/peer/package.json",
                r#"{"name":"first-peer","main":"index.js","peerDependencies":{"first":"*"}}"#,
            ),
            ("a/node_modules/peer/index.js", "export {};"),
            (
                "b/node_modules/peer/package.json",
                r#"{"name":"second-peer","main":"index.js","peerDependencies":{"second":"*"}}"#,
            ),
            ("b/node_modules/peer/index.js", "export {};"),
        ],
        json!({"entry":["a/index.ts!","b/index.ts!"],"project":["**/*.ts!"]}),
        AnalysisMode::Default,
    );
    assert!(packages(&result).is_empty(), "{:?}", packages(&result));
    for name in ["first", "second"] {
        assert!(
            result
                .dependencies
                .declarations
                .iter()
                .any(|d| d.declaration.package == name && !d.retained.is_empty())
        );
    }
}

#[test]
fn dependencies_derived_unknown_provider_follows_producer_activation() {
    use biome_project_analysis::{
        coverage::GapReason,
        dependencies::DependencyProducer,
        facts::{ResolutionEvent, ResolutionOutcome, ResolutionPurpose},
    };
    for activation in ["definite", "possible", "dead", "public"] {
        let entry = match activation {
            "definite" => "import './src/consumer.js';",
            "possible" => "import(flag ? './src/consumer.js' : './other.js');",
            _ => "export {};",
        };
        let manifest = if activation == "public" {
            r#"{"publishConfig":{"types":"src/consumer.d.ts"},"dependencies":{"host":"*","peer":"*","leaf":"*"}}"#
        } else {
            r#"{"private":true,"dependencies":{"host":"*","peer":"*","leaf":"*"}}"#
        };
        let (snapshot, result) = run(
            &[
                ("package.json", manifest),
                ("index.ts", entry),
                ("other.js", "export {};"),
                ("src/consumer.js", "import 'host';"),
                ("src/consumer.d.ts", "export type {Shape} from 'host';"),
                (
                    "node_modules/host/package.json",
                    r#"{"name":"host","main":"index.js","types":"index.d.ts","peerDependencies":{"peer":"*"}}"#,
                ),
                ("node_modules/host/index.js", "export {};"),
                ("node_modules/host/index.d.ts", "export interface Shape {}"),
                (
                    "node_modules/peer/package.json",
                    r#"{"name":"outer","main":"index.js","peerDependencies":{"leaf":"*"}}"#,
                ),
                ("node_modules/peer/index.js", "export {};"),
                (
                    "src/node_modules/peer/package.json",
                    r#"{"name":"inner","main":"index.js","peerDependencies":false}"#,
                ),
                ("src/node_modules/peer/index.js", "export {};"),
            ],
            json!({"entry":["index.ts!"],"project":["**/*.ts!","**/*.js!"]}),
            AnalysisMode::Default,
        );
        assert!(snapshot.inventory.package_providers.iter().any(|p| {
            p.consumer.is_some()
                && p.requested_package == "peer"
                && p.resolutions
                    .iter()
                    .any(|r| matches!(r.outcome, ResolutionOutcome::Unknown { .. }))
        }));
        let leaf = result
            .dependencies
            .declarations
            .iter()
            .find(|d| d.declaration.package == "leaf")
            .unwrap();
        assert!(leaf.used.is_empty() && leaf.retained.is_empty());
        if activation == "dead" {
            assert!(result.dependencies.gaps.is_empty());
            assert!(result.dependencies.uses.is_empty());
            assert!(leaf.affecting_gaps.is_empty());
            assert_eq!(packages(&result).len(), 3);
            continue;
        }
        assert!(
            !packages(&result).iter().any(|(_, _, name)| name == "leaf"),
            "{activation}: {:?}",
            packages(&result)
        );
        assert!(!leaf.affecting_gaps.is_empty());
        let peer = result
            .dependencies
            .uses
            .iter()
            .find(|u| u.package == "peer")
            .unwrap();
        assert_eq!(peer.definite, activation != "possible");
        assert!(peer.optional && peer.type_only);
        assert!(
            peer.resolutions
                .iter()
                .any(|r| matches!(r.outcome, ResolutionOutcome::Unknown { .. }))
        );
        assert!(peer.resolutions.iter().flat_map(|r|&r.trace).any(|e|matches!(e,ResolutionEvent::Manifest{file,known:false,..} if snapshot.sources[file].logical_path=="src/node_modules/peer/package.json") || matches!(e,ResolutionEvent::Manifest{file,diagnostics,..} if snapshot.sources[file].logical_path=="src/node_modules/peer/package.json" && !diagnostics.is_empty())));
        assert!(
            result
                .dependencies
                .gaps
                .iter()
                .any(|g| g.location == Some(peer.location)
                    && matches!(g.reason, GapReason::UnavailableInput(_)))
        );
        if activation == "public" {
            assert!(matches!(
                peer.producer,
                DependencyProducer::PublicOutput { .. }
            ));
            assert!(
                peer.resolutions
                    .iter()
                    .all(|r| r.context.purpose == ResolutionPurpose::PublicOutput)
            );
        } else {
            assert!(matches!(peer.producer, DependencyProducer::Import { .. }));
        }
    }
}

#[test]
fn dependencies_provider_evidence_scales_with_needed_closure() {
    for chained in [false, true] {
        for imports in [1, 4] {
            for unrelated in [2, 7] {
                let mut declarations = serde_json::Map::new();
                for name in ["host", "peer", "leaf"] {
                    declarations.insert(name.into(), json!("*"));
                }
                for i in 0..unrelated {
                    declarations.insert(format!("unrelated-{i}"), json!("*"));
                }
                let manifest = json!({"private":true,"dependencies":declarations}).to_string();
                let entry = "import 'host';\n".repeat(imports);
                let host = if chained {
                    r#"{"name":"host","main":"index.js","types":"index.d.ts","peerDependencies":{"peer":"*"}}"#
                } else {
                    r#"{"name":"host","main":"index.js","types":"index.d.ts"}"#
                };
                let (snapshot, _) = run(
                    &[
                        ("package.json", &manifest),
                        ("index.ts", &entry),
                        ("node_modules/host/package.json", host),
                        ("node_modules/host/index.js", "export {};"),
                        ("node_modules/host/index.d.ts", "export interface Shape {}"),
                        (
                            "node_modules/peer/package.json",
                            r#"{"name":"peer","main":"index.js","types":"index.d.ts","peerDependencies":{"leaf":"*"}}"#,
                        ),
                        ("node_modules/peer/index.js", "export {};"),
                        ("node_modules/peer/index.d.ts", "export {};"),
                        (
                            "node_modules/leaf/package.json",
                            r#"{"name":"leaf","main":"index.js","types":"index.d.ts","peerDependencies":{"host":"*"}}"#,
                        ),
                        ("node_modules/leaf/index.js", "export {};"),
                        ("node_modules/leaf/index.d.ts", "export {};"),
                    ],
                    json!({"entry":["index.ts!"],"project":["*.ts!"]}),
                    AnalysisMode::Default,
                );
                let providers = &snapshot.inventory.package_providers;
                assert_eq!(
                    providers.iter().filter(|p| p.consumer.is_none()).count(),
                    3 + unrelated
                );
                let consumers: Vec<_> = providers.iter().filter(|p| p.consumer.is_some()).collect();
                assert_eq!(
                    consumers.len(),
                    if chained { 3 * imports } else { 0 },
                    "chained={chained}, imports={imports}, unrelated={unrelated}"
                );
                assert!(
                    consumers
                        .iter()
                        .all(|p| matches!(p.requested_package.as_str(), "host" | "peer" | "leaf"))
                );
            }
        }
    }
}
