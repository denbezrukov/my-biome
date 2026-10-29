use biome_project_analysis::model::AnalysisSnapshot;
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn run_case(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"));
    let snapshot: AnalysisSnapshot = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    json!({ "stage": "model-only", "snapshot": snapshot })
}

pub fn expected_case(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.expected.json"));
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
