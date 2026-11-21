use super::*;

#[test]
fn test_extract_fnv_typealias() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
/// A convenience alias for creating a hash map with an FNV hasher.
pub(crate) type HashMap<K, V> =
    std::collections::HashMap<K, V, std::hash::BuildHasherDefault<Hasher>>;

/// A hasher that implements the Fowler–Noll–Vo (FNV) hash.
pub(crate) struct Hasher(u64);

impl Hasher {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
}

impl Default for Hasher {
    fn default() -> Hasher {
        Hasher(Hasher::OFFSET_BASIS)
    }
}

impl std::hash::Hasher for Hasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes.iter() {
            self.0 = self.0 ^ u64::from(byte);
            self.0 = self.0.wrapping_mul(Hasher::PRIME);
        }
    }
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract");

    // Also test raw query matches
    use crate::tree_sitter_query::executor::QueryExecutor;
    let executor = QueryExecutor::new();
    let matches = executor
        .execute_entity_query(&tree, code, &Language::Rust)
        .expect("query");
    assert!(
        !matches.is_empty(),
        "Query should extract at least one entity"
    );

    let type_aliases: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::TypeAlias)
        .collect();
    assert!(
        !type_aliases.is_empty(),
        "Should find the HashMap type alias"
    );
    assert_eq!(type_aliases[0].name, "HashMap");
}

#[test]
fn test_extract_c_signatures() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "struct Point {\n    int x;\n};\n\nint add(int a, int b) {\n    return a + b;\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::C)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::C)
        .expect("Failed to extract");

    for expected in [("Point", "Point"), ("add", "int add (int a, int b)")] {
        let entity = entities
            .iter()
            .find(|e| e.name == expected.0)
            .unwrap_or_else(|| panic!("Should find {}", expected.0));
        assert_eq!(entity.signature, expected.1, "signature of {}", expected.0);
        assert!(
            !entity.signature.contains("return a") && !entity.signature.contains("int x;"),
            "body must not leak for {}, got {:?}",
            expected.0,
            entity.signature
        );
    }
}
