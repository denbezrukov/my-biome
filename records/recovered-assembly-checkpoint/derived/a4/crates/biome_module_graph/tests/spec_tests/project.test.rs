use super::*;
use biome_module_graph::*;

fn facts(source: &str, path: &str, status: ProjectParseStatus) -> ProjectModuleFacts {
    let fs = MemoryFileSystem::default();
    fs.insert(path.into(), source);
    let db = TestModuleDb::with_fs(&fs);
    let parsed = parse(
        source,
        JsFileSource::try_from(Utf8Path::new(path)).unwrap(),
        JsParserOptions::default(),
    );
    if status == ProjectParseStatus::Valid {
        assert!(!parsed.has_errors(), "{path}: {:?}", parsed.diagnostics());
    }
    let root = parsed.tree();
    let semantic = std::sync::Arc::new(biome_js_semantic::semantic_model(
        &root,
        biome_js_semantic::SemanticModelOptions::default(),
    ));
    resolve_js_module_with_project_facts(
        &db,
        root,
        &BiomePath::new(path),
        semantic,
        TypeInferenceMode::Disabled,
        status,
    )
    .0
    .project_facts
    .clone()
}

#[test]
fn project_facts_declaration_file_classification() {
    for path in ["/ambient.d.ts", "/ambient.d.mts", "/ambient.d.cts"] {
        let ambient = facts(
            "declare const ambient: string;",
            path,
            ProjectParseStatus::Valid,
        );
        assert!(ambient.is_ambient_declaration_file());
        assert!(
            facts(
                "export {}; declare global { const globalValue: string; }",
                path,
                ProjectParseStatus::Valid
            )
            .is_ambient_declaration_file()
        );
        assert!(
            !facts(
                "export interface Exported {}",
                path,
                ProjectParseStatus::Valid
            )
            .is_ambient_declaration_file()
        );
        assert!(
            !facts("declare const :;", path, ProjectParseStatus::Invalid)
                .is_ambient_declaration_file()
        );
        assert!(
            !facts(
                "declare const ambient: string;",
                path,
                ProjectParseStatus::Unknown
            )
            .is_ambient_declaration_file()
        );
    }
}

#[test]
fn project_facts_repeated_and_type_occurrences() {
    let source = "import { type T, value } from './dep'; import type { T as T2 } from './dep'; import { value as other } from './dep'; type Q = T; const x = value;";
    let facts = facts(source, "/a.ts", ProjectParseStatus::Valid);
    assert_eq!(facts.imports.len(), 3);
    let imports: Vec<_> = facts.imports.iter().collect();
    assert_ne!(imports[0].range, imports[1].range);
    assert_eq!(&source[imports[0].specifier_range], "'./dep'");
    assert_eq!(
        imports[0]
            .bindings
            .iter()
            .find(|b| b.imported == "T")
            .unwrap()
            .declaration
            .namespace,
        ProjectSymbolNamespace::Type
    );
    assert!(
        imports[0]
            .bindings
            .iter()
            .any(|b| b.imported == "value"
                && b.declaration.namespace == ProjectSymbolNamespace::Value)
    );
    assert!(
        imports[1]
            .bindings
            .iter()
            .all(|b| b.declaration.namespace == ProjectSymbolNamespace::Type)
    );
    assert!(
        facts
            .uses
            .iter()
            .any(|u| u.declaration.namespace == ProjectSymbolNamespace::Type)
    );
    assert!(
        facts
            .uses
            .iter()
            .any(|u| u.declaration.namespace == ProjectSymbolNamespace::Value)
    );
}

#[test]
fn project_facts_namespace_keys() {
    let f = facts(
        "import * as NS from './dep'; NS.used; NS['used']; NS[key]; consume(NS);",
        "/a.ts",
        ProjectParseStatus::Valid,
    );
    let accesses: Vec<_> = f
        .uses
        .iter()
        .filter_map(|u| u.namespace_access.clone())
        .collect();
    assert_eq!(
        accesses,
        vec![
            ProjectNamespaceAccess::Known("used".into()),
            ProjectNamespaceAccess::Known("used".into()),
            ProjectNamespaceAccess::Computed,
            ProjectNamespaceAccess::Escape
        ]
    );
}

#[test]
fn project_facts_custom_element_registration() {
    let f = facts(
        "class A {} class B {} class C {} class D {} customElements.define('a', A); window.customElements.define('b', B); const registry = new CustomElementRegistry(); const alias = registry; alias.define('c', C); class E { static { customElements.define('e', this); } } other.define('d', D);",
        "/a.ts",
        ProjectParseStatus::Valid,
    );
    assert_eq!(
        f.registrations
            .iter()
            .filter(|r| r.provenance != ProjectRegistryProvenance::Unknown)
            .count(),
        4
    );
    assert!(f.uses.iter().any(|u| {
        f.declarations
            .iter()
            .any(|d| d.name == "D" && d.id == u.declaration)
    }));
    for source in [
        "const customElements = fake; class C {} customElements.define('x', C);",
        "function f(window) { class C {} window.customElements.define('x', C); }",
        "const host = {customElements: fake}; class C {} host.customElements.define('x', C);",
        "const CustomElementRegistry = Fake; const registry = new CustomElementRegistry(); class C {} registry.define('x', C);",
    ] {
        let f = facts(source, "/a.ts", ProjectParseStatus::Valid);
        assert!(
            f.registrations
                .iter()
                .all(|r| r.provenance == ProjectRegistryProvenance::Unknown),
            "{source}"
        );
    }
}

#[test]
fn project_exports_origin() {
    let fs = MemoryFileSystem::default();
    for (path, source) in [
        ("/a.ts", "export const value = 1; export interface Shape {}"),
        (
            "/b.ts",
            "export {value as alias} from './a'; export * from './cycle';",
        ),
        ("/cycle.ts", "export * from './b'; export * from './a';"),
        (
            "/other.ts",
            "export const value = 2; export const Shape = 1;",
        ),
        (
            "/ambiguous.ts",
            "export * from './a'; export * from './other';",
        ),
        (
            "/explicit.ts",
            "export * from './ambiguous'; export {value} from './a';",
        ),
        (
            "/unknown.ts",
            "export * from './unindexed'; export * from './a';",
        ),
        ("/unindexed.ts", "export const value = 3;"),
    ] {
        fs.insert(path.into(), source);
    }
    let paths = [
        "/a.ts",
        "/b.ts",
        "/cycle.ts",
        "/other.ts",
        "/ambiguous.ts",
        "/explicit.ts",
        "/unknown.ts",
    ]
    .map(BiomePath::new);
    let db = support::build_js_db(&fs, &paths, false);
    let lookup = |path: &str, name, facet| {
        resolve_project_export(
            &db,
            Utf8Path::new(path),
            name,
            facet,
            ProjectResolutionContext::default(),
        )
    };
    let value = lookup("/a.ts", "value", ProjectSymbolNamespace::Value);
    assert!(matches!(value, ProjectExportResolution::Found(_)));
    assert_eq!(
        lookup("/b.ts", "alias", ProjectSymbolNamespace::Value),
        value
    );
    assert_eq!(
        lookup("/cycle.ts", "value", ProjectSymbolNamespace::Value),
        value
    );
    assert_eq!(
        lookup("/explicit.ts", "value", ProjectSymbolNamespace::Value),
        value
    );
    assert!(matches!(
        lookup("/ambiguous.ts", "value", ProjectSymbolNamespace::Value),
        ProjectExportResolution::Ambiguous(_)
    ));
    assert!(matches!(
        lookup("/ambiguous.ts", "Shape", ProjectSymbolNamespace::Type),
        ProjectExportResolution::Found(_)
    ));
    assert!(matches!(
        lookup("/ambiguous.ts", "Shape", ProjectSymbolNamespace::Value),
        ProjectExportResolution::Found(_)
    ));
    let lookup = lookup("/unknown.ts", "value", ProjectSymbolNamespace::Value);
    assert!(matches!(lookup, ProjectExportResolution::Unknown(_)));
}

#[test]
fn project_facts_commonjs_and_entries() {
    let f = facts(
        "const dep = require('./dep'); exports.value = dep; module.exports.other = 1; require.resolve('./resolved'); import.meta.resolve('./meta'); new URL('./asset', import.meta.url); import('./dynamic'); require(computed); exports[key] = value; function f(require) { require('./shadowed'); require.resolve('./also-shadowed'); }",
        "/a.ts",
        ProjectParseStatus::Valid,
    );
    let specifiers: Vec<_> = f.imports.named_iter().map(|(s, _)| s.to_string()).collect();
    assert_eq!(
        specifiers,
        ["./dep", "./resolved", "./meta", "./asset", "./dynamic"]
    );
    assert!(
        f.exports
            .iter()
            .any(|e| e.name == "value" && e.form == ProjectExportForm::CommonJs)
    );
    assert!(f.exports.iter().any(|e| e.name == "other"));
    assert!(f.unknown.len() >= 2);
}

#[test]
fn project_resolution_contexts_and_target_union() {
    let fs = MemoryFileSystem::default();
    fs.insert("/app/package.json".into(), "{}");
    fs.insert("/app/main.ts".into(), "import type { T } from 'pkg'; const value = require('pkg'); import('pkg'); import 'node:fs'; import './missing';");
    for name in [
        "import.js",
        "require.js",
        "types.d.ts",
        "browser.js",
        "default.js",
    ] {
        fs.insert(
            format!("/app/node_modules/pkg/{name}").into(),
            "export const value = 1;",
        );
    }
    let manifest = r#"{"name":"pkg","exports":{"types":"./types.d.ts","require":"./require.js","import":"./import.js","default":"./default.js"}}"#;
    fs.insert("/app/node_modules/pkg/package.json".into(), manifest);
    let mut db = support::build_js_db(&fs, &[BiomePath::new("/app/main.ts")], false);
    let resolve = |db: &dyn ResolverDb, context| {
        resolve_project_specifier(db, Utf8Path::new("/app"), "pkg", context)
    };
    let target = |result: ProjectResolution| match result {
        ProjectResolution::Resolved { target, .. } => target,
        result => panic!("{result:?}"),
    };
    let runtime = ProjectResolutionContext::default();
    let declaration = ProjectResolutionContext {
        purpose: ProjectResolutionPurpose::Declaration,
        ..runtime.clone()
    };
    assert_eq!(
        target(resolve(&db, runtime.clone())),
        Utf8Path::new("/app/node_modules/pkg/require.js")
    );
    assert_eq!(
        target(resolve(&db, declaration.clone())),
        Utf8Path::new("/app/node_modules/pkg/types.d.ts")
    );
    let module = db.module_for_path(Utf8Path::new("/app/main.ts")).unwrap();
    let info = module.kind(&db).as_js_module_info().unwrap();
    let dependencies = project_dependencies(
        &db,
        Utf8Path::new("/app/main.ts"),
        info,
        &[runtime.clone(), declaration.clone()],
    );
    assert_eq!(dependencies.targets.len(), 2);
    assert_eq!(dependencies.occurrences.len(), 10);
    assert!(
        dependencies
            .occurrences
            .iter()
            .any(|occurrence| matches!(occurrence.resolution, ProjectResolution::Builtin { .. }))
    );
    assert!(
        dependencies.occurrences.iter().any(|occurrence| matches!(
            occurrence.resolution,
            ProjectResolution::Unresolved { .. }
        ))
    );
    assert_eq!(
        target(dependencies.occurrences[0].resolution.clone()),
        Utf8Path::new("/app/node_modules/pkg/require.js")
    );
    for (json, expected) in [
        (
            r#"{"exports":{"import":"./import.js","require":"./require.js"}}"#,
            "import.js",
        ),
        (
            r#"{"exports":{"default":"./default.js","import":"./import.js"}}"#,
            "default.js",
        ),
        (
            r#"{"exports":{"browser":"./browser.js","node":"./require.js"}}"#,
            "require.js",
        ),
        (
            r#"{"exports":{"browser":"./browser.js","node":"./missing.js"}}"#,
            "browser.js",
        ),
    ] {
        support::add_json_file(
            &mut db,
            Utf8Path::new("/app/node_modules/pkg/package.json"),
            json,
        );
        assert_eq!(
            target(resolve(&db, runtime.clone())),
            Utf8Path::new(&format!("/app/node_modules/pkg/{expected}"))
        );
    }
    support::add_json_file(
        &mut db,
        Utf8Path::new("/app/node_modules/pkg/package.json"),
        manifest,
    );
    let source = "/app/node_modules/pkg/import.js";
    let mapped = ProjectResolutionContext {
        source_mappings: vec![biome_resolver::SourceMapping {
            from: "/app/node_modules/pkg/require.js".into(),
            to: source.into(),
        }],
        ..runtime.clone()
    };
    assert_eq!(target(resolve(&db, mapped)), Utf8Path::new(source));
    assert_eq!(
        target(resolve(&db, runtime)),
        Utf8Path::new("/app/node_modules/pkg/require.js")
    );
}

#[test]
fn project_facts_node_entry_bindings() {
    let f = facts(
        "import { register as load } from 'node:module'; import { Worker } from 'node:worker_threads'; import { join } from 'node:path'; import { fork as launch } from 'node:child_process'; load('./loader', import.meta.url); new Worker(join(__dirname, 'worker.js')); launch(join(__dirname, 'child.js')); function local(Worker, join) { new Worker(join(__dirname, 'shadowed.js')); }",
        "/app/main.ts",
        ProjectParseStatus::Valid,
    );
    let entries: Vec<_> = f
        .imports
        .named_iter()
        .filter(|(_, occurrence)| {
            matches!(
                occurrence.syntax,
                ProjectImportSyntax::NodeEntry | ProjectImportSyntax::Worker
            )
        })
        .map(|(specifier, _)| specifier.to_string())
        .collect();
    assert_eq!(entries, ["./loader", "./worker.js", "./child.js"]);
}

#[test]
fn project_facts_typeof_and_commonjs_uncertainty() {
    let f = facts(
        "import * as NS from './dep'; type Alias = typeof NS; typeof NS; const { used: local } = require('./cjs'); Object.assign(exports, opaque); consume(module.exports);",
        "/a.ts",
        ProjectParseStatus::Valid,
    );
    let uses: Vec<_> = f
        .uses
        .iter()
        .filter(|usage| usage.namespace_access.is_some())
        .collect();
    assert_eq!(uses[0].kind, ProjectUseKind::TypeQuery);
    assert_eq!(uses[1].kind, ProjectUseKind::RuntimeTypeof);
    let require = f
        .imports
        .iter()
        .find(|import| import.syntax == ProjectImportSyntax::Require)
        .unwrap();
    assert_eq!(require.bindings[0].imported, "used");
    assert!(
        f.unknown
            .iter()
            .any(|fact| fact.reason.contains("CommonJS"))
    );
}

#[test]
fn project_facts_registration_alias_and_scope() {
    let f = facts(
        "class C {} const Alias = C; const root = document.body.attachShadow({mode: 'open'}) as ShadowRoot; root.customElements.define('a', Alias); let registry = customElements; registry = fake; registry.define('b', C);",
        "/a.ts",
        ProjectParseStatus::Valid,
    );
    assert_eq!(
        f.registrations[0].provenance,
        ProjectRegistryProvenance::ShadowRoot
    );
    assert_eq!(
        f.registrations[0].declaration.as_ref().unwrap().range,
        f.declarations
            .iter()
            .find(|declaration| declaration.name == "C")
            .unwrap()
            .id
            .range
    );
    assert_eq!(
        f.registrations[1].provenance,
        ProjectRegistryProvenance::Unknown
    );
}

#[test]
fn project_exports_unavailable_incremental_and_context() {
    let fs = MemoryFileSystem::default();
    fs.insert(
        "/barrel.ts".into(),
        "export * as NS from './late'; export {value} from './late';",
    );
    fs.insert("/late.ts".into(), "export const value = 1;");
    fs.insert("/other.ts".into(), "export const value = 2;");
    let mut db = support::build_js_db(
        &fs,
        &[BiomePath::new("/barrel.ts"), BiomePath::new("/other.ts")],
        false,
    );
    let lookup = |db: &dyn ModuleDb, symbol, context| {
        resolve_project_export(
            db,
            Utf8Path::new("/barrel.ts"),
            symbol,
            ProjectSymbolNamespace::Value,
            context,
        )
    };
    assert!(matches!(
        lookup(&db, "value", ProjectResolutionContext::default()),
        ProjectExportResolution::Unknown(_)
    ));
    assert!(matches!(
        lookup(&db, "NS", ProjectResolutionContext::default()),
        ProjectExportResolution::Unknown(_)
    ));
    support::add_js_modules(&mut db, &fs, &[BiomePath::new("/late.ts")], false);
    let fresh = support::build_js_db(
        &fs,
        &[
            BiomePath::new("/barrel.ts"),
            BiomePath::new("/other.ts"),
            BiomePath::new("/late.ts"),
        ],
        false,
    );
    assert_eq!(
        lookup(&db, "value", ProjectResolutionContext::default()),
        lookup(&fresh, "value", ProjectResolutionContext::default())
    );
    let mapped = ProjectResolutionContext {
        source_mappings: vec![biome_resolver::SourceMapping {
            from: "./late".into(),
            to: "/other.ts".into(),
        }],
        ..Default::default()
    };
    assert_ne!(
        lookup(&db, "value", mapped),
        lookup(&db, "value", ProjectResolutionContext::default())
    );
}

#[test]
fn project_facts_nested_exports_and_import_equals() {
    let f = facts(
        "namespace N { export const inner = 1; } export const outer = 1; import dep = require('./dep');",
        "/a.ts",
        ProjectParseStatus::Valid,
    );
    assert!(!f.exports.iter().any(|slot| slot.name == "inner"));
    let import = f.imports.iter().next().unwrap();
    assert_eq!(import.syntax, ProjectImportSyntax::Require);
    assert!(
        import
            .bindings
            .iter()
            .any(|binding| binding.imported == "*")
    );
    let f = facts(
        "exports.a = 1; exports.a += 2; exports = other;",
        "/a.js",
        ProjectParseStatus::Valid,
    );
    assert!(
        f.unknown
            .iter()
            .any(|fact| fact.reason.contains("CommonJS"))
    );
}

#[test]
fn project_exports_namespace_alias_and_default_class_facets() {
    let fs = MemoryFileSystem::default();
    fs.insert("/a.ts".into(), "export const value = 1;");
    fs.insert(
        "/barrel.ts".into(),
        "import * as NS from './a'; export {NS}; class C {} export default C;",
    );
    let db = support::build_js_db(
        &fs,
        &[BiomePath::new("/a.ts"), BiomePath::new("/barrel.ts")],
        false,
    );
    for (name, facet) in [
        ("NS", ProjectSymbolNamespace::Value),
        ("default", ProjectSymbolNamespace::Value),
        ("default", ProjectSymbolNamespace::Type),
    ] {
        assert!(
            matches!(
                resolve_project_export(
                    &db,
                    Utf8Path::new("/barrel.ts"),
                    name,
                    facet,
                    ProjectResolutionContext::default()
                ),
                ProjectExportResolution::Found(_)
            ),
            "{name} {facet:?}"
        );
    }
}

#[test]
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
