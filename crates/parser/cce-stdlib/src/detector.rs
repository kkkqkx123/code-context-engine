// Standard Library Detector
// Unified interface for standard library call detection across languages

use cce_types::language::{Language, effective_language};
use cce_types::relation::RelationType;
use cce_types::stdlib_category::StdlibCategory;

use super::{
    bash::BashStdlibDetector, c::CStdlibDetector, cpp::CppStdlibDetector,
    csharp::CSharpStdlibDetector, dart::DartStdlibDetector, go::GoStdlibDetector,
    java::JavaStdlibDetector, javascript::JavaScriptStdlibDetector, kotlin::KotlinStdlibDetector,
    lua::LuaStdlibDetector, php::PhpStdlibDetector, python::PythonStdlibDetector,
    ruby::RubyStdlibDetector, rust::RustStdlibDetector, scala::ScalaStdlibDetector,
};

/// Unified interface for standard library call detection across languages
pub struct StdlibDetector;

impl StdlibDetector {
    /// Check if a call is to the standard library using relation type (optimized)
    ///
    /// This is the optimized interface that uses static dispatch based on RelationType
    /// for O(1) performance. This method should be preferred over `is_stdlib_call`
    /// when RelationType information is available.
    pub fn is_stdlib_by_type(
        call_name: &str,
        relation_type: &RelationType,
        language: &Language,
    ) -> bool {
        // Remapped plugin languages inherit their host grammar's tables.
        let effective = effective_language(language);
        match &effective {
            Language::Bash => BashStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::C => CStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Cpp => CppStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::CSharp => CSharpStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Dart => DartStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Go => GoStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Java => JavaStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                JavaScriptStdlibDetector::is_stdlib_by_type(call_name, relation_type)
            }
            Language::Kotlin => KotlinStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Lua => LuaStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Php => PhpStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Python => PythonStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Ruby => RubyStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Rust => RustStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Scala => ScalaStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            _ => false,
        }
    }

    /// Check if a call is to the standard library (legacy interface)
    ///
    /// This is the legacy interface that uses string analysis. For better performance,
    /// use `is_stdlib_by_type` when RelationType information is available.
    pub fn is_stdlib_call(call_name: &str, language: &Language) -> bool {
        // Remapped plugin languages inherit their host grammar's tables.
        let effective = effective_language(language);
        match &effective {
            Language::Bash => BashStdlibDetector::is_stdlib_call(call_name),
            Language::C => CStdlibDetector::is_stdlib_call(call_name),
            Language::Cpp => CppStdlibDetector::is_stdlib_call(call_name),
            Language::CSharp => CSharpStdlibDetector::is_stdlib_call(call_name),
            Language::Dart => DartStdlibDetector::is_stdlib_call(call_name),
            Language::Go => GoStdlibDetector::is_stdlib_call(call_name),
            Language::Java => JavaStdlibDetector::is_stdlib_call(call_name),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                JavaScriptStdlibDetector::is_stdlib_call(call_name)
            }
            Language::Kotlin => KotlinStdlibDetector::is_stdlib_call(call_name),
            Language::Lua => LuaStdlibDetector::is_stdlib_call(call_name),
            Language::Php => PhpStdlibDetector::is_stdlib_call(call_name),
            Language::Python => PythonStdlibDetector::is_stdlib_call(call_name),
            Language::Ruby => RubyStdlibDetector::is_stdlib_call(call_name),
            Language::Rust => RustStdlibDetector::is_stdlib_call(call_name),
            Language::Scala => ScalaStdlibDetector::is_stdlib_call(call_name),
            _ => false,
        }
    }

    /// Look up the standard library category of a name.
    ///
    /// This is the single dispatch every layer shares: the entity layer caches
    /// the result on the entity, the relation layer attaches it to call edges
    /// and the grouper derives its semantic description from it. Languages
    /// without a category table (template languages, unregistered plugin
    /// languages) return `None` instead of a placeholder category.
    pub fn category(name: &str, language: &Language) -> Option<StdlibCategory> {
        // Remapped plugin languages inherit their host grammar's tables.
        let effective = effective_language(language);
        match &effective {
            Language::Bash => BashStdlibDetector::get_category(name),
            Language::C => CStdlibDetector::get_category(name),
            Language::Cpp => CppStdlibDetector::get_category(name),
            Language::CSharp => CSharpStdlibDetector::get_category(name),
            Language::Dart => DartStdlibDetector::get_category(name),
            Language::Go => GoStdlibDetector::get_category(name),
            Language::Java => JavaStdlibDetector::get_category(name),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                JavaScriptStdlibDetector::get_category(name)
            }
            Language::Kotlin => KotlinStdlibDetector::get_category(name),
            Language::Lua => LuaStdlibDetector::get_category(name),
            Language::Php => PhpStdlibDetector::get_category(name),
            Language::Python => PythonStdlibDetector::get_category(name),
            Language::Ruby => RubyStdlibDetector::get_category(name),
            Language::Rust => RustStdlibDetector::get_category(name),
            Language::Scala => ScalaStdlibDetector::get_category(name),
            _ => None,
        }
    }

    /// Check whether a name or module path refers to the standard library.
    ///
    /// Union of the three authoritative shapes because one target string can
    /// be any of them depending on where it came from: a bare symbol (`Vec`), a
    /// call expression (`Vec::new`) or a module/package path
    /// (`std::collections::HashMap`, `dart:async`, `os.path`). Callers that
    /// know the shape should prefer the narrower entry so a user definition
    /// that merely shares a name with a stdlib entry is not swallowed.
    pub fn is_stdlib_ref(name: &str, language: &Language) -> bool {
        Self::category(name, language).is_some()
            || Self::is_stdlib_call(name, language)
            || Self::is_stdlib_import_path(name, language)
    }

    /// Check whether an import path belongs to the standard library.
    ///
    /// This is the single import-side entry point: every language answers from
    /// its own table (`is_stdlib_module`, `is_stdlib_package`, `is_dotnet_path`,
    /// `is_dart_path`, `is_stdlib_require`, …) instead of the ad-hoc prefix
    /// lists that used to live in the import classifier. Prefix conventions
    /// survive only as *data* inside those tables (JDK package list, .NET
    /// namespace list, Dart `dart:`/`package:` forms).
    ///
    /// C/C++ deliberately return `false`: a system header is a parser flag
    /// (`StandardizedImport::is_system_header`) and those imports keep
    /// classifying as `ExternalPackage`, not `StandardLibrary`.
    pub fn is_stdlib_import_path(path: &str, language: &Language) -> bool {
        if path.is_empty() {
            return false;
        }
        // Remapped plugin languages inherit their host grammar's tables.
        let effective = effective_language(language);
        match &effective {
            // No module table of its own: `source lib.sh` targets a file, and
            // shell builtins are reached through `is_stdlib_call`.
            Language::Bash => false,
            // Header stdlib-ness is the parser's `is_system_header` flag.
            Language::C | Language::Cpp => false,
            Language::CSharp => CSharpStdlibDetector::is_dotnet_path(path),
            Language::Dart => DartStdlibDetector::is_dart_path(path),
            Language::Go => GoStdlibDetector::is_stdlib_package(first_segment(path, '/')),
            Language::Java => JavaStdlibDetector::is_jdk_path(path),
            // Kotlin/Scala files import JDK packages as well as their own.
            Language::Kotlin => {
                KotlinStdlibDetector::is_kotlin_path(path) || JavaStdlibDetector::is_jdk_path(path)
            }
            // `SCALA_PACKAGES` mixes third-party frameworks (cats/zio/akka),
            // so the import entry sticks to the `scala` root plus JDK imports.
            Language::Scala => {
                path == "scala"
                    || path.starts_with("scala.")
                    || JavaStdlibDetector::is_jdk_path(path)
            }
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                // Node specifiers are matched whole (`fs`, `fs/promises`), so a
                // project module such as `fs/helpers` stays out. The `node:`
                // protocol prefix is not part of the table.
                let specifier = path.strip_prefix("node:").unwrap_or(path);
                JavaScriptStdlibDetector::is_node_stdlib(specifier)
            }
            Language::Lua => {
                LuaStdlibDetector::is_stdlib_module(path)
                    || LuaStdlibDetector::is_stdlib_module(first_segment(path, '.'))
            }
            Language::Php => {
                PhpStdlibDetector::is_builtin_class(path)
                    || PhpStdlibDetector::is_core_extension(path)
            }
            Language::Python => PythonStdlibDetector::is_stdlib_module(first_segment(path, '.')),
            // `require 'json'` is a stdlib require; `Net::HTTP` a core module.
            Language::Ruby => {
                RubyStdlibDetector::is_stdlib_require(path)
                    || RubyStdlibDetector::is_stdlib_path(path)
            }
            // `std` / `core` / `alloc` / `proc_macro` crates.
            Language::Rust => RustStdlibDetector::is_stdlib_path(path),
            _ => false,
        }
    }

    /// Category of a stdlib reference, or `None` when the name is not one.
    ///
    /// Answers both halves of the question the reference layers ask: *is* this
    /// a stdlib reference (see `is_stdlib_ref`) and *which* category does it
    /// belong to. The category tables are keyed by symbol name while a
    /// reference carries a full path, so the lookup falls back to the path's
    /// last segment (`std::collections::HashMap` → `HashMap` → `Collection`)
    /// before settling on `Other`.
    pub fn ref_category(name: &str, language: &Language) -> Option<StdlibCategory> {
        if !Self::is_stdlib_ref(name, language) {
            return None;
        }
        Self::category(name, language)
            .or_else(|| {
                let last = name.rsplit([':', '.', '/', '\\']).find(|s| !s.is_empty())?;
                Self::category(last, language)
            })
            .or(Some(StdlibCategory::Other))
    }

    /// Check if a type is from the standard library (merged from symbol_classifier)
    ///
    /// Shell and scripting languages without a static type system conservatively
    /// return false. Remapped plugin languages inherit their host tables.
    pub fn is_stdlib_type(name: &str, language: &Language) -> bool {
        let effective = effective_language(language);
        match &effective {
            Language::C => CStdlibDetector::is_stdlib_type(name),
            Language::Cpp => CppStdlibDetector::is_stdlib_type(name),
            Language::Go => GoStdlibDetector::is_stdlib_type(name),
            Language::Rust => RustStdlibDetector::is_stdlib_type(name),
            Language::Python => PythonStdlibDetector::is_stdlib_type(name),
            Language::Java => JavaStdlibDetector::is_stdlib_type(name),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                JavaScriptStdlibDetector::is_stdlib_type(name)
            }
            Language::Kotlin => KotlinStdlibDetector::is_stdlib_type(name),
            Language::Dart => DartStdlibDetector::is_stdlib_type(name),
            Language::Php => PhpStdlibDetector::is_stdlib_type(name),
            Language::Ruby => RubyStdlibDetector::is_stdlib_type(name),
            Language::CSharp => CSharpStdlibDetector::is_stdlib_type(name),
            Language::Scala => ScalaStdlibDetector::is_stdlib_type(name),
            _ => false,
        }
    }

    /// Check if a trait is from the standard library (merged from symbol_classifier)
    ///
    /// Only Rust exposes traits as a language concept; other languages
    /// conservatively return false. Remapped plugin languages inherit host tables.
    pub fn is_stdlib_trait(name: &str, language: &Language) -> bool {
        let effective = effective_language(language);
        match &effective {
            Language::Rust => RustStdlibDetector::is_stdlib_trait(name),
            _ => false, // Rust-specific feature
        }
    }

    /// Check if a constant is from the standard library
    ///
    /// Languages with dedicated constant tables participate; the rest
    /// conservatively return false. Remapped plugin languages inherit host tables.
    pub fn is_stdlib_constant(name: &str, language: &Language) -> bool {
        let effective = effective_language(language);
        match &effective {
            Language::C => CStdlibDetector::is_stdlib_constant(name),
            Language::Cpp => CppStdlibDetector::is_stdlib_constant(name),
            Language::CSharp => CSharpStdlibDetector::is_stdlib_constant(name),
            Language::Go => GoStdlibDetector::is_stdlib_constant(name),
            Language::Php => PhpStdlibDetector::is_stdlib_constant(name),
            Language::Rust => RustStdlibDetector::is_stdlib_constant(name),
            Language::Python => PythonStdlibDetector::is_stdlib_constant(name),
            Language::Java => JavaStdlibDetector::is_stdlib_constant(name),
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                JavaScriptStdlibDetector::is_stdlib_constant(name)
            }
            Language::Kotlin => KotlinStdlibDetector::is_stdlib_constant(name),
            _ => false,
        }
    }
}

/// First component of a `separator`-delimited path.
///
/// Returns the whole input when the separator does not occur, so callers can
/// pass a bare module name without special-casing.
fn first_segment(path: &str, separator: char) -> &str {
    path.split(separator).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("printf", &Language::C));
        assert!(StdlibDetector::is_stdlib_call("malloc", &Language::C));
        assert!(!StdlibDetector::is_stdlib_call("my_function", &Language::C));
    }

    #[test]
    fn test_cpp_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("std::cout", &Language::Cpp));
        assert!(StdlibDetector::is_stdlib_call(
            "std::vector",
            &Language::Cpp
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "my_function",
            &Language::Cpp
        ));
    }

    #[test]
    fn test_csharp_stdlib() {
        assert!(StdlibDetector::is_stdlib_call(
            "Console.WriteLine",
            &Language::CSharp
        ));
        assert!(StdlibDetector::is_stdlib_call(
            "System.String",
            &Language::CSharp
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "MyClass",
            &Language::CSharp
        ));
    }

    #[test]
    fn test_go_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("fmt.Println", &Language::Go));
        assert!(StdlibDetector::is_stdlib_call(
            "strings.Contains",
            &Language::Go
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "my_function",
            &Language::Go
        ));
    }

    #[test]
    fn test_java_stdlib() {
        assert!(StdlibDetector::is_stdlib_call(
            "System.out.println",
            &Language::Java
        ));
        assert!(StdlibDetector::is_stdlib_call("ArrayList", &Language::Java));
        assert!(!StdlibDetector::is_stdlib_call("MyClass", &Language::Java));
    }

    #[test]
    fn test_javascript_stdlib() {
        assert!(StdlibDetector::is_stdlib_call(
            "console.log",
            &Language::JavaScript
        ));
        assert!(StdlibDetector::is_stdlib_call(
            "JSON.parse",
            &Language::JavaScript
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "customFunction",
            &Language::JavaScript
        ));
    }

    #[test]
    fn test_kotlin_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("listOf", &Language::Kotlin));
        assert!(StdlibDetector::is_stdlib_call("String", &Language::Kotlin));
        assert!(!StdlibDetector::is_stdlib_call(
            "myFunction",
            &Language::Kotlin
        ));
    }

    #[test]
    fn test_php_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("strlen", &Language::Php));
        assert!(StdlibDetector::is_stdlib_call("DateTime", &Language::Php));
        assert!(!StdlibDetector::is_stdlib_call(
            "my_function",
            &Language::Php
        ));
    }

    #[test]
    fn test_python_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("print", &Language::Python));
        assert!(StdlibDetector::is_stdlib_call("len", &Language::Python));
        assert!(!StdlibDetector::is_stdlib_call(
            "custom_function",
            &Language::Python
        ));
    }

    #[test]
    fn test_ruby_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("puts", &Language::Ruby));
        assert!(StdlibDetector::is_stdlib_call("File.open", &Language::Ruby));
        assert!(!StdlibDetector::is_stdlib_call(
            "my_method",
            &Language::Ruby
        ));
    }

    #[test]
    fn test_rust_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("println", &Language::Rust));
        assert!(StdlibDetector::is_stdlib_call("Vec::new", &Language::Rust));
        assert!(!StdlibDetector::is_stdlib_call(
            "custom_function",
            &Language::Rust
        ));
    }

    #[test]
    fn test_dart_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("print", &Language::Dart));
        assert!(StdlibDetector::is_stdlib_call("String", &Language::Dart));
        assert!(StdlibDetector::is_stdlib_call("List", &Language::Dart));
        assert!(StdlibDetector::is_stdlib_call("Future", &Language::Dart));
        assert!(StdlibDetector::is_stdlib_call("Widget", &Language::Dart));
        assert!(!StdlibDetector::is_stdlib_call("MyClass", &Language::Dart));
    }

    #[test]
    fn test_scala_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("println", &Language::Scala));
        assert!(StdlibDetector::is_stdlib_call("List", &Language::Scala));
        assert!(StdlibDetector::is_stdlib_call("Option", &Language::Scala));
        assert!(StdlibDetector::is_stdlib_call("Future", &Language::Scala));
        assert!(!StdlibDetector::is_stdlib_call("MyClass", &Language::Scala));
    }

    #[test]
    fn test_stdlib_by_type_rust() {
        use cce_types::relation::RelationType;

        // Test macro calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "println!",
            &RelationType::MacroCall,
            &Language::Rust
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "vec!",
            &RelationType::MacroCall,
            &Language::Rust
        ));

        // Test direct calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "Vec::new",
            &RelationType::DirectCall,
            &Language::Rust
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "std::fs::read",
            &RelationType::DirectCall,
            &Language::Rust
        ));

        // Test method calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "Vec::push",
            &RelationType::InstanceMethodCall,
            &Language::Rust
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "HashMap::new",
            &RelationType::StaticMethodCall,
            &Language::Rust
        ));

        // Test constructor calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "Vec::new()",
            &RelationType::ConstructorCall,
            &Language::Rust
        ));

        // Test generic calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "Vec<i32>",
            &RelationType::GenericCall,
            &Language::Rust
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "custom_function",
            &RelationType::DirectCall,
            &Language::Rust
        ));
        assert!(!StdlibDetector::is_stdlib_by_type(
            "serde::Serialize",
            &RelationType::StaticMethodCall,
            &Language::Rust
        ));
    }

    #[test]
    fn test_stdlib_by_type_python() {
        use cce_types::relation::RelationType;

        // Test builtin functions
        assert!(StdlibDetector::is_stdlib_by_type(
            "print",
            &RelationType::DirectCall,
            &Language::Python
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "len",
            &RelationType::DirectCall,
            &Language::Python
        ));

        // Test module calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "os.path.join",
            &RelationType::DirectCall,
            &Language::Python
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "json.loads",
            &RelationType::DirectCall,
            &Language::Python
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "custom_function",
            &RelationType::DirectCall,
            &Language::Python
        ));
    }

    #[test]
    fn test_stdlib_by_type_javascript() {
        use cce_types::relation::RelationType;

        // Test builtin objects
        assert!(StdlibDetector::is_stdlib_by_type(
            "console.log",
            &RelationType::DirectCall,
            &Language::JavaScript
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "JSON.parse",
            &RelationType::DirectCall,
            &Language::JavaScript
        ));

        // Test method calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "console.log",
            &RelationType::InstanceMethodCall,
            &Language::JavaScript
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "Math.random",
            &RelationType::StaticMethodCall,
            &Language::JavaScript
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "customFunction",
            &RelationType::DirectCall,
            &Language::JavaScript
        ));
    }

    #[test]
    fn test_stdlib_by_type_cpp() {
        use cce_types::relation::RelationType;

        // Test std:: calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "std::cout",
            &RelationType::DirectCall,
            &Language::Cpp
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "std::vector",
            &RelationType::DirectCall,
            &Language::Cpp
        ));

        // Test method calls (using simple type::method format)
        assert!(StdlibDetector::is_stdlib_by_type(
            "vector::push_back",
            &RelationType::InstanceMethodCall,
            &Language::Cpp
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "vector::size",
            &RelationType::InstanceMethodCall,
            &Language::Cpp
        ));

        // Test C standard library functions
        assert!(StdlibDetector::is_stdlib_by_type(
            "printf",
            &RelationType::DirectCall,
            &Language::Cpp
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "malloc",
            &RelationType::DirectCall,
            &Language::Cpp
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "custom_function",
            &RelationType::DirectCall,
            &Language::Cpp
        ));
    }

    #[test]
    fn test_stdlib_by_type_go() {
        use cce_types::relation::RelationType;

        // Test direct calls (most reliable with Go)
        assert!(StdlibDetector::is_stdlib_by_type(
            "fmt.Println",
            &RelationType::DirectCall,
            &Language::Go
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "strings.Contains",
            &RelationType::DirectCall,
            &Language::Go
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "os.Open",
            &RelationType::DirectCall,
            &Language::Go
        ));

        // Test that Go delegates InstanceMethodCall to is_stdlib_call
        // which checks if the package part is stdlib
        assert!(StdlibDetector::is_stdlib_by_type(
            "bytes.Split",
            &RelationType::InstanceMethodCall,
            &Language::Go
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "mypackage.MyFunction",
            &RelationType::DirectCall,
            &Language::Go
        ));
    }

    #[test]
    fn test_stdlib_by_type_ruby() {
        use cce_types::relation::RelationType;

        // Test direct calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "File.open",
            &RelationType::DirectCall,
            &Language::Ruby
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "Array.new",
            &RelationType::DirectCall,
            &Language::Ruby
        ));

        // Test instance method calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "String.upcase",
            &RelationType::InstanceMethodCall,
            &Language::Ruby
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "Array.push",
            &RelationType::InstanceMethodCall,
            &Language::Ruby
        ));

        // Test static method calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "Dir.glob",
            &RelationType::StaticMethodCall,
            &Language::Ruby
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "Time.now",
            &RelationType::StaticMethodCall,
            &Language::Ruby
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "MyModule.my_method",
            &RelationType::DirectCall,
            &Language::Ruby
        ));
    }

    #[test]
    fn test_stdlib_by_type_java() {
        use cce_types::relation::RelationType;

        // Test direct calls (class constructors/static methods)
        assert!(StdlibDetector::is_stdlib_by_type(
            "ArrayList",
            &RelationType::DirectCall,
            &Language::Java
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "HashMap",
            &RelationType::DirectCall,
            &Language::Java
        ));

        // Test instance method calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "ArrayList.add",
            &RelationType::InstanceMethodCall,
            &Language::Java
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "HashMap.get",
            &RelationType::InstanceMethodCall,
            &Language::Java
        ));

        // Test static method calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "System.out.println",
            &RelationType::StaticMethodCall,
            &Language::Java
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "Arrays.sort",
            &RelationType::StaticMethodCall,
            &Language::Java
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "MyClass.myMethod",
            &RelationType::DirectCall,
            &Language::Java
        ));
    }

    #[test]
    fn test_stdlib_by_type_c() {
        use cce_types::relation::RelationType;

        // Test direct calls
        assert!(StdlibDetector::is_stdlib_by_type(
            "printf",
            &RelationType::DirectCall,
            &Language::C
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "malloc",
            &RelationType::DirectCall,
            &Language::C
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "strlen",
            &RelationType::DirectCall,
            &Language::C
        ));

        // Test other relation types (should also return true for stdlib functions)
        assert!(StdlibDetector::is_stdlib_by_type(
            "memcpy",
            &RelationType::InstanceMethodCall,
            &Language::C
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "my_function",
            &RelationType::DirectCall,
            &Language::C
        ));
    }

    #[test]
    fn test_stdlib_by_type_php() {
        use cce_types::relation::RelationType;

        // Test direct calls (functions and class names)
        assert!(StdlibDetector::is_stdlib_by_type(
            "strlen",
            &RelationType::DirectCall,
            &Language::Php
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "array_merge",
            &RelationType::DirectCall,
            &Language::Php
        ));

        // Test class names
        assert!(StdlibDetector::is_stdlib_by_type(
            "DateTime",
            &RelationType::DirectCall,
            &Language::Php
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "PDO",
            &RelationType::DirectCall,
            &Language::Php
        ));

        // Test instance method calls (delegates to is_stdlib_call)
        // PHP will check if first component is a stdlib class/function
        assert!(StdlibDetector::is_stdlib_by_type(
            "strlen",
            &RelationType::InstanceMethodCall,
            &Language::Php
        ));
        assert!(StdlibDetector::is_stdlib_by_type(
            "array_keys",
            &RelationType::InstanceMethodCall,
            &Language::Php
        ));

        // Test non-stdlib calls
        assert!(!StdlibDetector::is_stdlib_by_type(
            "my_function",
            &RelationType::DirectCall,
            &Language::Php
        ));
    }

    #[test]
    fn test_false_positives_detection() {
        // These are user-defined functions/classes that could be confused with stdlib
        // False positive detection is critical to avoid misclassification

        // Go - user-defined package
        assert!(!StdlibDetector::is_stdlib_call(
            "myapp.Handler",
            &Language::Go
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "github.com/user/package.Function",
            &Language::Go
        ));

        // Ruby - user-defined modules
        assert!(!StdlibDetector::is_stdlib_call(
            "MyModule.my_method",
            &Language::Ruby
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "Config.load",
            &Language::Ruby
        ));

        // Java - user-defined classes
        assert!(!StdlibDetector::is_stdlib_call("MyClass", &Language::Java));
        assert!(!StdlibDetector::is_stdlib_call(
            "app.User.save",
            &Language::Java
        ));

        // Python - user-defined modules that resemble stdlib names
        assert!(!StdlibDetector::is_stdlib_call(
            "myos.path.join",
            &Language::Python
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "myutils.helper",
            &Language::Python
        ));

        // C - user-defined functions
        assert!(!StdlibDetector::is_stdlib_call("my_printf", &Language::C));
        assert!(!StdlibDetector::is_stdlib_call(
            "custom_malloc",
            &Language::C
        ));

        // PHP - user-defined functions and classes
        assert!(!StdlibDetector::is_stdlib_call("MyPDO", &Language::Php));
        assert!(!StdlibDetector::is_stdlib_call(
            "app_string_helper",
            &Language::Php
        ));

        // JavaScript - user-defined modules
        assert!(!StdlibDetector::is_stdlib_call(
            "mylogger.log",
            &Language::JavaScript
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "utils.helper",
            &Language::JavaScript
        ));

        // Rust - user-defined crates
        assert!(!StdlibDetector::is_stdlib_call(
            "serde::Serialize",
            &Language::Rust
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "tokio::task",
            &Language::Rust
        ));
    }

    #[test]
    fn test_stdlib_call_consistency() {
        // Verify consistency between is_stdlib_call and is_stdlib_by_type
        // Note: MacroCall types skip this check as is_stdlib_call doesn't trim '!'
        use cce_types::relation::RelationType;

        // Test Rust consistency (without macro calls which have special handling)
        let rust_cases = vec![
            ("Vec::new", RelationType::DirectCall),
            ("HashMap.insert", RelationType::InstanceMethodCall),
        ];
        for (name, rel_type) in rust_cases {
            let call_result = StdlibDetector::is_stdlib_call(name, &Language::Rust);
            let type_result = StdlibDetector::is_stdlib_by_type(name, &rel_type, &Language::Rust);
            assert_eq!(
                call_result, type_result,
                "Inconsistency for Rust '{}': is_stdlib_call={}, is_stdlib_by_type={}",
                name, call_result, type_result
            );
        }

        // Test Python consistency
        let python_cases = vec![
            ("print", RelationType::DirectCall),
            ("os.path.join", RelationType::DirectCall),
            ("json.loads", RelationType::DirectCall),
        ];
        for (name, rel_type) in python_cases {
            let call_result = StdlibDetector::is_stdlib_call(name, &Language::Python);
            let type_result = StdlibDetector::is_stdlib_by_type(name, &rel_type, &Language::Python);
            assert_eq!(
                call_result, type_result,
                "Inconsistency for Python '{}': is_stdlib_call={}, is_stdlib_by_type={}",
                name, call_result, type_result
            );
        }

        // Test Go consistency
        let go_cases = vec![
            ("fmt.Println", RelationType::DirectCall),
            ("strings.Contains", RelationType::DirectCall),
        ];
        for (name, rel_type) in go_cases {
            let call_result = StdlibDetector::is_stdlib_call(name, &Language::Go);
            let type_result = StdlibDetector::is_stdlib_by_type(name, &rel_type, &Language::Go);
            assert_eq!(
                call_result, type_result,
                "Inconsistency for Go '{}': is_stdlib_call={}, is_stdlib_by_type={}",
                name, call_result, type_result
            );
        }
    }

    #[test]
    fn test_bash_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("echo", &Language::Bash));
        assert!(StdlibDetector::is_stdlib_call("grep", &Language::Bash));
        assert!(StdlibDetector::is_stdlib_call("sed", &Language::Bash));
        assert!(StdlibDetector::is_stdlib_call("awk", &Language::Bash));
        assert!(!StdlibDetector::is_stdlib_call(
            "my_script",
            &Language::Bash
        ));
    }

    #[test]
    fn test_lua_stdlib() {
        assert!(StdlibDetector::is_stdlib_call("print", &Language::Lua));
        assert!(StdlibDetector::is_stdlib_call(
            "string.format",
            &Language::Lua
        ));
        assert!(StdlibDetector::is_stdlib_call("math.sqrt", &Language::Lua));
        assert!(StdlibDetector::is_stdlib_call(
            "table.insert",
            &Language::Lua
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "my_function",
            &Language::Lua
        ));
    }

    #[test]
    fn test_typescript_stdlib() {
        // TypeScript uses the same detector as JavaScript
        assert!(StdlibDetector::is_stdlib_call(
            "console.log",
            &Language::TypeScript
        ));
        assert!(StdlibDetector::is_stdlib_call(
            "JSON.parse",
            &Language::TypeScript
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "customFunction",
            &Language::TypeScript
        ));
    }

    #[test]
    fn test_stdlib_type_extended_languages() {
        assert!(StdlibDetector::is_stdlib_type("String", &Language::Java));
        assert!(StdlibDetector::is_stdlib_type(
            "java.util.ArrayList",
            &Language::Java
        ));
        assert!(!StdlibDetector::is_stdlib_type("MyClass", &Language::Java));

        assert!(StdlibDetector::is_stdlib_type(
            "Array",
            &Language::JavaScript
        ));
        assert!(StdlibDetector::is_stdlib_type("Array", &Language::Jsx));
        assert!(StdlibDetector::is_stdlib_type("Promise", &Language::Tsx));
        assert!(StdlibDetector::is_stdlib_type(
            "JSON",
            &Language::TypeScript
        ));
        assert!(!StdlibDetector::is_stdlib_type(
            "customFunction",
            &Language::JavaScript
        ));

        assert!(StdlibDetector::is_stdlib_type("String", &Language::Kotlin));
        assert!(StdlibDetector::is_stdlib_type(
            "kotlin.collections.List",
            &Language::Kotlin
        ));
        assert!(!StdlibDetector::is_stdlib_type(
            "myFunction",
            &Language::Kotlin
        ));

        assert!(StdlibDetector::is_stdlib_type("String", &Language::Dart));
        assert!(!StdlibDetector::is_stdlib_type("MyClass", &Language::Dart));

        assert!(StdlibDetector::is_stdlib_type("DateTime", &Language::Php));
        assert!(!StdlibDetector::is_stdlib_type(
            "MyController",
            &Language::Php
        ));

        assert!(StdlibDetector::is_stdlib_type("String", &Language::Ruby));
        assert!(StdlibDetector::is_stdlib_type("File", &Language::Ruby));
        assert!(!StdlibDetector::is_stdlib_type("MyModule", &Language::Ruby));

        assert!(StdlibDetector::is_stdlib_type("int", &Language::CSharp));
        assert!(StdlibDetector::is_stdlib_type(
            "System.String",
            &Language::CSharp
        ));
        assert!(!StdlibDetector::is_stdlib_type(
            "MyClass",
            &Language::CSharp
        ));

        assert!(StdlibDetector::is_stdlib_type("List", &Language::Scala));
        assert!(StdlibDetector::is_stdlib_type(
            "scala.collection.immutable.List",
            &Language::Scala
        ));
        assert!(!StdlibDetector::is_stdlib_type(
            "cats.effect.IO",
            &Language::Scala
        ));
        assert!(!StdlibDetector::is_stdlib_type("MyClass", &Language::Scala));
    }

    #[test]
    fn test_stdlib_constant_php() {
        assert!(StdlibDetector::is_stdlib_constant(
            "PHP_VERSION",
            &Language::Php
        ));
        assert!(StdlibDetector::is_stdlib_constant(
            "PHP_EOL",
            &Language::Php
        ));
        assert!(!StdlibDetector::is_stdlib_constant(
            "MY_CONSTANT",
            &Language::Php
        ));
    }

    #[test]
    fn test_stdlib_constant_covers_new_tables() {
        assert!(StdlibDetector::is_stdlib_constant(
            "u8::MAX",
            &Language::Rust
        ));
        assert!(StdlibDetector::is_stdlib_constant(
            "math.pi",
            &Language::Python
        ));
        assert!(StdlibDetector::is_stdlib_constant(
            "Math.PI",
            &Language::Java
        ));
        assert!(StdlibDetector::is_stdlib_constant(
            "Math.PI",
            &Language::JavaScript
        ));
        assert!(StdlibDetector::is_stdlib_constant(
            "Int.MAX_VALUE",
            &Language::Kotlin
        ));
        assert!(!StdlibDetector::is_stdlib_constant(
            "MY_CONSTANT",
            &Language::Rust
        ));
    }

    #[test]
    fn test_stdlib_constant_conservative_missing() {
        // C, C++, C#, Go, PHP, Rust, Python, Java, JS family and Kotlin
        // ship dedicated constant tables; the rest stay conservative.
        for language in [
            Language::Jsx,
            Language::Tsx,
            Language::Scala,
            Language::Dart,
            Language::Ruby,
            Language::Lua,
            Language::Bash,
        ] {
            assert!(
                !StdlibDetector::is_stdlib_constant("ANYTHING", &language),
                "{language:?} has no constant table and must stay conservative"
            );
        }
    }

    #[test]
    fn test_unsupported_language() {
        // Test that unsupported languages return false
        assert!(!StdlibDetector::is_stdlib_call(
            "something",
            &Language::Html
        ));
        assert!(!StdlibDetector::is_stdlib_call("something", &Language::Css));
        assert!(!StdlibDetector::is_stdlib_call(
            "something",
            &Language::Unknown
        ));
    }

    /// Plugin contract: a remapped language inherits its host's tables; a
    /// native plugin language keeps the conservative fallback (`false`).
    #[test]
    fn test_stdlib_custom_language_inheritance_and_fallback() {
        use cce_types::language::{
            clear_plugin_languages, register_plugin_language, register_remap_plugin_language,
        };

        clear_plugin_languages();
        let remapped = register_remap_plugin_language(
            "sync-stdlib-remap",
            &["syncsr".to_string()],
            &Language::Rust.to_string(),
        );
        let native = register_plugin_language("sync-stdlib-native", &["syncsn".to_string()]);
        assert!(StdlibDetector::is_stdlib_call(
            "Vec::new",
            &Language::Custom(remapped)
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "Vec::new",
            &Language::Custom(native)
        ));
        assert!(!StdlibDetector::is_stdlib_call(
            "Vec::new",
            &Language::Custom(u32::MAX)
        ));
        assert!(StdlibDetector::is_stdlib_type(
            "Vec",
            &Language::Custom(remapped)
        ));
        assert!(!StdlibDetector::is_stdlib_type(
            "Vec",
            &Language::Custom(native)
        ));
        assert!(StdlibDetector::is_stdlib_trait(
            "Clone",
            &Language::Custom(remapped)
        ));
        assert!(!StdlibDetector::is_stdlib_trait(
            "Clone",
            &Language::Custom(native)
        ));
        clear_plugin_languages();
    }

    /// Sync guard: every non-template parsable language must map its
    /// sentinel call to a real category, not the uncategorized bucket.
    /// Adding a language without taxonomy turns this red by design:
    /// an uncategorized stdlib call is incomplete implementation.
    #[test]
    fn test_stdlib_category_covers_all_categorized_languages() {
        use cce_types::StdlibCategory;
        let template = [
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
        ];
        for language in Language::ast_parsable_languages() {
            if template.contains(language) {
                continue;
            }
            let sentinel = match language {
                Language::Bash => ("echo", BashStdlibDetector::get_category("echo")),
                Language::C => ("printf", CStdlibDetector::get_category("printf")),
                Language::Cpp => ("cout", CppStdlibDetector::get_category("cout")),
                Language::CSharp => (
                    "Console.WriteLine",
                    CSharpStdlibDetector::get_category("Console.WriteLine"),
                ),
                Language::Dart => ("print", DartStdlibDetector::get_category("print")),
                Language::Go => ("fmt.Println", GoStdlibDetector::get_category("fmt.Println")),
                Language::Java => (
                    "System.out.println",
                    JavaStdlibDetector::get_category("System.out.println"),
                ),
                Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => (
                    "console.log",
                    JavaScriptStdlibDetector::get_category("console.log"),
                ),
                Language::Kotlin => ("listOf", KotlinStdlibDetector::get_category("listOf")),
                Language::Lua => (
                    "string.format",
                    LuaStdlibDetector::get_category("string.format"),
                ),
                Language::Php => ("strlen", PhpStdlibDetector::get_category("strlen")),
                Language::Python => ("print", PythonStdlibDetector::get_category("print")),
                Language::Ruby => ("puts", RubyStdlibDetector::get_category("puts")),
                Language::Rust => ("Vec::new", RustStdlibDetector::get_category("Vec::new")),
                Language::Scala => ("println", ScalaStdlibDetector::get_category("println")),
                other => panic!(
                    "{other:?} is parsable but has no category sentinel: \
                     add a categorizer table and a sentinel arm"
                ),
            };
            let (call, category) = sentinel;
            assert!(
                matches!(category, Some(c) if c != StdlibCategory::Other),
                "{call} in {language:?} must map to a real category, got {category:?}"
            );
        }
    }

    /// Sync guard: every parsable language must have a conscious stdlib
    /// decision. Add the new language (or an explicit opt-out) here when
    /// extending `Language::ast_parsable_languages`.
    #[test]
    fn test_stdlib_covers_all_parsable_languages() {
        // Template languages carry no stdlib calls of their own; their
        // embedded script/style blocks re-enter every layer as the host
        // language, so `false` here is the intended conservative answer.
        let template: &[Language] = &[
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
        ];
        let sentinels: &[(Language, &str)] = &[
            (Language::Bash, "echo"),
            (Language::C, "printf"),
            (Language::Cpp, "std::cout"),
            (Language::CSharp, "Console.WriteLine"),
            (Language::Dart, "print"),
            (Language::Go, "fmt.Println"),
            (Language::Java, "System.out.println"),
            (Language::JavaScript, "console.log"),
            (Language::TypeScript, "console.log"),
            (Language::Jsx, "console.log"),
            (Language::Tsx, "console.log"),
            (Language::Kotlin, "listOf"),
            (Language::Lua, "string.format"),
            (Language::Php, "strlen"),
            (Language::Python, "print"),
            (Language::Ruby, "puts"),
            (Language::Rust, "Vec::new"),
            (Language::Scala, "println"),
        ];
        for language in Language::ast_parsable_languages() {
            if template.contains(language) {
                assert!(
                    !StdlibDetector::is_stdlib_call("something", language),
                    "template language {language:?} must stay conservative"
                );
                continue;
            }
            let (_, sentinel) = sentinels
                .iter()
                .find(|(lang, _)| lang == language)
                .unwrap_or_else(|| {
                    panic!(
                        "{language:?} is parsable but has no stdlib sentinel: \
                         add a detector arm or an explicit opt-out"
                    )
                });
            assert!(
                StdlibDetector::is_stdlib_call(sentinel, language),
                "sentinel {sentinel} not recognized for {language:?}"
            );
        }
    }

    /// The unified category entry must answer for every language that ships a
    /// category table, and must stay empty for languages that do not.
    #[test]
    fn test_category_entry_covers_all_detectors() {
        use cce_types::StdlibCategory;

        let cases: &[(Language, &str)] = &[
            (Language::Bash, "echo"),
            (Language::C, "printf"),
            (Language::Cpp, "cout"),
            (Language::CSharp, "Console.WriteLine"),
            (Language::Dart, "print"),
            (Language::Go, "fmt.Println"),
            (Language::Java, "System.out.println"),
            (Language::JavaScript, "console.log"),
            (Language::TypeScript, "console.log"),
            (Language::Kotlin, "listOf"),
            (Language::Lua, "string.format"),
            (Language::Php, "strlen"),
            (Language::Python, "print"),
            (Language::Ruby, "puts"),
            (Language::Rust, "Vec::new"),
            (Language::Scala, "println"),
        ];
        for (language, name) in cases {
            let category = StdlibDetector::category(name, language);
            assert!(
                matches!(category, Some(c) if c != StdlibCategory::Other),
                "category for {name} in {language:?} must be a real category, got {category:?}"
            );
        }

        for language in [
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Unknown,
        ] {
            assert_eq!(
                StdlibDetector::category("something", &language),
                None,
                "{language:?} has no category table"
            );
        }
    }

    /// Every call relation must reach the name check. A relation type that a
    /// detector does not special-case used to answer `false` even when the
    /// name was stdlib, so the same call disagreed between the entity layer
    /// (name only) and the relation layer.
    #[test]
    fn test_stdlib_by_type_agrees_with_call_on_call_relations() {
        use cce_types::relation::RelationType;

        let call_relations = [
            RelationType::DirectCall,
            RelationType::InstanceMethodCall,
            RelationType::StaticMethodCall,
            RelationType::ChainedMethodCall,
            RelationType::ConstructorCall,
            RelationType::PointerCall,
            RelationType::CallbackCall,
            RelationType::GenericCall,
            RelationType::MacroCall,
            RelationType::GoroutineCall,
            RelationType::DeferredCall,
            RelationType::AsyncCall,
            RelationType::HigherOrderCall,
        ];
        // Languages whose answer does not depend on the call shape.
        let cases: &[(Language, &[&str])] = &[
            (Language::Bash, &["echo", "my_script"]),
            (Language::C, &["printf", "my_function"]),
            (Language::CSharp, &["Console.WriteLine", "MyClass"]),
            (Language::Dart, &["print", "MyClass"]),
            (Language::Go, &["fmt.Println", "mypackage.MyFunction"]),
            (Language::Java, &["System.out.println", "MyClass"]),
            (Language::Lua, &["string.format", "my_function"]),
            (Language::Php, &["strlen", "my_function"]),
            (Language::Python, &["os.path.join", "custom_function"]),
            (Language::Ruby, &["File.open", "MyModule.my_method"]),
            (Language::Scala, &["println", "MyClass"]),
        ];
        for (language, names) in cases {
            for name in *names {
                let by_call = StdlibDetector::is_stdlib_call(name, language);
                for relation in &call_relations {
                    assert_eq!(
                        StdlibDetector::is_stdlib_by_type(name, relation, language),
                        by_call,
                        "{language:?} {relation} {name}"
                    );
                }
            }
        }
    }

    /// A non-call relation carries no callee, so it never reports stdlib.
    #[test]
    fn test_stdlib_by_type_rejects_non_call_relations() {
        use cce_types::relation::RelationType;

        let non_call_relations = [
            RelationType::Use,
            RelationType::Inheritance,
            RelationType::TypeReference,
            RelationType::ElementContains,
        ];
        let sentinels: &[(Language, &str)] = &[
            (Language::Bash, "echo"),
            (Language::C, "printf"),
            (Language::Cpp, "std::cout"),
            (Language::CSharp, "Console.WriteLine"),
            (Language::Dart, "print"),
            (Language::Go, "fmt.Println"),
            (Language::Java, "System.out.println"),
            (Language::JavaScript, "console.log"),
            (Language::Kotlin, "listOf"),
            (Language::Lua, "string.format"),
            (Language::Php, "strlen"),
            (Language::Python, "print"),
            (Language::Ruby, "puts"),
            (Language::Rust, "Vec::new"),
            (Language::Scala, "println"),
        ];
        for (language, name) in sentinels {
            for relation in &non_call_relations {
                assert!(
                    !StdlibDetector::is_stdlib_by_type(name, relation, language),
                    "{language:?} {relation} {name} must not classify as a stdlib call"
                );
            }
        }
    }

    /// Regression cover for the call shapes that used to be silently rejected.
    #[test]
    fn test_stdlib_by_type_handles_wrapper_and_higher_order_calls() {
        use cce_types::relation::RelationType;

        for relation in [
            RelationType::GoroutineCall,
            RelationType::DeferredCall,
            RelationType::DirectCall,
        ] {
            assert!(
                StdlibDetector::is_stdlib_by_type("fmt.Println", &relation, &Language::Go),
                "Go {relation} must classify fmt.Println as stdlib"
            );
        }
        for relation in [RelationType::AsyncCall, RelationType::DirectCall] {
            assert!(
                StdlibDetector::is_stdlib_by_type("asyncio.sleep", &relation, &Language::Python),
                "Python {relation} must classify asyncio.sleep as stdlib"
            );
        }
        assert!(
            StdlibDetector::is_stdlib_by_type(
                "Vec::new",
                &RelationType::HigherOrderCall,
                &Language::Rust
            ),
            "Rust higher-order calls must consult the stdlib tables"
        );
    }

    /// The import entry must answer from each language's own table. The three
    /// cases that used to be prefix-matched by hand (`using System;`,
    /// `dart:async`, `require 'json'`) are the regression cover: the shared
    /// prefix list said `false` for all of them while the tables say `true`.
    #[test]
    fn test_import_path_entry_uses_tables() {
        let importable: &[(Language, &str)] = &[
            (Language::CSharp, "System"),
            (Language::CSharp, "System.Collections.Generic"),
            (Language::CSharp, "Microsoft.Extensions.Logging"),
            (Language::Dart, "dart:async"),
            (Language::Dart, "package:flutter/material.dart"),
            (Language::Go, "fmt"),
            (Language::Go, "net/http"),
            (Language::Java, "java.util.ArrayList"),
            (Language::Java, "javax.swing.JFrame"),
            (Language::Kotlin, "kotlin.collections.List"),
            (Language::Kotlin, "java.util.List"),
            (Language::JavaScript, "fs"),
            (Language::TypeScript, "node:fs"),
            (Language::Php, "DateTime"),
            (Language::Python, "os.path"),
            (Language::Ruby, "json"),
            (Language::Ruby, "Net::HTTP"),
            (Language::Rust, "std::collections::HashMap"),
            (Language::Rust, "proc_macro::TokenStream"),
            (Language::Scala, "scala.collection.immutable.List"),
            (Language::Scala, "java.util.List"),
        ];
        for (language, path) in importable {
            assert!(
                StdlibDetector::is_stdlib_import_path(path, language),
                "{path} in {language:?} must classify as a stdlib import"
            );
        }

        // C/C++ header stdlib-ness is the parser's `is_system_header` flag and
        // Bash has no module table: both stay out of the import entry.
        for (language, path) in [
            (Language::Bash, "echo"),
            (Language::C, "stdint.h"),
            (Language::Cpp, "vector"),
        ] {
            assert!(
                !StdlibDetector::is_stdlib_import_path(path, &language),
                "{path} in {language:?} has no import table"
            );
        }

        let foreign: &[(Language, &str)] = &[
            (Language::CSharp, "MyCompany.MyProject"),
            (Language::Dart, "package:my_app/main.dart"),
            (Language::Go, "github.com/gin-gonic/gin"),
            (Language::Java, "com.example.myapp.Helper"),
            (Language::JavaScript, "react"),
            (Language::JavaScript, "fs/helpers"),
            (Language::Lua, "my_module"),
            (Language::Php, "Symfony\\Component\\HttpFoundation"),
            (Language::Python, "numpy.linalg"),
            (Language::Ruby, "my_gem"),
            (Language::Rust, "serde::Serialize"),
            (Language::Scala, "cats.effect.IO"),
        ];
        for (language, path) in foreign {
            assert!(
                !StdlibDetector::is_stdlib_import_path(path, language),
                "{path} in {language:?} is not a stdlib import"
            );
        }
    }

    /// `is_stdlib_ref` is the union the relation layer needs: one target
    /// string can be a symbol, a call expression or a module path.
    #[test]
    fn test_is_stdlib_ref_unions_all_shapes() {
        // symbol name (category table)
        assert!(StdlibDetector::is_stdlib_ref("HashMap", &Language::Rust));
        // call expression
        assert!(StdlibDetector::is_stdlib_ref("Vec::new", &Language::Rust));
        // module path
        assert!(StdlibDetector::is_stdlib_ref(
            "std::collections::HashMap",
            &Language::Rust
        ));
        assert!(StdlibDetector::is_stdlib_ref("dart:async", &Language::Dart));

        // A user definition that merely shares a callable name stays out.
        assert!(!StdlibDetector::is_stdlib_ref(
            "myapp.Command",
            &Language::Rust
        ));
        assert!(!StdlibDetector::is_stdlib_ref(
            "app.User.save",
            &Language::Java
        ));
        assert!(!StdlibDetector::is_stdlib_ref("", &Language::Rust));
    }
}
