use super::*;

#[test]
fn test_extract_rust_entities() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
struct Point {
    x: f64,
    y: f64,
}

fn distance(p1: &Point, p2: &Point) -> f64 {
    0.0
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract");

    assert!(!entities.is_empty(), "Should find at least one entity");

    let structs: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Struct)
        .collect();
    assert!(!structs.is_empty(), "Should find at least one struct");

    let functions: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Function)
        .collect();
    assert!(!functions.is_empty(), "Should find at least one function");
}

#[test]
fn test_extract_rust_multiple_functions() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
pub fn normalize_name(input: &str) -> String {
    input.trim().to_lowercase()
}

pub fn format_user(user: &str) -> String {
    user.to_string()
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract");

    let names: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Function)
        .map(|e| e.name.as_str())
        .collect();

    assert!(
        names.contains(&"normalize_name"),
        "normalize_name should be extracted as a function"
    );
    assert!(
        names.contains(&"format_user"),
        "format_user should be extracted as a function"
    );
}

#[test]
fn test_extract_rust_generic_struct_and_fn() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "pub struct OnceCell<T> {\n    value: T,\n}\n\npub fn get<T>(key: &str) -> Option<T> {\n    None\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract");

    let strukt = entities
        .iter()
        .find(|e| e.kind == EntityKind::Struct && e.name == "OnceCell")
        .expect("Should find struct OnceCell");
    assert_eq!(strukt.signature, "OnceCell <T>");
    assert!(
        !strukt.signature.contains("value: T,"),
        "struct body must not leak, got {:?}",
        strukt.signature
    );

    let func = entities
        .iter()
        .find(|e| e.name == "get")
        .expect("Should find fn get");
    assert_eq!(func.signature, "get <T> (key: &str) Option<T>");
    assert!(
        !func.signature.contains("None"),
        "function body must not leak, got {:?}",
        func.signature
    );
}

#[test]
fn test_extract_rust_enum_trait_impl_const() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "pub enum Color {\n    Red,\n    Green,\n}\n\npub trait Shape {\n    fn area(&self) -> f64;\n}\n\nimpl Point {\n    pub fn new() -> Self {\n        Point\n    }\n}\n\npub const MAX: u32 = 100;\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract");

    let color = entities
        .iter()
        .find(|e| e.kind == EntityKind::Enum && e.name == "Color")
        .expect("Should find enum Color");
    assert_eq!(color.signature, "Color");

    let shape = entities
        .iter()
        .find(|e| e.name == "Shape")
        .expect("Should find trait Shape");
    assert_eq!(shape.signature, "Shape");
    assert!(
        !shape.signature.contains("area"),
        "trait body must not leak, got {:?}",
        shape.signature
    );

    let imp = entities
        .iter()
        .find(|e| e.kind == EntityKind::InherentImpl)
        .expect("Should find inherent impl");
    assert_eq!(imp.signature, "Point");
    assert!(
        !imp.signature.contains("Self"),
        "impl body must not leak, got {:?}",
        imp.signature
    );

    let max = entities
        .iter()
        .find(|e| e.name == "MAX")
        .expect("Should find const MAX");
    assert_eq!(max.signature, "MAX u32");
}

#[test]
fn test_extract_rust_module_excludes_body() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "pub mod sync {\n    pub struct Guard;\n    pub fn lock() -> Guard {\n        Guard\n    }\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract");

    let module = entities
        .iter()
        .find(|e| e.kind == EntityKind::Module && e.name == "sync")
        .expect("Should find mod sync");
    assert_eq!(module.signature, "sync");
    assert!(
        !module.signature.contains("Guard"),
        "module body must not leak into signature, got {:?}",
        module.signature
    );
}

#[test]
fn test_extract_rust_macro_rules_excludes_rules() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = "macro_rules! setup {\n    () => { 1 };\n}\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract");

    let found = entities.iter().find(|e| e.name == "setup");
    if let Some(mac) = found {
        assert!(
            !mac.signature.contains("=>"),
            "macro rules must not leak into signature, got {:?}",
            mac.signature
        );
    }
}

#[test]
fn test_extract_rust_inherent_impl_no_generics() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
pub struct OnceBool {
    inner: u32,
}

impl OnceBool {
    pub const fn new() -> Self {
        Self { inner: 0 }
    }

    pub fn get(&self) -> Option<bool> {
        None
    }

    pub fn set(&self, value: bool) -> Result<(), ()> {
        Ok(())
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

    let structs: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Struct)
        .collect();
    assert_eq!(structs.len(), 1, "Should find exactly one struct");
    assert_eq!(structs[0].name, "OnceBool");

    let impls: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::InherentImpl)
        .collect();
    assert_eq!(impls.len(), 1, "Should find exactly one inherent impl");
    assert_eq!(impls[0].name, "OnceBool");

    let methods: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Method)
        .collect();
    assert_eq!(
        methods.len(),
        0,
        "Methods inside impl block should be filtered out (they become children of impl)"
    );
}

#[test]
fn test_extract_rust_inherent_impl_with_generics() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
pub struct OnceCell<T> {
    value: T,
}

impl<T> OnceCell<T> {
    pub const fn new() -> OnceCell<T> {
        OnceCell { value: unsafe { std::mem::zeroed() } }
    }

    pub fn get(&self) -> Option<&T> {
        None
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        Ok(())
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

    let structs: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Struct)
        .collect();
    assert_eq!(structs.len(), 1, "Should find exactly one struct");
    assert_eq!(structs[0].name, "OnceCell");

    let impls: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::InherentImpl)
        .collect();
    assert_eq!(impls.len(), 1, "Should find exactly one inherent impl");
    assert_eq!(impls[0].name, "OnceCell");

    let methods: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Method)
        .collect();
    assert_eq!(
        methods.len(),
        0,
        "Methods inside generic impl block should be filtered out"
    );
}

#[test]
fn test_extract_rust_trait_impl() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
pub trait Display {
    fn fmt(&self) -> String;
}

pub struct Point {
    x: f64,
    y: f64,
}

impl Display for Point {
    fn fmt(&self) -> String {
        format!("({}, {})", self.x, self.y)
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

    let traits: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Trait)
        .collect();
    assert_eq!(traits.len(), 1, "Should find exactly one trait");
    assert_eq!(traits[0].name, "Display");

    let structs: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Struct)
        .collect();
    assert_eq!(structs.len(), 1, "Should find exactly one struct");
    assert_eq!(structs[0].name, "Point");

    let impls: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::TraitImpl)
        .collect();
    assert_eq!(impls.len(), 1, "Should find exactly one trait impl");
    assert_eq!(impls[0].name, "Display");

    let methods: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Method)
        .collect();
    assert_eq!(
        methods.len(),
        0,
        "Methods inside trait impl should be filtered out"
    );
}

#[test]
fn test_extract_rust_impl_with_unsafe_send_sync() {
    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();

    let code = r#"
pub struct OnceCell<T> {
    value: T,
}

unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

impl<T> OnceCell<T> {
    pub fn new() -> Self {
        OnceCell { value: unsafe { std::mem::zeroed() } }
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

    let impls: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::InherentImpl || e.kind == EntityKind::TraitImpl)
        .collect();

    let inherent_impls: Vec<_> = impls
        .iter()
        .filter(|e| e.kind == EntityKind::InherentImpl)
        .collect();
    let trait_impls: Vec<_> = impls
        .iter()
        .filter(|e| e.kind == EntityKind::TraitImpl)
        .collect();

    assert_eq!(
        inherent_impls.len(),
        1,
        "Should find exactly one inherent impl (impl<T> OnceCell<T>)"
    );
    assert_eq!(
        trait_impls.len(),
        2,
        "Should find two trait impls (Sync and Send)"
    );
    assert!(trait_impls.iter().any(|entity| entity.name == "Sync"));
    assert!(trait_impls.iter().any(|entity| entity.name == "Send"));
    assert!(
        trait_impls
            .iter()
            .all(|entity| entity.name.chars().all(|c| c.is_alphanumeric() || c == '_'))
    );
}
