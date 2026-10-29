use biome_project_analysis::{
    inventory::Inventory,
    model::{FileId, Location, WorkspaceId},
    options::AnalysisMode,
    scripts::{ScriptProducer, extract_script_uses},
};
use serde_json::json;
use std::collections::BTreeMap;

fn inventory(body: &str) -> Inventory {
    serde_json::from_value(json!({"files":[],"workspaces":[{"workspace":0,"path":".","manifest":0,"declaration_locations":[],"metadata_diagnostics":[],"metadata":[["scripts",{"value":{"start":body},"location":{"file":0,"range":[0,100]},"elements":[],"members":[["start",{"value":body,"location":{"file":0,"range":[10,90]},"members":[],"elements":[]}]]}]]}],"roots":[],"declarations":[]})).unwrap()
}
#[test]
fn static_shell_composition() {
    let inv = inventory(
        r#"X=ok node 'one file.js' && (node two\ file.js | node three.js > out) || node -- four.js; node $(touch MARKER); node $UNKNOWN"#,
    );
    let script_marker_before = serde_json::to_vec(&inv).unwrap();
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Default);
    let names: Vec<_> = usage.targets.iter().map(|t| t.specifier.as_str()).collect();
    assert_eq!(
        names,
        ["./one file.js", "./two file.js", "./three.js", "./four.js"]
    );
    assert!(!usage.gaps.is_empty());
    let script_marker_after = serde_json::to_vec(&inv).unwrap();
    assert_eq!(script_marker_before, script_marker_after);
    assert!(usage.targets.iter().all(|t| t.location
        == Location {
            file: FileId(0),
            range: biome_text_size::TextRange::new(10.into(), 90.into())
        }));
}
#[test]
fn node_preload_entries() {
    let usage = extract_script_uses(
        &inventory(
            "node --import=tsx --require ./setup.cjs app.js; node -e 'touch MARKER'; node -p 'process.exit()'",
        ),
        &BTreeMap::new(),
        AnalysisMode::Production,
    );
    assert_eq!(
        usage
            .targets
            .iter()
            .map(|t| t.specifier.as_str())
            .collect::<Vec<_>>(),
        ["tsx", "./setup.cjs", "./app.js"]
    );
    assert!(usage.packages.iter().any(|p| p.package_name == "tsx"));
    assert_eq!(usage.gaps.len(), 2);
}
#[test]
fn script_catalog_references() {
    let usage = extract_script_uses(
        &inventory("pnpm dlx sherif@catalog:; yarn dlx @scope/tool@catalog:tools"),
        &BTreeMap::new(),
        AnalysisMode::Default,
    );
    assert_eq!(usage.catalogs.len(), 2);
    assert_eq!(usage.catalogs[0].package_name, "sherif");
    assert_eq!(usage.catalogs[0].catalog_name, None);
    assert_eq!(usage.catalogs[1].catalog_name.as_deref(), Some("tools"));
    assert_eq!(usage.catalogs[1].package_name, "@scope/tool");
    assert_eq!(
        usage.catalogs[1].producer,
        ScriptProducer::Manifest {
            workspace: WorkspaceId(0),
            script: "start".into()
        }
    );
    assert!(usage.packages.iter().all(|p| p.optional));
}
#[test]
fn recursive_scripts_terminate() {
    let mut inv = inventory("npm run start && pnpm start && yarn run start");
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Default);
    assert!(usage.targets.is_empty());
    inv.workspaces[0].metadata[0].1.members[0].1.value = json!("npm run start -- --flag");
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Default);
    assert!(usage.targets.is_empty());
}
#[test]
fn binary_provider_differs_from_name() {
    let mut inv = inventory("boxen text");
    inv.binary_providers = serde_json::from_value(json!([{"workspace":0,"binary":"boxen","package_name":"boxen-cli","manifest":1,"path":"node_modules/boxen-cli/package.json","available":true}])).unwrap();
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Default);
    assert_eq!(usage.packages[0].package_name, "boxen-cli");
    assert!(!usage.packages.iter().any(|p| p.package_name == "boxen"));
    assert!(!usage.gaps.is_empty());
}
#[test]
fn tool_specific_arguments_are_visible_gaps() {
    let usage = extract_script_uses(
        &inventory("tsc -p tsconfig.json; nodemon app.js; unknown-command"),
        &BTreeMap::new(),
        AnalysisMode::Default,
    );
    assert!(usage.targets.is_empty());
    assert!(!usage.gaps.is_empty());
    assert!(usage.packages.is_empty());
}

#[test]
fn recursive_scripts_retain_nonempty_targets_and_workspace_cwd() {
    let mut inv = inventory(
        "npm run start && node root.js; cd child && node local.js; yarn workspace child run start",
    );
    let mut child = inventory("pnpm start && node child.js")
        .workspaces
        .remove(0);
    child.workspace = WorkspaceId(1);
    child.path = "child".into();
    child.name = Some("child".into());
    inv.workspaces.push(child);
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Production);
    assert!(
        usage
            .targets
            .iter()
            .any(|t| t.specifier == "./root.js" && t.cwd.as_str() == ".")
    );
    assert!(
        usage
            .targets
            .iter()
            .any(|t| t.specifier == "./local.js" && t.cwd.as_str() == "child")
    );
    assert!(usage.targets.iter().any(|t| t.specifier == "./child.js"
        && t.workspace == WorkspaceId(1)
        && t.cwd.as_str() == "child"));
    assert!(usage.targets.len() < 10);
}

#[test]
fn on_demand_names_are_not_assumed_package_providers() {
    let usage = extract_script_uses(
        &inventory(
            "npx boxen text; npx boxen-cli@1; npm exec --package sherif@catalog:tools -- sherif; pnpm exec node local.js",
        ),
        &BTreeMap::new(),
        AnalysisMode::Default,
    );
    assert!(!usage.packages.iter().any(|p| p.package_name == "boxen"));
    assert!(usage.binaries.iter().any(|b| b.binary == "boxen"));
    assert!(
        usage
            .packages
            .iter()
            .any(|p| p.package_name == "boxen-cli" && p.optional)
    );
    assert!(usage.catalogs.iter().any(|c| c.package_name == "sherif"));
    assert!(usage.targets.iter().any(|t| t.specifier == "./local.js"));
}
#[test]
fn forwarded_argv_preserves_literal_shell_characters() {
    let mut inv = inventory("npm run task -- 'semi;colon.js'");
    let mut task = inv.workspaces[0].metadata[0].1.members[0].1.clone();
    task.value = json!("node");
    inv.workspaces[0].metadata[0]
        .1
        .members
        .push(("task".into(), task));
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Production);
    assert_eq!(
        usage
            .targets
            .iter()
            .map(|t| t.specifier.as_str())
            .collect::<Vec<_>>(),
        ["./semi;colon.js"]
    );
}

#[test]
fn node_options_assignment_preserves_quoted_preload() {
    let usage = extract_script_uses(
        &inventory("NODE_OPTIONS=\"--require './setup file.cjs'\" node app.js"),
        &BTreeMap::new(),
        AnalysisMode::Default,
    );
    assert_eq!(
        usage
            .targets
            .iter()
            .map(|t| t.specifier.as_str())
            .collect::<Vec<_>>(),
        ["./setup file.cjs", "./app.js"]
    );
}

#[test]
fn source_command_adapter_gaps_are_explicit() {
    use biome_project_analysis::model::ModuleFacts;
    let mut inv = inventory("");
    inv.files =
        serde_json::from_value(json!([{"file":2,"workspace":0,"role":"reportingCandidate"}]))
            .unwrap();
    let mut module:ModuleFacts=serde_json::from_value(json!({"edges":[],"declarations":[],"exports":[],"uses":[],"tags":[],"collected":{"parse_status":"Valid","declaration_file":null,"has_static_exports":false,"has_module_marker":true,"declarations":[],"uses":[],"imports":[{"ordinal":0,"specifier":"execa","location":{"file":2,"range":[0,10]},"specifier_location":{"file":2,"range":[0,10]},"syntax":"Import","type_only":false,"optional":false,"bindings":[],"type_usage":null}],"exports":[],"registrations":[],"unknown":[],"resolutions":[]}})).unwrap();
    let usage = extract_script_uses(
        &inv,
        &[(FileId(2), module.clone())].into(),
        AnalysisMode::Default,
    );
    assert!(
        usage
            .gaps
            .iter()
            .any(|g| g.producer == ScriptProducer::Source { file: FileId(2) })
    );
    module.collected.as_mut().unwrap().imports[0].specifier = "zx".into();
    assert!(
        !extract_script_uses(&inv, &[(FileId(2), module)].into(), AnalysisMode::Default)
            .gaps
            .is_empty()
    );
}

#[test]
fn unknown_shell_forms_do_not_invent_empty_completeness() {
    for body in [
        "node",
        "node $ENTRY",
        "node `printf app.js`",
        "node $(echo app.js)",
        "node 'broken",
        "node app*.js",
        "node <(echo script)",
    ] {
        let usage = extract_script_uses(&inventory(body), &BTreeMap::new(), AnalysisMode::Default);
        assert!(!usage.gaps.is_empty(), "{body}");
    }
}

#[test]
fn static_cwd_keeps_parent_components_and_workspace_ownership() {
    let mut inv = inventory("cd ../../outside && node app.js");
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Default);
    assert_eq!(usage.targets[0].cwd.as_str(), "../../outside");
    inv.workspaces[0].metadata[0].1.members[0].1.value = json!("cd child && npm run start");
    let mut child = inventory("node child.js").workspaces.remove(0);
    child.workspace = WorkspaceId(1);
    child.path = "child".into();
    child.name = Some("child".into());
    inv.workspaces.push(child);
    let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Production);
    assert!(usage.targets.iter().any(|t| t.specifier == "./child.js"
        && t.producer
            == ScriptProducer::Manifest {
                workspace: WorkspaceId(0),
                script: "start".into()
            }
        && t.workspace == WorkspaceId(1)));
}

#[test]
fn shell_newlines_comments_and_descriptor_adjacency() {
    let usage = extract_script_uses(
        &inventory(
            "node first.js\nnode second.js # node ignored.js\nnode 123 > out; node app.js 2> err",
        ),
        &BTreeMap::new(),
        AnalysisMode::Default,
    );
    assert_eq!(
        usage
            .targets
            .iter()
            .map(|t| t.specifier.as_str())
            .collect::<Vec<_>>(),
        ["./first.js", "./second.js", "./123", "./app.js"]
    );
}

#[test]
fn b4_review_pipeline_cwd_is_local_to_each_command() {
    for (body, expected) in [
        ("cd child | node app.js; node after.js", vec![".", "."]),
        ("node app.js | cd child; node after.js", vec![".", "."]),
        (
            "cd child && node app.js | node other.js; node after.js",
            vec!["child", "child", "child"],
        ),
        (
            "cd child | (cd nested; node app.js); node after.js",
            vec!["nested", "."],
        ),
    ] {
        let usage = extract_script_uses(&inventory(body), &BTreeMap::new(), AnalysisMode::Default);
        assert!(usage.gaps.is_empty(), "{body}: {:?}", usage.gaps);
        assert_eq!(
            usage
                .targets
                .iter()
                .map(|t| t.cwd.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{body}"
        );
    }
}

#[test]
fn b4_review_node_separator_keeps_stdin_uncertainty() {
    for body in ["node --", "node -- -"] {
        let usage = extract_script_uses(&inventory(body), &BTreeMap::new(), AnalysisMode::Default);
        assert!(!usage.gaps.is_empty(), "{body}");
        assert!(usage.targets.is_empty(), "{body}");
    }
    let usage = extract_script_uses(
        &inventory("node -- app.js"),
        &BTreeMap::new(),
        AnalysisMode::Default,
    );
    assert!(usage.gaps.is_empty());
    assert_eq!(usage.targets[0].specifier, "./app.js");
}

#[test]
fn b4_review_nested_yarn_workspace_is_bounded() {
    for (prefixes, limited) in [(2, false), (66, true)] {
        let mut inv = inventory(&format!(
            "yarn {}run task",
            "workspace child ".repeat(prefixes)
        ));
        let mut child = inventory("node app.js").workspaces.remove(0);
        child.workspace = WorkspaceId(1);
        child.path = "child".into();
        child.name = Some("child".into());
        child.metadata[0].1.members[0].0 = "task".into();
        inv.workspaces.push(child);
        let usage = extract_script_uses(&inv, &BTreeMap::new(), AnalysisMode::Production);
        assert_eq!(!usage.gaps.is_empty(), limited, "prefixes={prefixes}");
        assert_eq!(usage.targets.is_empty(), limited);
        if !limited {
            assert_eq!(usage.targets[0].cwd.as_str(), "child");
        }
    }
}
