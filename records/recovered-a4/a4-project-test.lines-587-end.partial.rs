fn project_resolution_partial_manifest_quality_and_repair() {
    let fs = MemoryFileSystem::default();
    fs.insert("/app/package.json".into(), "{}");
    fs.insert(
        "/app/node_modules/pkg/package.json".into(),
        r#"{"name":false,"main":"./index.js"}"#,
    );
    fs.insert("/app/node_modules/pkg/index.js".into(), "");
    let mut db = support::build_js_db(&fs, &[], false);
    let resolve = |db: &dyn ResolverDb| {
        resolve_project_specifier(
            db,
            Utf8Path::new("/app"),
            "pkg",
            ProjectResolutionContext::default(),
        )
    };
    assert!(matches!(resolve(&db), ProjectResolution::Unknown { .. }));
    let legacy = |db: &dyn ResolverDb| {
        resolve_specifier(db, Utf8Path::new("/app"), "pkg", ResolutionMode::Css)
            .path()
            .clone()
    };
    let before = legacy(&db);
    support::add_json_file(
        &mut db,
        Utf8Path::new("/app/node_modules/pkg/package.json"),
        r#"{"name":"pkg","main":"./index.js"}"#,
    );
    assert!(matches!(resolve(&db), ProjectResolution::Resolved { .. }));
    assert_eq!(before, legacy(&db));
    support::add_json_file(
        &mut db,
        Utf8Path::new("/app/node_modules/pkg/package.json"),
        r#"{"name":false,"main":"./index.js"}"#,
    );
    assert!(matches!(resolve(&db), ProjectResolution::Unknown { .. }));
    support::add_json_file(
        &mut db,
        Utf8Path::new("/app/node_modules/pkg/package.json"),
        r#"{"name":"pkg","main":"./index.js"}"#,
    );
    support::add_json_file(
        &mut db,
        Utf8Path::new("/app/tsconfig.json"),
        r#"{"compilerOptions":{"paths":{"alias":false}}}"#,
    );
    assert!(matches!(resolve(&db), ProjectResolution::Unknown { .. }));
    support::add_json_file(&mut db, Utf8Path::new("/app/tsconfig.json"), "42");
    assert!(matches!(resolve(&db), ProjectResolution::Unknown { .. }));
    support::add_json_file(
        &mut db,
        Utf8Path::new("/app/tsconfig.json"),
        r#"{"compilerOptions":{"futureOption":true}}"#,
    );
    assert!(matches!(resolve(&db), ProjectResolution::Resolved { .. }));
    support::add_json_file(
        &mut db,
        Utf8Path::new("/app/node_modules/pkg/package.json"),
        "{\"main\":\"./index.js\", broken}",
    );
    assert!(matches!(resolve(&db), ProjectResolution::Unknown { .. }));
}

#[test]
fn project_facts_type_namespace_and_default_registration() {
    let f = facts(
        "import type * as NS from './types'; type Alias = NS.Shape; export default class { static { customElements.define('x', this); } }",
        "/a.ts",
        ProjectParseStatus::Valid,
    );
    assert_eq!(
        f.uses
            .iter()
            .find(|usage| usage.namespace_access.is_some())
            .unwrap()
            .namespace_access,
        Some(ProjectNamespaceAccess::Known("Shape".into()))
    );
    let default = f
        .exports
        .iter()
        .find(|slot| slot.name == "default")
        .unwrap();
    let ProjectExportTarget::Local(id) = &default.target else {
        panic!("default must have syntax identity")
    };
    assert_eq!(f.registrations[0].declaration.as_ref(), Some(id));
}

#[test]
fn project_facts_opaque_commonjs_mutations() {
    for source in [
        "delete exports.value;",
        "exports.value++;",
        "exports.value.deep = 1;",
        "module.exports = { [computed]: 1 };",
        "Object.assign(exports, opaque);",
    ] {
        let f = facts(source, "/a.js", ProjectParseStatus::Valid);
        assert!(
            f.unknown
                .iter()
                .any(|fact| fact.kind == ProjectUnknownKind::Exports),
            "{source}"
        );
    }
    let f = facts(
        "function f(exports, module) { exports.value = 1; module.exports = 2; }",
        "/a.js",
        ProjectParseStatus::Valid,
    );
    assert!(f.exports.is_empty());
}

#[test]
fn project_exports_requires_trusted_parse() {
    let fs = MemoryFileSystem::default();
    fs.insert("/a.ts".into(), "export const value = 1;");
    let db = build_js_test_module_db(&fs, &["/a.ts"], false);
    for symbol in ["value", "missing"] {
        assert!(matches!(
            resolve_project_export(
                &db,
                Utf8Path::new("/a.ts"),
                symbol,
                ProjectSymbolNamespace::Value,
                ProjectResolutionContext::default()
            ),
            ProjectExportResolution::Unknown(_)
        ));
    }
}

#[test]
fn project_resolution_pinned_condition_matrix() {
    let cases = [
        (
            r#"{"import":"./import.js","require":"./require.js","default":"./default.js"}"#,
            Some("import.js"),
            Some("import.js"),
        ),
        (
            r#"{"require":"./require.js","import":"./import.js","default":"./default.js"}"#,
            Some("require.js"),
            Some("require.js"),
        ),
        (
            r#"{"default":"./default.js","import":"./import.js","require":"./require.js"}"#,
            Some("default.js"),
            Some("default.js"),
        ),
        (
            r#"{"types":"./types.d.ts","import":"./import.js","require":"./require.js"}"#,
            Some("import.js"),
            Some("types.d.ts"),
        ),
        (
            r#"{"import":"./import.js","types":"./types.d.ts","require":"./require.js"}"#,
            Some("import.js"),
            Some("import.js"),
        ),
        (
            r#"{"browser":"./browser.js","node":"./node.js"}"#,
            Some("node.js"),
            Some("node.js"),
        ),
        (
            r#"{"browser":"./browser.js","node":"./missing.js"}"#,
            Some("browser.js"),
            None,
        ),
        (r#"{"browser":"./browser.js"}"#, Some("browser.js"), None),
    ];
    for (conditions, runtime, declaration) in cases {
        for imports in [false, true] {
            let fs = MemoryFileSystem::default();
            let root = if imports {
                "/app"
            } else {
                "/app/node_modules/pkg"
            };
            fs.insert("/app/package.json".into(), "{}");
            let manifest = if imports {
                format!("{{\"imports\":{{\"#probe\":{conditions}}}}}")
            } else {
                format!("{{\"exports\":{conditions}}}")
            };
            fs.insert(format!("{root}/package.json").into(), manifest);
            for target in [
                "import.js",
                "require.js",
                "default.js",
                "types.d.ts",
                "node.js",
                "browser.js",
            ] {
                fs.insert(format!("{root}/{target}").into(), "");
            }
            let db = support::build_js_db(&fs, &[], false);
            for syntax in [
                ProjectImportSyntax::Import,
                ProjectImportSyntax::Require,
                ProjectImportSyntax::DynamicImport,
            ] {
                for (purpose, expected) in [
                    (ProjectResolutionPurpose::Runtime, runtime),
                    (ProjectResolutionPurpose::Declaration, declaration),
                ] {
                    let result = resolve_project_specifier(
                        &db,
                        Utf8Path::new("/app"),
                        if imports { "#probe" } else { "pkg" },
                        ProjectResolutionContext {
                            syntax,
                            purpose,
                            ..Default::default()
                        },
                    );
                    match expected {
                        Some(expected) => assert!(
                            matches!(result, ProjectResolution::Resolved { target, .. } if target == Utf8Path::new(&format!("{root}/{expected}"))),
                            "{conditions} {syntax:?} {purpose:?}"
                        ),
                        None => assert!(
                            matches!(result, ProjectResolution::Unresolved { .. }),
                            "{conditions} {syntax:?} {purpose:?}: {result:?}"
                        ),
                    }
                }
            }
        }
    }
}
