use super::*;

#[test]
fn test_extract_python_entities() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def distance(self, other):
        return 0.0
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Python)
        .expect("Failed to extract");

    assert!(!entities.is_empty());

    let classes: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Class)
        .collect();
    assert!(!classes.is_empty());

    let point = classes
        .iter()
        .find(|e| e.name == "Point")
        .expect("Should find class Point");
    assert_eq!(point.signature, "Point");

    let init = entities
        .iter()
        .find(|e| e.name == "__init__")
        .expect("Should find __init__");
    assert_eq!(init.signature, "__init__ (self, x, y)");

    let distance = entities
        .iter()
        .find(|e| e.name == "distance")
        .expect("Should find distance");
    assert_eq!(distance.signature, "distance (self, other)");

    for entity in entities.iter().filter(|e| {
        matches!(
            e.kind,
            EntityKind::Function | EntityKind::Method | EntityKind::Class
        )
    }) {
        assert!(
            !entity.signature.contains("self.x = x") && !entity.signature.contains("return 0.0"),
            "signature of {} must exclude the body, got {:?}",
            entity.name,
            entity.signature
        );
    }
}

#[test]
fn test_extract_python_decorated_and_default_spacing() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "@app.route(\"/read\")\ndef read() -> str:\n    return \"ok\"\n\ndef greet(name: str = \"a  b\"):\n    return name\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Python)
        .expect("Failed to extract");

    let read = entities
        .iter()
        .find(|e| e.name == "read")
        .expect("Should find read");
    assert_eq!(read.signature, "read () str");
    assert!(
        !read.signature.contains("@app.route"),
        "decorator must not enter signature, got {:?}",
        read.signature
    );

    let greet = entities
        .iter()
        .find(|e| e.name == "greet")
        .expect("Should find greet");
    assert_eq!(greet.signature, "greet (name: str = \"a  b\")");
}

#[test]
fn test_extract_python_method_variants() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "class Service:\n    @classmethod\n    def create(cls, name: str) -> str:\n        return name\n\n    def get(self, key: str) -> str:\n        return key\n\n    @staticmethod\n    def helper(x: int) -> int:\n        return x\n\n    @property\n    def label(self) -> str:\n        return \"x\"\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Python)
        .expect("Failed to extract");

    for expected in [
        ("create", "create (cls, name: str) str"),
        ("get", "get (self, key: str) str"),
        ("helper", "helper (x: int) int"),
        ("label", "label (self) str"),
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
fn test_extract_python_async_and_generator() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "async def fetch(url: str) -> str:\n    return url\n\ndef gen(n: int):\n    yield n\n\ndouble = lambda x: x * 2\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Python)
        .expect("Failed to extract");

    let fetch = entities
        .iter()
        .find(|e| e.name == "fetch")
        .expect("Should find fetch");
    assert_eq!(fetch.signature, "fetch (url: str) str");

    let generator = entities
        .iter()
        .find(|e| e.name == "gen")
        .expect("Should find gen");
    assert_eq!(generator.signature, "gen (n: int)");

    let double = entities
        .iter()
        .find(|e| e.name == "double")
        .expect("Should find double");
    assert_eq!(double.signature, "double = lambda x: x * 2");
}

#[test]
fn test_extract_python_except_signature() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "try:\n    risky()\nexcept ValueError as e:\n    raise e\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Python)
        .expect("Failed to extract");

    let binding = entities
        .iter()
        .find(|e| e.kind == EntityKind::Variable && e.name == "e")
        .expect("Should find except binding e");
    assert_eq!(binding.signature, "ValueError e");
    assert!(
        !binding.signature.contains("raise"),
        "except body must not leak, got {:?}",
        binding.signature
    );
}
