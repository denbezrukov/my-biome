            vec![
                ("index.ts", "import * as NS from './provider'; consume(NS);"),
                ("provider.ts", "export const unused=1;"),
            ],
            vec![
                ("index.ts", "import * as NS from './provider'; NS.unused;"),
                ("provider.ts", "export const unused=1;"),
            ],
        ),
        (
            NsTypes,
            vec![
                (
                    "index.ts",
                    "import * as NS from './provider'; type Shape = typeof NS;",
                ),
                ("provider.ts", "export interface Item {}"),
            ],
            vec![
                (
                    "index.ts",
                    "import type {Item} from './provider'; type Shape = Item;",
                ),
                ("provider.ts", "export interface Item {}"),
            ],
        ),
        (
            Duplicates,
            vec![
                ("index.ts", "import './provider';"),
                (
                    "provider.ts",
                    "export function same() {} export default same;",
                ),
            ],
            vec![
                ("index.ts", "import './provider';"),
                ("provider.ts", "export function same() {}"),
            ],
        ),
        (
            EnumMembers,
            vec![
                ("index.ts", "import {Level} from './provider'; Level.used;"),
                ("provider.ts", "export enum Level { used, unused }"),
            ],
            vec![
                (
                    "index.ts",
                    "import {Level} from './provider'; Level.used; Level.unused;",
                ),
                ("provider.ts", "export enum Level { used, unused }"),
            ],
        ),
        (
            NamespaceMembers,
            vec![
                ("index.ts", "import {NS} from './provider'; NS.used;"),
                (
                    "provider.ts",
                    "export namespace NS { export const used=1; export const unused=2; }",
                ),
            ],
            vec![
                (
                    "index.ts",
                    "import {NS} from './provider'; NS.used; NS.unused;",
                ),
                (
                    "provider.ts",
                    "export namespace NS { export const used=1; export const unused=2; }",
                ),
            ],
        ),
        (
            Catalog,
            vec![(
                "package.json",
                r#"{"private":true,"catalog":{"unused":"1"}}"#,
            )],
            vec![(
                "package.json",
                r#"{"private":true,"catalog":{"used":"1"},"dependencies":{"used":"catalog:"}}"#,
            )],
        ),
        (
            CatalogReferences,
            vec![(
                "package.json",
                r#"{"private":true,"dependencies":{"missing":"catalog:"}}"#,
            )],
            vec![(
                "package.json",
                r#"{"private":true,"catalog":{"missing":"1"},"dependencies":{"missing":"catalog:"}}"#,
            )],
        ),
        (
            Cycles,
            vec![
                ("index.ts", "import './provider';"),
                ("provider.ts", "import './index';"),
            ],
            vec![("index.ts", "import './provider';"), ("provider.ts", "")],
        ),
    ];
    assert_eq!(cases.len(), IssueKind::ALL.len());
    for (kind, positive, negative) in cases {
        let config = json!({"include":[kind]});
        let full = run(&positive, config.clone(), AnalysisMode::Default);
        assert!(
            full.issues.iter().any(|issue| issue.kind == kind),
            "positive {kind:?}: {:?}",
            full.issues
        );
        let clean = run(&negative, config, AnalysisMode::Default);
        assert!(
            !clean.issues.iter().any(|issue| issue.kind == kind),
            "negative {kind:?}"
        );
        for selection in [
            ReportSelection {
                workspaces: [biome_project_analysis::model::WorkspaceId(999)].into(),
                ..Default::default()
            },
            ReportSelection {
                restrict_issues: true,
                ..Default::default()
            },
            ReportSelection {
                changed_paths: Some(Default::default()),
                ..Default::default()
            },
            ReportSelection {
                paths: vec!["not-selected".into()],
                ..Default::default()
            },
        ] {
            let selected = select_report(&full, &selection);
            assert!(selected.issues.is_empty(), "selection {kind:?}");
            assert_eq!(full.coverage, selected.coverage);
            assert_eq!(full.evidence, selected.evidence);
            assert_eq!(full.capabilities, selected.capabilities);
            assert_eq!(full.membership, selected.membership);
            assert_eq!(full.dependencies, selected.dependencies);
            assert_eq!(full.catalogs, selected.catalogs);
        }
    }
}
