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
