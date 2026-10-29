use super::ProjectAnalysisRequest;
use biome_fs::MemoryFileSystem;
use biome_project_analysis::{
    analyze,
    model::AnalysisSnapshot,
    options::AnalysisOptions,
    result::{IssueKind, ProjectAnalysisResult},
};
use serde_json::json;
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
        .filter(|i| {
            matches!(
                i.kind,
                IssueKind::NsExports
                    | IssueKind::NsTypes
                    | IssueKind::EnumMembers
                    | IssueKind::NamespaceMembers
                    | IssueKind::Duplicates
            )
        })
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
fn namespace_controls_are_independent() {
    for expression in [
        "consume(NS)",
        "const copy = {...NS}",
        "const alias = NS",
        "let alias; alias = NS",
        "type Shape = typeof NS",
    ] {
        for value in [false, true] {
            for types in [false, true] {
                let mut options = AnalysisOptions::default();
                if value {
                    options.enabled_issues.insert(IssueKind::NsExports);
                }
                if types {
                    options.enabled_issues.insert(IssueKind::NsTypes);
                }
                let entry = format!(
                    "import * as NS from './lib'; function consume(v: unknown) {{}} {expression};"
                );
                let (_, result) = run(
                    &[
                        ("index.ts", &entry),
                        ("lib.ts", "export const value=1; export interface Shape {}"),
                    ],
                    options,
                );
                let mut wanted = vec![];
                if value {
                    wanted.push(("lib.ts", "value", IssueKind::NsExports));
                }
                if types {
                    wanted.push(("lib.ts", "Shape", IssueKind::NsTypes));
                }
                assert_eq!(
                    symbols(&result),
                    expected(&wanted),
                    "{expression} {value} {types}"
                );
            }
        }
    }
}
#[test]
fn source_retains_member_identity_and_paths() {
    let (snapshot, _) = run(
        &[
            ("index.ts", "import { N } from './lib'; N.Nested.used;"),
            (
                "lib.ts",
                "export namespace N { export namespace Nested { export const used=1; export const unused=2; } }",
            ),
        ],
        Default::default(),
    );
    let module = snapshot
        .modules
        .iter()
        .find(|(id, _)| snapshot.sources[id].logical_path == "lib.ts")
        .unwrap()
        .1
        .collected
        .as_ref()
        .unwrap();
    let facts = serde_json::to_value(module).unwrap();
    assert!(
        facts["members"].as_array().is_some_and(|members| members
            .iter()
            .any(|m| m["path"] == json!(["Nested", "unused"]))),
        "Missing nested member inventory: {facts}"
    );
}
#[test]
fn typescript_member_identity() {
    let (_, result) = run(
        &[
            (
                "index.ts",
                "import { N, E } from './lib'; N.Nested.used; E.used;",
            ),
            (
                "lib.ts",
                "export namespace N { export namespace Nested { export const used=1; export const unused=2; } } export enum E { used, unused }",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[
            ("lib.ts", "N.Nested.unused", IssueKind::NamespaceMembers),
            ("lib.ts", "E.unused", IssueKind::EnumMembers)
        ])
    );
}
#[test]
fn duplicate_export_form_exceptions() {
    for (declaration, count) in [
        ("export function handler() {}", 1),
        ("function handler() {}; export { handler };", 0),
    ] {
        let source = format!("{declaration} export default handler;");
        let (_, result) = run(
            &[
                (
                    "index.ts",
                    "import def, { handler } from './lib'; def(); handler();",
                ),
                ("lib.ts", &source),
            ],
            Default::default(),
        );
        assert_eq!(symbols(&result).len(), count, "{source}");
    }
}
#[test]
fn source_retains_duplicate_alias_and_export_tags() {
    let (snapshot, _) = run(
        &[
            ("index.ts", "import './lib';"),
            (
                "lib.ts",
                "export const original = true; export const alias = original; /** @alias */ export default original;",
            ),
        ],
        Default::default(),
    );
    let module = snapshot
        .modules
        .iter()
        .find(|(id, _)| snapshot.sources[id].logical_path == "lib.ts")
        .unwrap()
        .1
        .collected
        .as_ref()
        .unwrap();
    let facts = serde_json::to_value(module).unwrap();
    assert!(
        facts["aliases"]
            .as_array()
            .is_some_and(|aliases| !aliases.is_empty()),
        "Missing alias identity: {facts}"
    );
    assert!(
        facts["exports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|export| export["name"] == "default" && export["tags"] == json!(["alias"]))
    );
}
#[test]
fn merged_member_origins_are_not_ambiguous() {
    let (snapshot, result) = run(
        &[
            ("index.ts", "import { N } from './lib'; N.used;"),
            (
                "lib.ts",
                "export namespace N { export const used=1; } export namespace N { export const unused=2; }",
            ),
        ],
        Default::default(),
    );
    let module = snapshot
        .modules
        .iter()
        .find(|(id, _)| snapshot.sources[id].logical_path == "lib.ts")
        .unwrap()
        .1
        .collected
        .as_ref()
        .unwrap();
    assert!(
        module
            .export_surfaces
            .iter()
            .flat_map(|surface| &surface.slots)
            .all(|slot| !matches!(
                slot.resolution,
                biome_project_analysis::facts::ExportResolution::Ambiguous(_)
            )),
        "{module:?}"
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "N.unused", IssueKind::NamespaceMembers)])
    );
}
#[test]
fn retained_b3_oracle_cases() {
    use sha2::{Digest, Sha256};
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/members-oracle.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 54);
    let mut failures = Vec::new();
    for case in cases.as_array().unwrap() {
        for file in case["files"].as_array().unwrap() {
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(file["source"].as_str().unwrap().as_bytes())
                ),
                file["sha256"].as_str().unwrap()
            );
        }
        let files: Vec<_> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| {
                (
                    file["path"].as_str().unwrap(),
                    file["source"].as_str().unwrap(),
                )
            })
            .collect();
        let config = &case["native_config"];
        let mut options = AnalysisOptions {
            include_entry_exports: config["includeEntryExports"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        if config["ignoreExportsUsedInFile"] == true {
            options.ignore_local_uses.insert("*".into());
        }
        if let Some(kinds) = config["include"].as_array() {
            for kind in kinds {
                if let Ok(kind) = serde_json::from_value(kind.clone()) {
                    options.enabled_issues.insert(kind);
                }
            }
        }
        if let Some(tags) = config["tags"].as_array() {
            for tag in tags.iter().filter_map(|tag| tag.as_str()) {
                if let Some(tag) = tag.strip_prefix('-') {
                    options.excluded_tags.insert(tag.into());
                }
            }
        }
        let (_, result) = run_config(
            &files,
            json!({"entry":config["entry"], "project":config["project"]}),
            options,
        );
        let mut actual: Vec<_> = result
            .issues
            .iter()
            .map(|issue| {
                json!([
                    result.sources[&issue.location.file].logical_path,
                    issue.symbol,
                    issue.kind
                ])
            })
            .collect();
        actual.sort_by_key(serde_json::Value::to_string);
        for observed in case["oracle_items"].as_array().unwrap() {
            let kind = observed["kind"].as_str().unwrap();
            if !matches!(
                kind,
                "enumMembers" | "namespaceMembers" | "nsExports" | "nsTypes"
            ) {
                continue;
            }
            let item = &observed["item"];
            let symbol = if matches!(kind, "enumMembers" | "namespaceMembers") {
                format!(
                    "{}.{}",
                    item["namespace"].as_str().unwrap(),
                    item["name"].as_str().unwrap()
                )
            } else {
                item["name"].as_str().unwrap().to_string()
            };
            if let Some(issue) = result.issues.iter().find(|issue| {
                issue.symbol.as_ref() == Some(&symbol)
                    && result.sources[&issue.location.file].logical_path
                        == observed["path"].as_str().unwrap()
            }) {
                assert_eq!(
                    u32::from(issue.location.range.start()) as u64,
                    item["pos"].as_u64().unwrap(),
                    "{}: {symbol}",
                    case["id"]
                );
            }
        }
        let mut wanted = case["expected"].as_array().unwrap().clone();
        wanted.sort_by_key(serde_json::Value::to_string);
        if actual != wanted {
            failures.push(format!(
                "{}: actual={}, expected={}",
                case["id"],
                json!(actual),
                json!(wanted)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn member_membership_uncertainty_and_report_selection() {
    use biome_project_analysis::{coverage::Coverage, options::ReportSelection, select_report};
    let files = [
        ("index.ts", "import './consumer';"),
        (
            "consumer.ts",
            "import { Known, Unknown } from './lib'; Known.used; Unknown[key];",
        ),
        (
            "lib.ts",
            "export namespace Known { export const used=1; export const unused=2; } export namespace Unknown { export const used=1; export const unused=2; }",
        ),
    ];
    let (_, result) = run_config(
        &files,
        json!({"entry":["index.ts"], "project":["**/*.ts"], "ignore":["consumer.ts"]}),
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "Known.unused", IssueKind::NamespaceMembers)])
    );
    assert!(matches!(result.coverage, Coverage::Incomplete(_)));
    assert!(!result.members.uncertainty.is_empty());
    assert!(result.members.used.iter().all(|id| {
        !result
            .members
            .uncertainty
            .iter()
            .any(|unknown| unknown.parent == *id)
    }));
    let selected = select_report(
        &result,
        &ReportSelection {
            paths: vec!["lib.ts".into()],
            ..Default::default()
        },
    );
    assert_eq!(selected.members, result.members);
    assert_eq!(selected.coverage, result.coverage);
    assert_eq!(selected.evidence, result.evidence);
    let (_, context) = run_config(
        &files,
        json!({"entry":["index.ts"], "project":["index.ts", "consumer.ts"]}),
        Default::default(),
    );
    assert!(symbols(&context).is_empty());
    assert!(!context.members.uncertainty.is_empty());
    let mut possible_files = files;
    possible_files[0] = ("index.ts", "import(flag ? './consumer' : './other');");
    let (_, possible) = run(&possible_files, Default::default());
    assert!(symbols(&possible).is_empty());
}
#[test]
fn genuine_uncertainty_is_independent_of_namespace_fallback() {
    let mut options = AnalysisOptions::default();
    options
        .enabled_issues
        .extend([IssueKind::NsExports, IssueKind::NsTypes]);
    let sources = [
        (
            "index.ts",
            "import * as NS from './determinate'; consume(NS); import * as Unknown from './dynamic'; consume(Unknown); const loaded = import('./dynamic');",
        ),
        (
            "determinate.ts",
            "export const value=1; export interface Shape {}",
        ),
        (
            "dynamic.ts",
            "export namespace N { export const unused=1; } export const other=2;",
        ),
    ];
    let (snapshot, result) = run(&sources, options.clone());
    assert_eq!(
        symbols(&result),
        expected(&[
            ("determinate.ts", "value", IssueKind::NsExports),
            ("determinate.ts", "Shape", IssueKind::NsTypes)
        ])
    );
    assert!(!result.exports.uncertainty.is_empty());
    let dynamic = snapshot
        .modules
        .values()
        .filter_map(|module| module.collected.as_ref())
        .flat_map(|module| &module.imports)
        .find(|import| import.dynamic_result_used)
        .unwrap();
    assert!(
        !result
            .exports
            .member_uses
            .iter()
            .any(|usage| usage.location == dynamic.location)
    );
    let mut sources = sources;
    sources[0] = (
        "index.ts",
        "import * as NS from './determinate'; consume(NS); import { Candidate } from './dynamic'; customElements.define('x-tag', Candidate);",
    );
    sources[2] = (
        "dynamic.ts",
        "export class Candidate {} export namespace N { export const unused=1; }",
    );
    let (_, uncertain) = run(&sources, options);
    assert!(symbols(&uncertain).is_empty());
    assert!(
        uncertain
            .exports
            .gaps
            .iter()
            .any(|gap| gap.domain == biome_project_analysis::coverage::TargetDomain::Unbounded)
    );
}
#[test]
fn warm_fresh_merge_edits_preserve_member_evidence() {
    use biome_project_analysis::normalize::{normalize_result, normalize_snapshot};
    let fs = MemoryFileSystem::default();
    for (path, source) in [
        ("package.json", r#"{"private":true,"type":"module"}"#),
        ("index.ts", "import { N } from './lib'; N.used;"),
        (
            "lib.ts",
            "export namespace N { export const used=1; export const unused=2; }",
        ),
    ] {
        fs.insert(format!("/project/{path}").into(), source);
    }
    let (service, project) = crate::test_utils::setup_workspace_and_open_project(
        MemoryFileSystem::from_files(fs.files.0.clone()),
        "/project",
    );
    let request = ProjectAnalysisRequest::default();
    let original = service.build_project_snapshot(project, &request).unwrap();
    assert_eq!(symbols(&analyze(&original, &Default::default())).len(), 1);
    for text in [
        "export namespace N { export const used=1; } export namespace N { export const unused=2; }",
        "export enum N { used, unused } export namespace N { export const extra=3; }",
    ] {
        fs.insert("/project/lib.ts".into(), text);
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
        for module in shuffled
            .modules
            .values_mut()
            .filter_map(|module| module.collected.as_mut())
        {
            module.declarations.reverse();
            module.members.reverse();
            module.merges.reverse();
            module.exports.reverse();
            module.uses.reverse();
        }
        assert_eq!(
            normalize_result(&result),
            normalize_result(&analyze(&shuffled, &Default::default())),
            "presentation facts: {:?}",
            shuffled
                .modules
                .values()
                .filter_map(|module| module.collected.as_ref())
                .map(|module| (&module.declarations, &module.members))
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn forwarding_and_default_slots_keep_distinct_report_policy() {
    let (_, result) = run(
        &[
            (
                "index.ts",
                "import { handler } from './lib'; handler(); import './mid';",
            ),
            (
                "lib.ts",
                "function handler() {} export { handler }; export default handler;",
            ),
            (
                "mid.ts",
                "export * as NS from './origin'; export { Mode } from './origin';",
            ),
            ("origin.ts", "export enum Mode { used }"),
        ],
        Default::default(),
    );
    let actual: Vec<_> = result
        .issues
        .iter()
        .map(|issue| {
            (
                result.sources[&issue.location.file].logical_path.as_str(),
                issue.symbol.as_deref().unwrap(),
                issue.kind,
            )
        })
        .collect();
    assert!(actual.contains(&("lib.ts", "default", IssueKind::Exports)));
    assert!(actual.contains(&("mid.ts", "Mode", IssueKind::Exports)));
    assert!(
        !actual
            .iter()
            .any(|(path, symbol, _)| *path == "mid.ts" && *symbol == "NS")
    );
    assert!(
        result
            .issues
            .iter()
            .all(|issue| issue.kind != IssueKind::Duplicates)
    );
}
#[test]
fn source_member_ignore_patterns_preserve_analysis_evidence() {
    let files = [
        ("index.ts", "import { N } from './lib'; N.used;"),
        (
            "lib.ts",
            "export namespace N { export const used=1; export const unused=2; }",
        ),
    ];
    let (_, normal) = run(&files, Default::default());
    assert_eq!(symbols(&normal).len(), 1);
    let (_, ignored) = run(
        &files,
        AnalysisOptions {
            ignore_members: vec!["unused".into()],
            ..Default::default()
        },
    );
    assert!(symbols(&ignored).is_empty());
    assert_eq!(ignored.exports.consumed, normal.exports.consumed);
}
#[test]
fn local_enumeration_protects_only_its_member_parent() {
    let (_, result) = run(
        &[
            (
                "index.ts",
                "import { E, Control } from './lib'; E.used; Control.used;",
            ),
            (
                "lib.ts",
                "export enum E { used, unused } Object.keys(E); export enum Control { used, unused }",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "Control.unused", IssueKind::EnumMembers)])
    );
}
#[test]
fn private_nested_namespace_is_not_an_exported_member_path() {
    let (_, result) = run(
        &[
            ("index.ts", "import { N } from './lib'; N.used;"),
            (
                "lib.ts",
                "export namespace N { export const used=1; export const unused=2; namespace Hidden { export const unused=3; } }",
            ),
        ],
        Default::default(),
    );
    assert_eq!(
        symbols(&result),
        expected(&[("lib.ts", "N.unused", IssueKind::NamespaceMembers)])
    );
}
#[test]
fn duplicate_forwarding_ambiguity_does_not_merge_declarations() {
    let (_, result) = run(
        &[
            (
                "index.ts",
                "import { shared } from './barrel'; import { original, alias } from './control'; shared; original; alias;",
            ),
            (
                "barrel.ts",
                "export * from './left'; export * from './right';",
            ),
            ("left.ts", "export const shared=1;"),
            ("right.ts", "export const shared=2;"),
            (
                "control.ts",
                "export const original=1; export const alias=original;",
            ),
        ],
        Default::default(),
    );
    assert!(!result.exports.uncertainty.is_empty());
    assert_eq!(
        symbols(&result),
        expected(&[("control.ts", "alias/original", IssueKind::Duplicates)])
    );
}

#[test]
fn imported_alias_member_accesses_keep_resolved_parent_identity() {
    use biome_project_analysis::coverage::Coverage;
    for (import, root, initializer, alias_root) in [
        ("import { N, Control } from './lib';", "N", "N", "alias"),
        ("import { N, Control } from './barrel';", "N", "N", "alias"),
        (
            "import * as NS from './lib'; import { Control } from './lib';",
            "NS.N",
            "NS",
            "alias.N",
        ),
    ] {
        for (access, uncertain) in [("[key]", true), (".unused", false)] {
            let entry = format!(
                "{import} {root}.used; Control.used; const first = {initializer}; const alias = first; {alias_root}{access};"
            );
            let (_, result) = run(
                &[
                    ("index.ts", &entry),
                    ("barrel.ts", "export { N, Control } from './lib';"),
                    (
                        "lib.ts",
                        "export namespace N { export const used=1; export const unused=2; } export namespace Control { export const used=1; export const unused=2; }",
                    ),
                ],
                Default::default(),
            );
            assert_eq!(
                symbols(&result),
                expected(&[("lib.ts", "Control.unused", IssueKind::NamespaceMembers)]),
                "{entry}"
            );
            assert_eq!(
                result.members.uncertainty.len(),
                usize::from(uncertain),
                "{entry}"
            );
            assert_eq!(
                matches!(result.coverage, Coverage::Incomplete(_)),
                uncertain,
                "{entry}"
            );
            if uncertain {
                let unknown = &result.members.uncertainty[0];
                assert_eq!(
                    result.sources[&unknown.parent.location.file].logical_path,
                    "lib.ts"
                );
                assert_eq!(
                    result.sources[&unknown.location.file].logical_path,
                    "index.ts"
                );
                assert!(unknown.path.is_empty());
            }
        }
    }
}

fn assert_member_tags_preserve_uncertainty(source: &str) {
    use biome_project_analysis::coverage::Coverage;
    let files = [
        ("index.ts", "import { N } from './lib'; N[key];"),
        ("lib.ts", source),
    ];
    let (_, normal) = run(&files, Default::default());
    assert!(matches!(normal.coverage, Coverage::Incomplete(_)));
    assert_eq!(normal.members.uncertainty.len(), 1);
    for included in [false, true] {
        let mut options = AnalysisOptions::default();
        if included {
            options.included_tags.insert("keep".into());
        } else {
            options.excluded_tags.insert("hidden".into());
        }
        let (_, filtered) = run(&files, options);
        assert_eq!(filtered.members.uncertainty, normal.members.uncertainty);
        assert_eq!(filtered.members.gaps, normal.members.gaps);
        assert_eq!(filtered.members.evidence, normal.members.evidence);
        assert_eq!(filtered.coverage, normal.coverage);
    }
}

#[test]
fn parent_tag_filters_preserve_member_uncertainty() {
    assert_member_tags_preserve_uncertainty(
        "/** @hidden */ export namespace N { export const item=1; }",
    );
}

#[test]
fn leaf_tag_filters_preserve_member_uncertainty() {
    assert_member_tags_preserve_uncertainty(
        "export namespace N { /** @hidden */ export const item=1; }",
    );
}

#[test]
fn escaped_member_declarations_match_decoded_accesses() {
    let source = r"export namespace N { export const \u0075sed=1; export const \u0075nused=2; export namespace \u004eested { export const used=3; export const \u0075nused=4; } }";
    let (snapshot, result) = run(
        &[
            (
                "index.ts",
                "import { N } from './lib'; N.used; N.Nested.used;",
            ),
            ("lib.ts", source),
        ],
        Default::default(),
    );
    let module = snapshot
        .modules
        .iter()
        .find(|(id, _)| snapshot.sources[id].logical_path == "lib.ts")
        .unwrap()
        .1
        .collected
        .as_ref()
        .unwrap();
    let mut paths: Vec<_> = module
        .members
        .iter()
        .map(|member| member.path.join("."))
        .collect();
    paths.sort();
    assert_eq!(paths, ["Nested.unused", "Nested.used", "unused", "used"]);
    assert_eq!(
        symbols(&result),
        expected(&[
            ("lib.ts", "N.unused", IssueKind::NamespaceMembers),
            ("lib.ts", "N.Nested.unused", IssueKind::NamespaceMembers),
        ])
    );
    for issue in &result.issues {
        assert_eq!(
            &source[usize::from(issue.location.range.start())
                ..usize::from(issue.location.range.end())],
            r"\u0075nused"
        );
    }
    assert!(result.members.uncertainty.is_empty());
}
