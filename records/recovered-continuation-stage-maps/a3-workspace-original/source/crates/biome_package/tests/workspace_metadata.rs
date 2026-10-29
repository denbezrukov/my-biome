use biome_package::read_workspace_metadata;

#[test]
fn workspace_metadata_preserves_empty_providers_and_ranges() {
    let source = "packages: ['packages/*', '!packages/excluded']\ncatalog: {}\ncatalogs:\n  named:\n    dep: '1'\n";
    let metadata = read_workspace_metadata(source);
    assert!(
        metadata.diagnostics.is_empty(),
        "{:?}",
        metadata.diagnostics
    );
    assert_eq!(metadata.fields["packages"].elements.len(), 2);
    assert!(
        metadata.fields["catalog"]
            .value
            .as_object()
            .unwrap()
            .is_empty()
    );
    let value = &metadata.fields["catalogs"].members["named"].members["dep"];
    assert_eq!(value.value.as_string().unwrap().as_str(), "1");
    assert_eq!(
        &source[usize::from(value.range.start())..usize::from(value.range.end())],
        "'1'"
    );
}

#[test]
fn workspace_metadata_diagnoses_non_string_selectors_and_versions() {
    for source in [
        "packages: [42]\n",
        "packages: [false]\n",
        "catalog:\n  dep: 42\n",
        "catalog:\n  dep: true\n",
        "catalog: *unsupported\n",
        "catalog: {}\n---\ncatalog: {}\n",
    ] {
        let metadata = read_workspace_metadata(source);
        assert!(!metadata.diagnostics.is_empty(), "{source}");
    }
}

#[test]
fn workspace_metadata_preserves_escaped_quote_boundaries() {
    let metadata = read_workspace_metadata("catalog:\n  dep: '''quoted'''\n");
    assert!(metadata.diagnostics.is_empty());
    assert_eq!(
        metadata.fields["catalog"].members["dep"].value.as_string().unwrap().as_str(),
        "'quoted'"
    );
}
