use biome_project_analysis::IssueKind;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn compatibility_inventory() -> Value {
    serde_json::from_str(include_str!("fixtures/compatibility.json"))
        .expect("valid compatibility inventory")
}

#[test]
fn issue_keys_match_pinned_reference() {
    let inventory = compatibility_inventory();
    let issues = inventory["issues"].as_array().expect("issue inventory");
    assert_eq!(issues.len(), 17);

    let mut keys = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for (issue, expected_kind) in issues.iter().zip(IssueKind::ALL) {
        let key = issue["key"].as_str().expect("issue key");
        assert!(keys.insert(key), "duplicate issue key: {key}");
        let kind: IssueKind = serde_json::from_value(json!(key)).expect("known issue key");
        assert_eq!(kind, expected_kind);
        assert!(kinds.insert(kind), "duplicate issue kind: {kind:?}");
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(key));
        let serialized = serde_json::to_string(&kind).unwrap();
        assert_eq!(
            serde_json::from_str::<IssueKind>(&serialized).unwrap(),
            kind
        );
    }
    assert_eq!(IssueKind::ALL.len(), 17);
    assert_eq!(kinds, IssueKind::ALL.into_iter().collect());

    assert_eq!(inventory["schemaVersion"], 1);
    let source = &inventory["sourceReference"];
    assert_eq!(source["commit"], "07121567e60b73d1e640f395028c525a2b1d9778");
    assert_eq!(
        source["rawUrl"],
        "https://raw.githubusercontent.com/webpro-nl/knip/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/types/issues.ts"
    );
    assert_eq!(
        source["responseSha256"],
        "7cd0a6673a2dad455f610db9f98df72671ddd1f1e40cccc1e5fe1ace59c5b93d"
    );
    let artifact = &inventory["publishedArtifact"];
    assert_eq!(artifact["name"], "knip");
    assert_eq!(artifact["version"], "6.43.0");
    assert_eq!(
        artifact["tarballUrl"],
        "https://registry.npmjs.org/knip/-/knip-6.43.0.tgz"
    );
    assert_eq!(
        artifact["registryIntegrity"],
        "sha512-8l3cCOg/EXoILO0qcSoMra49yQM91+lEIR4fu5Ko8rrArGX8xKdKZiGI9psPiY6rhLmfLERcMfFEByF0wli5CA=="
    );
    assert_eq!(artifact["integrityProvenance"], "registry-declared");
    assert_eq!(artifact["tarballDownloaded"], false);
    assert_eq!(artifact["tarballIntegrityVerified"], false);
    assert_eq!(artifact.get("gitHead"), Some(&Value::Null));
    assert_eq!(
        inventory["identityAssessment"]["status"],
        "source-published-artifact-correspondence-unverified"
    );
}

#[test]
fn unknown_issue_keys_are_rejected() {
    for key in ["classMembers", "Files", "", "futureIssue"] {
        assert!(
            serde_json::from_value::<IssueKind>(json!(key)).is_err(),
            "unexpectedly accepted issue key: {key:?}"
        );
    }
}

#[test]
fn inventory_does_not_claim_implementation_or_conformance() {
    let inventory = compatibility_inventory();
    assert_eq!(inventory["projectReplacementClaim"], false);
    let issues = inventory["issues"].as_array().expect("issue inventory");
    assert_eq!(issues.len(), 17);
    for issue in issues {
        assert_eq!(issue.as_object().expect("issue disposition").len(), 3);
        assert_eq!(issue["implementation"], "unimplemented");
        assert_eq!(issue["conformance"], "untested");
    }
}
