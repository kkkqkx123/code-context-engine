//! C/C++ header file handler
//!
//! Parses header files to extract public API declarations for external
//! library resolution. The parser is intentionally lightweight and tolerant of
//! incomplete input.

use std::collections::HashMap;
use std::path::Path;

use super::{ExportedSymbol, ModuleInfo, ModuleType};
use cce_types::entity::EntityKind;
use cce_types::language::Language;

/// Handler for C/C++ header files.
///
/// Supports both system headers (`<stdio.h>`) and project headers (`"myheader.h"`).
/// Declarations are extracted via lightweight pattern matching rather than full
/// tree-sitter parsing, keeping the handler fast and dependency-free.
#[derive(Debug, Default)]
pub struct HeaderFileHandler {
    declarations: HashMap<String, FunctionDeclaration>,
}

/// Function declaration extracted from a header.
#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub name: String,
    pub return_type: Option<String>,
    pub params: Vec<String>,
    pub is_system: bool,
}

/// Parsed namespace with nesting information.
#[derive(Debug, Clone)]
pub struct NamespaceDeclaration {
    /// Full qualified name (e.g., "A::B::C").
    pub qualified_name: String,
    /// Nesting segments (e.g., ["A", "B", "C"]).
    pub segments: Vec<String>,
    /// Start byte offset of the namespace body.
    pub start_byte: usize,
    /// End byte offset (brace-matched).
    pub end_byte: usize,
}

impl HeaderFileHandler {
    pub fn new() -> Self {
        Self {
            declarations: HashMap::new(),
        }
    }

    /// Parse a header file and return aggregated [`ModuleInfo`].
    ///
    /// When `path` points to a directory, all `*.h`/`*.hpp` files inside are
    /// collected; otherwise the single file is parsed.
    pub fn parse_header(
        &mut self,
        path: &Path,
        language: Language,
    ) -> Result<ModuleInfo, crate::error::RelationError> {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "headers".to_string());
        let mut info = ModuleInfo::new(
            name.clone(),
            path.to_path_buf(),
            language,
            ModuleType::Header,
        );

        let header_paths = if path.is_dir() {
            collect_headers(path)
        } else {
            vec![path.to_path_buf()]
        };

        for header_path in header_paths {
            let content = std::fs::read_to_string(&header_path).map_err(|e| {
                crate::error::RelationError::Index(crate::error::IndexError::InconsistentState(
                    format!("failed to read header {}: {e}", header_path.display()),
                ))
            })?;
            let declarations = Self::extract_declarations(&content);
            for decl in &declarations {
                self.declarations.insert(decl.name.clone(), decl.clone());
                info.exports.push(
                    ExportedSymbol::new(decl.name.clone(), EntityKind::Function)
                        .with_source_file(header_path.to_string_lossy().to_string()),
                );
            }
            for type_name in Self::extract_type_declarations(&content) {
                info.exports.push(
                    ExportedSymbol::new(type_name, EntityKind::Class)
                        .with_source_file(header_path.to_string_lossy().to_string()),
                );
            }
            for ns in Self::extract_namespace_declarations_nested(&content) {
                info.exports.push(
                    ExportedSymbol::new(ns.qualified_name.clone(), EntityKind::Namespace)
                        .with_source_file(header_path.to_string_lossy().to_string()),
                );
                for i in 1..ns.segments.len() {
                    let prefix = ns.segments[..i].join("::");
                    if !info.exports.iter().any(|e| e.name == prefix) {
                        info.exports.push(
                            ExportedSymbol::new(prefix, EntityKind::Namespace)
                                .with_source_file(header_path.to_string_lossy().to_string()),
                        );
                    }
                }
            }
            for td in Self::extract_typedef_declarations(&content) {
                info.exports.push(
                    ExportedSymbol::new(td, EntityKind::Class)
                        .with_source_file(header_path.to_string_lossy().to_string()),
                );
            }
            for mac in Self::extract_macro_declarations(&content) {
                info.exports.push(
                    ExportedSymbol::new(mac, EntityKind::Constant)
                        .with_source_file(header_path.to_string_lossy().to_string()),
                );
            }
            if has_ifdef_guards(&content) {
                // Mark conditional exports if needed
            }
        }

        if info.exports.is_empty() {
            // Ensure at least the header stem is available as a module marker
            info.exports
                .push(ExportedSymbol::new(name.clone(), EntityKind::Module));
        }

        Ok(info)
    }

    /// Extract function declarations from header content.
    ///
    /// Matches patterns like:
    /// - `int foo(int a, char *b);`
    /// - `void bar(void);`
    /// - `extern int baz(double x);`
    /// - `template<typename T> T foo(T x);`
    pub fn extract_declarations(content: &str) -> Vec<FunctionDeclaration> {
        let mut results = Vec::new();
        let without_comments = strip_comments(content);
        let func_re = regex::Regex::new(
            r"(?m)^\s*(?:template\s*<[^;]*>\s*)?(?:extern\s+)?(?:static\s+|inline\s+|constexpr\s+)*(?:[A-Za-z_][A-Za-z0-9_:<>,\s\*&]*\s+)+([A-Za-z_][A-Za-z0-9_]*)\s*\([^;{]*\)\s*(?:const\s*)?;",
        );
        let Ok(re) = func_re else {
            return results;
        };
        for caps in re.captures_iter(&without_comments) {
            if let Some(name_match) = caps.get(1) {
                let name = name_match.as_str().to_string();
                if is_c_keyword(&name) {
                    continue;
                }
                let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                let params = extract_params(full);
                let return_type = extract_return_type(full, &name);
                results.push(FunctionDeclaration {
                    name,
                    return_type,
                    params,
                    is_system: false,
                });
            }
        }
        results
    }

    /// Extract struct/class/enum/namespace/typedef/using type names from header content.
    pub fn extract_type_declarations(content: &str) -> Vec<String> {
        let without_comments = strip_comments(content);
        let mut names = Vec::new();
        for re_str in &[
            r"\b(?:struct|class|enum|union)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
            r"\bnamespace\s+([A-Za-z_][A-Za-z0-9_]*)\b",
            r"\btypedef\s+(?:struct\s+)?(?:[A-Za-z_][A-Za-z0-9_\s\*]+)\s+([A-Za-z_][A-Za-z0-9_]*)\s*;",
            r"\busing\s+([A-Za-z_][A-Za-z0-9_]*)\s*=",
        ] {
            let Ok(re) = regex::Regex::new(re_str) else {
                continue;
            };
            for caps in re.captures_iter(&without_comments) {
                if let Some(m) = caps.get(1) {
                    let name = m.as_str().to_string();
                    if !names.contains(&name) && !is_c_keyword(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    pub fn extract_namespace_declarations(content: &str) -> Vec<String> {
        Self::extract_namespace_declarations_nested(content)
            .into_iter()
            .map(|d| d.qualified_name)
            .collect()
    }

    /// Parsed namespace with nesting information.
    pub fn extract_namespace_declarations_nested(content: &str) -> Vec<NamespaceDeclaration> {
        let without_comments = strip_comments(content);
        let Ok(ns_re) = regex::Regex::new(r"\bnamespace\s+([A-Za-z_][A-Za-z0-9_:]*)\b") else {
            return Vec::new();
        };
        let mut results = Vec::new();
        let mut stack: Vec<(String, usize)> = Vec::new();

        for caps in ns_re.captures_iter(&without_comments) {
            let Some(name_match) = caps.get(1) else {
                continue;
            };
            let name = name_match.as_str().to_string();
            let Some(full_match) = caps.get(0) else {
                continue;
            };
            let start = full_match.start();

            while let Some((_, prev_end)) = stack.last() {
                if *prev_end < start {
                    stack.pop();
                } else {
                    break;
                }
            }

            let qualified = if let Some((parent, _)) = stack.last() {
                format!("{}::{}", parent, name)
            } else {
                name.clone()
            };

            let segments: Vec<String> = qualified.split("::").map(String::from).collect();

            let body_start = match find_char_after(&without_comments, '{', start) {
                Some(pos) => pos,
                None => continue,
            };
            let body_end = match find_matching_brace(&without_comments, body_start) {
                Some(pos) => pos,
                None => without_comments.len(),
            };

            results.push(NamespaceDeclaration {
                qualified_name: qualified.clone(),
                segments,
                start_byte: body_start,
                end_byte: body_end,
            });

            stack.push((qualified, body_end));
        }

        // Deduplicate by qualified name while preserving order
        let mut seen = std::collections::HashSet::new();
        results.retain(|d| seen.insert(d.qualified_name.clone()));
        results
    }

    pub fn extract_typedef_declarations(content: &str) -> Vec<String> {
        let without_comments = strip_comments(content);
        let Ok(re) = regex::Regex::new(r"\btypedef\s+[^;]*\b([A-Za-z_][A-Za-z0-9_]*)\s*;") else {
            return Vec::new();
        };
        re.captures_iter(&without_comments)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .filter(|n| !is_c_keyword(n))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn extract_macro_declarations(content: &str) -> Vec<String> {
        let Ok(re) = regex::Regex::new(r"(?m)^\s*#\s*define\s+([A-Za-z_][A-Za-z0-9_]*)\b") else {
            return Vec::new();
        };
        re.captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Get cached declarations.
    pub fn declarations(&self) -> &HashMap<String, FunctionDeclaration> {
        &self.declarations
    }

    /// Check if a symbol was declared in the parsed headers.
    pub fn contains(&self, name: &str) -> bool {
        self.declarations.contains_key(name)
    }
}

fn collect_headers(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut headers = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "h" || ext == "hpp" {
                        headers.push(path);
                    }
                }
            }
        }
    }
    headers.sort();
    headers
}

fn strip_comments(content: &str) -> String {
    // Remove // line comments
    let line_re = regex::Regex::new(r"//[^\n]*").unwrap();
    let without_line = line_re.replace_all(content, "");
    // Remove /* block comments */
    let block_re = regex::Regex::new(r"/\*[\s\S]*?\*/").unwrap();
    block_re.replace_all(&without_line, "").to_string()
}

fn find_char_after(content: &str, ch: char, start: usize) -> Option<usize> {
    content[start..].find(ch).map(|pos| start + pos)
}

fn find_matching_brace(content: &str, open_pos: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    if bytes.get(open_pos) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(open_pos) {
        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

fn extract_params(decl: &str) -> Vec<String> {
    let start = decl.find('(');
    let end = decl.rfind(')');
    match (start, end) {
        (Some(s), Some(e)) if e > s => {
            let inner = &decl[s + 1..e];
            if inner.trim().is_empty() || inner.trim() == "void" {
                Vec::new()
            } else {
                inner
                    .split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            }
        }
        _ => Vec::new(),
    }
}

fn extract_return_type(decl: &str, func_name: &str) -> Option<String> {
    // Text before the function name is the return type
    if let Some(pos) = decl.find(func_name) {
        let before = decl[..pos].trim();
        // Take the last type token segment before the name
        let ret = before
            .trim_start_matches("extern")
            .trim_start_matches("static")
            .trim_start_matches("inline")
            .trim()
            .to_string();
        if ret.is_empty() { None } else { Some(ret) }
    } else {
        None
    }
}

fn is_c_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "for" | "while" | "return" | "sizeof" | "typedef" | "struct" | "enum" | "union"
    )
}

fn has_ifdef_guards(content: &str) -> bool {
    content.contains("#ifdef")
        || content.contains("#ifndef")
        || content.contains("#if ")
        || content.contains("#endif")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_declarations() {
        let content = r#"
            int foo(int a, char *b);
            void bar(void);
            extern double baz(double x, double y);
        "#;
        let decls = HeaderFileHandler::extract_declarations(content);
        let names: Vec<_> = decls.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"foo"), "foo should be extracted: {names:?}");
        assert!(names.contains(&"bar"), "bar should be extracted: {names:?}");
        assert!(names.contains(&"baz"), "baz should be extracted: {names:?}");
    }

    #[test]
    fn test_extract_type_declarations() {
        let content = r#"
            struct MyStruct { int x; };
            class MyClass { public: void method(); };
            enum Color { Red, Green, Blue };
        "#;
        let types = HeaderFileHandler::extract_type_declarations(content);
        assert!(types.contains(&"MyStruct".to_string()));
        assert!(types.contains(&"MyClass".to_string()));
        assert!(types.contains(&"Color".to_string()));
    }

    #[test]
    fn test_header_handler_parse_temp_file() {
        let dir = std::env::temp_dir().join("cce_header_test");
        let _ = std::fs::create_dir_all(&dir);
        let header = dir.join("test.h");
        let _ = std::fs::write(
            &header,
            "int my_function(int x);\nstruct MyStruct { int a; };\n",
        );
        let mut handler = HeaderFileHandler::new();
        let info = handler
            .parse_header(&header, cce_types::language::Language::Cpp)
            .expect("parse should succeed");
        assert!(info.exports.iter().any(|e| e.name == "my_function"));
        assert!(info.exports.iter().any(|e| e.name == "MyStruct"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_strip_comments() {
        let content = "int foo(); // comment\n /* block */ int bar();";
        let decls = HeaderFileHandler::extract_declarations(content);
        let names: Vec<_> = decls.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
    }
}
