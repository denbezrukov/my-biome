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
