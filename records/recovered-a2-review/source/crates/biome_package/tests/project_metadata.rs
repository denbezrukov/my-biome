use biome_deserialize::json::deserialize_from_json_str;
use biome_json_parser::JsonParserOptions;
use biome_package::PackageJson;

#[test]
fn package_metadata_locations_and_types_precedence() {
    let malformed_catalog = metadata(r#"{"catalog":{"react":false}}"#);
    let metadata = metadata(r#"{"typings":"legacy.d.ts","types":"index.d.ts"}"#);
    assert_eq!(metadata.types.as_deref(), Some("index.d.ts"));
    assert!(!malformed_catalog.diagnostics.is_empty());
    let resolver = deserialize_from_json_str::<PackageJson>(
        r#"{"typings":"legacy.d.ts","types":"index.d.ts"}"#,
        JsonParserOptions::default(),
        "package.json",
    )
    .into_deserialized()
    .unwrap();
    assert_eq!(resolver.types.as_deref(), Some("index.d.ts"));
}

fn metadata(source: &str) -> biome_package::ProjectMetadata {
    biome_package::read_project_metadata(
        &biome_json_parser::parse_json(source, JsonParserOptions::default()).tree(),
    )
}

#[test]
fn manifest_entry_metadata_is_lossless() {
    let source = r##"{"types":"index.d.ts","typings":"legacy.d.ts","module":"esm.js","browser":"browser.js","imports":{"#conditional":{"import":"./src/a.ts","default":"pkg"},"#list":["./a.ts",null]},"scripts":{"start":"node src/index.js"},"bin":{"cli":"cli.js"},"workspaces":{"packages":["packages/*"],"catalog":{"react":"^19"}},"peerDependenciesMeta":{"react":{"optional":true}},"private":true,"publishConfig":{"types":"public.d.ts"},"typesVersions":{"*":{"*": ["types/*"]}}}"##;
    let parsed = metadata(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.types.as_deref(), Some("index.d.ts"));
    for (key, value) in [
        ("types", "index.d.ts"),
        ("typings", "legacy.d.ts"),
        ("module", "esm.js"),
        ("browser", "browser.js"),
    ] {
        let field = &parsed.fields[key];
        assert_eq!(field.value.as_string().unwrap().as_str(), value);
        assert_eq!(
            &source[usize::from(field.range.start())..usize::from(field.range.end())],
            format!("\"{value}\"")
        );
    }
    assert_eq!(parsed.raw_types.as_deref(), Some("index.d.ts"));
    assert_eq!(parsed.typings.as_deref(), Some("legacy.d.ts"));
    assert!(
        parsed
            .imports
            .as_ref()
            .unwrap()
            .value
            .as_object()
            .unwrap()
            .contains_key("#conditional")
    );
    for (field, child) in [
        ("scripts", "start"),
        ("bin", "cli"),
        ("peerDependenciesMeta", "react"),
        ("workspaces", "packages"),
    ] {
        assert!(!parsed.fields[field].members[child].range.is_empty());
    }
    assert_eq!(parsed.fields["private"].value.as_bool(), Some(true));
    assert_eq!(
        parsed.fields["peerDependenciesMeta"].members["react"].members["optional"]
            .value
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        parsed.fields["publishConfig"].members["types"]
            .value
            .as_string()
            .unwrap()
            .as_str(),
        "public.d.ts"
    );
    assert!(
        !parsed.fields["typesVersions"].members["*"].members["*"].elements[0]
            .range
            .is_empty()
    );
    let alternate = metadata(r#"{"bin":"cli.js","workspaces":["packages/*"]}"#);
    assert!(alternate.diagnostics.is_empty());
    assert_eq!(
        alternate.fields["bin"].value.as_string().unwrap().as_str(),
        "cli.js"
    );
    assert_eq!(
        alternate.fields["workspaces"].elements[0]
            .value
            .as_string()
            .unwrap()
            .as_str(),
        "packages/*"
    );
    let browser = metadata(r#"{"browser":{"./a.js":"./b.js","fs":false}}"#);
    assert!(browser.diagnostics.is_empty());
    assert_eq!(
        browser.browser.unwrap().members["fs"].value.as_bool(),
        Some(false)
    );
    assert_eq!(metadata(r#"{"typings":"legacy","types":42}"#).types, None);
    assert_eq!(metadata(r#"{"types":42,"typings":"legacy"}"#).types, None);
}

#[test]
fn malformed_metadata_and_catalogs_are_visible() {
    for source in [
        r#"{"catalog":true}"#,
        r#"{"catalogs":{"legacy":{"react":false}}}"#,
        r#"{"workspaces":{"packages":false,"catalog":{"react":42}}}"#,
        r#"{"scripts":{"start":false}}"#,
        r#"{"bin":["cli"]}"#,
        r#"{"peerDependenciesMeta":{"pkg":{"optional":"yes"}}}"#,
        r#"{"private":"yes"}"#,
        r#"{"typesVersions":false}"#,
        r#"{"publishConfig":{"types":42}}"#,
        r#"{"browser":{"fs":42}}"#,
        r##"{"imports":{"#x":true}}"##,
    ] {
        assert!(!metadata(source).diagnostics.is_empty(), "{source}");
    }
    let parsed = metadata(r#"{"catalog":{"react":"^19","bad":false}}"#);
    assert_eq!(
        parsed.catalogs.entries[0].value.members["react"]
            .value
            .as_string()
            .unwrap()
            .as_str(),
        "^19"
    );
    assert!(!parsed.catalogs.diagnostics.is_empty());
    assert!(!parsed.fields["catalog"].members["bad"].range.is_empty());
}

#[test]
fn tsconfig_metadata_preserves_resolution_inputs_and_ranges() {
    use biome_package::TsConfigJson;
    let parsed = deserialize_from_json_str::<TsConfigJson>(
        r#"{
      "files":["src/index.ts"],"include":["src/**"],"exclude":["dist"],"references":[{"path":"../lib"}],
      "compilerOptions":{"rootDir":"src","outDir":"dist","module":"nodenext","moduleResolution":"bundler",
      "customConditions":["source"],"allowJs":true,"resolveJsonModule":true,"types":["node"],"rootDirs":["src","generated"]}
    }"#,
        JsonParserOptions::default(),
        "tsconfig.json",
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let parsed = parsed.into_deserialized().unwrap();
    assert_eq!(
        parsed.metadata.as_ref().unwrap().members["compilerOptions"].members["rootDir"]
            .value
            .as_string()
            .unwrap()
            .as_str(),
        "src"
    );
    assert_eq!(parsed.files.unwrap().value, ["src/index.ts"]);
    assert_eq!(parsed.include.unwrap().value, ["src/**"]);
    assert_eq!(parsed.exclude.unwrap().value, ["dist"]);
    assert!(!parsed.references[0].location.unwrap().is_empty());
    assert_eq!(parsed.compiler_options.root_dir.unwrap().value, "src");
    assert_eq!(
        parsed.compiler_options.custom_conditions.unwrap().value,
        ["source"]
    );
    assert!(parsed.compiler_options.allow_js.unwrap().value);
}

#[test]
fn package_discovery_retains_raw_evidence_and_reports_once() {
    use biome_package::{Manifest, NodeJsPackage, Package};
    let source = r#"{"types":42,"typings":"legacy","scripts":{"start":false},"catalogs":false}"#;
    let parsed = biome_json_parser::parse_json(source, JsonParserOptions::default());
    let mut package = NodeJsPackage::default();
    package.insert_serialized_manifest(&parsed.tree(), camino::Utf8Path::new("/package.json"));
    let manifest = package.manifest.unwrap();
    assert!(manifest.types.is_none());
    assert_eq!(manifest.project_metadata.typings.as_deref(), Some("legacy"));
    assert_eq!(manifest.project_metadata.diagnostics.len(), 3);
    assert_eq!(package.diagnostics.len(), 1);
    assert!(!manifest.project_metadata.catalogs.diagnostics.is_empty());
    let reread =
        PackageJson::deserialize_manifest(&parsed.tree(), camino::Utf8Path::new("/package.json"));
    assert_eq!(reread.diagnostics().len(), 1);
}

#[test]
fn conditional_import_order_is_preserved() {
    let parsed = metadata(
        r##"{"imports":{"#x":{"z-source":"./src.ts","default":"./dist.js","a-fallback":"./fallback.js"}}}"##,
    );
    let imports = parsed.imports.unwrap();
    let conditional = imports
        .value
        .as_object()
        .unwrap()
        .get("#x")
        .unwrap()
        .as_object()
        .unwrap();
    assert_eq!(
        conditional
            .keys()
            .map(|key| key.as_str())
            .collect::<Vec<_>>(),
        ["z-source", "default", "a-fallback"]
    );
    assert_eq!(
        imports.members["#x"]
            .members
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["z-source", "default", "a-fallback"]
    );
}

#[test]
fn missing_manifest_value_cannot_look_complete() {
    assert!(!metadata("").diagnostics.is_empty());
}

#[test]
fn manifest_catalog_candidates_preserve_distinct_origins() {
    let parsed = metadata(
        r#"{"catalog":{"root":"^1"},"catalogs":{"legacy":{"root":"^2"}},"workspaces":{"catalog":{"nested":"^3"},"catalogs":{"legacy":{"nested":"^4"}}}}"#,
    );
    assert_eq!(parsed.catalogs.entries.len(), 4);
    assert_eq!(
        parsed
            .catalogs
            .entries
            .iter()
            .map(|entry| entry.path.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [
            vec!["catalog"],
            vec!["catalogs", "legacy"],
            vec!["workspaces", "catalog"],
            vec!["workspaces", "catalogs", "legacy"]
        ]
    );
}

#[test]
fn metadata_diagnostics_preserve_legacy_manifest_behavior() {
    let source = r#"{"types":"index.d.ts","scripts":{"start":false},"catalog":{"react":false}}"#;
    let legacy = deserialize_from_json_str::<PackageJson>(
        source,
        JsonParserOptions::default(),
        "package.json",
    );
    assert!(legacy.diagnostics().is_empty());
    assert_eq!(
        legacy
            .into_deserialized()
            .unwrap()
            .project_metadata
            .diagnostics
            .len(),
        2
    );
    assert_eq!(metadata(source).diagnostics.len(), 2);
}
