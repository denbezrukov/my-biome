
#[test]
fn dependencies_derived_unknown_provider_follows_producer_activation() {
    use biome_project_analysis::{
        coverage::GapReason,
        dependencies::DependencyProducer,
        facts::{ResolutionEvent, ResolutionOutcome, ResolutionPurpose},
    };
    for activation in ["definite", "possible", "dead", "public"] {
        let entry = match activation {
            "definite" => "import './src/consumer.js';",
            "possible" => "import(flag ? './src/consumer.js' : './other.js');",
            _ => "export {};",
        };
        let manifest = if activation == "public" {
            r#"{"publishConfig":{"types":"src/consumer.d.ts"},"dependencies":{"host":"*","peer":"*","leaf":"*"}}"#
        } else {
            r#"{"private":true,"dependencies":{"host":"*","peer":"*","leaf":"*"}}"#
        };
        let (snapshot, result) = run(
            &[
                ("package.json", manifest),
                ("index.ts", entry),
                ("other.js", "export {};"),
                ("src/consumer.js", "import 'host';"),
                ("src/consumer.d.ts", "export type {Shape} from 'host';"),
                (
                    "node_modules/host/package.json",
                    r#"{"name":"host","main":"index.js","types":"index.d.ts","peerDependencies":{"peer":"*"}}"#,
                ),
                ("node_modules/host/index.js", "export {};"),
                ("node_modules/host/index.d.ts", "export interface Shape {}"),
                (
                    "node_modules/peer/package.json",
                    r#"{"name":"outer","main":"index.js","peerDependencies":{"leaf":"*"}}"#,
                ),
                ("node_modules/peer/index.js", "export {};"),
                (
                    "src/node_modules/peer/package.json",
                    r#"{"name":"inner","main":"index.js","peerDependencies":false}"#,
                ),
                ("src/node_modules/peer/index.js", "export {};"),
            ],
            json!({"entry":["index.ts!"],"project":["**/*.ts!","**/*.js!"]}),
            AnalysisMode::Default,
        );
        assert!(snapshot.inventory.package_providers.iter().any(|p| {
            p.consumer.is_some()
                && p.requested_package == "peer"
                && p.resolutions
                    .iter()
                    .any(|r| matches!(r.outcome, ResolutionOutcome::Unknown { .. }))
        }));
        let leaf = result
            .dependencies
            .declarations
            .iter()
            .find(|d| d.declaration.package == "leaf")
            .unwrap();
        assert!(leaf.used.is_empty() && leaf.retained.is_empty());
        if activation == "dead" {
            assert!(result.dependencies.gaps.is_empty());
            assert!(result.dependencies.uses.is_empty());
            assert!(leaf.affecting_gaps.is_empty());
            assert_eq!(packages(&result).len(), 3);
            continue;
        }
        assert!(
            !packages(&result).iter().any(|(_, _, name)| name == "leaf"),
            "{activation}: {:?}",
            packages(&result)
        );
        assert!(!leaf.affecting_gaps.is_empty());
        let peer = result
            .dependencies
            .uses
            .iter()
            .find(|u| u.package == "peer")
            .unwrap();
        assert_eq!(peer.definite, activation != "possible");
        assert!(peer.optional && peer.type_only);
        assert!(
            peer.resolutions
                .iter()
                .any(|r| matches!(r.outcome, ResolutionOutcome::Unknown { .. }))
        );
        assert!(peer.resolutions.iter().flat_map(|r|&r.trace).any(|e|matches!(e,ResolutionEvent::Manifest{file,known:false,..} if snapshot.sources[file].logical_path=="src/node_modules/peer/package.json") || matches!(e,ResolutionEvent::Manifest{file,diagnostics,..} if snapshot.sources[file].logical_path=="src/node_modules/peer/package.json" && !diagnostics.is_empty())));
        assert!(
            result
                .dependencies
                .gaps
                .iter()
                .any(|g| g.location == Some(peer.location)
                    && matches!(g.reason, GapReason::UnavailableInput(_)))
        );
        if activation == "public" {
            assert!(matches!(
                peer.producer,
                DependencyProducer::PublicOutput { .. }
            ));
            assert!(
                peer.resolutions
                    .iter()
                    .all(|r| r.context.purpose == ResolutionPurpose::PublicOutput)
            );
        } else {
            assert!(matches!(peer.producer, DependencyProducer::Import { .. }));
        }
    }
}

#[test]
fn dependencies_provider_evidence_scales_with_needed_closure() {
    for chained in [false, true] {
        for imports in [1, 4] {
            for unrelated in [2, 7] {
                let mut declarations = serde_json::Map::new();
                for name in ["host", "peer", "leaf"] {
                    declarations.insert(name.into(), json!("*"));
                }
                for i in 0..unrelated {
                    declarations.insert(format!("unrelated-{i}"), json!("*"));
                }
                let manifest = json!({"private":true,"dependencies":declarations}).to_string();
                let entry = "import 'host';\n".repeat(imports);
                let host = if chained {
                    r#"{"name":"host","main":"index.js","types":"index.d.ts","peerDependencies":{"peer":"*"}}"#
                } else {
                    r#"{"name":"host","main":"index.js","types":"index.d.ts"}"#
                };
                let (snapshot, _) = run(
                    &[
                        ("package.json", &manifest),
                        ("index.ts", &entry),
                        ("node_modules/host/package.json", host),
                        ("node_modules/host/index.js", "export {};"),
                        ("node_modules/host/index.d.ts", "export interface Shape {}"),
                        (
                            "node_modules/peer/package.json",
                            r#"{"name":"peer","main":"index.js","types":"index.d.ts","peerDependencies":{"leaf":"*"}}"#,
                        ),
                        ("node_modules/peer/index.js", "export {};"),
                        ("node_modules/peer/index.d.ts", "export {};"),
                        (
                            "node_modules/leaf/package.json",
                            r#"{"name":"leaf","main":"index.js","types":"index.d.ts","peerDependencies":{"host":"*"}}"#,
                        ),
                        ("node_modules/leaf/index.js", "export {};"),
                        ("node_modules/leaf/index.d.ts", "export {};"),
                    ],
                    json!({"entry":["index.ts!"],"project":["*.ts!"]}),
                    AnalysisMode::Default,
                );
                let providers = &snapshot.inventory.package_providers;
                assert_eq!(
                    providers.iter().filter(|p| p.consumer.is_none()).count(),
                    3 + unrelated
                );
                let consumers: Vec<_> = providers.iter().filter(|p| p.consumer.is_some()).collect();
                assert_eq!(
                    consumers.len(),
                    if chained { 3 * imports } else { 0 },
                    "chained={chained}, imports={imports}, unrelated={unrelated}"
                );
                assert!(
                    consumers
                        .iter()
                        .all(|p| matches!(p.requested_package.as_str(), "host" | "peer" | "leaf"))
                );
            }
        }
    }
}
