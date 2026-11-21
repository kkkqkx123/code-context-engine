use super::*;

#[test]
fn test_extract_csharp_class_base_metadata() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
public abstract class Shape
{
    public abstract string Kind { get; }
}

public class Circle : Shape
{
    public override string Kind => "Circle";
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::CSharp)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::CSharp)
        .expect("Failed to extract");

    let circle = entities
        .iter()
        .find(|e| e.kind == EntityKind::Class && e.name == "Circle")
        .expect("Circle class must be extracted");
    assert_eq!(
        circle.metadata.get("base_classes").map(String::as_str),
        Some("Shape"),
        "Circle must record its Shape base; all classes: {:?}",
        entities
            .iter()
            .filter(|e| e.kind == EntityKind::Class)
            .map(|e| (&e.name, e.metadata.get("base_classes")))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_extract_csharp_method_signature() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "public class Store {\n    public string Get(string key) {\n        return key;\n    }\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::CSharp)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::CSharp)
        .expect("Failed to extract");

    let class = entities
        .iter()
        .find(|e| e.kind == EntityKind::Class && e.name == "Store")
        .expect("Should find class Store");
    assert_eq!(class.signature, "Store");

    let method = entities
        .iter()
        .find(|e| e.name == "Get")
        .expect("Should find method Get");
    assert_eq!(method.signature, "string Get (string key)");
    assert!(
        !method.signature.contains("return key"),
        "body must not leak, got {:?}",
        method.signature
    );
}

#[test]
fn test_extract_csharp_namespace_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "namespace Shop {\n    public class Store {\n    }\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::CSharp)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::CSharp)
        .expect("Failed to extract");

    let namespace = entities
        .iter()
        .find(|e| e.kind == EntityKind::Namespace && e.name == "Shop")
        .expect("Should find namespace Shop");
    assert_eq!(namespace.signature, "Shop");
    assert!(
        !namespace.signature.contains("class Store"),
        "namespace body must not leak, got {:?}",
        namespace.signature
    );

    let file_scoped = "namespace Shop;\npublic class Store {\n}\n";
    let tree = ast_parser
        .parse_with_tree(file_scoped, &Language::CSharp)
        .expect("Failed to parse")
        .0;
    let entities = extractor
        .extract(&tree, file_scoped, &Language::CSharp)
        .expect("Failed to extract");
    let namespace = entities
        .iter()
        .find(|e| e.kind == EntityKind::Namespace && e.name == "Shop")
        .expect("Should find file-scoped namespace Shop");
    assert_eq!(namespace.signature, "Shop");
}

#[test]
fn test_extract_csharp_property_signature() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "public class Fixture {\n    public IServiceProvider Provider {\n        get {\n            return container;\n        }\n    }\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::CSharp)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::CSharp)
        .expect("Failed to extract");

    let property = entities
        .iter()
        .find(|e| e.kind == EntityKind::Property && e.name == "Provider")
        .expect("Should find property Provider");
    assert_eq!(property.signature, "IServiceProvider Provider");
    assert!(
        !property.signature.contains("return"),
        "accessor body must not leak, got {:?}",
        property.signature
    );
}
