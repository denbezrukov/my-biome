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
