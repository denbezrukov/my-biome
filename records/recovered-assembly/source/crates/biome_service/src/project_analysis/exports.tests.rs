use super::ProjectAnalysisRequest;
use biome_fs::MemoryFileSystem;
use biome_project_analysis::{
    analyze,
    model::AnalysisSnapshot,
    options::AnalysisOptions,
    result::{IssueKind, ProjectAnalysisResult},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn run(
    files: &[(&str, &str)],
    options: AnalysisOptions,
) -> (Arc<AnalysisSnapshot>, ProjectAnalysisResult) {
    run_config(
        files,
        json!({"entry":["index.ts!"],"project":["**/*.ts!","**/*.js!"]}),
        options,
    )
}
fn run_config(
    files: &[(&str, &str)],
    config: serde_json::Value,
    options: AnalysisOptions,
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
        mode: options.mode,
        ..Default::default()
    };
    let snapshot = service.build_project_snapshot(project, &request).unwrap();
    let result = analyze(&snapshot, &options);
    (snapshot, result)
}
fn symbols(result: &ProjectAnalysisResult) -> Vec<(String, String, IssueKind)> {
    let mut values: Vec<_> = result
        .issues
        .iter()
        .filter(|i| matches!(i.kind, IssueKind::Exports | IssueKind::Types))
        .map(|i| {
            (
                result.sources[&i.location.file].logical_path.to_string(),
                i.symbol.clone().unwrap(),
                i.kind,
            )
        })
        .collect();
    values.sort();
    values
}
fn expected(items: &[(&str, &str, IssueKind)]) -> Vec<(String, String, IssueKind)> {
    let mut values: Vec<_> = items
        .iter()
        .map(|(p, n, k)| (p.to_string(), n.to_string(), *k))
        .collect();
    values.sort();
    values
}

#[test]
fn known_namespace_keys_and_escapes() {
    for expression in ["NS.used", "NS['used']", r"NS.\u0075sed"] {
        let entry = format!("import * as NS from './lib'; {expression};");
        let (_, result) = run(
            &[
                ("index.ts", &entry),
                ("lib.ts", "export const used = 1; export const unused = 2;"),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("lib.ts", "unused", IssueKind::Exports)]),
            "{expression}"
        );
    }
    for expression in ["NS.Used", r"NS.\u0055sed"] {
        let entry = format!("import * as NS from './lib'; type T = {expression};");
        let (_, result) = run(
            &[
                ("index.ts", &entry),
                (
                    "lib.ts",
                    "export interface Used {} export interface Unused {}",
                ),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("lib.ts", "Unused", IssueKind::Types)]),
            "{expression}"
        );
    }
}
#[test]
fn unknown_namespace_keys_do_not_guess() {
    for expression in ["NS[key]", "NS[tag`used`]"] {
        let entry = format!(
            "import * as NS from './lib'; import './control'; const tag = () => 'unused'; {expression};"
        );
        let (_, result) = run(
            &[
                ("index.ts", &entry),
                ("lib.ts", "export const used = 1; export const unused = 2;"),
                ("control.ts", "export const control = 3;"),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("control.ts", "control", IssueKind::Exports)]),
            "{expression}"
        );
    }
}
#[test]
fn aliases_star_cycles_explicit_precedence_and_default() {
    let (_, result) = run(
        &[
            ("index.ts", "import { chosen } from './barrel';"),
            (
                "barrel.ts",
                "export * from './cycle'; export { selected as chosen } from './left'; export * from './right';",
            ),
            ("cycle.ts", "export * from './barrel';"),
            ("left.ts", "export const selected = 1; export default 1;"),
            ("right.ts", "export const chosen = 2;"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[
            ("left.ts", "default", IssueKind::Exports),
            ("right.ts", "chosen", IssueKind::Exports)
        ])
    );
}
#[test]
fn type_categories_and_signature_closure() {
    let (_, result) = run(
        &[
            ("index.ts", "import { used } from './lib';"),
            (
                "lib.ts",
                "export interface Signature {} export interface Body {} export interface Unused {} export type Alias = string; export enum E { A } export class C {} export function used(x: Signature): Signature { let y: Body; return x; }",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[
            ("lib.ts", "Body", IssueKind::Types),
            ("lib.ts", "Unused", IssueKind::Types),
            ("lib.ts", "Alias", IssueKind::Types),
            ("lib.ts", "E", IssueKind::Types),
            ("lib.ts", "C", IssueKind::Exports)
        ])
    );
}
#[test]
fn local_use_is_policy_gated_without_function_dce() {
    let files = [
        ("index.ts", "import './lib';"),
        (
            "lib.ts",
            "export interface Inner {} export interface Outer { field: Inner } export const bare = 1; bare; export function a() { b(); } export function b() { a(); } const alias = 1; export { alias as AliasOnly };",
        ),
    ];
    let (_, disabled) = run(&files, Default::default());
    assert_eq!(symbols(&disabled).len(), 6);
    let (_, enabled) = run(
        &files,
        AnalysisOptions {
            ignore_local_uses: ["*".into()].into(),
            ..Default::default()
        },
    );
    assert_eq!(
        symbols(&enabled),
        expected(&[
            ("lib.ts", "Outer", IssueKind::Types),
            ("lib.ts", "AliasOnly", IssueKind::Exports)
        ])
    );
}
#[test]
fn native_custom_element_registration() {
    for registration in [
        "customElements.define('my-card', MyCard);",
        "window.customElements.define('my-card', MyCard);",
        "const registry = new CustomElementRegistry(); registry.define('my-card', MyCard);",
        "const registry = customElements; registry.define('my-card', MyCard);",
    ] {
        let source = format!("export class MyCard {{}} export class Control {{}} {registration}");
        let (_, result) = run(
            &[("index.ts", "import './lib';"), ("lib.ts", &source)],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("lib.ts", "Control", IssueKind::Exports)]),
            "{registration}"
        );
    }
}
#[test]
fn forwarding_slots_are_reported_when_entry_exports_included() {
    let files = [
        ("index.ts", "export { forwarded } from './barrel';"),
        ("barrel.ts", "export { origin as forwarded } from './lib';"),
        ("lib.ts", "export const origin = 1;"),
    ];
    let (_, protected) = run(&files, Default::default());
    assert!(symbols(&protected).is_empty());
    let (_, included) = run(
        &files,
        AnalysisOptions {
            include_entry_exports: true,
            ..Default::default()
        },
    );
    assert_eq!(
        symbols(&included),
        expected(&[
            ("index.ts", "forwarded", IssueKind::Exports),
            ("barrel.ts", "forwarded", IssueKind::Exports),
            ("lib.ts", "origin", IssueKind::Exports)
        ])
    );
}
#[test]
fn source_namespace_identifier_escapes_are_decoded() {
    use biome_project_analysis::facts::NamespaceAccess;
    for (entry, name) in [
        (r"import * as NS from './lib'; NS.\u0075sed;", "used"),
        (
            r"import * as NS from './lib'; type T = NS.\u0055sed;",
            "Used",
        ),
    ] {
        let (snapshot, _) = run(
            &[
                ("index.ts", entry),
                ("lib.ts", "export const used = 1; export interface Used {}"),
            ],
            Default::default(),
        );
        let module = snapshot
            .modules
            .iter()
            .find(|(id, _)| snapshot.sources[id].logical_path == "index.ts")
            .unwrap()
            .1
            .collected
            .as_ref()
            .unwrap();
        assert!(
            module
                .uses
                .iter()
                .any(|usage| usage.namespace_access == Some(NamespaceAccess::Known(name.into()))),
            "{:#?}",
            module.uses
        );
    }
}
#[test]
fn source_tagged_namespace_key_is_unknown() {
    use biome_project_analysis::facts::NamespaceAccess;
    let (snapshot, _) = run(
        &[
            (
                "index.ts",
                "import * as NS from './lib'; const tag = () => 'unused'; NS[tag`used`];",
            ),
            ("lib.ts", "export const used = 1; export const unused = 2;"),
        ],
        Default::default(),
    );
    let module = snapshot
        .modules
        .iter()
        .find(|(id, _)| snapshot.sources[id].logical_path == "index.ts")
        .unwrap()
        .1
        .collected
        .as_ref()
        .unwrap();
    assert!(
        module
            .uses
            .iter()
            .any(|usage| usage.namespace_access == Some(NamespaceAccess::Computed)),
        "{:#?}",
        module.uses
    );
}
#[test]
fn retained_b2_oracle_cases() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/exports-oracle.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 40);
    let mut failures = Vec::new();
    for case in cases.as_array().unwrap() {
        for file in case["files"].as_array().unwrap() {
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(file["source"].as_str().unwrap().as_bytes())
                ),
                file["sha256"].as_str().unwrap(),
                "{}: {}",
                case["id"],
                file["path"]
            );
        }
        let files: Vec<_> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f["path"].as_str().unwrap(), f["source"].as_str().unwrap()))
            .collect();
        let mut options = AnalysisOptions {
            include_entry_exports: case["config"]["includeEntryExports"]
                .as_bool()
                .unwrap_or(false),
            ..Default::default()
        };
        match &case["config"]["ignoreExportsUsedInFile"] {
            serde_json::Value::Bool(true) => {
                options.ignore_local_uses.insert("*".into());
            }
            serde_json::Value::Object(kinds) => options.ignore_local_uses.extend(
                kinds
                    .iter()
                    .filter(|(_, enabled)| enabled.as_bool() == Some(true))
                    .map(|(name, _)| name.clone()),
            ),
            _ => {}
        }
        let (_, result) = run_config(&files, case["config"].clone(), options);
        if let Some(path) = match case["id"].as_str().unwrap() {
            "cjs-shadowed-require" => Some("provider.js"),
            "registry-unreachable" => Some("registry.ts"),
            _ => None,
        } {
            assert!(
                result
                    .issues
                    .iter()
                    .any(|issue| issue.kind == IssueKind::Files
                        && result.sources[&issue.location.file].logical_path == path),
                "{} must remain unreachable",
                case["id"]
            );
        }
        let actual: Vec<_> = symbols(&result)
            .into_iter()
            .map(|(p, n, k)| json!([p, n, k]))
            .collect();
        if json!(actual) != case["expected"] {
            failures.push(format!(
                "{} ({}): actual={}, expected={}",
                case["id"],
                case["disposition"],
                json!(actual),
                case["expected"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn namespace_wrapper_and_named_member_provenance() {
    let (_, result) = run(
        &[
            ("index.ts", "import { NS } from './barrel'; NS.used;"),
            ("barrel.ts", "export * as NS from './lib';"),
            ("lib.ts", "export const used = 1; export const unused = 2;"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "unused", IssueKind::Exports)])
    );
}
#[test]
fn namespace_enumeration_and_runtime_typeof() {
    for (expression, expected_count) in [
        ("Object.keys(NS)", 0),
        ("typeof NS", 4),
        ("consume(NS)", 0),
        ("type T = typeof NS", 0),
    ] {
        let entry = format!("import * as NS from './lib'; {expression};");
        let (_, result) = run(
            &[
                ("index.ts", &entry),
                (
                    "lib.ts",
                    "export const a = 1; export const b = 2; export interface A {} export type B = string;",
                ),
            ],
            Default::default(),
        );
        assert_eq!(symbols(&result).len(), expected_count, "{expression}");
    }
}
#[test]
fn manifest_entry_export_protection_in_all_modes() {
    use biome_project_analysis::{inventory::ExportProtection, options::AnalysisMode};
    let files = [
        (
            "package.json",
            r##"{"private":true,"module":"module.ts","browser":"browser.ts","types":"types.d.ts","typings":"typings.d.ts","imports":{"#local":"./local.ts"}}"##,
        ),
        ("index.ts", "import './ordinary';"),
        ("module.ts", "export const Module = 1;"),
        ("browser.ts", "export const Browser = 1;"),
        ("types.d.ts", "export interface Types {}"),
        ("typings.d.ts", "export type Typings = string;"),
        ("local.ts", "export const Local = 1;"),
        ("ordinary.ts", "export const Ordinary = 1;"),
    ];
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let (snapshot, result) = run(
            &files,
            AnalysisOptions {
                mode,
                ..Default::default()
            },
        );
        assert_eq!(
            symbols(&result),
            expected(&[("ordinary.ts", "Ordinary", IssueKind::Exports)])
        );
        assert_eq!(
            snapshot
                .inventory
                .roots
                .iter()
                .filter(|root| root.producer == "manifest entry"
                    && root.exports == ExportProtection::IncludeWhenRequested)
                .count(),
            5
        );
        let (_, included) = run(
            &files,
            AnalysisOptions {
                mode,
                include_entry_exports: true,
                ..Default::default()
            },
        );
        assert_eq!(symbols(&included).len(), 6);
    }
}
#[test]
fn declaration_roots_and_program_candidates_are_independent() {
    use biome_project_analysis::{inventory::FileRole, options::AnalysisMode};
    let files = [
        (
            "configs/base.json",
            r#"{"include":["../types"],"exclude":["../types/no.d.ts"]}"#,
        ),
        (
            "tsconfig.json",
            r#"{"extends":"./configs/base.json","files":["types/no.d.ts","explicit.d.cts","program.ts"],"references":[{"path":"child"}]}"#,
        ),
        (
            "types/a.d.ts",
            "import type {T} from '../provider'; export type A = T;",
        ),
        ("types/b.d.mts", "export interface B {}"),
        ("types/no.d.ts", "export interface No {}"),
        ("explicit.d.cts", "export interface Explicit {}"),
        ("unselected.d.ts", "export interface Unselected {}"),
        ("child/tsconfig.json", r#"{"files":["child.d.ts"]}"#),
        ("child/child.d.ts", "export interface Child {}"),
        ("program.ts", "export const Program = 1;"),
        (
            "provider.ts",
            "export interface T {} export interface Spare {}",
        ),
    ];
    for mode in [
        AnalysisMode::Default,
        AnalysisMode::Production,
        AnalysisMode::Strict,
    ] {
        let (snapshot, result) = run_config(
            &files,
            json!({"entry":[],"project":["**/*.ts!","**/*.mts!","**/*.cts!"],"useTsconfigFiles":false}),
            AnalysisOptions {
                mode,
                ..Default::default()
            },
        );
        assert_eq!(
            symbols(&result),
            expected(&[("provider.ts", "Spare", IssueKind::Types)])
        );
        assert_eq!(
            snapshot
                .inventory
                .roots
                .iter()
                .filter(|r| r.producer == "tsconfig declaration")
                .count(),
            4
        );
        let (_, included) = run_config(
            &files,
            json!({"entry":[],"project":["**/*.ts!","**/*.mts!","**/*.cts!"],"useTsconfigFiles":false}),
            AnalysisOptions {
                mode,
                include_entry_exports: true,
                ..Default::default()
            },
        );
        assert_eq!(symbols(&included).len(), 5);
        let (snapshot, result) = run_config(
            &files,
            json!({"entry":[],"useTsconfigFiles":true}),
            AnalysisOptions {
                mode,
                ..Default::default()
            },
        );
        assert!(symbols(&result).contains(&(
            "program.ts".into(),
            "Program".into(),
            IssueKind::Exports
        )));
        let provider = *snapshot
            .sources
            .iter()
            .find(|(_, source)| source.logical_path == "provider.ts")
            .unwrap()
            .0;
        assert_eq!(
            snapshot
                .inventory
                .files
                .iter()
                .find(|record| record.file == provider)
                .unwrap()
                .role,
            FileRole::Context
        );
        assert!(
            result
                .membership
                .files
                .iter()
                .any(|member| member.file == provider
                    && member.definite
                    && !member.diagnostic_eligible)
        );
    }
}
#[test]
fn public_roots_ignored_consumers_and_selection() {
    use biome_project_analysis::{options::ReportSelection, select_report};
    let files = [
        ("index.ts", "import './consumer';"),
        (
            "consumer.ts",
            "import {used} from './lib'; function unused() { return used; }",
        ),
        ("lib.ts", "export const used = 1; export const unused = 2;"),
        ("public.ts", "export const Public = 3;"),
    ];
    let (_, result) = run_config(
        &files,
        json!({"entry":["index.ts!"],"publicEntries":["public.ts"],"project":["**/*.ts"],"ignore":["consumer.ts"]}),
        AnalysisOptions {
            include_entry_exports: true,
            ..Default::default()
        },
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "unused", IssueKind::Exports)])
    );
    let selected = select_report(
        &result,
        &ReportSelection {
            paths: vec!["lib.ts".into()],
            ..Default::default()
        },
    );
    assert_eq!(selected.exports, result.exports);
    assert_eq!(selected.evidence, result.evidence);
    assert_eq!(selected.coverage, result.coverage);
}
#[test]
fn known_selection_with_escape_keeps_unselected_siblings() {
    for (expression, count) in [("NS.used; consume(NS)", 1), ("NS.used; Object.keys(NS)", 0)] {
        let entry = format!("import * as NS from './lib'; {expression};");
        let (_, result) = run(
            &[
                ("index.ts", &entry),
                ("lib.ts", "export const used = 1; export const unused = 2;"),
            ],
            Default::default(),
        );
        assert_eq!(symbols(&result).len(), count, "{expression}");
    }
}
#[test]
fn unavailable_origins_are_unbounded_but_ambiguous_origins_are_exhaustive() {
    use biome_project_analysis::{
        coverage::{Coverage, TargetDomain},
        options::ReportSelection,
        select_report,
    };
    let files = [
        (
            "index.ts",
            "import { used } from './barrel'; import './independent';",
        ),
        (
            "barrel.ts",
            "export * from './origin'; export * from './missing';",
        ),
        (
            "origin.ts",
            "export const used = 1; export const unused = 2;",
        ),
        ("independent.ts", "export const independent = 3;"),
    ];
    let (_, result) = run(&files, Default::default());
    assert!(symbols(&result).is_empty());
    assert!(matches!(result.coverage, Coverage::Incomplete(_)));
    assert!(
        result
            .exports
            .gaps
            .iter()
            .any(|gap| gap.domain == TargetDomain::Unbounded)
    );
    let selected = select_report(
        &result,
        &ReportSelection {
            paths: vec!["independent.ts".into()],
            ..Default::default()
        },
    );
    assert_eq!(selected.exports.uncertainty, result.exports.uncertainty);
    assert_eq!(selected.coverage, result.coverage);
    let (_, result) = run(
        &[
            (
                "index.ts",
                "import { used } from './barrel'; import './independent';",
            ),
            (
                "barrel.ts",
                "export * from './left'; export * from './right';",
            ),
            ("left.ts", "export const used = 1; export const spare = 2;"),
            ("right.ts", "export const used = 3;"),
            ("independent.ts", "export const independent = 4;"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[
            ("left.ts", "spare", IssueKind::Exports),
            ("independent.ts", "independent", IssueKind::Exports)
        ])
    );
    assert!(!result.exports.uncertainty.is_empty());
    assert!(result.exports.consumed.is_empty());
    let (_, result) = run(
        &[
            ("index.ts", "import './independent';"),
            ("barrel.ts", "export * from './missing';"),
            ("independent.ts", "export const independent = 1;"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("independent.ts", "independent", IssueKind::Exports)])
    );
}
#[test]
fn type_only_imports_use_declaration_context() {
    let (snapshot, result) = run(
        &[
            ("index.ts", "import type { Shape } from 'library';"),
            (
                "node_modules/library/package.json",
                r#"{"name":"library","exports":{"types":"./types.d.ts","import":"./runtime.js"}}"#,
            ),
            ("node_modules/library/types.d.ts", "export class Shape {}"),
            ("node_modules/library/runtime.js", "export class Shape {}"),
        ],
        Default::default(),
    );
    let consumed: Vec<_> = result
        .exports
        .consumed
        .iter()
        .map(|id| snapshot.sources[&id.location.file].logical_path.as_str())
        .collect();
    assert_eq!(consumed, ["node_modules/library/types.d.ts"]);
    assert!(
        result
            .membership
            .files
            .iter()
            .any(
                |m| snapshot.sources[&m.file].logical_path == "node_modules/library/runtime.js"
                    && m.definite
            )
    );
}
#[test]
fn warm_fresh_barrel_edits_and_order_preserve_export_evidence() {
    use biome_project_analysis::{
        model::SourceContent,
        normalize::{normalize_result, normalize_snapshot},
    };
    let fs = MemoryFileSystem::default();
    for (path, source) in [
        ("package.json", r#"{"private":true,"type":"module"}"#),
        ("index.ts", "import { chosen } from './barrel';"),
        ("barrel.ts", "export { used as chosen } from './lib';"),
        ("lib.ts", "export const used = 1; export const unused = 2;"),
    ] {
        fs.insert(format!("/project/{path}").into(), source);
    }
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(
        MemoryFileSystem::from_files(fs.files.0.clone()),
        "/project",
    );
    let request = ProjectAnalysisRequest::default();
    let first = service.build_project_snapshot(project, &request).unwrap();
    let first_result = analyze(&first, &Default::default());
    assert_eq!(
        symbols(&first_result),
        expected(&[("lib.ts", "unused", IssueKind::Exports)])
    );
    for text in [
        "export { unused as chosen } from './lib';",
        "export * as chosen from './lib';",
    ] {
        fs.insert("/project/barrel.ts".into(), text);
        let warm = service.build_project_snapshot(project, &request).unwrap();
        let (fresh_service, fresh_project) = crate::test_utils::setup_workspace_and_open_project(
            MemoryFileSystem::from_files(fs.files.0.clone()),
            "/project",
        );
        let fresh = fresh_service
            .build_project_snapshot(fresh_project, &request)
            .unwrap();
        assert_ne!(warm.revision, fresh.revision);
        assert_eq!(normalize_snapshot(&warm), normalize_snapshot(&fresh));
        let result = analyze(&warm, &Default::default());
        assert_eq!(
            normalize_result(&result),
            normalize_result(&analyze(&fresh, &Default::default()))
        );
        let mut shuffled = (*warm).clone();
        shuffled.inventory.files.reverse();
        shuffled.inventory.roots.reverse();
        for module in shuffled
            .modules
            .values_mut()
            .filter_map(|m| m.collected.as_mut())
        {
            module.exports.reverse();
            module.declarations.reverse();
            module.uses.reverse();
            module.export_surfaces.reverse();
            for surface in &mut module.export_surfaces {
                surface.slots.reverse();
            }
        }
        assert_eq!(
            normalize_result(&result),
            normalize_result(&analyze(&shuffled, &Default::default()))
        );
    }
    assert!(first_result.sources.values().any(|source|matches!(&source.content,SourceContent::Available {text,..} if text=="export { used as chosen } from './lib';")));
}
#[test]
fn regression_public_protection_uses_the_roots_context() {
    let (_, result) = run_config(
        &[
            (
                "package.json",
                r#"{"private":true,"workspaces":["packages/*"]}"#,
            ),
            ("packages/a/package.json", "{}"),
            ("packages/b/package.json", "{}"),
            ("packages/a/index.ts", "export { selected } from '@alias';"),
            (
                "packages/b/index.ts",
                "import '../../left'; import '../../right';",
            ),
            ("left.ts", "export const selected = 1;"),
            ("right.ts", "export const selected = 2;"),
        ],
        json!({"entry":[],"project":["**/*.ts!"],"workspaces":{".":{"entry":[]},"packages/a":{"entry":["index.ts!"],"paths":{"@alias":["../../left.ts"]}},"packages/b":{"entry":["index.ts!"],"paths":{"@alias":["../../right.ts"]}}}}),
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("right.ts", "selected", IssueKind::Exports)])
    );
}
#[test]
fn regression_anonymous_default_class_has_one_category() {
    let (_, result) = run(
        &[
            ("index.ts", "import './lib';"),
            ("lib.ts", "export default class {}"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "default", IssueKind::Exports)])
    );
}
#[test]
fn namespace_escape_is_not_definite_member_consumption() {
    let (_, result) = run(
        &[
            ("index.ts", "import * as NS from './lib'; consume(NS);"),
            ("lib.ts", "export const used = 1;"),
        ],
        Default::default(),
    );
    assert!(result.exports.consumed.is_empty());
    assert!(result.exports.uncertainty.is_empty());
    assert!(!result.exports.namespace_only.is_empty());
    assert!(
        result
            .exports
            .evidence
            .nodes
            .values()
            .any(|evidence| evidence.producer == "namespace fallback")
    );
}
#[test]
fn nested_namespace_selection_retains_explicit_uncertainty() {
    use biome_project_analysis::coverage::Coverage;
    let (_, result) = run(
        &[
            (
                "index.ts",
                "import * as Outer from './barrel'; Outer.NS.used; import './control';",
            ),
            ("barrel.ts", "export * as NS from './lib';"),
            ("lib.ts", "export const used = 1; export const unused = 2;"),
            ("control.ts", "export const control = 1;"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("control.ts", "control", IssueKind::Exports)])
    );
    assert!(matches!(result.coverage, Coverage::Incomplete(_)));
    let (_, result) = run(
        &[
            (
                "index.ts",
                "import * as NS from './barrel'; NS[key]; import './control';",
            ),
            ("barrel.ts", "export * as Nested from './lib';"),
            ("lib.ts", "export const used = 1;"),
            ("control.ts", "export const control = 1;"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("control.ts", "control", IssueKind::Exports)])
    );
}
#[test]
fn uncertain_registry_retains_only_the_affected_class() {
    use biome_project_analysis::coverage::Coverage;
    for registration in [
        "declare const registry: CustomElementRegistry; registry.define('my-card', MyCard);",
        "const define = customElements.define.bind(customElements); define('my-card', MyCard);",
        "declare const registry: CustomElementRegistry; const define = registry.define.bind(registry); define('my-card', MyCard);",
    ] {
        let source = format!("export class MyCard {{}} export class Control {{}} {registration}");
        let (_, result) = run(
            &[
                ("index.ts", "import './registry';"),
                ("registry.ts", &source),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("registry.ts", "Control", IssueKind::Exports)])
        );
        assert!(result.exports.consumed.is_empty());
        assert!(!result.exports.uncertainty.is_empty());
        assert!(matches!(result.coverage, Coverage::Incomplete(_)));
        let (_, result) = run(
            &[("index.ts", "export {};"), ("registry.ts", &source)],
            Default::default(),
        );
        assert!(result.exports.uncertainty.is_empty());
        assert!(symbols(&result).is_empty());
        assert_eq!(
            result
                .issues
                .iter()
                .filter(|issue| issue.kind == IssueKind::Files)
                .count(),
            1
        );
    }
}
#[test]
fn exported_arrow_signature_references_keep_types() {
    let (_, result) = run(
        &[
            ("index.ts", "import { api } from './lib';"),
            (
                "lib.ts",
                "export interface Input {} export interface Output {} export interface Body {} export const api = (input: Input): Output => { let body: Body; return input; };",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "Body", IssueKind::Types)])
    );
}
#[test]
fn registry_noop_proof_rejects_executable_or_mutable_shapes() {
    for setup in [
        "const registry = {define(value = sideEffect()) {}};",
        "const registry = {define({value}) {}};",
        "const registry = {define() {void external;}};",
        "const registry = {define() {}, define: customElements.define.bind(customElements)};",
        "const registry = {define() {}, ...unknown};",
        "const fake = {define() {}}; const host = {customElements: fake, customElements: window.customElements}; const registry = host.customElements;",
        "const registry = {get define() {return customElements.define.bind(customElements)}};",
        "const registry = {define() {}}; registry.define = customElements.define.bind(customElements);",
        "const registry = {define() {}}; consume(registry);",
        "let registry = customElements; if(flag) { registry = {define() {}}; }",
        "const fake = {define() {}}; let registry = customElements; function change() {registry = fake;}",
    ] {
        let source = format!(
            "export class MyCard {{}} export class Control {{}} {setup} registry.define('my-card', MyCard);"
        );
        let (_, result) = run(
            &[
                ("index.ts", "import './registry';"),
                ("registry.ts", &source),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("registry.ts", "Control", IssueKind::Exports)]),
            "{setup}"
        );
        assert!(!result.exports.uncertainty.is_empty(), "{setup}");
    }
}
#[test]
fn unknown_registered_class_identity_is_not_an_empty_effect() {
    use biome_project_analysis::coverage::TargetDomain;
    let source = "export class MyCard {} export class OtherCard {} export class Control {} customElements.define('my-card', flag ? MyCard : OtherCard);";
    let (_, result) = run(
        &[
            ("index.ts", "import './registry';"),
            ("registry.ts", source),
        ],
        Default::default(),
    );
    assert!(symbols(&result).is_empty());
    assert!(
        result
            .exports
            .gaps
            .iter()
            .any(|gap| gap.domain == TargetDomain::Unbounded)
    );
    let (_, result) = run(
        &[("index.ts", "export {};"), ("registry.ts", source)],
        Default::default(),
    );
    assert!(result.exports.gaps.is_empty());
}
#[test]
fn default_arrow_and_class_method_signatures_keep_types() {
    let mut failures = Vec::new();
    for (entry, api) in [
        (
            "import api from './lib';",
            "export default (input: Input): Output => { let local: Body; return input; };",
        ),
        (
            "import { Api } from './lib';",
            "export class Api { method(input: Input): Output { let local: Body; return input; } }",
        ),
    ] {
        let source = format!(
            "export interface Input {{}} export interface Output {{}} export interface Body {{}} {api}"
        );
        let (_, result) = run(
            &[("index.ts", entry), ("lib.ts", &source)],
            Default::default(),
        );
        if symbols(&result) != expected(&[("lib.ts", "Body", IssueKind::Types)]) {
            failures.push(format!("{api}: {:?}", symbols(&result)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn import_only_bindings_and_uncalled_require_are_consumers() {
    let (_, result) = run(
        &[
            ("index.ts", "import picked, {used as alias} from './lib';"),
            (
                "lib.ts",
                "export default function picked() {} export const used = 1; export const unused = 2;",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "unused", IssueKind::Exports)])
    );
    let (_, result) = run(
        &[
            (
                "index.ts",
                "function unused() { const {used} = require('./lib'); }",
            ),
            ("lib.js", "exports.used = 1; exports.unused = 2;"),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.js", "unused", IssueKind::Exports)])
    );
}
#[test]
fn manifest_protection_evidence_keeps_its_workspace() {
    let (_, result) = run_config(
        &[
            (
                "package.json",
                r#"{"private":true,"workspaces":["packages/*"]}"#,
            ),
            (
                "packages/a/package.json",
                r#"{"name":"a","module":"./api.ts"}"#,
            ),
            ("packages/a/api.ts", "export const Surface = 1;"),
        ],
        json!({"entry":[],"workspaces":{"packages/*":{"entry":[]}}}),
        Default::default(),
    );
    let evidence = result
        .exports
        .evidence
        .nodes
        .values()
        .find(|evidence| evidence.producer == "public export: manifest entry")
        .unwrap();
    assert_eq!(result.workspaces[&evidence.workspace].path, "packages/a");
}
#[test]
fn member_records_preserve_actual_occurrences() {
    let mut failures = Vec::new();
    let (_, result) = run(
        &[
            ("index.ts", "import * as NS from './lib'; NS.used;"),
            ("lib.ts", "export const used = 1;"),
        ],
        Default::default(),
    );
    if result.exports.member_uses.len() != 1 {
        failures.push(format!(
            "one namespace use: {:?}",
            result.exports.member_uses
        ));
    }
    let (_, result) = run(
        &[
            ("index.ts", "import {E} from './lib'; export {E};"),
            ("lib.ts", "export enum E { A, B }"),
        ],
        Default::default(),
    );
    if !result.exports.member_uses.is_empty() {
        failures.push(format!(
            "export syntax is not a member use: {:?}",
            result.exports.member_uses
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn commonjs_object_members_keep_authoritative_origins() {
    use biome_project_analysis::facts::{ExportResolution, ExportTarget};
    let (snapshot, result) = run(
        &[
            ("index.ts", "import './lib';"),
            (
                "lib.js",
                r"const used = 1; module.exports = { used, '\u0075nused': 2 };",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.js", "default", IssueKind::Exports)])
    );
    let module = snapshot
        .modules
        .iter()
        .find(|(file, _)| snapshot.sources[file].logical_path == "lib.js")
        .unwrap()
        .1
        .collected
        .as_ref()
        .unwrap();
    for name in ["used", "unused"] {
        assert!(
            module
                .exports
                .iter()
                .any(|slot| slot.name == name && matches!(slot.target, ExportTarget::Local(_))),
            "known object member {name} must retain its local identity"
        );
        assert!(
            module.export_surfaces.iter().any(|surface| surface
                .slots
                .iter()
                .any(|slot| slot.name == name
                    && matches!(slot.resolution, ExportResolution::Found(_)))),
            "known object member {name} must remain graph-resolvable"
        );
    }
}

#[test]
fn commonjs_default_name_collision_retains_explicit_uncertainty() {
    use biome_project_analysis::{
        coverage::{Coverage, TargetDomain},
        facts::ExportResolution,
    };
    for entry in [
        "import './provider'; import './control';",
        "const {used} = require('./provider'); import './control';",
    ] {
        let (snapshot, result) = run(
            &[
                ("index.ts", entry),
                ("provider.js", "module.exports = { default: 1, used: 2 };"),
                ("control.ts", "export const unrelated = 1;"),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("control.ts", "unrelated", IssueKind::Exports)])
        );
        let (&file, module) = snapshot
            .modules
            .iter()
            .find(|(file, _)| snapshot.sources[file].logical_path == "provider.js")
            .unwrap();
        let module = module.collected.as_ref().unwrap();
        assert_eq!(
            module
                .exports
                .iter()
                .filter(|slot| slot.name == "default")
                .count(),
            2
        );
        assert!(module.export_surfaces.iter().any(|surface| surface.slots.iter().any(|slot| slot.name == "default" && matches!(&slot.resolution, ExportResolution::Ambiguous(ids) if ids.len() == 2))));
        assert!(
            matches!(result.coverage, Coverage::Incomplete(_)),
            "{entry}: representation collision must be visible"
        );
        assert!(
            result
                .exports
                .uncertainty
                .iter()
                .any(|uncertainty| uncertainty.declarations.len() == 2
                    && uncertainty.reason.contains("snapshot representation"))
        );
        assert!(result.exports.gaps.iter().any(|gap| matches!(&gap.domain, TargetDomain::Bounded {files,..} if files.iter().all(|candidate| *candidate == file))));
        assert!(
            result
                .exports
                .evidence
                .nodes
                .values()
                .any(|node| node.explanation.contains("snapshot representation"))
        );
    }
}

#[test]
fn public_namespace_protection_preserves_each_resolution_context() {
    let (snapshot, result) = run(
        &[
            ("index.ts", "export * as Wrapper from './barrel';"),
            ("barrel.ts", "export * as Inner from 'pkg';"),
            (
                "node_modules/pkg/package.json",
                r#"{"name":"pkg","exports":{"types":"./types.d.ts","import":"./runtime.js"}}"#,
            ),
            ("node_modules/pkg/runtime.js", "export const runtime = 1;"),
            ("node_modules/pkg/types.d.ts", "export interface Shape {}"),
        ],
        Default::default(),
    );
    for (path, name) in [
        ("node_modules/pkg/runtime.js", "runtime"),
        ("node_modules/pkg/types.d.ts", "Shape"),
    ] {
        let module = snapshot
            .modules
            .iter()
            .find(|(file, _)| snapshot.sources[file].logical_path == path)
            .unwrap()
            .1
            .collected
            .as_ref()
            .unwrap();
        let declaration = module
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .unwrap();
        assert!(
            result
                .exports
                .consumed
                .iter()
                .any(|id| id.location == declaration.id.location),
            "public namespace must protect {path}:{name} in its resolution context"
        );
    }
    let (_, result) = run(
        &[
            ("index.ts", "export * as A from './a';"),
            ("a.ts", "export * as B from './b'; export const a = 1;"),
            ("b.ts", "export * as A from './a'; export const b = 2;"),
        ],
        Default::default(),
    );
    assert!(symbols(&result).is_empty());
    for name in ["a", "b"] {
        assert!(
            result
                .exports
                .protected_slots
                .iter()
                .any(|slot| slot.name == name)
        );
    }
}

#[test]
fn parentheses_preserve_exported_signature_ownership() {
    for (entry, library) in [
        (
            "import fn from './lib';",
            "export interface Signature {} export default (((value: Signature) => value));",
        ),
        (
            "import {fn} from './lib';",
            "export interface Signature {} export const fn = (((value: Signature) => value));",
        ),
    ] {
        let (_, result) = run(
            &[("index.ts", entry), ("lib.ts", library)],
            Default::default(),
        );
        assert!(
            symbols(&result).is_empty(),
            "parentheses must preserve signature owner: {:?}",
            symbols(&result)
        );
    }
}

#[test]
fn expression_arrow_return_dependencies_follow_reference_policy() {
    use biome_project_analysis::facts::UseRole;
    let (snapshot, result) = run(
        &[
            ("index.ts", "import {fn} from './lib';"),
            (
                "lib.ts",
                "export interface Signature {} export interface Body {} export const fn = (value: Signature) => null as Body;",
            ),
        ],
        Default::default(),
    );
    let module = snapshot
        .modules
        .iter()
        .find(|(file, _)| snapshot.sources[file].logical_path == "lib.ts")
        .unwrap()
        .1
        .collected
        .as_ref()
        .unwrap();
    let body = module
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Body")
        .unwrap();
    assert!(
        module
            .uses
            .iter()
            .any(|usage| usage.declaration == body.id && usage.role == UseRole::Signature),
        "the asserted expression result participates in exported-surface traversal"
    );
    assert!(symbols(&result).is_empty());
}

#[test]
fn dynamic_import_results_retain_target_uncertainty() {
    use biome_project_analysis::coverage::Coverage;
    for expression in [
        "const { used } = await import('./lib'); console.log(used);",
        "const NS = await import('./lib'); NS.used;",
        "import('./lib').then(NS => NS.used);",
        "consume(await import('./lib'));",
        "(await import('./lib')).used;",
        "function load() { return import('./lib'); }",
        "const load = () => import('./lib');",
        "const pending = import('./lib');",
    ] {
        let entry = format!("import './control'; {expression}");
        let (_, result) = run(
            &[
                ("index.ts", &entry),
                ("lib.ts", "export const used = 1; export const unused = 2;"),
                ("control.ts", "export const control = 3;"),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("control.ts", "control", IssueKind::Exports)]),
            "{expression}"
        );
        assert!(result.exports.consumed.is_empty(), "{expression}");
        assert!(!result.exports.uncertainty.is_empty(), "{expression}");
        assert!(
            matches!(result.coverage, Coverage::Incomplete(_)),
            "{expression}"
        );
        let selected = biome_project_analysis::select_report(
            &result,
            &biome_project_analysis::options::ReportSelection {
                paths: vec!["control.ts".into()],
                ..Default::default()
            },
        );
        assert_eq!(selected.exports.uncertainty, result.exports.uncertainty);
        assert_eq!(selected.coverage, result.coverage);
        assert_eq!(selected.evidence, result.evidence);
    }
    let (_, dead) = run(
        &[
            ("index.ts", "export {};"),
            ("loader.ts", "const { used } = await import('./lib'); used;"),
            ("lib.ts", "export const used = 1;"),
        ],
        Default::default(),
    );
    assert!(dead.exports.uncertainty.is_empty());
    assert!(symbols(&dead).is_empty());
    for expression in [
        "import('./lib');",
        "await import('./lib');",
        "(await (import('./lib')));",
    ] {
        let (_, result) = run(
            &[
                ("index.ts", expression),
                ("lib.ts", "export const unused = 1;"),
            ],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("lib.ts", "unused", IssueKind::Exports)]),
            "{expression}"
        );
        assert!(result.exports.uncertainty.is_empty(), "{expression}");
    }
}

#[test]
fn registered_class_alias_requires_proven_terminal_identity() {
    use biome_project_analysis::coverage::TargetDomain;
    for alias in [
        "const Candidate = flag ? MyCard : OtherCard;",
        "let Candidate = MyCard; Candidate = OtherCard;",
        "let Candidate = MyCard; if (flag) Candidate = OtherCard;",
    ] {
        let source = format!(
            "export class MyCard {{}} export class OtherCard {{}} {alias} customElements.define('my-card', Candidate);"
        );
        let (_, result) = run(
            &[("index.ts", "import './lib';"), ("lib.ts", &source)],
            Default::default(),
        );
        assert!(
            symbols(&result).is_empty(),
            "{alias}: {:?}",
            symbols(&result)
        );
        assert!(result.exports.consumed.is_empty(), "{alias}");
        assert!(
            result
                .exports
                .gaps
                .iter()
                .any(|gap| gap.domain == TargetDomain::Unbounded),
            "{alias}"
        );
        let (_, dead) = run(
            &[("index.ts", "export {};"), ("lib.ts", &source)],
            Default::default(),
        );
        assert!(dead.exports.gaps.is_empty());
    }
    let (_, result) = run(
        &[
            ("index.ts", "import './lib';"),
            (
                "lib.ts",
                "export class MyCard {} export class Control {} const First = MyCard; const Candidate = First; customElements.define('my-card', Candidate);",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "Control", IssueKind::Exports)])
    );
    assert!(result.exports.uncertainty.is_empty());
}

#[test]
fn bound_registry_constructor_accounts_for_prebound_arguments() {
    use biome_project_analysis::coverage::TargetDomain;
    for registration in [
        "const register = customElements.define.bind(customElements, 'my-card', MyCard); register({}, Control);",
        "const register = customElements.define.bind(customElements, 'my-card'); register(MyCard, Control);",
        "const register = customElements.define.bind(customElements); register('my-card', MyCard);",
    ] {
        let source = format!("export class MyCard {{}} export class Control {{}} {registration}");
        let (_, result) = run(
            &[("index.ts", "import './lib';"), ("lib.ts", &source)],
            Default::default(),
        );
        assert_eq!(
            symbols(&result),
            expected(&[("lib.ts", "Control", IssueKind::Exports)]),
            "{registration}"
        );
        assert!(result.exports.consumed.is_empty());
        assert!(!result.exports.uncertainty.is_empty());
        assert!(
            result
                .exports
                .gaps
                .iter()
                .all(|gap| gap.domain != TargetDomain::Unbounded)
        );
        let (_, dead) = run(
            &[("index.ts", "export {};"), ("lib.ts", &source)],
            Default::default(),
        );
        assert!(dead.exports.gaps.is_empty());
    }
    for registration in [
        "const register = customElements.define.bind(customElements, ...args); register({}, Control);",
        "const register = customElements.define.bind(customElements); register(...args, Control);",
        "let register = customElements.define.bind(customElements, 'my-card', Control); register = customElements.define.bind(customElements, 'my-card', MyCard); register({}, Control);",
    ] {
        let source = format!("export class MyCard {{}} export class Control {{}} {registration}");
        let (_, result) = run(
            &[("index.ts", "import './lib';"), ("lib.ts", &source)],
            Default::default(),
        );
        assert!(
            symbols(&result).is_empty(),
            "{registration}: {:?}",
            symbols(&result)
        );
        assert!(result.exports.consumed.is_empty());
        assert!(
            result
                .exports
                .gaps
                .iter()
                .any(|gap| gap.domain == TargetDomain::Unbounded)
        );
    }
}

#[test]
fn registered_class_destructuring_is_not_an_identity_alias() {
    use biome_project_analysis::coverage::TargetDomain;
    let (_, result) = run(
        &[
            ("index.ts", "import './lib';"),
            (
                "lib.ts",
                "export class MyCard {} export class Control {} class Holder { static Candidate = MyCard; } const {Candidate} = Holder; customElements.define('my-card', Candidate);",
            ),
        ],
        Default::default(),
    );
    assert!(symbols(&result).is_empty(), "{:?}", symbols(&result));
    assert!(result.exports.consumed.is_empty());
    assert!(
        result
            .exports
            .gaps
            .iter()
            .any(|gap| gap.domain == TargetDomain::Unbounded)
    );
}

#[test]
fn imported_constructor_variable_does_not_prove_registered_class_identity() {
    use biome_project_analysis::coverage::TargetDomain;
    for provider in [
        "export class MyCard {} export class OtherCard {} export const Candidate = flag ? MyCard : OtherCard;",
        "export class Candidate {} export class Control {}",
    ] {
        let files = [
            ("index.ts", "import './registry';"),
            (
                "registry.ts",
                "import {Candidate} from './lib'; customElements.define('my-card', Candidate);",
            ),
            ("lib.ts", provider),
        ];
        let (_, result) = run(&files, Default::default());
        assert!(
            result
                .exports
                .gaps
                .iter()
                .any(|gap| gap.domain == TargetDomain::Unbounded),
            "{:?}",
            symbols(&result)
        );
        assert!(symbols(&result).is_empty());
        assert!(!result.exports.consumed.is_empty());
        let mut dead_files = files;
        dead_files[0] = ("index.ts", "export {};");
        let (_, dead) = run(&dead_files, Default::default());
        assert!(dead.exports.gaps.is_empty());
        assert!(dead.exports.consumed.is_empty());
    }
}
