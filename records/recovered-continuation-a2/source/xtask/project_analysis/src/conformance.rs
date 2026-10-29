use biome_project_analysis::coverage::CapabilityStatus;
use serde::{Serialize, Serializer};

pub struct CaseOutcome {
    pub capability: CapabilityStatus,
    pub matches: bool,
}

impl Serialize for CaseOutcome {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self.capability {
            CapabilityStatus::Unsupported => "unsupported",
            CapabilityStatus::Untested => "untested",
            CapabilityStatus::Supported if self.matches => "passed",
            CapabilityStatus::Supported => "failed",
        })
    }
}

use crate::artifact::{InstalledOracle, install, sha256};
use anyhow::{Context, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub const CASES: [&str; 3] = ["orphan-file", "namespace-known-key", "production-defaults"];

pub fn normalize_oracle(report: &Value) -> anyhow::Result<Value> {
    let rows = report["issues"]
        .as_array()
        .context("oracle report must contain an issues array")?;
    let mut issues = Vec::new();
    let mut enabled = BTreeSet::new();
    for row in rows {
        let path = row["file"]
            .as_str()
            .context("oracle issue row must contain a file")?;
        for (kind, items) in row
            .as_object()
            .context("oracle issue row must be an object")?
        {
            if kind == "file" || kind == "owners" {
                continue;
            }
            enabled.insert(kind);
            for item in items
                .as_array()
                .context("oracle issue kind must contain an array")?
            {
                issues.push(json!({"kind":kind,"path":path,"symbol":item.get("name"),"severity":"error","location":{"pos":item.get("pos"),"line":item.get("line"),"col":item.get("col")},"details":item}));
            }
        }
    }
    issues.sort_by_cached_key(Value::to_string);
    Ok(
        json!({"issues":issues,"enabled_issue_kinds":enabled,"coverage":null,"gaps":null,"capabilities":null,"evidence":null,"unavailable":"The Knip JSON reporter does not expose coverage, capabilities or the evidence graph."}),
    )
}

fn profile_is_validated(observations: &[Value], identity: &Value) -> bool {
    CASES.iter().all(|name| {
        observations.iter().any(|observation| {
            observation["case"] == *name
                && observation["oracle_matches_expectation"] == true
                && observation["identity"] == *identity
        })
    })
}

fn fixture_digest(directory: &Path) -> anyhow::Result<String> {
    fn collect(
        directory: &Path,
        base: &Path,
        files: &mut Vec<(PathBuf, Vec<u8>)>,
    ) -> anyhow::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                collect(&entry.path(), base, files)?;
            } else {
                files.push((
                    entry.path().strip_prefix(base)?.to_path_buf(),
                    fs::read(entry.path())?,
                ));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    collect(directory, directory, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(sha256(&serde_json::to_vec(&files)?))
}

fn write_json(path: &Path, value: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn retain_output(
    output: &Output,
    observation: &mut Value,
    pending_path: &Path,
    normalize: impl FnOnce(Value) -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    observation["exit_code"] = json!(output.status.code());
    observation["stdout"] = json!(String::from_utf8_lossy(&output.stdout));
    observation["stderr"] = json!(String::from_utf8_lossy(&output.stderr));
    if std::str::from_utf8(&output.stdout).is_err() {
        observation["stdout_bytes"] = json!(output.stdout);
    }
    if std::str::from_utf8(&output.stderr).is_err() {
        observation["stderr_bytes"] = json!(output.stderr);
    }
    write_json(pending_path, observation)?;
    let normalized = serde_json::from_slice(&output.stdout)
        .context("process did not emit JSON")
        .and_then(normalize);
    if let Err(error) = &normalized {
        observation["normalization_error"] = json!(format!("{error:#}"));
        write_json(pending_path, observation)?;
    }
    normalized
}

pub fn run(
    name: &str,
    oracle_only: bool,
    engine: Option<&Path>,
    artifact: Option<&Path>,
) -> anyhow::Result<()> {
    run_at_root(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        name,
        oracle_only,
        engine,
        artifact,
    )
}

fn run_at_root(
    root: &Path,
    name: &str,
    oracle_only: bool,
    engine: Option<&Path>,
    artifact: Option<&Path>,
) -> anyhow::Result<()> {
    ensure!(CASES.contains(&name), "unknown conformance case: {name}");
    let fixture = root.join("fixtures").join(name);
    let observations_dir = root.join("observations");
    fs::create_dir_all(&observations_dir)?;
    if !oracle_only {
        let engine = engine
            .context("provide --oracle-only or --engine <binary>")?
            .canonicalize()?;
        let output = Command::new(&engine)
            .args(["--case", name])
            .current_dir(&fixture)
            .output()?;
        let outcome = CaseOutcome {
            capability: CapabilityStatus::Untested,
            matches: false,
        };
        let mut observation = json!({"case":name,"command":[engine.to_string_lossy(),"--case",name],"cwd":fixture,"outcome":outcome,"reason":"No reviewed native expectations or engine capabilities exist yet."});
        let pending_path = observations_dir.join(format!("{name}.native.pending.json"));
        observation["normalized"] = retain_output(&output, &mut observation, &pending_path, Ok)?;
        write_json(&pending_path, &observation)?;
        bail!("native conformance is untested; output retained without a parity claim");
    }
    ensure!(
        engine.is_none(),
        "--engine and --oracle-only are mutually exclusive"
    );
    let oracle = install(root, artifact)?;
    run_oracle(root, name, oracle)
}

fn run_oracle(root: &Path, name: &str, oracle: InstalledOracle) -> anyhow::Result<()> {
    let fixture = root.join("fixtures").join(name);
    let observations_dir = root.join("observations");
    let mut args = vec![
        "--config",
        "knip.json",
        "--reporter",
        "json",
        "--no-progress",
    ];
    if name == "production-defaults" {
        args.push("--production");
    }
    let identity = json!({"artifact":oracle.identity,"executable":oracle.executable,"executable_sha256":sha256(&fs::read(&oracle.executable)?),"node_executable":oracle.node,"node_sha256":sha256(&fs::read(&oracle.node)?)});
    let command: Vec<_> = [
        oracle.node.to_string_lossy().into_owned(),
        oracle.executable.to_string_lossy().into_owned(),
    ]
    .into_iter()
    .chain(args.iter().map(|arg| (*arg).to_owned()))
    .collect();
    let mut observation = json!({"case":name,"identity":identity,"command":command,"cwd":fixture,"environment":{"NO_COLOR":"1","NODE_OPTIONS":null,"NODE_PATH":null},"installation":oracle.installation,"fixture_digest":fixture_digest(&fixture)?,"native_status":"untested"});
    let pending_path = observations_dir.join(format!("{name}.pending.json"));
    write_validation(root, &identity, false)?;
    write_json(&pending_path, &observation)?;
    let output = Command::new(&oracle.node)
        .arg(&oracle.executable)
        .args(&args)
        .current_dir(&fixture)
        .env("NO_COLOR", "1")
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .output()?;
    observation["normalized"] = retain_output(&output, &mut observation, &pending_path, |value| {
        normalize_oracle(&value)
    })?;
    let expectation_path = root
        .join("expectations/oracle")
        .join(format!("{name}.json"));
    let expected: Option<Value> = if expectation_path.is_file() {
        Some(serde_json::from_slice(&fs::read(&expectation_path)?)?)
    } else {
        None
    };
    let matches = expected.as_ref().is_some_and(|expected| {
        expected["normalized"] == observation["normalized"]
            && expected["exit_code"] == observation["exit_code"]
            && expected["stderr"] == observation["stderr"]
    });
    observation["oracle_matches_expectation"] = json!(matches);
    let suffix = if matches { "json" } else { "pending.json" };
    write_json(
        &observations_dir.join(format!("{name}.{suffix}")),
        &observation,
    )?;
    if matches {
        fs::remove_file(&pending_path)?;
    }
    let mut observations = Vec::new();
    for case in CASES {
        let path = observations_dir.join(format!("{case}.json"));
        if path.is_file()
            && !observations_dir
                .join(format!("{case}.pending.json"))
                .exists()
        {
            let observed: Value = serde_json::from_slice(&fs::read(path)?)?;
            let expectation: Value = serde_json::from_slice(&fs::read(
                root.join("expectations/oracle")
                    .join(format!("{case}.json")),
            )?)?;
            if observed["fixture_digest"] == fixture_digest(&root.join("fixtures").join(case))?
                && observed["normalized"] == expectation["normalized"]
                && observed["exit_code"] == expectation["exit_code"]
                && observed["stderr"] == expectation["stderr"]
            {
                observations.push(observed);
            }
        }
    }
    let validated = matches && profile_is_validated(&observations, &identity);
    write_validation(root, &identity, validated)?;
    ensure!(
        matches,
        "oracle output needs review or differs from expectation; retained {name}.pending.json"
    );
    println!(
        "retained oracle observation {name}; profile validated: {validated}; native parity: untested"
    );
    Ok(())
}

fn write_validation(root: &Path, identity: &Value, validated: bool) -> anyhow::Result<()> {
    write_json(
        &root.join("oracle/validation.json"),
        &json!({"profile":"oracle-smoke-3","identity":identity,"validated":validated,"cases":CASES,"native_status":"untested"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_oracle_process_retains_output_and_invalidates_validation() {
        let directory = tempfile::tempdir().unwrap();
        for path in ["fixtures/orphan-file", "observations", "oracle"] {
            fs::create_dir_all(directory.path().join(path)).unwrap();
        }
        let validation_path = directory.path().join("oracle/validation.json");
        write_json(&validation_path, &json!({"validated": true})).unwrap();
        let executable = std::env::current_exe().unwrap();
        let oracle = InstalledOracle {
            identity: crate::artifact::ArtifactIdentity {
                package: "test-oracle".into(),
                version: "1".into(),
                integrity: "test".into(),
                dependency_lock_digest: "test".into(),
                runtime_version: "test".into(),
            },
            node: executable.clone(),
            executable,
            installation: json!({"exit_code": 0}),
        };
        assert!(run_oracle(directory.path(), "orphan-file", oracle).is_err());
        let observation: Value = serde_json::from_slice(
            &fs::read(
                directory
                    .path()
                    .join("observations/orphan-file.pending.json"),
            )
            .expect("failed oracle output must be retained"),
        )
        .unwrap();
        assert_eq!(observation["stdout"], "");
        assert!(observation["stderr"].as_str().unwrap().contains("config"));
        assert_ne!(observation["exit_code"], json!(0));
        assert_eq!(
            observation["identity"]["artifact"]["package"],
            "test-oracle"
        );
        assert_eq!(observation["installation"]["exit_code"], 0);
        assert!(observation["normalization_error"].as_str().is_some());
        let validation: Value =
            serde_json::from_slice(&fs::read(validation_path).unwrap()).unwrap();
        assert_eq!(validation["validated"], false);
    }

    #[test]
    fn failed_native_process_retains_raw_output_and_parse_error() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("fixtures/orphan-file")).unwrap();
        let executable = std::env::current_exe().unwrap();
        let result = run_at_root(
            directory.path(),
            "orphan-file",
            false,
            Some(&executable),
            None,
        );
        assert!(result.is_err());
        let observation: Value = serde_json::from_slice(
            &fs::read(
                directory
                    .path()
                    .join("observations/orphan-file.native.pending.json"),
            )
            .expect("failed process observation must be retained"),
        )
        .unwrap();
        assert_eq!(observation["stdout"], "");
        assert!(observation["stderr"].as_str().unwrap().contains("case"));
        assert_ne!(observation["exit_code"], json!(0));
        assert!(observation["normalization_error"].as_str().is_some());
    }

    #[test]
    fn oracle_normalization_errors_preserve_process_provenance() {
        let directory = tempfile::tempdir().unwrap();
        let pending_path = directory.path().join("oracle.pending.json");
        let failed = Command::new(std::env::current_exe().unwrap())
            .args(["--case", "orphan-file"])
            .output()
            .unwrap();
        assert!(!failed.status.success());
        for stdout in [
            b"not JSON\n".as_slice(),
            br#"{"error":"invalid config"}"#,
            &[0xff],
        ] {
            let output = Output {
                status: failed.status,
                stdout: stdout.to_vec(),
                stderr: b"configuration failed\n\xff".to_vec(),
            };
            let mut observation = json!({
                "command": ["oracle", "--reporter", "json"],
                "identity": {"artifact": "pinned"},
                "installation": {"exit_code": 0}
            });
            assert!(
                retain_output(&output, &mut observation, &pending_path, |value| {
                    normalize_oracle(&value)
                })
                .is_err()
            );
            let retained: Value =
                serde_json::from_slice(&fs::read(&pending_path).unwrap()).unwrap();
            assert_eq!(retained["command"], json!(["oracle", "--reporter", "json"]));
            assert_eq!(retained["identity"]["artifact"], "pinned");
            assert_eq!(retained["installation"]["exit_code"], 0);
            assert_eq!(retained["exit_code"], json!(output.status.code()));
            assert_eq!(retained["stdout"], String::from_utf8_lossy(stdout).as_ref());
            assert_eq!(retained["stderr_bytes"], json!(output.stderr));
            assert!(retained["normalization_error"].as_str().is_some());
            if std::str::from_utf8(stdout).is_err() {
                assert_eq!(retained["stdout_bytes"], json!(stdout));
            }
        }
    }

    #[test]
    fn unsupported_is_not_success() {
        let outcome = CaseOutcome {
            capability: CapabilityStatus::Unsupported,
            matches: true,
        };
        assert_eq!(serde_json::to_value(outcome).unwrap(), "unsupported");
    }

    #[test]
    fn untested_is_not_success() {
        let outcome = CaseOutcome {
            capability: CapabilityStatus::Untested,
            matches: true,
        };
        assert_eq!(serde_json::to_value(outcome).unwrap(), "untested");
    }
}

#[cfg(test)]
mod normalization_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn oracle_normalization_retains_empty_categories_and_locations() {
        let report = json!({"issues": [{"file":"src/lib.ts", "exports":[{"name":"unused", "line":2,"col":14,"pos":35}],"types":[]}]});
        let normalized = normalize_oracle(&report).unwrap();
        assert_eq!(
            normalized["enabled_issue_kinds"],
            json!(["exports", "types"])
        );
        assert_eq!(normalized["issues"][0]["kind"], "exports");
        assert_eq!(normalized["issues"][0]["path"], "src/lib.ts");
        assert_eq!(normalized["issues"][0]["symbol"], "unused");
        assert_eq!(normalized["issues"][0]["location"]["pos"], 35);
        assert!(normalized["coverage"].is_null());
        assert!(normalized["evidence"].is_null());
        assert!(normalize_oracle(&json!({"unrelated":[]})).is_err());
    }

    #[test]
    fn oracle_validation_requires_all_profile_cases_and_matching_identity() {
        assert!(!profile_is_validated(&[], &json!({"pin": 1})));
        let observations: Vec<_> = CASES
            .iter()
            .map(
                |name| json!({"case": name,"oracle_matches_expectation":true,"identity":{"pin":1}}),
            )
            .collect();
        assert!(profile_is_validated(&observations, &json!({"pin": 1})));
        assert!(!profile_is_validated(
            &observations[..2],
            &json!({"pin": 1})
        ));
        assert!(!profile_is_validated(&observations, &json!({"pin": 2})));
    }
}
