use biome_project_analysis::model::AnalysisSnapshot;
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn load_case(name: &str) -> AnalysisSnapshot {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"));
    let snapshot: AnalysisSnapshot = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    snapshot
}

pub fn expected_case(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.expected.json"));
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

pub fn run_case(name: &str) -> Value {
    let result = biome_project_analysis::analyze(&load_case(name), &Default::default());
    let normalized = biome_project_analysis::normalize::normalize_result(&result);
    let issues: Vec<_> = result.issues.iter().map(|issue|json!({"kind":issue.kind,"path":result.sources[&issue.location.file].logical_path,"symbol":issue.symbol,"severity":issue.severity,"location":issue.location.range})).collect();
    json!({"coverage":normalized["coverage"]["status"], "issues":issues,"gaps":normalized["gaps"],"capabilities":normalized["capabilities"],"evidence":normalized["evidence"]})
}
