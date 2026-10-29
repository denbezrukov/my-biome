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
