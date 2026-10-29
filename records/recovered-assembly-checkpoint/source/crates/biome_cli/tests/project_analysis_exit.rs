use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Project(PathBuf);

impl Project {
    fn new(configuration: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "biome-project-exit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let project = Self(path);
        project.write("biome.json", configuration);
        project.write("package.json", r#"{"name":"fixture","private":true}"#);
        project
    }

    fn write(&self, path: &str, content: &str) {
        std::fs::write(self.0.join(path), content).unwrap();
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_biome"))
            .args(arguments)
            .arg("--colors=off")
            .current_dir(&self.0)
            .env("XDG_CACHE_HOME", self.0.join("cache"))
            .env_remove("BIOME_CONFIG_PATH")
            .output()
            .unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn native_project_analysis_exit_codes() {
    let project = Project::new(r#"{"projectAnalysis":{"entry":[],"project":[]}}"#);
    let clean = project.run(&["analyze"]);
    let complete_clean_exit = clean.status.code().unwrap();
    assert_eq!(
        complete_clean_exit,
        0,
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );

    project.write(
        "biome.json",
        r#"{"projectAnalysis":{"entry":["index.js"],"project":["**/*.js"]}}"#,
    );
    project.write("provider.js", "export const value = 1;\n");
    project.write(
        "index.js",
        "import { value } from './provider.js';\nconsole.log(value);\n",
    );
    let resolved = project.run(&["analyze"]);
    assert_eq!(
        resolved.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    project.write("index.js", "import(process.env.MODULE);\n");
    let incomplete = project.run(&["analyze"]);
    let incomplete_exit = incomplete.status.code().unwrap();
    assert_eq!(incomplete_exit, 2);
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&incomplete.stdout),
        String::from_utf8_lossy(&incomplete.stderr)
    );
    assert!(output.contains("incomplete"), "{output}");
}

#[test]
fn disabled_local_exit_is_unchanged() {
    let project = Project::new(r#"{"linter":{"enabled":false}}"#);
    project.write("index.js", "const x = 1;\n");
    let before_disabled = project.run(&["lint", "index.js"]).status.code();
    project.write(
        "biome.json",
        r#"{"linter":{"enabled":false},"projectAnalysis":{"enabled":false}}"#,
    );
    let after_disabled = project.run(&["lint", "index.js"]).status.code();
    assert_eq!(before_disabled, after_disabled);
}

#[test]
fn findings_warnings_overrides_and_failures() {
    let project =
        Project::new(r#"{"projectAnalysis":{"entry":["index.js"],"project":["**/*.js"]}}"#);
    project.write("index.js", "console.log('entry');\n");
    project.write("orphan.js", "console.log('orphan');\n");
    for (args, expected) in [
        (vec!["analyze"], 1),
        (vec!["analyze", "--no-exit-code"], 0),
        (vec!["analyze", "--exclude=files"], 0),
        (vec!["analyze", "--workspace=fixture"], 1),
        (vec!["analyze", "--workspace=missing"], 0),
        (vec!["analyze", "--analysis-mode=invalid"], 2),
        (vec!["analyze", "--include=unknown", "--no-exit-code"], 2),
        (vec!["analyze", "--unknown", "--no-exit-code"], 2),
        (vec!["analyze", "--watch"], 2),
        (vec!["analyze", "--stdin-file-path=index.js"], 2),
        (vec!["analyze", "--reporter-file=."], 2),
        (vec!["analyze", "--config-path=missing.json"], 2),
    ] {
        let output = project.run(&args);
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if args == ["analyze", "--no-exit-code"] {
            assert!(String::from_utf8_lossy(&output.stderr).contains("coverage: complete"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("orphan.js"));
        }
    }
    project.write("biome.json", r#"{"projectAnalysis":{"entry":["index.js"],"project":["**/*.js"],"rules":{"files":"warn"}}}"#);
    assert_eq!(project.run(&["analyze"]).status.code(), Some(0));
    assert_eq!(
        project
            .run(&["analyze", "--error-on-warnings"])
            .status
            .code(),
        Some(1)
    );
    project.write("index.js", "import(process.env.MODULE);\n");
    let incomplete = project.run(&["analyze", "--no-exit-code", "--workspace=missing"]);
    assert_eq!(incomplete.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&incomplete.stderr).contains("coverage: incomplete"));

    project.write("biome.json", "{invalid json");
    assert_eq!(project.run(&["analyze"]).status.code(), Some(2));
    assert_eq!(project.run(&["lint", "--unknown"]).status.code(), Some(1));
}

#[test]
fn opted_in_local_commands_and_configuration_failures() {
    let project = Project::new(
        r#"{"formatter":{"enabled":false},"linter":{"enabled":false},"assist":{"enabled":false},"projectAnalysis":{"enabled":true,"entry":[],"project":[]}}"#,
    );
    for command in ["check", "lint"] {
        let output = project.run(&[command]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            project
                .run(&[command, "--analysis-mode=invalid"])
                .status
                .code(),
            Some(2)
        );
        assert_eq!(project.run(&[command, "--watch"]).status.code(), Some(2));
        assert_eq!(
            project.run(&[command, "--watch", "--write"]).status.code(),
            Some(2)
        );
        assert_eq!(
            project
                .run(&[command, "--stdin-file-path=index.js"])
                .status
                .code(),
            Some(2)
        );
        assert_eq!(
            project.run(&[command, "--reporter-file=."]).status.code(),
            Some(2)
        );
    }
}

#[test]
fn opted_in_local_findings_still_fail_with_project_override() {
    let project = Project::new(
        r#"{"formatter":{"enabled":false},"assist":{"enabled":false},"linter":{"rules":{"preset":"none","suspicious":{"noDebugger":"error"}}},"projectAnalysis":{"enabled":true,"entry":["index.js"],"project":["**/*.js"]}}"#,
    );
    project.write("index.js", "debugger;\n");
    for command in ["check", "lint"] {
        assert_eq!(
            project
                .run(&[command, "--no-exit-code", "index.js"])
                .status
                .code(),
            Some(1)
        );
        assert_eq!(project.run(&[command, "missing.js"]).status.code(), Some(2));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn startup_and_early_validation_follow_effective_enablement() {
    let project = Project::new(r#"{"projectAnalysis":{"enabled":false}}"#);
    project.write("enabled.json", r#"{"projectAnalysis":{"enabled":true}}"#);
    project.write("alternate.json", r#"{"extends":["./enabled.json"]}"#);
    project.write("invalid.json", "{invalid");
    for command in ["check", "lint"] {
        for early in [vec!["--use-server"], vec!["--watch", "--write"]] {
            let mut args = vec![command];
            args.extend(early.clone());
            assert_eq!(project.run(&args).status.code(), Some(1));
            args.push("--config-path=alternate.json");
            let enabled = project.run(&args);
            assert_eq!(
                enabled.status.code(),
                Some(2),
                "{args:?}: {}",
                String::from_utf8_lossy(&enabled.stderr)
            );
            let original_message = if early == ["--use-server"] {
                "No running instance of the Biome daemon server was found."
            } else {
                "--watch"
            };
            assert!(String::from_utf8_lossy(&enabled.stderr).contains(original_message));
            args.pop();
            args.push("--config-path=invalid.json");
            assert_eq!(project.run(&args).status.code(), Some(1));
        }
    }
    assert_eq!(
        project.run(&["analyze", "--use-server"]).status.code(),
        Some(2)
    );
}

#[test]
fn parser_classification_uses_command_grammar() {
    let project = Project::new("{}");
    assert_eq!(
        project.run(&["analyze", "--unknown"]).status.code(),
        Some(2)
    );
    assert_eq!(project.run(&["analyze", "--help"]).status.code(), Some(0));
    assert_eq!(
        project.run(&["lint", "--unknown", "analyze"]).status.code(),
        Some(1)
    );
    assert_eq!(
        project.run(&["--", "analyze", "--unknown"]).status.code(),
        Some(1)
    );
}

#[test]
fn configuration_hint_controls_do_not_change_coverage() {
    let project = Project::new(
        r#"{"projectAnalysis":{"entry":["missing*.ts"],"project":["nothing/**"],"treatConfigHintsAsErrors":true}}"#,
    );
    for (args, expected, hints) in [
        (vec!["analyze"], 1, 2),
        (vec!["analyze", "--no-exit-code"], 0, 2),
        (
            vec!["analyze", "--treat-config-hints-as-errors=false"],
            0,
            2,
        ),
        (vec!["analyze", "--configuration-hints=false"], 0, 0),
        (vec!["analyze", "--workspace=fixture"], 0, 0),
        (vec!["analyze", "--workspace=missing"], 0, 0),
        (
            vec![
                "analyze",
                "--analysis-mode=production",
                "--configuration-hints=true",
            ],
            0,
            0,
        ),
    ] {
        let output = project.run(&args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(expected), "{args:?}: {stderr}");
        assert!(
            stderr.contains(&format!("coverage: complete; 0 findings, {hints} hints")),
            "{args:?}: {stderr}"
        );
    }
}

fn runtime_failure_with_diagnostic_budget(command: &str, budget: &str) {
    const ENABLED: &str = r#"{"formatter":{"enabled":false},"assist":{"enabled":false},"linter":{"rules":{"preset":"none"}},"projectAnalysis":{"enabled":true,"entry":[],"project":[]}}"#;
    let project = Project::new(ENABLED);
    project.write("clean.js", "");
    project.write("unsupported.txt", "unsupported language\n");
    assert!(!project.0.join("missing.js").exists());

    let mut paths = Vec::new();
    if budget == "1" {
        // Explicit paths are evaluated in order, before parallel file processing.
        paths.push("unsupported.txt");
    }
    let mut clean_args = vec![command, "--verbose", "--max-diagnostics", budget];
    clean_args.extend(paths.iter().copied());
    clean_args.push("clean.js");
    let clean = project.run(&clean_args);
    let clean_stderr = String::from_utf8_lossy(&clean.stderr);
    assert_eq!(clean.status.code(), Some(0), "{clean_stderr}");
    assert!(clean_stderr.contains("coverage: complete; 0 findings, 0 hints"));
    if budget == "1" {
        assert!(
            clean_stderr.contains("files/missingHandler"),
            "{clean_stderr}"
        );
    }

    paths.push("missing.js");
    let mut visible_args = vec![command, "--verbose"];
    visible_args.extend(paths.iter().copied());
    let visible = project.run(&visible_args);
    let visible_stderr = String::from_utf8_lossy(&visible.stderr);
    assert_eq!(visible.status.code(), Some(2), "{visible_stderr}");
    assert!(
        visible_stderr.contains("internalError/io"),
        "{visible_stderr}"
    );
    assert!(visible_stderr.contains("missing.js"), "{visible_stderr}");

    let mut capped_args = vec![command, "--verbose", "--max-diagnostics", budget];
    capped_args.extend(paths.iter().copied());
    for disabled in [
        r#"{"formatter":{"enabled":false},"assist":{"enabled":false},"linter":{"rules":{"preset":"none"}}}"#,
        r#"{"formatter":{"enabled":false},"assist":{"enabled":false},"linter":{"rules":{"preset":"none"}},"projectAnalysis":{"enabled":false,"entry":[],"project":[]}}"#,
    ] {
        project.write("biome.json", disabled);
        let output = project.run(&capped_args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert!(!stderr.contains("Project analysis coverage"), "{stderr}");
    }
    project.write("biome.json", ENABLED);
    for no_exit in [false, true] {
        let mut args = capped_args.clone();
        if no_exit {
            args.push("--no-exit-code");
        }
        let output = project.run(&args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("coverage: complete; 0 findings, 0 hints"),
            "{stderr}"
        );
        assert!(!stderr.contains("internalError/io"), "{stderr}");
        if budget == "1" {
            assert!(stderr.contains("files/missingHandler"), "{stderr}");
        }
        assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
    }
}

#[test]
fn runtime_failure_with_zero_budget_check() {
    runtime_failure_with_diagnostic_budget("check", "0");
}

#[test]
fn runtime_failure_with_zero_budget_lint() {
    runtime_failure_with_diagnostic_budget("lint", "0");
}

#[test]
fn runtime_failure_with_exhausted_budget_check() {
    runtime_failure_with_diagnostic_budget("check", "1");
}

#[test]
fn runtime_failure_with_exhausted_budget_lint() {
    runtime_failure_with_diagnostic_budget("lint", "1");
}

#[test]
fn reporter_project_analysis_process_policy_and_destinations() {
    let project = Project::new(
        r#"{"projectAnalysis":{"entry":["entry.ts"],"project":["**/*.ts"],"include":["files"]}}"#,
    );
    project.write("entry.ts", "console.log('entry');\n");
    project.write("orphan.ts", "console.log('orphan');\n");
    for (extra, expected_exit) in [
        (vec![], 1),
        (vec!["--max-issues=1"], 0),
        (vec!["--no-exit-code"], 0),
        (vec!["--max-issues=invalid", "--no-exit-code"], 2),
        (vec!["--unknown-option", "--no-exit-code"], 1),
    ] {
        let mut args = vec![
            "analyze",
            "--report-profile=knip-6.43",
            "--project-reporter=json",
        ];
        args.extend(extra);
        let actual = project.run(&args);
        let actual_exit = actual.status.code().unwrap();
        assert_eq!(
            actual_exit,
            expected_exit,
            "{args:?}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        if expected_exit != 2 && !args.contains(&"--unknown-option") {
            let report: serde_json::Value = serde_json::from_slice(&actual.stdout).unwrap();
            assert_eq!(report["issues"][0]["file"], "orphan.ts");
            assert!(String::from_utf8_lossy(&actual.stderr).contains("qualification_eligible"));
        }
    }
    let output = project.run(&[
        "analyze",
        "--report-profile=knip-6.43",
        "--project-reporter=json",
        "--project-report-file=report.json",
        "--project-reporter=sarif",
        "--project-report-file=report.sarif",
    ]);
    assert_eq!(output.status.code(), Some(1));
    for name in [
        "report.json",
        "report.sarif",
        "report.json.coverage.json",
        "report.sarif.coverage.json",
    ] {
        let bytes = std::fs::read(project.0.join(name)).unwrap();
        let _: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    }
    assert!(output.stdout.is_empty());
    for args in [
        vec!["--project-reporter=json", "--project-reporter=sarif"],
        vec![
            "--project-reporter=json",
            "--project-report-file=collision",
            "--project-reporter=sarif",
            "--project-report-file=./collision",
        ],
        vec![
            "--project-reporter=json",
            "--project-report-file=collision",
            "--project-reporter=sarif",
            "--project-report-file=collision.coverage.json",
        ],
        vec!["--project-reporter=json", "--project-report-file=."],
    ] {
        let mut arguments = vec!["analyze", "--report-profile=knip-6.43", "--no-exit-code"];
        arguments.extend(args);
        assert_eq!(project.run(&arguments).status.code(), Some(2));
    }
    project.write(
        "custom.js",
        "require('node:fs').writeFileSync('EXECUTED', 'bad');\n",
    );
    let custom = project.run(&[
        "analyze",
        "--report-profile=knip-6.43",
        "--project-reporter=./custom.js",
    ]);
    assert_eq!(custom.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&custom.stderr).contains("Unsupported custom project reporter")
    );
    assert!(!project.0.join("EXECUTED").exists());
}

#[test]
fn reporter_project_analysis_runtime_failure_survives_hidden_output() {
    let project = Project::new(
        r#"{"formatter":{"enabled":false},"assist":{"enabled":false},"linter":{"rules":{"preset":"none"}},"projectAnalysis":{"enabled":true,"entry":[],"project":[]}}"#,
    );
    for command in ["check", "lint"] {
        let output = project.run(&[
            command,
            "--report-profile=knip-6.43",
            "--project-reporter=json",
            "--max-diagnostics=0",
            "--max-show-issues=0",
            "--max-issues=999",
            "--no-exit-code",
            "missing.js",
        ]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report, serde_json::json!({"issues":[]}));
        let companion = String::from_utf8(output.stderr)
            .unwrap()
            .lines()
            .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .unwrap();
        assert_eq!(companion["coverage"]["status"], "complete");
        assert_eq!(companion["runtime_failure"], true);
        assert_eq!(companion["command_exit_code"], 2);
        assert_eq!(companion["qualification_eligible"], false);
    }
}

#[test]
fn reporter_project_analysis_hints_and_warning_policy() {
    let project = Project::new(
        r#"{"projectAnalysis":{"entry":["entry.ts"],"project":["**/*.ts"],"include":["files"],"ignore":["unused/**"],"treatConfigHintsAsErrors":true}}"#,
    );
    project.write("entry.ts", "");
    for (flags, exit) in [
        (vec![], 1),
        (vec!["--max-issues=999"], 1),
        (vec!["--no-exit-code"], 0),
        (vec!["--configuration-hints=false"], 0),
    ] {
        let mut arguments = vec![
            "analyze",
            "--report-profile=knip-6.43",
            "--project-reporter=json",
        ];
        arguments.extend(flags);
        let output = project.run(&arguments);
        assert_eq!(
            output.status.code(),
            Some(exit),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            serde_json::json!({"issues":[]})
        );
    }
    project.write("biome.json",r#"{"projectAnalysis":{"entry":["entry.ts"],"project":["**/*.ts"],"include":["files"],"rules":{"files":"warn"}}}"#);
    project.write("orphan.ts", "");
    for (profile, exit) in [("native", 1), ("knip-6.43", 0)] {
        let flag = format!("--report-profile={profile}");
        let output = project.run(&[
            "analyze",
            &flag,
            "--project-reporter=json",
            "--error-on-warnings",
        ]);
        assert_eq!(
            output.status.code(),
            Some(exit),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn c4_boundary_companion_failure_preserves_existing_report() {
    let project = Project::new(r#"{"projectAnalysis":{"entry":[],"project":[]}}"#);
    project.write("report.json", "previous report\n");
    std::fs::create_dir(project.0.join("report.json.coverage.json")).unwrap();
    let output = project.run(&[
        "analyze",
        "--report-profile=knip-6.43",
        "--project-reporter=json",
        "--project-report-file=report.json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        std::fs::read_to_string(project.0.join("report.json")).unwrap(),
        "previous report\n"
    );
}

#[test]
fn c4_boundary_disabled_commands_reject_project_reports() {
    let project = Project::new(r#"{"projectAnalysis":{"enabled":false}}"#);
    project.write("entry.ts", "");
    for command in ["check", "lint"] {
        let output = project.run(&[command, "--project-reporter=json", "entry.ts"]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("projectAnalysis.enabled"));
    }
}

#[test]
fn reporter_project_analysis_missing_codeowners_is_runtime_failure() {
    let project = Project::new(r#"{"projectAnalysis":{"entry":[],"project":[]}}"#);
    let output = project.run(&[
        "analyze",
        "--report-profile=knip-6.43",
        "--project-reporter=codeowners",
        "--no-exit-code",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("CODEOWNERS report input is unavailable")
    );
    assert!(output.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn reporter_project_analysis_process_each_pinned_format() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/project_reporters/fixtures.json")).unwrap();
    let goldens: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/project_reporters/oracle.json")).unwrap();
    for case in goldens["cases"].as_array().unwrap() {
        let name = case["id"].as_str().unwrap();
        let format = case["format"].as_str().unwrap();
        let files = &fixtures[case["fixture"].as_str().unwrap()];
        let project = Project::new(files["biome.json"].as_str().unwrap());
        for (path, text) in files.as_object().unwrap() {
            let path = project.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let reporter = format!("--project-reporter={format}");
        let mut arguments = vec![
            "analyze",
            "--report-profile=knip-6.43",
            &reporter,
            "--project-report-file=_c4_report",
        ];
        for argument in case["arguments"].as_array().unwrap() {
            let argument = argument.as_str().unwrap();
            arguments.push(if argument == "--reporter-options" {
                "--project-reporter-options"
            } else {
                argument
            });
        }
        arguments.push("--colors=off");
        let environment = std::collections::BTreeMap::from([
            ("PATH", std::env::var("PATH").unwrap_or_default()),
            ("NO_COLOR", "1".to_string()),
            ("LANG", "C.UTF-8".to_string()),
            (
                "TMPDIR",
                std::env::temp_dir().to_string_lossy().into_owned(),
            ),
            (
                "XDG_CACHE_HOME",
                project.0.join("cache").to_string_lossy().into_owned(),
            ),
        ]);
        let started = std::time::Instant::now();
        let output = Command::new(env!("CARGO_BIN_EXE_biome"))
            .args(&arguments)
            .current_dir(&project.0)
            .env_clear()
            .envs(&environment)
            .output()
            .unwrap();
        let elapsed_ms = started.elapsed().as_millis();
        let expected_exit = case
            .get("native_exit_code")
            .unwrap_or(&case["exit_code"])
            .as_i64()
            .unwrap() as i32;
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = std::fs::read_to_string(project.0.join("_c4_report")).ok();
        let companion = std::fs::read(project.0.join("_c4_report.coverage.json"))
            .ok()
            .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).unwrap());
        if let Some(directory) = std::env::var_os("BIOME_PROJECT_PROCESS_CAPTURE_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("{name}.json")),serde_json::to_vec_pretty(&serde_json::json!({"case":name,"format":format,"arguments":arguments,"environment":environment,"elapsed_ms":elapsed_ms,"cwd":project.0,"executable":env!("CARGO_BIN_EXE_biome"),"stdout":output.stdout,"stderr":output.stderr,"exit_code":output.status.code(),"report":report,"companion":companion})).unwrap()).unwrap();
        }
        assert!(output.stdout.is_empty(), "{name}");
        if name == "codeowners-missing" {
            assert!(report.is_none());
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("CODEOWNERS report input is unavailable")
            );
            continue;
        }
        let actual_report = report.unwrap();
        let expected_report = case["native_stdout"].as_str().unwrap();
        if matches!(format, "json" | "codeclimate" | "sarif") {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&actual_report).unwrap(),
                serde_json::from_str::<serde_json::Value>(expected_report).unwrap(),
                "{name}"
            );
        } else {
            assert_eq!(actual_report, expected_report, "{name}");
        }
        let companion = companion.unwrap();
        assert_eq!(companion["issue_total"], case["issue_total"], "{name}");
        assert_eq!(companion["command_exit_code"], expected_exit, "{name}");
        assert_eq!(companion["qualification_eligible"], false, "{name}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .starts_with(case["native_stderr"].as_str().unwrap()),
            "{name}"
        );
    }
}

#[cfg(unix)]
fn c4_fix1_command(project: &Project, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_biome"));
    command
        .args(arguments)
        .arg("--colors=off")
        .current_dir(&project.0)
        .env_clear()
        .envs([
            ("PATH", std::env::var("PATH").unwrap_or_default()),
            ("NO_COLOR", "1".into()),
            ("LANG", "C.UTF-8".into()),
            (
                "TMPDIR",
                std::env::temp_dir().to_string_lossy().into_owned(),
            ),
            (
                "XDG_CACHE_HOME",
                project.0.join("cache").to_string_lossy().into_owned(),
            ),
        ]);
    command
}

#[cfg(unix)]
fn c4_fix1_output(mut command: Command, case: &str) -> Output {
    let started = std::time::Instant::now();
    let output = command.output().unwrap();
    if let Some(directory) = std::env::var_os("BIOME_PROJECT_FIX_CAPTURE_DIR") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let environment: std::collections::BTreeMap<_, _> = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy(),
                    value.map(|value| value.to_string_lossy()),
                )
            })
            .collect();
        let capture = serde_json::json!({"case":case,"executable":command.get_program().to_string_lossy(),"arguments":command.get_args().map(|value|value.to_string_lossy()).collect::<Vec<_>>(),"cwd":command.get_current_dir(),"environment":environment,"elapsed_ms":started.elapsed().as_millis(),"exit_code":output.status.code(),"stdout":output.stdout,"stderr":output.stderr});
        std::fs::write(
            directory.join(format!("{case}.json")),
            serde_json::to_vec_pretty(&capture).unwrap(),
        )
        .unwrap();
    }
    output
}

#[cfg(unix)]
#[test]
fn c4_fix1_symlink_parent_destination_aliases() {
    let mut observations = Vec::new();
    for existing in [false, true] {
        for companion_alias in [false, true] {
            let project = Project::new(r#"{"projectAnalysis":{"entry":[],"project":[]}}"#);
            std::fs::create_dir_all(project.0.join("real/sub")).unwrap();
            std::os::unix::fs::symlink("real/sub", project.0.join("link")).unwrap();
            let paths = [
                "real/report.json",
                "real/report.json.coverage.json",
                "real/report.json.coverage.json.coverage.json",
            ];
            if existing {
                for path in paths {
                    project.write(path, "preserved\n");
                }
            }
            let before: Vec<_> = paths
                .iter()
                .map(|path| std::fs::read(project.0.join(path)).ok())
                .collect();
            let second = if companion_alias {
                "--project-report-file=real/report.json.coverage.json"
            } else {
                "--project-report-file=real/report.json"
            };
            let output = c4_fix1_output(
                c4_fix1_command(
                    &project,
                    &[
                        "analyze",
                        "--no-exit-code",
                        "--project-reporter=json",
                        "--project-report-file=link/../report.json",
                        "--project-reporter=sarif",
                        second,
                    ],
                ),
                &format!("destination-existing-{existing}-companion-{companion_alias}"),
            );
            let after: Vec<_> = paths
                .iter()
                .map(|path| std::fs::read(project.0.join(path)).ok())
                .collect();
            observations.push((
                existing,
                companion_alias,
                output.status.code(),
                before == after,
                String::from_utf8_lossy(&output.stderr).contains("destination collision"),
            ));
        }
    }
    assert!(
        observations
            .iter()
            .all(|(_, _, status, preserved, diagnostic)| *status == Some(2)
                && *preserved
                && *diagnostic),
        "{observations:?}"
    );
}

#[cfg(unix)]
#[test]
fn c4_fix1_symlink_parent_ownership_aliases() {
    let mut observations = Vec::new();
    for companion_alias in [false, true] {
        for input_alias in [false, true] {
            let project = Project::new(r#"{"projectAnalysis":{"entry":[],"project":[]}}"#);
            std::fs::create_dir_all(project.0.join("real/sub")).unwrap();
            std::os::unix::fs::symlink("real/sub", project.0.join("link")).unwrap();
            let owner_name = if companion_alias {
                "OWNERS.coverage.json"
            } else {
                "OWNERS"
            };
            let input = format!(
                "{}/{owner_name}",
                if input_alias { "link/.." } else { "real" }
            );
            let destination = if input_alias {
                "--project-report-file=real/OWNERS"
            } else {
                "--project-report-file=link/../OWNERS"
            };
            let actual_input = format!("real/{owner_name}");
            project.write(&actual_input, "* @preserve\n");
            let options = serde_json::json!({"path":input}).to_string();
            let output = c4_fix1_output(
                c4_fix1_command(
                    &project,
                    &[
                        "analyze",
                        "--no-exit-code",
                        "--project-reporter=codeowners",
                        "--project-reporter-options",
                        &options,
                        destination,
                    ],
                ),
                &format!("ownership-companion-{companion_alias}-input-alias-{input_alias}"),
            );
            observations.push((
                companion_alias,
                input_alias,
                output.status.code(),
                std::fs::read_to_string(project.0.join(actual_input)).unwrap() == "* @preserve\n",
                String::from_utf8_lossy(&output.stderr).contains("overwrite ownership input"),
            ));
        }
    }
    assert!(
        observations
            .iter()
            .all(|(_, _, status, preserved, diagnostic)| *status == Some(2)
                && *preserved
                && *diagnostic),
        "{observations:?}"
    );
}

#[cfg(target_os = "linux")]
fn c4_fix1_stream_failure(stream: &str, full: bool) {
    use std::process::Stdio;
    let project = Project::new(if stream == "hint" {
        r#"{"projectAnalysis":{"entry":["missing*.ts"],"project":[]}}"#
    } else {
        r#"{"projectAnalysis":{"entry":[],"project":[]}}"#
    });
    let mut observations = Vec::new();
    for no_exit in [false, true] {
        let failed = if full {
            Stdio::from(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            )
        } else {
            let (reader, writer) = std::io::pipe().unwrap();
            drop(reader);
            Stdio::from(writer)
        };
        let mut command = c4_fix1_command(&project, &["analyze", "--report-profile=knip-6.43"]);
        command
            .arg(if stream == "hint" {
                "--project-reporter=symbols"
            } else {
                "--project-reporter=json"
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if no_exit {
            command.arg("--no-exit-code");
        }
        if stream == "hint" {
            command.arg("--project-report-file=report.txt");
        }
        if matches!(stream, "stdout" | "both") {
            command.stdout(failed);
        } else {
            command.stderr(failed);
        }
        if stream == "both" {
            command.stderr(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            );
        }
        let output = c4_fix1_output(
            command,
            &format!("stream-{stream}-full-{full}-no-exit-{no_exit}"),
        );
        if stream == "hint" {
            assert!(project.0.join("report.txt.coverage.json").exists());
            assert!(project.0.join("report.txt").exists());
        }
        observations.push((
            no_exit,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    assert!(
        observations.iter().all(|(_, status, _)| *status == Some(2)),
        "{stream}, full={full}: {observations:?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn c4_fix1_stdout_broken_pipe() {
    c4_fix1_stream_failure("stdout", false);
}
#[cfg(target_os = "linux")]
#[test]
fn c4_fix1_stdout_full() {
    c4_fix1_stream_failure("stdout", true);
}
#[cfg(target_os = "linux")]
#[test]
fn c4_fix1_companion_broken_pipe() {
    c4_fix1_stream_failure("companion", false);
}
#[cfg(target_os = "linux")]
#[test]
fn c4_fix1_companion_full() {
    c4_fix1_stream_failure("companion", true);
}
#[cfg(target_os = "linux")]
#[test]
fn c4_fix1_hint_broken_pipe() {
    c4_fix1_stream_failure("hint", false);
}
#[cfg(target_os = "linux")]
#[test]
fn c4_fix1_hint_full() {
    c4_fix1_stream_failure("hint", true);
}

#[test]
fn c4_fix1_fallible_console_buffers_preserve_raw_text() {
    use biome_console::{BufferConsole, Console, FileBufferConsole, LogLevel};
    let text = "\u{1b}[31mraw\n";
    let mut buffer = BufferConsole::default();
    buffer.try_print_raw(LogLevel::Error, text).unwrap();
    assert!(buffer.out_buffer[0].is_raw);
    assert_eq!(buffer.out_buffer[0].level, LogLevel::Error);
    assert_eq!(buffer.out_buffer[0].content.0[0].content, text);
    let mut file = FileBufferConsole::default();
    file.try_print_raw(LogLevel::Log, text).unwrap();
    assert_eq!(file.dump().as_deref(), Some(text));
}

#[cfg(target_os = "linux")]
#[test]
fn c4_fix1_secondary_diagnostic_failure_preserves_runtime_status() {
    c4_fix1_stream_failure("both", true);
}
