
#[test]
fn b7_all_seventeen_categories_compose_with_policy() {
    use IssueKind::*;
    use biome_project_analysis::{options::ReportSelection, select_report};
    type Sources = Vec<(&'static str, &'static str)>;
    let cases: Vec<(IssueKind, Sources, Sources)> = vec![
        (
            Files,
            vec![("orphan.ts", "export const unused=1;")],
            vec![
                ("index.ts", "import './orphan';"),
                ("orphan.ts", "export const unused=1;"),
            ],
        ),
        (
            Dependencies,
            vec![(
                "package.json",
                r#"{"private":true,"dependencies":{"unused":"1"}}"#,
            )],
            vec![("package.json", r#"{"private":true,"dependencies":{}}"#)],
        ),
        (
            DevDependencies,
            vec![(
                "package.json",
                r#"{"private":true,"devDependencies":{"unused":"1"}}"#,
            )],
            vec![("package.json", r#"{"private":true,"devDependencies":{}}"#)],
        ),
        (
            OptionalPeerDependencies,
            vec![
                (
                    "package.json",
                    r#"{"private":true,"peerDependencies":{"peer":"1"},"peerDependenciesMeta":{"peer":{"optional":true}}}"#,
                ),
                ("index.ts", "import 'peer';"),
            ],
