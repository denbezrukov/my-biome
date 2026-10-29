
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
