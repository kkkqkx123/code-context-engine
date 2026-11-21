use super::*;

#[test]
fn test_extract_go_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "package main\n\ntype Point struct {\n    X int\n}\n\nfunc (p Point) Distance(other Point) int {\n    return 0\n}\n\nfunc New(name string) Point {\n    return Point{}\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Go)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Go)
        .expect("Failed to extract");

    for expected in [
        ("Point", "Point"),
        ("Distance", "Distance (other Point) int"),
        ("New", "New (name string) Point"),
    ] {
        let entity = entities
            .iter()
            .find(|e| e.name == expected.0)
            .unwrap_or_else(|| panic!("Should find {}", expected.0));
        assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
        assert!(
            !entity.signature.contains("return 0") && !entity.signature.contains("X int"),
            "body must not leak for {}, got {:?}",
            expected.0,
            entity.signature
        );
    }
}

#[test]
fn test_extract_go_range_loop_skips_blank_and_summarizes_source() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "package main\n\nfunc f() {\n\tfor _, tt := range []struct {\n\t\tname string\n\t\tvalue any\n\t}{\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t\t{\"base type\", 1},\n\t\t{\"zero value\", 0},\n\t} {\n\t\tprintln(tt.name)\n\t}\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Go)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Go)
        .expect("Failed to extract");

    assert!(
        entities.iter().all(|e| e.name != "_"),
        "blank range bindings must not become entities"
    );
    let tt = entities
        .iter()
        .find(|e| e.name == "tt")
        .expect("Should find range variable tt");
    assert!(
        tt.signature.starts_with("tt []struct {"),
        "summary must keep the collection head, got {:?}",
        tt.signature
    );
    assert!(
        tt.signature.contains("name string"),
        "summary must keep field shapes, got {:?}",
        tt.signature
    );
    assert!(
        !tt.signature.contains("base type"),
        "summary must drop literal rows, got {:?}",
        tt.signature
    );
    assert!(
        tt.signature.ends_with("..."),
        "summary must be marked, got {:?}",
        tt.signature
    );
    assert!(tt.signature.chars().count() <= 500);
}
