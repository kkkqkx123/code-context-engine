use super::*;

#[test]
fn test_extract_java_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "public class Point extends Shape {\n    public Point(int x) {\n        this.x = x;\n    }\n    public int getX() {\n        return x;\n    }\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Java)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Java)
        .expect("Failed to extract");

    for expected in [("Point", "Point Shape"), ("getX", "int getX ()")] {
        let entity = entities
            .iter()
            .find(|e| e.name == expected.0)
            .unwrap_or_else(|| panic!("Should find {}", expected.0));
        assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
    }
    let ctor = entities
        .iter()
        .find(|e| e.kind == EntityKind::Constructor)
        .expect("Should find constructor");
    assert_eq!(ctor.signature, "Point (int x)");
    for entity in &entities {
        assert!(
            !entity.signature.contains("this.x") && !entity.signature.contains("return x"),
            "body must not leak for {}, got {:?}",
            entity.name,
            entity.signature
        );
    }
}

#[test]
fn test_extract_java_enum_constant_signature() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "enum Status {\n    ASYNC(\"a\") {\n        void run() {\n        }\n    },\n    SYNC;\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Java)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Java)
        .expect("Failed to extract");

    let constant = entities
        .iter()
        .find(|e| e.kind == EntityKind::EnumVariant && e.name == "ASYNC")
        .expect("Should find enum constant ASYNC");
    assert_eq!(constant.signature, "ASYNC (\"a\")");
    assert!(
        !constant.signature.contains("run"),
        "constant class body must not leak, got {:?}",
        constant.signature
    );
}
