//! Code block embedding text generation
//!
//! Processes code blocks from document files through tree-sitter parsing
//! to extract semantic identifiers for embedding vector generation.

use cce_types::language::Language;

/// Generate embedding text for a code block.
///
/// In the standalone `cce-document` crate the tree-sitter extraction path is
/// not available (it lives in `cce-parser-core`/`cce-parser`). All code blocks
/// therefore use the raw fallback, which preserves the full content for
/// indexing and remains compatible with existing tests.
pub fn code_block_embedding(code: &str, language_tag: Option<&str>, _max_tokens: usize) -> String {
    raw_code_full(code, language_tag)
}

/// Return full code content for small blocks or unsupported languages.
fn raw_code_full(code: &str, language_tag: Option<&str>) -> String {
    let lang = language_tag.unwrap_or("text");
    format!("Code ({}): {}", lang, code)
}

/// Map a markdown code fence language tag to a Language enum.
#[allow(dead_code)]
fn map_language_tag(tag: &str) -> Option<Language> {
    match tag.to_lowercase().as_str() {
        "rust" | "rs" => Some(Language::Rust),
        "python" | "py" => Some(Language::Python),
        "javascript" | "js" => Some(Language::JavaScript),
        "typescript" | "ts" => Some(Language::TypeScript),
        "tsx" => Some(Language::Tsx),
        "jsx" => Some(Language::Jsx),
        "go" | "golang" => Some(Language::Go),
        "java" => Some(Language::Java),
        "c" => Some(Language::C),
        "cpp" | "c++" => Some(Language::Cpp),
        "c#" | "csharp" | "cs" => Some(Language::CSharp),
        "ruby" | "rb" => Some(Language::Ruby),
        "php" => Some(Language::Php),
        "kotlin" | "kt" => Some(Language::Kotlin),
        "scala" => Some(Language::Scala),
        "dart" => Some(Language::Dart),
        "bash" | "sh" | "shell" | "zsh" => Some(Language::Bash),
        "lua" => Some(Language::Lua),
        "html" => Some(Language::Html),
        "css" => Some(Language::Css),
        "vue" => Some(Language::Vue),
        "svelte" => Some(Language::Svelte),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_block_full_content() {
        let code = "let x = 42;\nprintln!(\"{}\", x);";
        let result = code_block_embedding(code, Some("rust"), 512);
        assert!(result.contains("Code (rust)"));
        assert!(result.contains("let x = 42"));
        assert!(result.contains("println!"));
    }

    #[test]
    fn test_large_rust_block() {
        let code = r#"
fn main() {
    println!("Hello");
}

struct Config {
    host: String,
    port: u16,
}

impl Config {
    fn new() -> Self {
        Self { host: "localhost".into(), port: 8080 }
    }
}

enum Status {
    Active,
    Inactive,
}

trait Display {
    fn display(&self) -> String;
}

mod utils {
    pub fn helper() {}
}
"#
        .trim();

        let result = code_block_embedding(code, Some("rust"), 512);
        assert!(result.contains("main"));
        assert!(result.contains("Config"));
        assert!(result.contains("Status"));
        assert!(result.contains("Display"));
    }

    #[test]
    fn test_large_python_block() {
        let code = r#"
class User:
    def __init__(self, name):
        self.name = name

    def greet(self):
        return f"Hello, {self.name}"

    def to_dict(self):
        return {"name": self.name}

def create_user(name):
    return User(name)

def delete_user(user_id):
    pass

class Admin(User):
    def __init__(self, name, role):
        super().__init__(name)
        self.role = role

    def has_permission(self, perm):
        return perm in self.role
"#
        .trim();

        let result = code_block_embedding(code, Some("python"), 512);
        assert!(result.contains("User"), "got: {}", result);
        assert!(result.contains("create_user"), "got: {}", result);
        assert!(result.contains("Admin"), "got: {}", result);
    }

    #[test]
    fn test_unsupported_language() {
        let code = "some config value\n".repeat(20);
        let result = code_block_embedding(code.trim(), Some("toml"), 512);
        assert!(result.contains("Code (toml)"));
        assert!(result.contains("some config value"));
    }

    #[test]
    fn test_map_language_tag() {
        assert_eq!(map_language_tag("rust"), Some(Language::Rust));
        assert_eq!(map_language_tag("Python"), Some(Language::Python));
        assert_eq!(map_language_tag("tsx"), Some(Language::Tsx));
        assert_eq!(map_language_tag("toml"), None);
        assert_eq!(map_language_tag("diff"), None);
    }
}
