use biome_configuration::Configuration;
use biome_deserialize::json::deserialize_from_json_str;
use biome_json_parser::JsonParserOptions;

#[test]
fn jsonc_configuration_preserves_analysis_distinctions() {
    let result = deserialize_from_json_str::<Configuration>(
        r#"{ // reporting suppression must not become discovery exclusion
          "projectAnalysis": {"ignore": "generated/**", "ignoreFiles": ["fixtures/**"],},
        }"#,
        JsonParserOptions::default()
            .with_allow_comments()
            .with_allow_trailing_commas(),
        "biome.jsonc",
    );
    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );
    let config = result.into_deserialized().unwrap();
    assert!(config.files.is_none());
    let analysis = config.project_analysis.unwrap();
    assert_eq!(analysis.ignore.unwrap().0, ["generated/**"]);
    assert_eq!(analysis.ignore_files.unwrap().0, ["fixtures/**"]);
}

#[test]
fn production_pattern_roles() {
    use biome_configuration::project_analysis::{ProjectAnalysisMode, ProjectPattern};
    for (input, pattern, negated, production) in [
        ("src/**!", "src/**", false, true),
        ("!test/**!", "test/**", true, true),
        ("!test/**", "test/**", true, false),
        ("src/**", "src/**", false, false),
    ] {
        let parsed = ProjectPattern::new(input);
        assert_eq!(parsed.pattern, pattern);
        assert_eq!(parsed.negated, negated);
        assert_eq!(parsed.production, production);
        assert!(parsed.applies_to(ProjectAnalysisMode::Default));
        assert_eq!(
            parsed.applies_to(ProjectAnalysisMode::Production),
            production
        );
        assert_eq!(parsed.applies_to(ProjectAnalysisMode::Strict), production);
    }
}

#[test]
fn default_entries_are_production_roots() {
    use biome_configuration::project_analysis::{
        ProjectAnalysisMode, WorkspaceAnalysisConfiguration,
    };
    let defaults = WorkspaceAnalysisConfiguration::default();
    assert_eq!(
        defaults
            .entry_patterns()
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        [
            "{index,cli,main}.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!",
            "src/{index,cli,main}.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!",
        ]
    );
    assert_eq!(
        defaults
            .project_patterns()
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        ["**/*.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!"]
    );
    for mode in [
        ProjectAnalysisMode::Default,
        ProjectAnalysisMode::Production,
        ProjectAnalysisMode::Strict,
    ] {
        assert!(
            defaults
                .normalized_entries()
                .iter()
                .all(|p| p.production && p.applies_to(mode))
        );
    }
    let result = deserialize_from_json_str::<WorkspaceAnalysisConfiguration>(
        r#"{"entry":"custom.ts","project":[]}"#,
        JsonParserOptions::default(),
        "",
    );
    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );
    let explicit = result.into_deserialized().unwrap();
    assert_eq!(explicit.entry_patterns(), ["custom.ts"]);
    assert!(explicit.project_patterns().is_empty());
    let patterns = explicit.normalized_entries();
    assert!(patterns[0].applies_to(ProjectAnalysisMode::Default));
    assert!(!patterns[0].applies_to(ProjectAnalysisMode::Production));
    assert!(!patterns[0].applies_to(ProjectAnalysisMode::Strict));
}

#[test]
fn static_policies_and_workspace_schema_are_validated() {
    let result = deserialize_from_json_str::<Configuration>(
        r#"{"projectAnalysis":{
      "enabled":true,"mode":"strict", "entry":"root.ts", "project":[],
      "paths":{"@/*":"src/*"}, "sourceMappings":[{"source":"src","output":"dist"}],
      "dependencyUses":[{"package":"runtime","workspace":".","reason":"external loader"}],
      "publicEntries":"api.ts","useTsconfigFiles":true,"tsconfig":"tsconfig.build.json",
      "rules":{"files":"warn","cycles":"off"}, "include":["nsExports"],"exclude":["cycles"],
      "ignore":"generated/**","ignoreFiles":"fixtures/**","ignoreDependencies":"runtime",
      "ignoreBinaries":["global"],"ignoreGlobalBinaries":false,"ignoreMembers":"member",
      "ignoreUnresolved":"virtual:*","ignoreExportsUsedInFile":{"type":true,"member":false},
      "ignoreIssues":{"generated/**":["files","exports"]},"includeEntryExports":true,
      "ignoreWorkspaces":["packages/**","!packages/live"],"tags":["+public","-internal"],
      "cycles":{"dynamicImports":true,"allow":[["a.ts","b.ts"]]},
      "treatConfigHintsAsErrors":true,"treatTagHintsAsErrors":true,
      "workspaces":{"packages/*":{"entry":"src/main.ts","ignoreFiles":"fixtures/**"}}
    }}"#,
        JsonParserOptions::default(),
        "biome.json",
    );
    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );
    let analysis = result
        .into_deserialized()
        .unwrap()
        .project_analysis
        .unwrap();
    assert!(analysis.enabled.unwrap());
    let uses = analysis.dependency_uses.unwrap();
    let location = uses[0].location.as_ref().unwrap();
    assert_eq!(location.source.as_deref(), Some("biome.json"));
    assert!(!location.range.is_empty());
    for source in [
        r#"{"projectAnalysis":{"compilers":{}}}"#,
        r#"{"projectAnalysis":{"preprocessor":"run.js"}}"#,
        r#"{"projectAnalysis":{"workspaces":{".":{"mode":"strict"}}}}"#,
        r#"{"projectAnalysis":{"rules":{"notAnIssue":"warn"}}}"#,
        r#"{"projectAnalysis":{"rules":{"files":"warning"}}}"#,
        r#"{"projectAnalysis":{"ignoreExportsUsedInFile":{"unknown":true}}}"#,
        r#"{"projectAnalysis":{"dependencyUses":[{"package":"runtime"}]}}"#,
    ] {
        assert!(
            !deserialize_from_json_str::<Configuration>(
                source,
                JsonParserOptions::default(),
                "biome.json"
            )
            .diagnostics()
            .is_empty(),
            "{source}"
        );
    }
    assert!(Configuration::default().project_analysis.is_none());
}

#[test]
fn ordered_patterns_replace_inherited_arrays() {
    use biome_deserialize::Merge;
    let parse = |source| {
        deserialize_from_json_str::<Configuration>(source, JsonParserOptions::default(), "")
            .into_deserialized()
            .unwrap()
    };
    let mut base =
        parse(r#"{"projectAnalysis":{"entry":["base.ts"],"ignoreWorkspaces":["base"]}}"#);
    base.merge_with(parse(r#"{"projectAnalysis":{"entry":["custom.ts","!custom.ts","custom.ts"],"ignoreWorkspaces":["packages/**","!packages/live","packages/**"]}}"#));
    let analysis = base.project_analysis.unwrap();
    assert_eq!(
        analysis.entry.unwrap().0,
        ["custom.ts", "!custom.ts", "custom.ts"]
    );
    assert_eq!(
        analysis.ignore_workspaces.unwrap().0,
        ["packages/**", "!packages/live", "packages/**"]
    );
}

#[test]
fn compatibility_manifest_static_keys_are_exhaustively_classified() {
    use serde_json::{Value, json};
    let ledger: Value = serde_json::from_str(include_str!(
        "../../../xtask/project_analysis/compatibility.json"
    ))
    .unwrap();
    let samples = json!({
        "$schema":"https://example.com/schema.json", "rules":{"files":"error"},
        "entry":"index.ts", "project":["src/**!"], "paths":{"@/*":["src/*"]},
        "ignore":"generated/**", "ignoreFiles":"fixtures/**", "ignoreBinaries":["zip"],
        "ignoreGlobalBinaries":true, "ignoreDependencies":["pkg"], "ignoreMembers":["render"],
        "ignoreUnresolved":["virtual:*"], "ignoreExportsUsedInFile":true,
        "ignoreIssues":{"generated/**":["files"]}, "cycles":{"allow":[],"dynamicImports":false},
        "ignoreWorkspaces":["packages/old"], "includeEntryExports":true,
        "tags":["-internal"], "treatConfigHintsAsErrors":false,"treatTagHintsAsErrors":false,
        "include":["files"],"exclude":["cycles"],"workspaces":{".":{"entry":"index.ts"}}
    });
    for category in ["root_configuration", "workspace_configuration"] {
        for (key, status) in ledger[category].as_object().unwrap() {
            assert_eq!(
                status["status"],
                if status["static_schema"] == "unsupported" {
                    "unsupported"
                } else {
                    "untested"
                }
            );
            let value = samples.get(key).cloned().unwrap_or(json!({}));
            let mut settings = json!({});
            settings[key] = value;
            if category == "workspace_configuration" {
                settings = json!({"workspaces":{".":settings}});
            }
            let result = deserialize_from_json_str::<Configuration>(
                &json!({"projectAnalysis":settings}).to_string(),
                JsonParserOptions::default(),
                "",
            );
            assert_eq!(
                result.diagnostics().is_empty(),
                status["static_schema"] == "supported",
                "{category}.{key}: {:?}",
                result.diagnostics()
            );
        }
    }
    for issue in ledger["issues"].as_object().unwrap().keys() {
        let mut rules = json!({});
        rules[issue] = json!("warn");
        let source = json!({"projectAnalysis":{"rules":rules,"include":[issue],"exclude":[issue],"ignoreIssues":{"**":[issue]}}});
        assert!(
            deserialize_from_json_str::<Configuration>(
                &source.to_string(),
                JsonParserOptions::default(),
                ""
            )
            .diagnostics()
            .is_empty(),
            "{issue}"
        );
    }
    for plugin in ledger["plugins"].as_object().unwrap().keys() {
        for workspace in [false, true] {
            let mut settings = json!({});
            settings[plugin] = json!({});
            if workspace {
                settings = json!({"workspaces":{".":settings}});
            }
            assert!(
                !deserialize_from_json_str::<Configuration>(
                    &json!({"projectAnalysis":settings}).to_string(),
                    JsonParserOptions::default(),
                    ""
                )
                .diagnostics()
                .is_empty(),
                "{plugin}"
            );
        }
    }
}

#[test]
fn workspace_pattern_map_preserves_source_order() {
    let source = r#"{"projectAnalysis":{"workspaces":{"z/**":{"entry":"first.ts"},"a/**":{"entry":"second.ts"}}}}"#;
    let analysis =
        deserialize_from_json_str::<Configuration>(source, JsonParserOptions::default(), "")
            .into_deserialized()
            .unwrap()
            .project_analysis
            .unwrap();
    assert_eq!(
        analysis
            .workspaces
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["z/**", "a/**"]
    );
}

#[test]
fn source_mappings_require_both_directories_in_transport() {
    use biome_configuration::project_analysis::SourceMapping;
    assert!(serde_json::from_str::<SourceMapping>(r#"{"source":"src"}"#).is_err());
    assert!(serde_json::from_str::<SourceMapping>(r#"{"output":"dist"}"#).is_err());
}

#[test]
fn dependency_use_provenance_survives_configuration_merges() {
    use biome_deserialize::Merge;
    let parse = |source, id| {
        deserialize_from_json_str::<Configuration>(source, JsonParserOptions::default(), id)
            .into_deserialized()
            .unwrap()
    };
    let mut base = parse(
        r#"{"projectAnalysis":{"dependencyUses":[{"package":"base","workspace":".","reason":"loader"}]}}"#,
        "base.json",
    );
    base.merge_with(parse(r#"{"projectAnalysis":{"dependencyUses":[{"package":"root","workspace":".","reason":"external runner"}]}}"#, "root.json"));
    let uses = base.project_analysis.unwrap().dependency_uses.unwrap();
    assert_eq!(
        uses.iter()
            .map(|item| item.location.as_ref().unwrap().source.as_deref())
            .collect::<Vec<_>>(),
        [Some("base.json"), Some("root.json")]
    );
    let unknown = parse(
        r#"{"projectAnalysis":{"dependencyUses":[{"package":"pkg","workspace":".","reason":"loader"}]}}"#,
        "",
    );
    assert!(
        unknown.project_analysis.unwrap().dependency_uses.unwrap()[0]
            .location
            .as_ref()
            .unwrap()
            .source
            .is_none()
    );
}

#[test]
fn alias_patterns_preserve_source_order() {
    let source = r#"{"projectAnalysis":{"paths":{"@*bazbar":["first/*"],"@*bar":["second/*"]}}}"#;
    let analysis =
        deserialize_from_json_str::<Configuration>(source, JsonParserOptions::default(), "")
            .into_deserialized()
            .unwrap()
            .project_analysis
            .unwrap();
    assert_eq!(
        analysis
            .paths
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["@*bazbar", "@*bar"]
    );
}
