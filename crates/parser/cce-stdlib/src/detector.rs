// Standard Library Detector
// Unified interface for standard library call detection across languages

use cce_types::language::Language;
use cce_types::relation::RelationType;

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
        match language {
            Language::Bash => BashStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::C => CStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Cpp => CppStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::CSharp => CSharpStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Dart => DartStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Go => GoStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::Java => JavaStdlibDetector::is_stdlib_by_type(call_name, relation_type),
            Language::JavaScript | Language::TypeScript => {
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
        match language {
            Language::Bash => BashStdlibDetector::is_stdlib_call(call_name),
            Language::C => CStdlibDetector::is_stdlib_call(call_name),
            Language::Cpp => CppStdlibDetector::is_stdlib_call(call_name),
            Language::CSharp => CSharpStdlibDetector::is_stdlib_call(call_name),
            Language::Dart => DartStdlibDetector::is_stdlib_call(call_name),
            Language::Go => GoStdlibDetector::is_stdlib_call(call_name),
            Language::Java => JavaStdlibDetector::is_stdlib_call(call_name),
            Language::JavaScript | Language::TypeScript => {
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

    /// Check if a type is from the standard library (merged from symbol_classifier)
    pub fn is_stdlib_type(name: &str, language: &Language) -> bool {
        match language {
            Language::C => CStdlibDetector::is_stdlib_type(name),
            Language::Cpp => CppStdlibDetector::is_stdlib_type(name),
            Language::Go => GoStdlibDetector::is_stdlib_type(name),
            Language::Rust => RustStdlibDetector::is_stdlib_type(name),
            Language::Python => PythonStdlibDetector::is_stdlib_type(name),
            _ => false, // Other languages don't have this method yet
        }
    }

    /// Check if a trait is from the standard library (merged from symbol_classifier)
    pub fn is_stdlib_trait(name: &str, language: &Language) -> bool {
        match language {
            Language::Rust => RustStdlibDetector::is_stdlib_trait(name),
            _ => false, // Rust-specific feature
        }
    }

    /// Check if a constant is from the standard library
    pub fn is_stdlib_constant(name: &str, language: &Language) -> bool {
        match language {
            Language::C => CStdlibDetector::is_stdlib_constant(name),
            Language::Cpp => CppStdlibDetector::is_stdlib_constant(name),
            Language::CSharp => CSharpStdlibDetector::is_stdlib_constant(name),
            Language::Go => GoStdlibDetector::is_stdlib_constant(name),
            _ => false, // Other languages don't have this method yet
        }
    }
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
}
