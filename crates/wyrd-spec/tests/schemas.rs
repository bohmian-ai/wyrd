#[test]
fn generated_schemas_match_goldens() {
    let generated = std::path::Path::new("schemas");
    let golden = std::path::Path::new("tests/schemas");
    for entry in std::fs::read_dir(golden).expect("golden schema dir") {
        let entry = entry.expect("schema entry");
        let expected = std::fs::read_to_string(entry.path()).expect("read golden");
        let actual =
            std::fs::read_to_string(generated.join(entry.file_name())).expect("read generated");
        assert_eq!(actual, expected, "schema drift: {:?}", entry.file_name());
    }
}
