//! Test fixture loader for E2E tests
//!
//! Provides utilities for loading and managing test fixtures from the fixtures directory.

use std::io;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Fixture category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureCategory {
    /// Rust project fixtures
    Rust,
    /// Python project fixtures
    Python,
    /// Java project fixtures
    Java,
    /// TypeScript project fixtures
    TypeScript,
    /// C# project fixtures
    CSharp,
    /// C project fixtures
    C,
    /// C++ project fixtures
    Cpp,
    /// Scala project fixtures
    Scala,
    /// Dart project fixtures
    Dart,
    /// Go project fixtures
    Go,
    /// Kotlin project fixtures
    Kotlin,
    /// PHP project fixtures
    Php,
    /// Ruby project fixtures
    Ruby,
    /// JavaScript project fixtures
    JavaScript,
    /// Bash project fixtures
    Bash,
    /// Lua project fixtures
    Lua,
    /// Multi-language project fixtures
    MultiLanguage,
    /// Document fixtures (markdown, plain text, logs, config files)
    Documents,
    /// Standalone tool fixtures
    Tools,
}

impl FixtureCategory {
    /// Get directory name for this category
    pub fn dir_name(&self) -> &'static str {
        match self {
            FixtureCategory::Rust => "rust",
            FixtureCategory::Python => "python",
            FixtureCategory::Java => "java",
            FixtureCategory::TypeScript => "typescript",
            FixtureCategory::CSharp => "csharp",
            FixtureCategory::C => "c",
            FixtureCategory::Cpp => "cpp",
            FixtureCategory::Scala => "scala",
            FixtureCategory::Dart => "dart",
            FixtureCategory::Go => "go",
            FixtureCategory::Kotlin => "kotlin",
            FixtureCategory::Php => "php",
            FixtureCategory::Ruby => "ruby",
            FixtureCategory::JavaScript => "javascript",
            FixtureCategory::Bash => "bash",
            FixtureCategory::Lua => "lua",
            FixtureCategory::MultiLanguage => "multi_language",
            FixtureCategory::Documents => "documents",
            FixtureCategory::Tools => "tools",
        }
    }
}

/// Fixture specification
#[derive(Debug, Clone)]
pub struct FixtureSpec {
    /// Category of the fixture
    pub category: FixtureCategory,
    /// Subdirectory name (e.g., "basic")
    pub subdirectory: String,
}

impl FixtureSpec {
    /// Create a new fixture specification
    pub fn new(category: FixtureCategory, subdirectory: impl Into<String>) -> Self {
        Self {
            category,
            subdirectory: subdirectory.into(),
        }
    }

    /// Root directory of the fixture's source tree on disk.
    ///
    /// Judgment line ranges are anchored to the on-disk fixture, so tooling
    /// that verifies them needs the original tree rather than the temporary
    /// copy a test run works on.
    pub fn source_root(&self) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(self.category.dir_name());
        if !self.subdirectory.is_empty() {
            path = path.join(&self.subdirectory);
        }
        path
    }

    /// Rust basic project fixture
    pub fn rust_basic() -> Self {
        Self::new(FixtureCategory::Rust, "basic")
    }

    /// Rust tree-sitter query regression fixture
    pub fn rust_query_regression() -> Self {
        Self::new(FixtureCategory::Rust, "query_regression")
    }

    /// Rust review fixture group
    pub fn rust_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Rust, format!("review/{}", name.into()))
    }

    /// Rust once_cell review fixture
    pub fn rust_once_cell() -> Self {
        Self::rust_review("once_cell")
    }

    /// Rust distractor fixture (unrelated code for negative samples)
    pub fn rust_distractor() -> Self {
        Self::new(FixtureCategory::Rust, "distractor")
    }

    /// Rust index_sidecar review fixture
    pub fn rust_index_sidecar() -> Self {
        Self::rust_review("index_sidecar")
    }

    /// Rust relation_demo review fixture
    pub fn rust_relation_demo() -> Self {
        Self::rust_review("relation_demo")
    }

    /// Rust relation_diamond review fixture
    pub fn rust_relation_diamond() -> Self {
        Self::rust_review("relation_diamond")
    }

    /// Rust re-export chain review fixture
    pub fn rust_review_re_export() -> Self {
        Self::rust_review("re_export")
    }

    /// Rust ripgrep review fixture
    pub fn rust_ripgrep() -> Self {
        Self::rust_review("ripgrep")
    }

    /// Rust type inference generics fixture
    pub fn rust_type_inference_generics() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/generics")
    }

    /// Rust type inference control flow fixture
    pub fn rust_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/control_flow")
    }

    /// Rust type inference wildcard import fixture
    pub fn rust_type_inference_wildcard() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/wildcard")
    }

    /// Rust type inference closure fixture
    pub fn rust_type_inference_closure() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/closure")
    }

    /// Rust type inference destructuring fixture
    pub fn rust_type_inference_destructuring() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/destructuring")
    }

    /// Rust type inference lifetime fixture
    pub fn rust_type_inference_lifetime() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/lifetime")
    }

    /// Rust type inference impl-Self fixture
    pub fn rust_type_inference_impl_self() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/impl_self")
    }

    /// Rust type inference reference fixture
    pub fn rust_type_inference_reference() -> Self {
        Self::new(FixtureCategory::Rust, "type_inference/reference")
    }

    /// Python basic project fixture
    pub fn python_basic() -> Self {
        Self::new(FixtureCategory::Python, "basic")
    }

    /// Python tree-sitter query regression fixture
    pub fn python_query_regression() -> Self {
        Self::new(FixtureCategory::Python, "query_regression")
    }

    /// Python type inference type hints fixture
    pub fn python_type_inference_type_hints() -> Self {
        Self::new(FixtureCategory::Python, "type_inference/type_hints")
    }

    /// Python type inference control flow fixture
    pub fn python_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::Python, "type_inference/control_flow")
    }

    /// Python type inference cross-file fixture
    pub fn python_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Python, "type_inference/cross_file")
    }

    /// Python type inference visibility fixture
    pub fn python_type_inference_visibility() -> Self {
        Self::new(FixtureCategory::Python, "type_inference/visibility")
    }

    /// Python type inference lambda fixture
    pub fn python_type_inference_lambda() -> Self {
        Self::new(FixtureCategory::Python, "type_inference/lambda")
    }

    /// Python type inference discriminated union fixture
    pub fn python_type_inference_discriminated_union() -> Self {
        Self::new(
            FixtureCategory::Python,
            "type_inference/discriminated_union",
        )
    }

    /// Python type inference destructuring fixture
    pub fn python_type_inference_destructuring() -> Self {
        Self::new(FixtureCategory::Python, "type_inference/destructuring")
    }

    /// Python type inference negated checks fixture
    pub fn python_type_inference_negated_checks() -> Self {
        Self::new(FixtureCategory::Python, "type_inference/negated_checks")
    }

    /// Java basic project fixture
    pub fn java_basic() -> Self {
        Self::new(FixtureCategory::Java, "basic")
    }

    /// Java tree-sitter query regression fixture
    pub fn java_query_regression() -> Self {
        Self::new(FixtureCategory::Java, "query_regression")
    }

    /// Java type inference generics fixture
    pub fn java_type_inference_generics() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/generics")
    }

    /// Java type inference control flow fixture
    pub fn java_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/control_flow")
    }

    /// Java type inference visibility fixture
    pub fn java_type_inference_visibility() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/visibility")
    }

    /// Java type inference overloads fixture
    pub fn java_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/overloads")
    }

    /// Java type inference cross-file fixture
    pub fn java_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/cross_file")
    }

    /// Java type inference lambda fixture
    pub fn java_type_inference_lambda() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/lambda")
    }

    /// Java type inference discriminated union fixture
    pub fn java_type_inference_discriminated_union() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/discriminated_union")
    }

    /// Java type inference pattern matching fixture
    pub fn java_type_inference_pattern_matching() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/pattern_matching")
    }

    /// Java type inference var inference fixture
    pub fn java_type_inference_var_inference() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/var_inference")
    }

    /// Java type inference negated checks fixture
    pub fn java_type_inference_negated_checks() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/negated_checks")
    }

    /// Java type inference control positions fixture
    pub fn java_type_inference_control_positions() -> Self {
        Self::new(FixtureCategory::Java, "type_inference/control_positions")
    }

    /// Java Spring Boot minimal demo fixture
    pub fn java_spring_boot() -> Self {
        Self::java_review("springboot-minimal-demo")
    }

    /// Java jackson-core review fixture
    pub fn java_jackson_core() -> Self {
        Self::java_review("jackson-core")
    }

    /// Java review fixture group
    pub fn java_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Java, format!("review/{}", name.into()))
    }

    /// Java index_sidecar review fixture
    pub fn java_index_sidecar() -> Self {
        Self::java_review("index_sidecar")
    }

    /// Python review fixture group
    pub fn python_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Python, format!("review/{}", name.into()))
    }

    /// Python index_sidecar review fixture
    pub fn python_index_sidecar() -> Self {
        Self::python_review("index_sidecar")
    }

    /// Python Flask review fixture
    pub fn python_flask() -> Self {
        Self::python_review("flask")
    }

    /// Python re-export chain review fixture
    pub fn python_review_re_export() -> Self {
        Self::python_review("re_export")
    }

    /// Python wildcard import review fixture
    pub fn python_review_wildcard() -> Self {
        Self::python_review("wildcard")
    }

    /// TypeScript basic project fixture (class, functions, module imports)
    pub fn typescript_basic() -> Self {
        Self::new(FixtureCategory::TypeScript, "basic")
    }

    /// TypeScript tree-sitter query regression fixture
    pub fn typescript_query_regression() -> Self {
        Self::new(FixtureCategory::TypeScript, "query_regression")
    }

    /// TypeScript review fixture group
    pub fn typescript_review(name: impl Into<String>) -> Self {
        Self::new(
            FixtureCategory::TypeScript,
            format!("review/{}", name.into()),
        )
    }

    /// TypeScript index_sidecar review fixture
    pub fn typescript_index_sidecar() -> Self {
        Self::typescript_review("index_sidecar")
    }

    /// TypeScript type inference generics fixture
    pub fn typescript_type_inference_generics() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/generics")
    }

    /// TypeScript type inference unions fixture
    pub fn typescript_type_inference_unions() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/unions")
    }

    /// TypeScript type inference cross-file fixture
    pub fn typescript_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/cross_file")
    }

    /// TypeScript re-export chain review fixture
    pub fn typescript_review_re_export() -> Self {
        Self::typescript_review("re_export")
    }

    /// TypeScript wildcard import review fixture
    pub fn typescript_review_wildcard() -> Self {
        Self::typescript_review("wildcard")
    }

    /// TypeScript type inference overloads fixture
    pub fn typescript_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/overloads")
    }

    /// TypeScript type inference visibility fixture
    pub fn typescript_type_inference_visibility() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/visibility")
    }

    /// TypeScript type inference lambda fixture
    pub fn typescript_type_inference_lambda() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/lambda")
    }

    /// TypeScript type inference destructuring fixture
    pub fn typescript_type_inference_destructuring() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/destructuring")
    }

    /// TypeScript type inference negated checks fixture
    pub fn typescript_type_inference_negated_checks() -> Self {
        Self::new(FixtureCategory::TypeScript, "type_inference/negated_checks")
    }

    /// TypeScript type inference control positions fixture
    pub fn typescript_type_inference_control_positions() -> Self {
        Self::new(
            FixtureCategory::TypeScript,
            "type_inference/control_positions",
        )
    }

    /// Multi-language project fixture
    pub fn multi_language() -> Self {
        Self::new(FixtureCategory::MultiLanguage, "")
    }

    /// C# basic project fixture
    pub fn csharp_basic() -> Self {
        Self::new(FixtureCategory::CSharp, "basic")
    }

    /// C# tree-sitter query regression fixture
    pub fn csharp_query_regression() -> Self {
        Self::new(FixtureCategory::CSharp, "query_regression")
    }

    /// C# type inference generics fixture
    pub fn csharp_type_inference_generics() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/generics")
    }

    /// C# type inference control flow fixture
    pub fn csharp_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/control_flow")
    }

    /// C# type inference overloads fixture
    pub fn csharp_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/overloads")
    }

    /// C# type inference cross-file fixture
    pub fn csharp_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/cross_file")
    }

    /// C# type inference visibility fixture
    pub fn csharp_type_inference_visibility() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/visibility")
    }

    /// C# type inference lambda fixture
    pub fn csharp_type_inference_lambda() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/lambda")
    }

    /// C# type inference discriminated union fixture
    pub fn csharp_type_inference_discriminated_union() -> Self {
        Self::new(
            FixtureCategory::CSharp,
            "type_inference/discriminated_union",
        )
    }

    /// C# type inference is-not fixture
    pub fn csharp_type_inference_is_not() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/is_not")
    }

    /// C# type inference control positions fixture
    pub fn csharp_type_inference_control_positions() -> Self {
        Self::new(FixtureCategory::CSharp, "type_inference/control_positions")
    }

    /// C# review fixture group
    pub fn csharp_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::CSharp, format!("review/{}", name.into()))
    }

    /// C# MediatR review fixture
    pub fn csharp_mediatr() -> Self {
        Self::csharp_review("MediatR")
    }

    /// Go basic project fixture
    pub fn go_basic() -> Self {
        Self::new(FixtureCategory::Go, "basic")
    }

    /// Go tree-sitter query regression fixture
    pub fn go_query_regression() -> Self {
        Self::new(FixtureCategory::Go, "query_regression")
    }

    /// Go type inference interfaces fixture
    pub fn go_type_inference_interfaces() -> Self {
        Self::new(FixtureCategory::Go, "type_inference/interfaces")
    }

    /// Go type inference control flow fixture
    pub fn go_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::Go, "type_inference/control_flow")
    }

    /// Go type inference cross-file fixture
    pub fn go_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Go, "type_inference/cross_file")
    }

    /// Go type inference visibility fixture
    pub fn go_type_inference_visibility() -> Self {
        Self::new(FixtureCategory::Go, "type_inference/visibility")
    }

    /// Go type inference type assertion fixture
    pub fn go_type_inference_type_assertion() -> Self {
        Self::new(FixtureCategory::Go, "type_inference/type_assertion")
    }

    /// Go review fixture group
    pub fn go_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Go, format!("review/{}", name.into()))
    }

    /// Go gin review fixture
    pub fn go_gin() -> Self {
        Self::go_review("gin")
    }

    /// Kotlin basic project fixture
    pub fn kotlin_basic() -> Self {
        Self::new(FixtureCategory::Kotlin, "basic")
    }

    /// Kotlin tree-sitter query regression fixture
    pub fn kotlin_query_regression() -> Self {
        Self::new(FixtureCategory::Kotlin, "query_regression")
    }

    /// Kotlin review fixture group
    pub fn kotlin_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Kotlin, format!("review/{}", name.into()))
    }

    /// Kotlin coroutines review fixture
    pub fn kotlin_review_coroutines() -> Self {
        Self::kotlin_review("coroutines")
    }

    /// PHP basic project fixture
    pub fn php_basic() -> Self {
        Self::new(FixtureCategory::Php, "basic")
    }

    /// PHP tree-sitter query regression fixture
    pub fn php_query_regression() -> Self {
        Self::new(FixtureCategory::Php, "query_regression")
    }

    /// PHP review fixture group
    pub fn php_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Php, format!("review/{}", name.into()))
    }

    /// PHP namespace/trait review fixture
    pub fn php_review_namespace_trait() -> Self {
        Self::php_review("namespace_trait")
    }

    /// Ruby basic project fixture
    pub fn ruby_basic() -> Self {
        Self::new(FixtureCategory::Ruby, "basic")
    }

    /// Ruby tree-sitter query regression fixture
    pub fn ruby_query_regression() -> Self {
        Self::new(FixtureCategory::Ruby, "query_regression")
    }

    /// Ruby review fixture group
    pub fn ruby_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Ruby, format!("review/{}", name.into()))
    }

    /// Ruby mixin review fixture
    pub fn ruby_review_mixin() -> Self {
        Self::ruby_review("mixin")
    }

    /// JavaScript basic project fixture (class, functions, CommonJS imports)
    pub fn javascript_basic() -> Self {
        Self::new(FixtureCategory::JavaScript, "basic")
    }

    /// JavaScript tree-sitter query regression fixture
    pub fn javascript_query_regression() -> Self {
        Self::new(FixtureCategory::JavaScript, "query_regression")
    }

    /// JavaScript review fixture group
    pub fn javascript_review(name: impl Into<String>) -> Self {
        Self::new(
            FixtureCategory::JavaScript,
            format!("review/{}", name.into()),
        )
    }

    /// JavaScript Express review fixture
    pub fn javascript_express() -> Self {
        Self::javascript_review("express")
    }

    /// JavaScript re-export chain review fixture
    pub fn javascript_review_re_export() -> Self {
        Self::javascript_review("re_export")
    }

    /// Document fixture group (markdown / plain text / logs / config files)
    pub fn documents() -> Self {
        Self::new(FixtureCategory::Documents, "")
    }

    /// File-fold tool fixture group
    pub fn tools_file_fold() -> Self {
        Self::new(FixtureCategory::Tools, "file-fold")
    }

    /// C basic project fixture (header/source separation)
    pub fn c_basic() -> Self {
        Self::new(FixtureCategory::C, "basic")
    }

    /// C tree-sitter query regression fixture
    pub fn c_query_regression() -> Self {
        Self::new(FixtureCategory::C, "query_regression")
    }

    /// C review fixture group
    pub fn c_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::C, format!("review/{}", name.into()))
    }

    /// C macros review fixture
    pub fn c_review_macros() -> Self {
        Self::c_review("macros")
    }

    /// C type inference declarations fixture
    pub fn c_type_inference_declarations() -> Self {
        Self::new(FixtureCategory::C, "type_inference/declarations")
    }

    /// Bash basic project fixture (source/function)
    pub fn bash_basic() -> Self {
        Self::new(FixtureCategory::Bash, "basic")
    }

    /// Bash tree-sitter query regression fixture
    pub fn bash_query_regression() -> Self {
        Self::new(FixtureCategory::Bash, "query_regression")
    }

    /// Bash review fixture group
    pub fn bash_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Bash, format!("review/{}", name.into()))
    }

    /// Bash pipeline review fixture
    pub fn bash_review_pipeline() -> Self {
        Self::bash_review("pipeline")
    }

    /// Bash type inference variables fixture
    pub fn bash_type_inference_variables() -> Self {
        Self::new(FixtureCategory::Bash, "type_inference/variables")
    }

    /// Lua basic project fixture (require/function)
    pub fn lua_basic() -> Self {
        Self::new(FixtureCategory::Lua, "basic")
    }

    /// Lua tree-sitter query regression fixture
    pub fn lua_query_regression() -> Self {
        Self::new(FixtureCategory::Lua, "query_regression")
    }

    /// Lua review fixture group
    pub fn lua_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Lua, format!("review/{}", name.into()))
    }

    /// Lua closure review fixture
    pub fn lua_review_closure() -> Self {
        Self::lua_review("closure")
    }

    /// Lua type inference variables fixture
    pub fn lua_type_inference_variables() -> Self {
        Self::new(FixtureCategory::Lua, "type_inference/variables")
    }

    /// C++ basic project fixture (header/source separation, class, calls)
    pub fn cpp_basic() -> Self {
        Self::new(FixtureCategory::Cpp, "basic")
    }

    /// C++ tree-sitter query regression fixture
    pub fn cpp_query_regression() -> Self {
        Self::new(FixtureCategory::Cpp, "query_regression")
    }

    /// C++ review fixture group
    pub fn cpp_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Cpp, format!("review/{}", name.into()))
    }

    /// C++ templates review fixture
    pub fn cpp_review_templates() -> Self {
        Self::cpp_review("templates")
    }

    /// C++ media-server review fixture (multi-protocol streaming: FLV/MOV/RTSP/RTP/RTMP/HLS/MPEG/SIP)
    pub fn cpp_media_server() -> Self {
        Self::cpp_review("media-server")
    }

    /// C++ type inference declarations fixture
    pub fn cpp_type_inference_declarations() -> Self {
        Self::new(FixtureCategory::Cpp, "type_inference/declarations")
    }

    /// C++ type inference overloads fixture
    pub fn cpp_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::Cpp, "type_inference/overloads")
    }

    /// Kotlin type inference generics fixture
    pub fn kotlin_type_inference_generics() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/generics")
    }

    /// Kotlin type inference control flow fixture
    pub fn kotlin_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/control_flow")
    }

    /// Kotlin type inference cross-file fixture
    pub fn kotlin_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/cross_file")
    }

    /// Kotlin type inference overloads fixture
    pub fn kotlin_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/overloads")
    }

    /// Kotlin type inference visibility fixture
    pub fn kotlin_type_inference_visibility() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/visibility")
    }

    /// Kotlin type inference lambda fixture
    pub fn kotlin_type_inference_lambda() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/lambda")
    }

    /// Kotlin type inference discriminated union fixture
    pub fn kotlin_type_inference_discriminated_union() -> Self {
        Self::new(
            FixtureCategory::Kotlin,
            "type_inference/discriminated_union",
        )
    }

    /// Kotlin type inference null safety fixture
    pub fn kotlin_type_inference_null_safety() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/null_safety")
    }

    /// Kotlin type inference scope functions fixture
    pub fn kotlin_type_inference_scope_functions() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/scope_functions")
    }

    /// Kotlin type inference negated is fixture
    pub fn kotlin_type_inference_negated_is() -> Self {
        Self::new(FixtureCategory::Kotlin, "type_inference/negated_is")
    }

    /// Scala type inference declarations fixture
    pub fn scala_type_inference_declarations() -> Self {
        Self::new(FixtureCategory::Scala, "type_inference/declarations")
    }

    /// Scala type inference control flow fixture
    pub fn scala_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::Scala, "type_inference/control_flow")
    }

    /// Scala type inference cross-file fixture
    pub fn scala_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Scala, "type_inference/cross_file")
    }

    /// Scala type inference overloads fixture
    pub fn scala_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::Scala, "type_inference/overloads")
    }

    /// Scala type inference visibility fixture
    pub fn scala_type_inference_visibility() -> Self {
        Self::new(FixtureCategory::Scala, "type_inference/visibility")
    }

    /// Scala type inference for-comprehension fixture
    pub fn scala_type_inference_for_comprehension() -> Self {
        Self::new(FixtureCategory::Scala, "type_inference/for_comprehension")
    }

    /// Scala basic project fixture (class, object, trait)
    pub fn scala_basic() -> Self {
        Self::new(FixtureCategory::Scala, "basic")
    }

    /// Scala tree-sitter query regression fixture
    pub fn scala_query_regression() -> Self {
        Self::new(FixtureCategory::Scala, "query_regression")
    }

    /// Scala review fixture group
    pub fn scala_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Scala, format!("review/{}", name.into()))
    }

    /// Scala case class review fixture
    pub fn scala_review_case_class() -> Self {
        Self::scala_review("case_class")
    }

    /// Ruby type inference constructors fixture
    pub fn ruby_type_inference_constructors() -> Self {
        Self::new(FixtureCategory::Ruby, "type_inference/constructors")
    }

    /// Ruby type inference cross-file fixture
    pub fn ruby_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Ruby, "type_inference/cross_file")
    }

    /// PHP type inference phpdoc fixture
    pub fn php_type_inference_phpdoc() -> Self {
        Self::new(FixtureCategory::Php, "type_inference/phpdoc")
    }

    /// PHP type inference cross-file fixture
    pub fn php_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Php, "type_inference/cross_file")
    }

    /// PHP type inference overloads fixture
    pub fn php_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::Php, "type_inference/overloads")
    }

    /// Dart type inference declarations fixture
    pub fn dart_type_inference_declarations() -> Self {
        Self::new(FixtureCategory::Dart, "type_inference/declarations")
    }

    /// Dart type inference control flow fixture
    pub fn dart_type_inference_control_flow() -> Self {
        Self::new(FixtureCategory::Dart, "type_inference/control_flow")
    }

    /// Dart type inference cross-file fixture
    pub fn dart_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::Dart, "type_inference/cross_file")
    }

    /// Dart type inference overloads fixture
    pub fn dart_type_inference_overloads() -> Self {
        Self::new(FixtureCategory::Dart, "type_inference/overloads")
    }

    /// Dart type inference discriminated union fixture
    pub fn dart_type_inference_discriminated_union() -> Self {
        Self::new(FixtureCategory::Dart, "type_inference/discriminated_union")
    }

    /// Dart type inference is-negated fixture
    pub fn dart_type_inference_is_negated() -> Self {
        Self::new(FixtureCategory::Dart, "type_inference/is_negated")
    }

    /// Dart type inference control positions fixture
    pub fn dart_type_inference_control_positions() -> Self {
        Self::new(FixtureCategory::Dart, "type_inference/control_positions")
    }

    /// Dart basic project fixture (class, functions, imports)
    pub fn dart_basic() -> Self {
        Self::new(FixtureCategory::Dart, "basic")
    }

    /// Dart tree-sitter query regression fixture
    pub fn dart_query_regression() -> Self {
        Self::new(FixtureCategory::Dart, "query_regression")
    }

    /// Dart review fixture group
    pub fn dart_review(name: impl Into<String>) -> Self {
        Self::new(FixtureCategory::Dart, format!("review/{}", name.into()))
    }

    /// Dart mixin/async review fixture
    pub fn dart_review_mixin_async() -> Self {
        Self::dart_review("mixin_async")
    }

    /// JavaScript type inference narrowing fixture
    pub fn javascript_type_inference_narrowing() -> Self {
        Self::new(FixtureCategory::JavaScript, "type_inference/narrowing")
    }

    /// JavaScript type inference cross-file fixture
    pub fn javascript_type_inference_cross_file() -> Self {
        Self::new(FixtureCategory::JavaScript, "type_inference/cross_file")
    }

    /// JavaScript type inference wildcard import fixture
    pub fn javascript_type_inference_wildcard() -> Self {
        Self::new(FixtureCategory::JavaScript, "type_inference/wildcard")
    }
}

/// Common fixture capabilities used by workflow helpers.
pub trait FixtureAccess {
    /// Get the root path of the fixture.
    fn root(&self) -> &Path;

    /// Get the root path of the fixture.
    fn root_path(&self) -> &Path {
        self.root()
    }

    /// Get a file path relative to the fixture root.
    fn file<P: AsRef<Path>>(&self, relative_path: P) -> PathBuf;

    /// Add a new file to the fixture.
    fn add_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &self,
        relative_path: P,
        content: C,
    ) -> io::Result<PathBuf>;

    /// Modify an existing file in the fixture.
    fn modify_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &self,
        relative_path: P,
        content: C,
    ) -> io::Result<PathBuf>;

    /// Delete a file from the fixture.
    fn delete_file<P: AsRef<Path>>(&self, relative_path: P) -> io::Result<()>;
}

/// Test fixture manager
///
/// Loads fixtures from the crate-root fixtures directory and copies them to a temporary directory.
pub struct TestFixture {
    /// Temporary directory where fixture is copied
    temp_dir: TempDir,
}

impl FixtureAccess for TestFixture {
    fn root(&self) -> &Path {
        TestFixture::root(self)
    }

    fn file<P: AsRef<Path>>(&self, relative_path: P) -> PathBuf {
        TestFixture::file(self, relative_path)
    }

    fn add_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &self,
        relative_path: P,
        content: C,
    ) -> io::Result<PathBuf> {
        TestFixture::add_file(self, relative_path, content)
    }

    fn modify_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &self,
        relative_path: P,
        content: C,
    ) -> io::Result<PathBuf> {
        TestFixture::modify_file(self, relative_path, content)
    }

    fn delete_file<P: AsRef<Path>>(&self, relative_path: P) -> io::Result<()> {
        TestFixture::delete_file(self, relative_path)
    }
}

impl TestFixture {
    /// Get the base fixtures directory
    fn fixtures_base() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    /// Load a fixture by specification
    pub fn load(spec: FixtureSpec) -> io::Result<Self> {
        let base = Self::fixtures_base();
        let mut fixture_path = base.join(spec.category.dir_name());

        if !spec.subdirectory.is_empty() {
            fixture_path = fixture_path.join(&spec.subdirectory);
        }

        // Verify fixture exists
        if !fixture_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Fixture not found: {:?}", fixture_path),
            ));
        }

        // Create temp directory
        let temp_dir = TempDir::new()?;

        // Copy fixture to temp directory
        Self::copy_dir_all(&fixture_path, temp_dir.path())?;

        Ok(Self { temp_dir })
    }

    /// Load Rust basic project fixture
    pub fn rust_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_basic())
    }

    /// Load Rust tree-sitter query regression fixture
    pub fn rust_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_query_regression())
    }

    /// Load Rust review fixture
    pub fn rust_review(name: impl Into<String>) -> io::Result<Self> {
        Self::load(FixtureSpec::rust_review(name))
    }

    /// Load Rust once_cell review fixture
    pub fn rust_once_cell() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_once_cell())
    }

    /// Load Rust distractor fixture
    pub fn rust_distractor() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_distractor())
    }

    /// Load Rust index_sidecar review fixture
    pub fn rust_index_sidecar() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_index_sidecar())
    }

    /// Load Rust relation_demo review fixture
    pub fn rust_relation_demo() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_relation_demo())
    }

    /// Load Rust relation_diamond review fixture
    pub fn rust_relation_diamond() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_relation_diamond())
    }

    /// Load Rust re-export chain review fixture
    pub fn rust_review_re_export() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_review_re_export())
    }

    /// Load Rust ripgrep review fixture
    pub fn rust_ripgrep() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_ripgrep())
    }

    /// Load Rust type inference generics fixture
    pub fn rust_type_inference_generics() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_generics())
    }

    /// Load Rust type inference control flow fixture
    pub fn rust_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_control_flow())
    }

    /// Load Rust type inference wildcard import fixture
    pub fn rust_type_inference_wildcard() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_wildcard())
    }

    /// Load Rust type inference closure fixture
    pub fn rust_type_inference_closure() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_closure())
    }

    /// Load Rust type inference destructuring fixture
    pub fn rust_type_inference_destructuring() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_destructuring())
    }

    /// Load Rust type inference lifetime fixture
    pub fn rust_type_inference_lifetime() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_lifetime())
    }

    /// Load Rust type inference impl-Self fixture
    pub fn rust_type_inference_impl_self() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_impl_self())
    }

    /// Load Rust type inference reference fixture
    pub fn rust_type_inference_reference() -> io::Result<Self> {
        Self::load(FixtureSpec::rust_type_inference_reference())
    }

    /// Load Python basic project fixture
    pub fn python_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::python_basic())
    }

    /// Load Python tree-sitter query regression fixture
    pub fn python_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::python_query_regression())
    }

    /// Load Python type inference type hints fixture
    pub fn python_type_inference_type_hints() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_type_hints())
    }

    /// Load Python type inference control flow fixture
    pub fn python_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_control_flow())
    }

    /// Load Python type inference cross-file fixture
    pub fn python_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_cross_file())
    }

    /// Load Python type inference visibility fixture
    pub fn python_type_inference_visibility() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_visibility())
    }

    /// Load Python type inference lambda fixture
    pub fn python_type_inference_lambda() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_lambda())
    }

    /// Load Python type inference discriminated union fixture
    pub fn python_type_inference_discriminated_union() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_discriminated_union())
    }

    /// Load Python type inference destructuring fixture
    pub fn python_type_inference_destructuring() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_destructuring())
    }

    /// Load Python type inference negated checks fixture
    pub fn python_type_inference_negated_checks() -> io::Result<Self> {
        Self::load(FixtureSpec::python_type_inference_negated_checks())
    }

    /// Load Java basic project fixture
    pub fn java_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::java_basic())
    }

    /// Load Java tree-sitter query regression fixture
    pub fn java_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::java_query_regression())
    }

    /// Load Java type inference generics fixture
    pub fn java_type_inference_generics() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_generics())
    }

    /// Load Java type inference control flow fixture
    pub fn java_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_control_flow())
    }

    /// Load Java type inference visibility fixture
    pub fn java_type_inference_visibility() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_visibility())
    }

    /// Load Java type inference overloads fixture
    pub fn java_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_overloads())
    }

    /// Load Java type inference cross-file fixture
    pub fn java_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_cross_file())
    }

    /// Load Java type inference lambda fixture
    pub fn java_type_inference_lambda() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_lambda())
    }

    /// Load Java type inference discriminated union fixture
    pub fn java_type_inference_discriminated_union() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_discriminated_union())
    }

    /// Load Java type inference pattern matching fixture
    pub fn java_type_inference_pattern_matching() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_pattern_matching())
    }

    /// Load Java type inference var inference fixture
    pub fn java_type_inference_var_inference() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_var_inference())
    }

    /// Load Java type inference negated checks fixture
    pub fn java_type_inference_negated_checks() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_negated_checks())
    }

    /// Load Java type inference control positions fixture
    pub fn java_type_inference_control_positions() -> io::Result<Self> {
        Self::load(FixtureSpec::java_type_inference_control_positions())
    }

    /// Load Java Spring Boot minimal demo fixture
    pub fn java_spring_boot() -> io::Result<Self> {
        Self::load(FixtureSpec::java_spring_boot())
    }

    /// Load Java index_sidecar review fixture
    pub fn java_index_sidecar() -> io::Result<Self> {
        Self::load(FixtureSpec::java_index_sidecar())
    }

    /// Load Java jackson-core review fixture
    pub fn java_jackson_core() -> io::Result<Self> {
        Self::load(FixtureSpec::java_jackson_core())
    }

    /// Load Python index_sidecar review fixture
    pub fn python_index_sidecar() -> io::Result<Self> {
        Self::load(FixtureSpec::python_index_sidecar())
    }

    /// Load Python re-export chain review fixture
    pub fn python_review_re_export() -> io::Result<Self> {
        Self::load(FixtureSpec::python_review_re_export())
    }

    /// Load Python wildcard import review fixture
    pub fn python_review_wildcard() -> io::Result<Self> {
        Self::load(FixtureSpec::python_review_wildcard())
    }

    /// Load TypeScript basic project fixture
    pub fn typescript_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_basic())
    }

    /// Load TypeScript tree-sitter query regression fixture
    pub fn typescript_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_query_regression())
    }

    /// Load TypeScript index_sidecar review fixture
    pub fn typescript_index_sidecar() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_index_sidecar())
    }

    /// Load TypeScript type inference generics fixture
    pub fn typescript_type_inference_generics() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_generics())
    }

    /// Load TypeScript type inference unions fixture
    pub fn typescript_type_inference_unions() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_unions())
    }

    /// Load TypeScript type inference cross-file fixture
    pub fn typescript_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_cross_file())
    }

    /// Load TypeScript re-export chain review fixture
    pub fn typescript_review_re_export() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_review_re_export())
    }

    /// Load TypeScript wildcard import review fixture
    pub fn typescript_review_wildcard() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_review_wildcard())
    }

    /// Load TypeScript type inference overloads fixture
    pub fn typescript_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_overloads())
    }

    /// Load TypeScript type inference visibility fixture
    pub fn typescript_type_inference_visibility() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_visibility())
    }

    /// Load TypeScript type inference lambda fixture
    pub fn typescript_type_inference_lambda() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_lambda())
    }

    /// Load TypeScript type inference destructuring fixture
    pub fn typescript_type_inference_destructuring() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_destructuring())
    }

    /// Load TypeScript type inference negated checks fixture
    pub fn typescript_type_inference_negated_checks() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_negated_checks())
    }

    /// Load TypeScript type inference control positions fixture
    pub fn typescript_type_inference_control_positions() -> io::Result<Self> {
        Self::load(FixtureSpec::typescript_type_inference_control_positions())
    }

    /// Load C# basic project fixture
    pub fn csharp_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_basic())
    }

    /// Load C# tree-sitter query regression fixture
    pub fn csharp_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_query_regression())
    }

    /// Load C# type inference generics fixture
    pub fn csharp_type_inference_generics() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_generics())
    }

    /// Load C# type inference control flow fixture
    pub fn csharp_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_control_flow())
    }

    /// Load C# type inference overloads fixture
    pub fn csharp_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_overloads())
    }

    /// Load C# type inference cross-file fixture
    pub fn csharp_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_cross_file())
    }

    /// Load C# type inference visibility fixture
    pub fn csharp_type_inference_visibility() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_visibility())
    }

    /// Load C# type inference lambda fixture
    pub fn csharp_type_inference_lambda() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_lambda())
    }

    /// Load C# type inference discriminated union fixture
    pub fn csharp_type_inference_discriminated_union() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_discriminated_union())
    }

    /// Load C# type inference is-not fixture
    pub fn csharp_type_inference_is_not() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_is_not())
    }

    /// Load C# type inference control positions fixture
    pub fn csharp_type_inference_control_positions() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_type_inference_control_positions())
    }

    /// Load C# MediatR review fixture
    pub fn csharp_mediatr() -> io::Result<Self> {
        Self::load(FixtureSpec::csharp_mediatr())
    }

    /// Load Go basic project fixture
    pub fn go_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::go_basic())
    }

    /// Load Go tree-sitter query regression fixture
    pub fn go_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::go_query_regression())
    }

    /// Load Go type inference interfaces fixture
    pub fn go_type_inference_interfaces() -> io::Result<Self> {
        Self::load(FixtureSpec::go_type_inference_interfaces())
    }

    /// Load Go type inference control flow fixture
    pub fn go_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::go_type_inference_control_flow())
    }

    /// Load Go type inference cross-file fixture
    pub fn go_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::go_type_inference_cross_file())
    }

    /// Load Go type inference visibility fixture
    pub fn go_type_inference_visibility() -> io::Result<Self> {
        Self::load(FixtureSpec::go_type_inference_visibility())
    }

    /// Load Go type inference type assertion fixture
    pub fn go_type_inference_type_assertion() -> io::Result<Self> {
        Self::load(FixtureSpec::go_type_inference_type_assertion())
    }

    /// Load Go gin review fixture
    pub fn go_gin() -> io::Result<Self> {
        Self::load(FixtureSpec::go_gin())
    }

    /// Load Kotlin basic project fixture
    pub fn kotlin_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_basic())
    }

    /// Load Kotlin tree-sitter query regression fixture
    pub fn kotlin_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_query_regression())
    }

    /// Load Kotlin coroutines review fixture
    pub fn kotlin_review_coroutines() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_review_coroutines())
    }

    /// Load PHP basic project fixture
    pub fn php_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::php_basic())
    }

    /// Load PHP tree-sitter query regression fixture
    pub fn php_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::php_query_regression())
    }

    /// Load PHP namespace/trait review fixture
    pub fn php_review_namespace_trait() -> io::Result<Self> {
        Self::load(FixtureSpec::php_review_namespace_trait())
    }

    /// Load Ruby basic project fixture
    pub fn ruby_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::ruby_basic())
    }

    /// Load Ruby tree-sitter query regression fixture
    pub fn ruby_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::ruby_query_regression())
    }

    /// Load Ruby mixin review fixture
    pub fn ruby_review_mixin() -> io::Result<Self> {
        Self::load(FixtureSpec::ruby_review_mixin())
    }

    /// Load JavaScript basic project fixture
    pub fn javascript_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::javascript_basic())
    }

    /// Load JavaScript tree-sitter query regression fixture
    pub fn javascript_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::javascript_query_regression())
    }

    /// Load JavaScript Express review fixture
    pub fn javascript_express() -> io::Result<Self> {
        Self::load(FixtureSpec::javascript_express())
    }

    /// Load JavaScript re-export chain review fixture
    pub fn javascript_review_re_export() -> io::Result<Self> {
        Self::load(FixtureSpec::javascript_review_re_export())
    }

    /// Load multi-language project fixture
    pub fn multi_language() -> io::Result<Self> {
        Self::load(FixtureSpec::multi_language())
    }

    /// Load C basic project fixture
    pub fn c_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::c_basic())
    }

    /// Load C tree-sitter query regression fixture
    pub fn c_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::c_query_regression())
    }

    /// Load C macros review fixture
    pub fn c_review_macros() -> io::Result<Self> {
        Self::load(FixtureSpec::c_review_macros())
    }

    /// Load C type inference declarations fixture
    pub fn c_type_inference_declarations() -> io::Result<Self> {
        Self::load(FixtureSpec::c_type_inference_declarations())
    }

    /// Load Bash basic project fixture
    pub fn bash_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::bash_basic())
    }

    /// Load Bash tree-sitter query regression fixture
    pub fn bash_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::bash_query_regression())
    }

    /// Load Bash pipeline review fixture
    pub fn bash_review_pipeline() -> io::Result<Self> {
        Self::load(FixtureSpec::bash_review_pipeline())
    }

    /// Load Bash type inference variables fixture
    pub fn bash_type_inference_variables() -> io::Result<Self> {
        Self::load(FixtureSpec::bash_type_inference_variables())
    }

    /// Load Lua basic project fixture
    pub fn lua_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::lua_basic())
    }

    /// Load Lua tree-sitter query regression fixture
    pub fn lua_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::lua_query_regression())
    }

    /// Load Lua closure review fixture
    pub fn lua_review_closure() -> io::Result<Self> {
        Self::load(FixtureSpec::lua_review_closure())
    }

    /// Load Lua type inference variables fixture
    pub fn lua_type_inference_variables() -> io::Result<Self> {
        Self::load(FixtureSpec::lua_type_inference_variables())
    }

    /// Load C++ basic project fixture
    pub fn cpp_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::cpp_basic())
    }

    /// Load C++ tree-sitter query regression fixture
    pub fn cpp_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::cpp_query_regression())
    }

    /// Load C++ templates review fixture
    pub fn cpp_review_templates() -> io::Result<Self> {
        Self::load(FixtureSpec::cpp_review_templates())
    }

    /// Load C++ type inference declarations fixture
    pub fn cpp_type_inference_declarations() -> io::Result<Self> {
        Self::load(FixtureSpec::cpp_type_inference_declarations())
    }

    /// Load C++ type inference overloads fixture
    pub fn cpp_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::cpp_type_inference_overloads())
    }

    /// Load Kotlin type inference generics fixture
    pub fn kotlin_type_inference_generics() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_generics())
    }

    /// Load Kotlin type inference control flow fixture
    pub fn kotlin_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_control_flow())
    }

    /// Load Kotlin type inference cross-file fixture
    pub fn kotlin_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_cross_file())
    }

    /// Load Kotlin type inference overloads fixture
    pub fn kotlin_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_overloads())
    }

    /// Load Kotlin type inference visibility fixture
    pub fn kotlin_type_inference_visibility() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_visibility())
    }

    /// Load Kotlin type inference lambda fixture
    pub fn kotlin_type_inference_lambda() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_lambda())
    }

    /// Load Kotlin type inference discriminated union fixture
    pub fn kotlin_type_inference_discriminated_union() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_discriminated_union())
    }

    /// Load Kotlin type inference null safety fixture
    pub fn kotlin_type_inference_null_safety() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_null_safety())
    }

    /// Load Kotlin type inference scope functions fixture
    pub fn kotlin_type_inference_scope_functions() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_scope_functions())
    }

    /// Load Kotlin type inference negated is fixture
    pub fn kotlin_type_inference_negated_is() -> io::Result<Self> {
        Self::load(FixtureSpec::kotlin_type_inference_negated_is())
    }

    /// Load Scala type inference declarations fixture
    pub fn scala_type_inference_declarations() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_type_inference_declarations())
    }

    /// Load Scala type inference control flow fixture
    pub fn scala_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_type_inference_control_flow())
    }

    /// Load Scala type inference cross-file fixture
    pub fn scala_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_type_inference_cross_file())
    }

    /// Load Scala type inference overloads fixture
    pub fn scala_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_type_inference_overloads())
    }

    /// Load Scala type inference visibility fixture
    pub fn scala_type_inference_visibility() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_type_inference_visibility())
    }

    /// Load Scala type inference for-comprehension fixture
    pub fn scala_type_inference_for_comprehension() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_type_inference_for_comprehension())
    }

    /// Load Scala basic project fixture
    pub fn scala_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_basic())
    }

    /// Load Scala tree-sitter query regression fixture
    pub fn scala_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_query_regression())
    }

    /// Load Scala case class review fixture
    pub fn scala_review_case_class() -> io::Result<Self> {
        Self::load(FixtureSpec::scala_review_case_class())
    }

    /// Load Ruby type inference constructors fixture
    pub fn ruby_type_inference_constructors() -> io::Result<Self> {
        Self::load(FixtureSpec::ruby_type_inference_constructors())
    }

    /// Load Ruby type inference cross-file fixture
    pub fn ruby_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::ruby_type_inference_cross_file())
    }

    /// Load PHP type inference phpdoc fixture
    pub fn php_type_inference_phpdoc() -> io::Result<Self> {
        Self::load(FixtureSpec::php_type_inference_phpdoc())
    }

    /// Load PHP type inference cross-file fixture
    pub fn php_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::php_type_inference_cross_file())
    }

    /// Load PHP type inference overloads fixture
    pub fn php_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::php_type_inference_overloads())
    }

    /// Load Dart type inference declarations fixture
    pub fn dart_type_inference_declarations() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_type_inference_declarations())
    }

    /// Load Dart type inference control flow fixture
    pub fn dart_type_inference_control_flow() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_type_inference_control_flow())
    }

    /// Load Dart type inference cross-file fixture
    pub fn dart_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_type_inference_cross_file())
    }

    /// Load Dart type inference overloads fixture
    pub fn dart_type_inference_overloads() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_type_inference_overloads())
    }

    /// Load Dart type inference discriminated union fixture
    pub fn dart_type_inference_discriminated_union() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_type_inference_discriminated_union())
    }

    /// Load Dart type inference is-negated fixture
    pub fn dart_type_inference_is_negated() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_type_inference_is_negated())
    }

    /// Load Dart type inference control positions fixture
    pub fn dart_type_inference_control_positions() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_type_inference_control_positions())
    }

    /// Load Dart basic project fixture
    pub fn dart_basic() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_basic())
    }

    /// Load Dart tree-sitter query regression fixture
    pub fn dart_query_regression() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_query_regression())
    }

    /// Load Dart mixin/async review fixture
    pub fn dart_review_mixin_async() -> io::Result<Self> {
        Self::load(FixtureSpec::dart_review_mixin_async())
    }

    /// Load JavaScript type inference narrowing fixture
    pub fn javascript_type_inference_narrowing() -> io::Result<Self> {
        Self::load(FixtureSpec::javascript_type_inference_narrowing())
    }

    /// Load JavaScript type inference cross-file fixture
    pub fn javascript_type_inference_cross_file() -> io::Result<Self> {
        Self::load(FixtureSpec::javascript_type_inference_cross_file())
    }

    /// Load JavaScript type inference wildcard import fixture
    pub fn javascript_type_inference_wildcard() -> io::Result<Self> {
        Self::load(FixtureSpec::javascript_type_inference_wildcard())
    }

    /// Load the document fixture group
    pub fn documents() -> io::Result<Self> {
        Self::load(FixtureSpec::documents())
    }

    /// Load the file-fold tool fixture group
    pub fn tools_file_fold() -> io::Result<Self> {
        Self::load(FixtureSpec::tools_file_fold())
    }

    /// Get the root path of the fixture (in temp directory)
    pub fn root(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Get the root path of the fixture (alias for root())
    pub fn root_path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Get a file path relative to the fixture root
    pub fn file(&self, relative_path: impl AsRef<Path>) -> PathBuf {
        self.temp_dir.path().join(relative_path.as_ref())
    }

    /// Add a new file to the fixture
    pub fn add_file(
        &self,
        relative_path: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
    ) -> io::Result<PathBuf> {
        let path = self.file(relative_path.as_ref());

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&path, content.as_ref())?;
        Ok(path)
    }

    /// Modify an existing file in the fixture
    pub fn modify_file(
        &self,
        relative_path: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
    ) -> io::Result<PathBuf> {
        let path = self.file(relative_path.as_ref());
        std::fs::write(&path, content.as_ref())?;
        Ok(path)
    }

    /// Delete a file from the fixture
    pub fn delete_file(&self, relative_path: impl AsRef<Path>) -> io::Result<()> {
        let path = self.file(relative_path.as_ref());
        std::fs::remove_file(&path)
    }

    /// List all files in the fixture
    pub fn list_files(&self) -> io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        Self::collect_files(self.temp_dir.path(), &mut files)?;
        Ok(files)
    }

    /// Copy directory recursively
    fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dst)?;

        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if ty.is_dir() {
                Self::copy_dir_all(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }

    /// Collect all files recursively
    fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let path = entry.path();

            if ty.is_dir() {
                Self::collect_files(&path, files)?;
            } else {
                files.push(path);
            }
        }
        Ok(())
    }
}

/// Empty fixture for creating test projects from scratch
pub struct EmptyFixture {
    /// Temporary directory
    temp_dir: TempDir,
}

impl EmptyFixture {
    /// Create an empty fixture
    pub fn new() -> io::Result<Self> {
        let temp_dir = TempDir::new()?;
        Ok(Self { temp_dir })
    }

    /// Get the root path
    pub fn root(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Add a file
    pub fn add_file(
        &self,
        relative_path: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
    ) -> io::Result<PathBuf> {
        let path = self.temp_dir.path().join(relative_path.as_ref());

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&path, content.as_ref())?;
        Ok(path)
    }

    /// Convert to TestFixture (for compatibility)
    pub fn into_test_fixture(self) -> TestFixture {
        TestFixture {
            temp_dir: self.temp_dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_rust_basic() {
        let fixture = TestFixture::rust_basic().expect("Failed to load fixture");
        assert!(fixture.root().exists());
        assert!(fixture.file("src/main.rs").exists());
        assert!(fixture.file("src/lib.rs").exists());
        assert!(fixture.file("Cargo.toml").exists());
    }

    #[test]
    fn test_list_files() {
        let fixture = TestFixture::rust_basic().expect("Failed to load fixture");
        let files = fixture.list_files().expect("Failed to list files");
        assert!(!files.is_empty());
    }

    #[test]
    fn test_add_file() {
        let fixture = TestFixture::rust_basic().expect("Failed to load fixture");
        let path = fixture
            .add_file("src/new.rs", "fn new() {}")
            .expect("Failed to add file");
        assert!(path.exists());
    }

    #[test]
    fn test_empty_fixture() {
        let fixture = EmptyFixture::new().expect("Failed to create empty fixture");
        let path = fixture
            .add_file("test.rs", "fn test() {}")
            .expect("Failed to add file");
        assert!(path.exists());
    }
}
