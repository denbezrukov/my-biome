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

#[test]
fn b7_all_seventeen_categories_compose_with_policy() {
    use IssueKind::*;
    use biome_project_analysis::{options::ReportSelection, select_report};
    type Sources = Vec<(&'static str, &'static str)>;
    let cases: Vec<(IssueKind, Sources, Sources)> = vec![
        (
            Files,
            vec![("orphan.ts", "export const unused=1;")],
            vec![
                ("index.ts", "import './orphan';"),
                ("orphan.ts", "export const unused=1;"),
            ],
        ),
        (
            Dependencies,
            vec![(
                "package.json",
                r#"{"private":true,"dependencies":{"unused":"1"}}"#,
            )],
            vec![("package.json", r#"{"private":true,"dependencies":{}}"#)],
        ),
        (
            DevDependencies,
            vec![(
                "package.json",
                r#"{"private":true,"devDependencies":{"unused":"1"}}"#,
            )],
            vec![("package.json", r#"{"private":true,"devDependencies":{}}"#)],
        ),
        (
            OptionalPeerDependencies,
            vec![
                (
                    "package.json",
                    r#"{"private":true,"peerDependencies":{"peer":"1"},"peerDependenciesMeta":{"peer":{"optional":true}}}"#,
                ),
                ("index.ts", "import 'peer';"),
            ],
            vec![(
                "package.json",
                r#"{"private":true,"peerDependencies":{"peer":"1"},"peerDependenciesMeta":{"peer":{"optional":true}}}"#,
            )],
        ),
        (
            Unlisted,
            vec![("index.ts", "import 'missing';")],
            vec![("index.ts", "import 'node:fs';")],
        ),
        (
            Binaries,
            vec![(
                "package.json",
                r#"{"private":true,"scripts":{"start":"b7-missing-command"}}"#,
            )],
            vec![(
                "package.json",
                r#"{"private":true,"scripts":{"start":"echo hello"}}"#,
            )],
        ),
        (
            Unresolved,
            vec![("index.ts", "import './missing';")],
            vec![("index.ts", "import './present';"), ("present.ts", "")],
        ),
        (
            Exports,
            vec![
                ("index.ts", "import './provider';"),
                ("provider.ts", "export const unused=1;"),
            ],
            vec![
                ("index.ts", "import {unused} from './provider'; unused;"),
                ("provider.ts", "export const unused=1;"),
            ],
        ),
        (
            Types,
            vec![
                ("index.ts", "import './provider';"),
                ("provider.ts", "export interface Shape {}"),
            ],
            vec![
                (
                    "index.ts",
                    "import type {Shape} from './provider'; type Used = Shape;",
                ),
                ("provider.ts", "export interface Shape {}"),
            ],
        ),
        (
            NsExports,
            vec![
                ("index.ts", "import * as NS from './provider'; consume(NS);"),
                ("provider.ts", "export const unused=1;"),
            ],
            vec![
                ("index.ts", "import * as NS from './provider'; NS.unused;"),
                ("provider.ts", "export const unused=1;"),
            ],
        ),
        (
            NsTypes,
            vec![
                (
                    "index.ts",
                    "import * as NS from './provider'; type Shape = typeof NS;",
                ),
                ("provider.ts", "export interface Item {}"),
            ],
            vec![
                (
                    "index.ts",
                    "import type {Item} from './provider'; type Shape = Item;",
                ),
                ("provider.ts", "export interface Item {}"),
            ],
        ),
        (
            Duplicates,
            vec![
                ("index.ts", "import './provider';"),
                (
                    "provider.ts",
                    "export function same() {} export default same;",
                ),
            ],
            vec![
                ("index.ts", "import './provider';"),
                ("provider.ts", "export function same() {}"),
            ],
        ),
        (
            EnumMembers,
            vec![
                ("index.ts", "import {Level} from './provider'; Level.used;"),
                ("provider.ts", "export enum Level { used, unused }"),
            ],
            vec![
                (
                    "index.ts",
                    "import {Level} from './provider'; Level.used; Level.unused;",
                ),
                ("provider.ts", "export enum Level { used, unused }"),
            ],
        ),
        (
            NamespaceMembers,
            vec![
                ("index.ts", "import {NS} from './provider'; NS.used;"),
                (
                    "provider.ts",
                    "export namespace NS { export const used=1; export const unused=2; }",
                ),
            ],
            vec![
                (
                    "index.ts",
                    "import {NS} from './provider'; NS.used; NS.unused;",
                ),
                (
                    "provider.ts",
                    "export namespace NS { export const used=1; export const unused=2; }",
                ),
            ],
        ),
        (
            Catalog,
            vec![(
                "package.json",
                r#"{"private":true,"catalog":{"unused":"1"}}"#,
            )],
            vec![(
                "package.json",
                r#"{"private":true,"catalog":{"used":"1"},"dependencies":{"used":"catalog:"}}"#,
            )],
        ),
        (
            CatalogReferences,
            vec![(
                "package.json",
                r#"{"private":true,"dependencies":{"missing":"catalog:"}}"#,
            )],
            vec![(
                "package.json",
                r#"{"private":true,"catalog":{"missing":"1"},"dependencies":{"missing":"catalog:"}}"#,
            )],
        ),
        (
            Cycles,
            vec![
                ("index.ts", "import './provider';"),
                ("provider.ts", "import './index';"),
            ],
            vec![("index.ts", "import './provider';"), ("provider.ts", "")],
        ),
    ];
    assert_eq!(cases.len(), IssueKind::ALL.len());
    for (kind, positive, negative) in cases {
        let config = json!({"include":[kind]});
        let full = run(&positive, config.clone(), AnalysisMode::Default);
        assert!(
            full.issues.iter().any(|issue| issue.kind == kind),
            "positive {kind:?}: {:?}",
            full.issues
        );
        let clean = run(&negative, config, AnalysisMode::Default);
        assert!(
            !clean.issues.iter().any(|issue| issue.kind == kind),
            "negative {kind:?}"
        );
        for selection in [
            ReportSelection {
                workspaces: [biome_project_analysis::model::WorkspaceId(999)].into(),
                ..Default::default()
            },
            ReportSelection {
                restrict_issues: true,
                ..Default::default()
            },
            ReportSelection {
                changed_paths: Some(Default::default()),
                ..Default::default()
            },
            ReportSelection {
                paths: vec!["not-selected".into()],
                ..Default::default()
            },
        ] {
            let selected = select_report(&full, &selection);
            assert!(selected.issues.is_empty(), "selection {kind:?}");
            assert_eq!(full.coverage, selected.coverage);
            assert_eq!(full.evidence, selected.evidence);
            assert_eq!(full.capabilities, selected.capabilities);
            assert_eq!(full.membership, selected.membership);
            assert_eq!(full.dependencies, selected.dependencies);
            assert_eq!(full.catalogs, selected.catalogs);
        }
    }
}

#[test]
fn b7_tagged_imports_keep_credit_and_resolution_evidence() {
    let files = [
        (
            "index.ts",
            "// @custom\nimport './missing';\n// @custom\nimport 'unlisted';\n// @custom\nimport {used} from './provider'; used;",
        ),
        ("provider.ts", "export const used = 1;"),
    ];
    let plain = run(&files, json!({}), AnalysisMode::Default);
    assert!(
        plain
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Unresolved)
    );
    assert!(
        plain
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Unlisted)
    );
    let tagged = run(&files, json!({"tags":["-custom"]}), AnalysisMode::Default);
    assert!(tagged.issues.is_empty(), "{:?}", tagged.issues);
    assert_eq!(plain.coverage, tagged.coverage);
    assert_eq!(plain.membership, tagged.membership);
    assert_eq!(plain.dependencies.decisions, tagged.dependencies.decisions);
    assert_eq!(plain.dependencies.evidence, tagged.dependencies.evidence);
    assert_eq!(
        plain
            .dependencies
            .uses
            .iter()
            .map(|usage| (&usage.package, usage.definite))
            .collect::<Vec<_>>(),
        tagged
            .dependencies
            .uses
            .iter()
            .map(|usage| (&usage.package, usage.definite))
            .collect::<Vec<_>>()
    );
}

#[test]
fn b7_member_tags_and_stale_parent_causes() {
    let files = [
        (
            "index.ts",
            "import {Level, Status} from './provider'; Level.used; Level.other; Status.used;",
        ),
        (
            "provider.ts",
            "// @custom\nexport enum Level { used, other }\n// @custom\nexport enum Status { used, unused }",
        ),
    ];
    let result = run(&files, json!({"tags":["-custom"]}), AnalysisMode::Default);
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::EnumMembers)
    );
    let hints: Vec<_> = result
        .hints
        .iter()
        .filter(|hint| hint.kind == biome_project_analysis::result::HintKind::Tag)
        .collect();
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].message.contains("Level"));
    let location = hints[0].location.unwrap();
    let biome_project_analysis::model::SourceContent::Available { text, .. } =
        &result.sources[&location.file].content
    else {
        panic!()
    };
    assert_eq!(
        &text[usize::from(location.range.start())..usize::from(location.range.end())],
        "@custom"
    );
    let result = run(
        &[
            ("index.ts", "import {NS} from './provider'; NS.used;"),
            (
                "provider.ts",
                "export namespace NS { export const used=1;\n// @custom\nexport const tagged=2; export const other=3; }",
            ),
        ],
        json!({"tags":["custom"]}),
        AnalysisMode::Default,
    );
    assert_eq!(
        result
            .issues
            .iter()
            .filter(|issue| issue.kind == IssueKind::NamespaceMembers)
            .map(|issue| issue.symbol.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["NS.tagged"]
    );
}

#[test]
fn b7_binary_policy_inheritance_and_used_ignore_causes() {
    let files = [
        (
            "package.json",
            r#"{"private":true,"workspaces":["packages/*"]}"#,
        ),
        (
            "packages/a/package.json",
            r#"{"private":true,"scripts":{"start":"echo hello"}}"#,
        ),
    ];
    let result = run(
        &files,
        json!({"ignoreGlobalBinaries":false,"workspaces":{"packages/*":{}}}),
        AnalysisMode::Default,
    );
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Binaries)
    );
    let result = run(
        &files,
        json!({"ignoreGlobalBinaries":false,"workspaces":{"packages/*":{"ignoreGlobalBinaries":true}}}),
        AnalysisMode::Default,
    );
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Binaries)
    );
    let result = run(
        &[(
            "package.json",
            r#"{"private":true,"scripts":{"start":"tool"}}"#,
        )],
        json!({"ignoreBinaries":["tool"]}),
        AnalysisMode::Default,
    );
    assert!(
        !result
            .hints
            .iter()
            .any(|hint| hint.message.contains("tool")),
        "{:?}",
        result.hints
    );
}

#[test]
fn b7_report_ignore_families_and_production_suffix() {
    let files = [
        ("index.ts", "import './missing'; import './provider';"),
        ("provider.ts", "export const unused=1;"),
        ("orphan.ts", ""),
    ];
    for (setting, kind) in [
        (json!({"ignore":["provider.ts"]}), IssueKind::Exports),
        (json!({"ignoreFiles":["orphan.ts"]}), IssueKind::Files),
        (
            json!({"ignoreUnresolved":["./missing"]}),
            IssueKind::Unresolved,
        ),
        (
            json!({"ignoreIssues":{"provider.ts":["exports"]}}),
            IssueKind::Exports,
        ),
    ] {
        let result = run(&files, setting, AnalysisMode::Default);
        assert!(
            !result.issues.iter().any(|issue| issue.kind == kind),
            "{kind:?}"
        );
        assert!(!result.policy.suppressions.is_empty());
        assert_eq!(
            biome_project_analysis::policy::apply_policy(
                result.clone(),
                &AnalysisOptions::default()
            ),
            result
        );
    }
    let combined = run(
        &[(
            "package.json",
            r#"{"private":true,"dependencies":{"unused":"1"},"scripts":{"start":"tool"}}"#,
        )],
        json!({"ignoreDependencies":["unused!"],"ignoreBinaries":["tool!"]}),
        AnalysisMode::Default,
    );
    assert!(
        !combined
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Dependencies)
    );
    assert!(
        combined
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Binaries)
    );
    assert!(!matches!(
        combined.coverage,
        biome_project_analysis::coverage::Coverage::Complete
    ));
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        for (manifest, kind, config) in [
            (
                r#"{"private":true,"dependencies":{"unused":"1"}}"#,
                IssueKind::Dependencies,
                json!({"ignoreDependencies":["unused!"]}),
            ),
            (
                r#"{"private":true,"scripts":{"start":"tool"}}"#,
                IssueKind::Binaries,
                json!({"ignoreBinaries":["tool!"]}),
            ),
        ] {
            let result = run(&[("package.json", manifest)], config, mode);
            assert_eq!(
                result.issues.iter().any(|issue| issue.kind == kind),
                mode == AnalysisMode::Default,
                "{mode:?} {kind:?}"
            );
        }
    }
}

#[test]
fn b7_empty_discovery_patterns_have_configuration_hints() {
    let result = run(
        &[],
        json!({"entry":["missing.ts"],"project":["nothing/**"]}),
        AnalysisMode::Default,
    );
    assert!(
        result
            .hints
            .iter()
            .any(|hint| hint.message.contains("entry") && hint.message.contains("missing.ts")),
        "{:?}",
        result.hints
    );
    assert!(
        result
            .hints
            .iter()
            .any(|hint| hint.message.contains("project") && hint.message.contains("nothing/**"))
    );
}

#[test]
fn b7_symbol_patterns_and_member_ignores() {
    let files = [
        (
            "package.json",
            r#"{"private":true,"dependencies":{"unused-package":"1"}}"#,
        ),
        (
            "index.ts",
            "import './missing'; import {Level} from './provider'; Level.used;",
        ),
        ("provider.ts", "export enum Level {used,unused}"),
    ];
    let result = run(
        &files,
        json!({"ignoreDependencies":["^unused-"],"ignoreUnresolved":["^\\./miss"],"ignoreMembers":["unused"]}),
        AnalysisMode::Default,
    );
    assert!(
        !result.issues.iter().any(|issue| matches!(
            issue.kind,
            IssueKind::Dependencies | IssueKind::Unresolved | IssueKind::EnumMembers
        )),
        "{:?}",
        result.issues
    );
    let result = run(
        &files,
        json!({"ignoreDependencies":["(?=unused)"]}),
        AnalysisMode::Default,
    );
    assert!(
        result
            .coverage
            .gaps()
            .iter()
            .any(|gap| gap.producer == "analysis policy")
    );
}

#[test]
fn b7_used_member_tag_has_its_own_hint() {
    let result = run(
        &[
            ("index.ts", "import {Level} from './provider'; Level.used;"),
            (
                "provider.ts",
                "export enum Level {\n// @custom\nused, unused }",
            ),
        ],
        json!({"tags":["-custom"]}),
        AnalysisMode::Default,
    );
    assert!(
        result
            .hints
            .iter()
            .any(|hint| hint.message.contains("Level.used")
                && hint.kind == biome_project_analysis::result::HintKind::Tag),
        "{:?}",
        result.hints
    );
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.symbol.as_deref() == Some("Level.unused"))
    );
}

#[test]
fn b7_reexport_keeps_origin_tag_policy() {
    let result = run(
        &[
            ("index.ts", "import './barrel';"),
            ("barrel.ts", "export {hidden} from './provider';"),
            ("provider.ts", "// @public\nexport const hidden=1;"),
        ],
        json!({}),
        AnalysisMode::Default,
    );
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Exports),
        "{:?}",
        result.issues
    );
}

fn assert_tag_policy_preserves_analysis(
    plain: &ProjectAnalysisResult,
    filtered: &ProjectAnalysisResult,
) {
    assert_eq!(plain.coverage, filtered.coverage);
    assert_eq!(plain.membership, filtered.membership);
    assert_eq!(plain.capabilities, filtered.capabilities);
    assert_eq!(
        plain.dependencies.decisions,
        filtered.dependencies.decisions
    );
    assert_eq!(plain.dependencies.evidence, filtered.dependencies.evidence);
    assert_eq!(plain.resolution.evidence, filtered.resolution.evidence);
    assert_eq!(plain.exports.consumed, filtered.exports.consumed);
    assert_eq!(plain.exports.used_slots, filtered.exports.used_slots);
    assert_eq!(plain.members.used, filtered.members.used);
    assert_eq!(plain.members.uncertainty, filtered.members.uncertainty);
    assert_eq!(
        plain
            .resolution
            .requests
            .iter()
            .map(|request| (
                &request.specifier,
                request.location,
                request.package_category
            ))
            .collect::<Vec<_>>(),
        filtered
            .resolution
            .requests
            .iter()
            .map(|request| (
                &request.specifier,
                request.location,
                request.package_category
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn b7_review_untagged_imports_survive_positive_tag_selection() {
    let files = [("index.ts", "import 'missing-package'; import './missing';")];
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let plain = run(&files, json!({}), mode);
        let selected = run(&files, json!({"tags":["+custom"]}), mode);
        for kind in [IssueKind::Unlisted, IssueKind::Unresolved] {
            assert!(plain.issues.iter().any(|issue| issue.kind == kind));
            assert!(
                selected.issues.iter().any(|issue| issue.kind == kind),
                "{mode:?} {kind:?}: {:?}",
                selected.issues
            );
        }
        assert!(selected.policy.tag_suppressions.is_empty());
        assert_tag_policy_preserves_analysis(&plain, &selected);
    }
}

#[test]
fn b7_review_import_tags_only_apply_configured_selectors() {
    for tag in ["public", "beta", "alias", "internal", "custom"] {
        let source =
            format!("// @{tag}\nimport 'missing-package';\n// @{tag}\nimport './missing';");
        let files = [("index.ts", source.as_str())];
        for mode in [
            AnalysisMode::Default,
            AnalysisMode::Production,
            AnalysisMode::Strict,
        ] {
            let plain = run(&files, json!({}), mode);
            for (selectors, expected) in [
                (vec![], true),
                (vec![format!("+{tag}")], true),
                (vec![format!("-{tag}")], false),
                (vec!["+other".into()], false),
            ] {
                let result = run(&files, json!({"tags":selectors}), mode);
                for kind in [IssueKind::Unlisted, IssueKind::Unresolved] {
                    assert_eq!(
                        result.issues.iter().any(|issue| issue.kind == kind),
                        expected,
                        "{tag} {mode:?} {selectors:?} {kind:?}"
                    );
                }
                assert_eq!(
                    result.policy.tag_suppressions.len(),
                    if expected { 0 } else { 2 }
                );
                for suppression in &result.policy.tag_suppressions {
                    assert!(result.evidence.nodes.contains_key(&suppression.evidence));
                }
                assert_tag_policy_preserves_analysis(&plain, &result);
            }
        }
    }
}

#[test]
fn b7_review_named_reexport_combines_own_and_origin_tags() {
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        for (own, origin, selectors, expected) in [
            ("custom", "", vec!["-custom"], false),
            ("custom", "", vec!["+custom"], true),
            ("", "", vec!["+custom"], false),
            ("", "", vec![], true),
            ("public", "", vec![], false),
            ("beta", "", vec![], false),
            ("alias", "", vec![], false),
            ("internal", "", vec![], mode == AnalysisMode::Default),
            ("", "custom", vec!["-custom"], false),
            ("", "custom", vec!["+custom"], true),
            ("custom", "public", vec!["+custom"], false),
        ] {
            let barrel = format!("// @{own}\nexport {{hidden}} from './provider';");
            let provider = format!("// @{origin}\nexport const hidden=1;");
            let files = [
                ("index.ts", "import './barrel';"),
                ("barrel.ts", barrel.as_str()),
                ("provider.ts", provider.as_str()),
            ];
            let plain = run(&files, json!({}), mode);
            let result = run(&files, json!({"tags": selectors}), mode);
            assert_eq!(
                result
                    .issues
                    .iter()
                    .any(|issue| issue.kind == IssueKind::Exports
                        && result.sources[&issue.location.file].logical_path == "barrel.ts"),
                expected,
                "own={own} origin={origin} {mode:?} {selectors:?}: {:?}",
                result.issues
            );
            assert_tag_policy_preserves_analysis(&plain, &result);
        }
    }
}

#[test]
fn b7_review_independent_member_protection_leaves_stale_parent_hint() {
    use biome_diagnostics::Severity;
    use biome_project_analysis::{model::SourceContent, result::HintKind};
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let files = [
            ("index.ts", "import {E} from './provider'; E.used;"),
            (
                "provider.ts",
                "// @selected @custom\nexport enum E { used, unused }",
            ),
        ];
        let selected = run(&files, json!({"tags":["+selected"]}), mode);
        assert!(
            selected
                .issues
                .iter()
                .any(|issue| issue.kind == IssueKind::EnumMembers)
        );
        let suppressed = run(&files, json!({"tags":["+selected", "-custom"]}), mode);
        assert!(
            !suppressed
                .issues
                .iter()
                .any(|issue| issue.kind == IssueKind::EnumMembers)
        );
        assert!(
            !suppressed
                .hints
                .iter()
                .any(|hint| hint.kind == HintKind::Tag)
        );
        assert_tag_policy_preserves_analysis(&selected, &suppressed);
        for member_tag in ["public", "beta", "alias", "internal", "other", ""] {
            let provider =
                format!("// @custom\nexport enum E {{ used,\n// @{member_tag}\nunused }}");
            let files = [
                ("index.ts", "import {E} from './provider'; E.used;"),
                ("provider.ts", provider.as_str()),
            ];
            let plain = run(&files, json!({"tags":["-other"]}), mode);
            let protected = !member_tag.is_empty()
                && (member_tag != "internal" || mode != AnalysisMode::Default);
            assert_eq!(
                plain
                    .issues
                    .iter()
                    .any(|issue| issue.kind == IssueKind::EnumMembers),
                !protected
            );
            for escalate in [false, true] {
                let result = run(
                    &files,
                    json!({"tags":["-custom", "-other"], "treatTagHintsAsErrors":escalate}),
                    mode,
                );
                assert!(
                    !result
                        .issues
                        .iter()
                        .any(|issue| issue.kind == IssueKind::EnumMembers)
                );
                let hints: Vec<_> = result
                    .hints
                    .iter()
                    .filter(|hint| hint.kind == HintKind::Tag && hint.message.contains("@custom"))
                    .collect();
                assert_eq!(
                    hints.len(),
                    usize::from(protected),
                    "{member_tag} {mode:?} escalate={escalate}: {:?}",
                    result.hints
                );
                if let Some(hint) = hints.first() {
                    assert_eq!(
                        hint.severity,
                        if escalate {
                            Severity::Error
                        } else {
                            Severity::Warning
                        }
                    );
                    let location = hint.location.unwrap();
                    let SourceContent::Available { text, .. } =
                        &result.sources[&location.file].content
                    else {
                        panic!()
                    };
                    assert_eq!(
                        &text[usize::from(location.range.start())
                            ..usize::from(location.range.end())],
                        "@custom"
                    );
                    assert!(!hint.related_evidence.is_empty());
                    for evidence in &hint.related_evidence {
                        assert!(result.evidence.nodes.contains_key(evidence));
                    }
                }
                assert_tag_policy_preserves_analysis(&plain, &result);
            }
        }
    }
}
