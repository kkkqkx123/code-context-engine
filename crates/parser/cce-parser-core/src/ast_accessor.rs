//! Deterministic AST accessor layer
//!
//! Provides a structured interface on top of `tree_sitter::Node` that replaces
//! source-text heuristics (string splits, regexes) with field-based queries.
//!
//! The design follows `docs/plan/symbol-resolution-deterministic.md`:
//! - `AstAccessor` trait for per-node field access
//! - Deterministic helpers for callee name construction, argument counting,
//!   and type annotation extraction

use tree_sitter::Node;

/// Structured accessor for `tree_sitter::Node`.
pub trait AstAccessor<'a> {
    /// Get a named field child.
    fn field(&self, name: &str) -> Option<Node<'a>>;

    /// Get the callee node of a call expression, if this node is a call.
    fn call_callee(&self) -> Option<Node<'a>>;

    /// Get call argument nodes (named children of the `arguments` field).
    fn call_arguments(&self) -> Vec<Node<'a>>;

    /// Get parameter name and optional type annotation nodes.
    fn parameter_parts(&self) -> Option<(Node<'a>, Option<Node<'a>>)>;

    /// Get type annotation text using structured fields.
    fn type_annotation_text(&self, source: &[u8]) -> Option<String>;
}

impl<'a> AstAccessor<'a> for Node<'a> {
    fn field(&self, name: &str) -> Option<Node<'a>> {
        self.child_by_field_name(name)
    }

    fn call_callee(&self) -> Option<Node<'a>> {
        if self.kind() == "call_expression" {
            self.child_by_field_name("function")
        } else {
            None
        }
    }

    fn call_arguments(&self) -> Vec<Node<'a>> {
        if self.kind() != "call_expression" {
            return Vec::new();
        }
        if let Some(args) = self.child_by_field_name("arguments") {
            let mut result = Vec::new();
            let mut cursor = args.walk();
            for child in args.children(&mut cursor) {
                if child.is_named() {
                    result.push(child);
                }
            }
            result
        } else {
            Vec::new()
        }
    }

    fn parameter_parts(&self) -> Option<(Node<'a>, Option<Node<'a>>)> {
        // Common patterns:
        // - Rust: `parameter` has `pattern` and `type`
        // - Python/JS: `parameter` / `required_parameter` etc has similar
        let name = self
            .child_by_field_name("pattern")
            .or_else(|| self.child_by_field_name("name"))
            .or_else(|| {
                let mut cursor = self.walk();
                self.children(&mut cursor).find(|c| c.is_named())
            })?;
        let ty = self
            .child_by_field_name("type")
            .or_else(|| self.child_by_field_name("type_annotation"));
        Some((name, ty))
    }

    fn type_annotation_text(&self, source: &[u8]) -> Option<String> {
        extract_type_annotation(*self, source)
    }
}

/// Build a callee name deterministically from an AST node.
///
/// Handles:
/// - `identifier` -> `foo`
/// - `field_access` / `member_expression` -> `a.b.c`
/// - `scoped_identifier` (Rust `Type::method`) -> `Type::method`
/// - `call_expression` -> recurse into callee
pub fn build_callee_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier"
        | "property_identifier"
        | "shorthand_property_identifier"
        | "field_identifier"
        | "type_identifier"
        | "builtin_type" => node.utf8_text(source).ok().map(|s| s.to_string()),
        "field_access" | "field_expression" | "member_expression" | "property_access"
        | "member_access" => {
            let object = node
                .child_by_field_name("object")
                .or_else(|| node.child_by_field_name("value"))
                .or_else(|| node.child_by_field_name("receiver"))
                .or_else(|| {
                    let mut cursor = node.walk();
                    let children: Vec<Node> = node
                        .children(&mut cursor)
                        .filter(|c| c.is_named())
                        .collect();
                    children.first().copied()
                })?;
            let field = node
                .child_by_field_name("field")
                .or_else(|| node.child_by_field_name("property"))
                .or_else(|| node.child_by_field_name("name"))
                .or_else(|| {
                    let mut cursor = node.walk();
                    let children: Vec<Node> = node
                        .children(&mut cursor)
                        .filter(|c| c.is_named())
                        .collect();
                    children.get(1).copied()
                })?;
            let obj_name = build_callee_name(object, source)?;
            let field_name = field.utf8_text(source).ok()?;
            Some(format!("{}.{}", obj_name, field_name))
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let path = node.child_by_field_name("path")?;
            let name = node.child_by_field_name("name")?;
            let path_text = path.utf8_text(source).ok()?;
            let name_text = name.utf8_text(source).ok()?;
            Some(format!("{}::{}", path_text, name_text))
        }
        "call_expression" => {
            let func = node.child_by_field_name("function")?;
            build_callee_name(func, source)
        }
        "generic_type" | "type_identifier_with_generics" => {
            if let Some(inner) = node.child_by_field_name("type") {
                return build_callee_name(inner, source);
            }
            node.utf8_text(source).ok().map(|s| {
                // Strip generics deterministically without split('<') heuristic on raw text
                // by returning the base identifier before '<'
                if let Some(pos) = s.find('<') {
                    s[..pos].trim().to_string()
                } else {
                    s.trim().to_string()
                }
            })
        }
        _ => {
            // Fallback: try to extract identifier text directly if node is leaf-like,
            // but avoid string heuristics like split('(')
            if node.child_count() == 0 {
                return node.utf8_text(source).ok().map(|s| s.trim().to_string());
            }
            // For compound nodes without specific handling, attempt field-based
            // reconstruction
            if let Some(func) = node.child_by_field_name("function") {
                return build_callee_name(func, source);
            }
            if let Some(obj) = node.child_by_field_name("object") {
                if let Some(field) = node.child_by_field_name("field") {
                    let obj_name = build_callee_name(obj, source)?;
                    let field_name = field.utf8_text(source).ok()?;
                    return Some(format!("{}.{}", obj_name, field_name));
                }
            }
            None
        }
    }
}

/// Count call arguments deterministically via AST structure.
pub fn count_arguments(node: Node) -> Option<usize> {
    if node.kind() != "call_expression" {
        return None;
    }
    let args_node = node.child_by_field_name("arguments")?;
    let mut cursor = args_node.walk();
    let count = args_node
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .count();
    Some(count)
}

/// Extract type annotation deterministically via AST fields.
pub fn extract_type_annotation(node: Node, source: &[u8]) -> Option<String> {
    if let Some(type_node) = node.child_by_field_name("type") {
        if let Ok(text) = type_node.utf8_text(source) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(return_type) = node.child_by_field_name("return_type") {
        if let Ok(text) = return_type.utf8_text(source) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    // Fallback for parameter nodes where type is stored as `type` field with
    // colon separator, but we prefer structured access
    if let Some(ty) = node.child_by_field_name("type_annotation") {
        if let Ok(text) = ty.utf8_text(source) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Extract parameter name and optional type from a parameter node.
pub fn extract_parameter(node: Node, source: &[u8]) -> Option<(String, Option<String>)> {
    let (name_node, ty_node) = node.parameter_parts()?;
    let name_text = name_node.utf8_text(source).ok()?.trim().to_string();
    if name_text.is_empty() {
        return None;
    }
    let type_text = ty_node
        .and_then(|ty| ty.utf8_text(source).ok().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty());
    Some((name_text, type_text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstParser, set_language_resolver};
    use cce_types::language::Language;

    fn resolver(lang: &Language) -> Option<tree_sitter::Language> {
        match lang {
            Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
            Language::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
            Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            _ => None,
        }
    }

    #[test]
    fn test_build_callee_identifier() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "fn foo() { bar(); }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("parse");
        let root = tree.root_node();
        let mut found = None;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "call_expression" {
                found = Some(node);
                break;
            }
            let mut c = node.walk();
            let children: Vec<_> = node.children(&mut c).collect();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }
        let call = found.expect("call_expression found");
        let callee = call.child_by_field_name("function").unwrap();
        let name = build_callee_name(callee, code.as_bytes()).expect("name");
        assert_eq!(name, "bar");
        assert_eq!(count_arguments(call), Some(0));
    }

    #[test]
    fn test_build_callee_field_access() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "fn foo() { obj.method(1, 2); }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("parse");
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut call = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "call_expression" {
                // Find the one with dot
                if let Some(func) = node.child_by_field_name("function") {
                    if func.kind() == "field_expression" {
                        call = Some(node);
                        break;
                    }
                }
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        let call = call.expect("field call");
        let func = call.child_by_field_name("function").unwrap();
        let name = build_callee_name(func, code.as_bytes()).expect("name");
        // Rust field_expression uses `value.field` or `object.field` depending on grammar
        assert!(name == "obj.method" || name.contains("obj") && name.contains("method"));
        assert_eq!(count_arguments(call), Some(2));
    }

    #[test]
    fn test_build_callee_scoped_identifier() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "fn foo() { Vec::new(); }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("parse");
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut scoped = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "scoped_identifier" {
                scoped = Some(node);
                break;
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        let scoped = scoped.expect("scoped_identifier found");
        let name = build_callee_name(scoped, code.as_bytes()).expect("name");
        assert_eq!(name, "Vec::new");
    }

    #[test]
    fn test_count_arguments_nested() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "fn foo() { bar(a, foo(b, c), d); }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("parse");
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut call = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "call_expression" {
                if let Some(func) = node.child_by_field_name("function") {
                    if func.utf8_text(code.as_bytes()).unwrap_or("") == "bar" {
                        call = Some(node);
                        break;
                    }
                }
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        let call = call.expect("bar call");
        // Should be 3 arguments, correctly handling nested parentheses
        assert_eq!(count_arguments(call), Some(3));
    }

    #[test]
    fn test_extract_type_annotation_rust() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "fn foo(x: i32, y: Option<String>) -> bool { true }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("parse");
        let root = tree.root_node();
        // Find function_definition
        let mut stack = vec![root];
        let mut func = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "function_item" || node.kind() == "function_definition" {
                func = Some(node);
                break;
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        let func = func.expect("function found");
        // return type
        let ret = extract_type_annotation(func, code.as_bytes());
        assert!(ret.is_some());
        assert!(ret.unwrap().contains("bool"));
        // parameter
        let mut cursor = func.walk();
        for child in func.children(&mut cursor) {
            if child.kind() == "parameters" {
                let mut pc = child.walk();
                for param in child.children(&mut pc) {
                    if param.is_named() && param.kind().contains("parameter") {
                        if let Some((name, ty)) = extract_parameter(param, code.as_bytes()) {
                            if name == "x" {
                                assert_eq!(ty.as_deref(), Some("i32"));
                            }
                            if name == "y" {
                                assert!(ty.unwrap().contains("Option"));
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_parameter_parts_no_type() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        // Use Python where a parameter without annotation is valid and
        // represented as a `parameter` node with only a name.
        let code = "def foo(x):\n    pass";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::Python)
            .expect("parse");
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut param_node = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "identifier" && node.utf8_text(code.as_bytes()).unwrap() == "x" {
                // The parent should be a parameter-like node
                if let Some(parent) = node.parent() {
                    if parent.kind().contains("parameter") {
                        param_node = Some(parent);
                        break;
                    }
                }
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        // Fallback: search any parameter node directly
        if param_node.is_none() {
            let mut stack2 = vec![root];
            while let Some(node) = stack2.pop() {
                if node.kind().contains("parameter") {
                    param_node = Some(node);
                    break;
                }
                let mut c = node.walk();
                for child in {
                    let children: Vec<_> = node.children(&mut c).collect();
                    children.into_iter().rev()
                } {
                    stack2.push(child);
                }
            }
        }
        let param = param_node.expect("parameter found");
        let (name_node, ty) = param.parameter_parts().expect("parts");
        assert_eq!(name_node.utf8_text(code.as_bytes()).unwrap(), "x");
        assert!(ty.is_none());
    }

    #[test]
    fn test_build_callee_javascript_method() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "function main() { console.log('hello'); }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("parse");
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut call = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "call_expression" {
                call = Some(node);
                break;
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        let call = call.expect("call_expression found");
        let func = call.child_by_field_name("function").unwrap();
        let name = build_callee_name(func, code.as_bytes()).expect("name");
        assert_eq!(name, "console.log");
        assert_eq!(count_arguments(call), Some(1));
    }

    #[test]
    fn test_build_callee_typescript_typed_function() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "function add(x: number, y: number): number { return x + y; }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::TypeScript)
            .expect("parse");
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut func = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "function_declaration" || node.kind() == "function_item" {
                func = Some(node);
                break;
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        let func = func.expect("function found");
        let ret = extract_type_annotation(func, code.as_bytes());
        assert!(ret.is_some());
        let ret_text = ret.unwrap();
        // TypeScript grammar may include ':' in the type node
        assert!(
            ret_text.contains("number"),
            "expected return type to contain 'number', got: {ret_text}"
        );
        // Check parameter types
        let mut cursor = func.walk();
        for child in func.children(&mut cursor) {
            if child.kind() == "formal_parameters" || child.kind() == "parameters" {
                let mut pc = child.walk();
                for param in child.children(&mut pc) {
                    if param.is_named() && param.kind().contains("parameter") {
                        if let Some((name, ty)) = extract_parameter(param, code.as_bytes()) {
                            if name == "x" || name == "y" {
                                let ty_text = ty.expect("type annotation present");
                                assert!(
                                    ty_text.contains("number"),
                                    "expected parameter type to contain 'number', got: {ty_text}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_count_arguments_javascript() {
        set_language_resolver(resolver);
        let mut parser = AstParser::new();
        let code = "function main() { foo(1, bar(2, 3), 'a'); }";
        let (tree, _) = parser
            .parse_with_tree(code, &Language::JavaScript)
            .expect("parse");
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut call = None;
        while let Some(node) = stack.pop() {
            if node.kind() == "call_expression" {
                if let Some(func) = node.child_by_field_name("function") {
                    if func.kind() == "identifier"
                        && func.utf8_text(code.as_bytes()).unwrap_or("") == "foo"
                    {
                        call = Some(node);
                        break;
                    }
                }
            }
            let mut c = node.walk();
            for child in {
                let children: Vec<_> = node.children(&mut c).collect();
                children.into_iter().rev()
            } {
                stack.push(child);
            }
        }
        let call = call.expect("foo call");
        assert_eq!(count_arguments(call), Some(3));
    }
}
