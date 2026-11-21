use super::*;

#[test]
fn test_extract_javascript_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "class Dog extends Animal {\n    bark(volume) {\n        return volume;\n    }\n}\nfunction add(a, b) {\n    return a + b;\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract");

    for expected in [
        ("Dog", "Dog Animal"),
        ("bark", "bark (volume)"),
        ("add", "add (a, b)"),
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
fn test_extract_javascript_chained_assignment_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "req.get =\nreq.header = function header(name) {\n    return name;\n};\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract");

    for expected in [("get", "get (name)"), ("header", "header (name)")] {
        let entity = entities
            .iter()
            .find(|e| e.kind == EntityKind::Method && e.name == expected.0)
            .unwrap_or_else(|| panic!("Should find method {}", expected.0));
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
fn test_extract_javascript_chained_non_function_stays_variable() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "req.originalUrl = req.url = req.originalUrl.replace(/x/, 'y');\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract");

    assert!(
        entities.iter().all(|e| !e.signature.trim().is_empty()),
        "chained non-function assignment must not produce empty signatures"
    );
    assert!(
        entities
            .iter()
            .all(|e| e.kind != EntityKind::Method || !e.name.contains("originalUrl")),
        "chained non-function assignment must not be classified as a method"
    );
}

#[test]
fn test_extract_javascript_callback_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "function suite() {\n    describe('Auth', function () {\n        it('logs in', function (done) {\n            return done;\n        });\n    });\n    app.post('/login', function (req, res) {\n        return res;\n    });\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract");

    for expected in [
        ("describe", "describe 'Auth'"),
        ("it", "it 'logs in'"),
        ("app.post", "app.post '/login'"),
    ] {
        let entity = entities
            .iter()
            .find(|e| e.kind == EntityKind::Function && e.name == expected.0)
            .unwrap_or_else(|| panic!("Should find callback {}", expected.0));
        assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
        assert!(
            !entity.signature.contains("return"),
            "callback body must not leak for {}, got {:?}",
            expected.0,
            entity.signature
        );
    }
}

#[test]
fn test_extract_javascript_arrow_callback_without_label() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "function setup() {\n    app.use(function (req, res, next) {\n        return next;\n    });\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract");

    let entity = entities
        .iter()
        .find(|e| e.kind == EntityKind::Function && e.name == "app.use")
        .expect("Should find callback app.use");
    assert_eq!(entity.signature, "app.use");
    assert!(
        !entity.signature.contains("return"),
        "callback body must not leak, got {:?}",
        entity.signature
    );
}
