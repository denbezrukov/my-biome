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
