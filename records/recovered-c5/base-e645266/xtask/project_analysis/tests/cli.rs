#[cfg(unix)]
#[test]
fn compare_cli_retains_observed_process_failure() {
    use serde_json::json;
    let directory =
        std::env::temp_dir().join(format!("biome-report-observer-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let request = directory.join("request.json");
    let output = directory.join("observation.json");
    std::fs::write(
        &request,
        serde_json::to_vec(&json!({
            "executable":"/bin/sh", "arguments":["-c","printf output; printf error >&2; exit 7"],
            "cwd":directory,"environment":{},"timeout_ms":1000,"max_output_bytes":64
        }))
        .unwrap(),
    )
    .unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_xtask_project_analysis"))
        .arg("observe")
        .arg(&request)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let observation: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(observation["exit_code"], 7);
    assert_eq!(observation["stdout"], json!(b"output".to_vec()));
    assert_eq!(observation["stderr"], json!(b"error".to_vec()));
    std::fs::remove_dir_all(directory).unwrap();
}
