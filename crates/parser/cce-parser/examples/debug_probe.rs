//! Probe: template entity subtypes for cpp.

use cce_parser::parser::ast_parser::AstParser;
use cce_parser::parser::extractor::EntityExtractor;
use cce_types::Language;

fn main() {
    let code = "template <typename T>\nT identity(T x) {\n  return x;\n}\n\ntemplate <typename K, typename V>\nstd::map<K, V> to_map(K key, V value) {\n  return {};\n}\n\ntemplate <typename T>\nclass Box {\n  T value;\n};\n";
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();
    let (tree, _) = ast_parser.parse_with_tree(code, &Language::Cpp).unwrap();
    let entities = extractor.extract(&tree, code, &Language::Cpp).unwrap();
    for e in &entities {
        println!(
            "{:?} name={:?} subtype={:?} return_type={:?}",
            e.kind, e.name, e.subtype, e.return_type,
        );
    }
}
