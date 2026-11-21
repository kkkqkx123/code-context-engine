use super::*;

#[test]
fn test_extract_typescript_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "class Store extends Base {\n    get(key: string): string {\n        return \"\";\n    }\n}\nfunction add(a: number, b: number): number {\n    return a + b;\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::TypeScript)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::TypeScript)
        .expect("Failed to extract");

    for expected in [
        ("Store", "Store Base"),
        ("get", "get (key: string) string"),
        ("add", "add (a: number, b: number) number"),
    ] {
        let entity = entities
            .iter()
            .find(|e| e.name == expected.0)
            .unwrap_or_else(|| panic!("Should find {}", expected.0));
        assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
        assert!(
            !entity.signature.contains("return"),
            "body must not leak for {}, got {:?}",
            expected.0,
            entity.signature
        );
    }
}

#[test]
fn test_extract_typescript_namespace_signature() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "namespace Shop {\n    export const rate = 1;\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::TypeScript)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::TypeScript)
        .expect("Failed to extract");

    let namespace = entities
        .iter()
        .find(|e| e.kind == EntityKind::Namespace && e.name == "Shop")
        .expect("Should find namespace Shop");
    assert_eq!(namespace.signature, "Shop");
    assert!(
        !namespace.signature.contains("rate"),
        "namespace body must not leak, got {:?}",
        namespace.signature
    );
}
