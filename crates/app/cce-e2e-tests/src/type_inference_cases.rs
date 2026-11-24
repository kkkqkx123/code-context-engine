//! Shared type-inference fixture case list.
//!
//! Single source of truth for all `type_inference` fixtures. Both the
//! `export_type_inference` example and the regression baseline test iterate
//! over [`all_type_inference_cases`], so adding a fixture only requires
//! appending one entry here.

use crate::fixture::FixtureSpec;

/// One type-inference fixture case.
pub struct TypeInferenceCase {
    /// Language directory under `fixtures/` (e.g. `"rust"`).
    pub language: &'static str,
    /// Scenario directory under `fixtures/<lang>/type_inference/` (e.g. `"generics"`).
    pub scenario: &'static str,
    /// Fixture specification used by `TestFixture::load`.
    pub spec: FixtureSpec,
    /// Glob patterns used by the export example scanner.
    pub patterns: Vec<&'static str>,
}

/// All type-inference fixture cases in export order.
pub fn all_type_inference_cases() -> Vec<TypeInferenceCase> {
    vec![
        TypeInferenceCase {
            language: "rust",
            scenario: "generics",
            spec: FixtureSpec::rust_type_inference_generics(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "rust",
            scenario: "control_flow",
            spec: FixtureSpec::rust_type_inference_control_flow(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "type_hints",
            spec: FixtureSpec::python_type_inference_type_hints(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "control_flow",
            spec: FixtureSpec::python_type_inference_control_flow(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "cross_file",
            spec: FixtureSpec::python_type_inference_cross_file(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "generics",
            spec: FixtureSpec::typescript_type_inference_generics(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "unions",
            spec: FixtureSpec::typescript_type_inference_unions(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "cross_file",
            spec: FixtureSpec::typescript_type_inference_cross_file(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "overloads",
            spec: FixtureSpec::typescript_type_inference_overloads(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "generics",
            spec: FixtureSpec::java_type_inference_generics(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "control_flow",
            spec: FixtureSpec::java_type_inference_control_flow(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "visibility",
            spec: FixtureSpec::java_type_inference_visibility(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "overloads",
            spec: FixtureSpec::java_type_inference_overloads(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "generics",
            spec: FixtureSpec::csharp_type_inference_generics(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "control_flow",
            spec: FixtureSpec::csharp_type_inference_control_flow(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "overloads",
            spec: FixtureSpec::csharp_type_inference_overloads(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "go",
            scenario: "interfaces",
            spec: FixtureSpec::go_type_inference_interfaces(),
            patterns: vec!["*.go"],
        },
        TypeInferenceCase {
            language: "go",
            scenario: "control_flow",
            spec: FixtureSpec::go_type_inference_control_flow(),
            patterns: vec!["*.go"],
        },
        TypeInferenceCase {
            language: "c",
            scenario: "declarations",
            spec: FixtureSpec::c_type_inference_declarations(),
            patterns: vec!["*.c", "*.h"],
        },
        TypeInferenceCase {
            language: "cpp",
            scenario: "declarations",
            spec: FixtureSpec::cpp_type_inference_declarations(),
            patterns: vec!["*.cpp", "*.hpp"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "generics",
            spec: FixtureSpec::kotlin_type_inference_generics(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "control_flow",
            spec: FixtureSpec::kotlin_type_inference_control_flow(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "scala",
            scenario: "declarations",
            spec: FixtureSpec::scala_type_inference_declarations(),
            patterns: vec!["*.scala"],
        },
        TypeInferenceCase {
            language: "scala",
            scenario: "control_flow",
            spec: FixtureSpec::scala_type_inference_control_flow(),
            patterns: vec!["*.scala"],
        },
        TypeInferenceCase {
            language: "ruby",
            scenario: "constructors",
            spec: FixtureSpec::ruby_type_inference_constructors(),
            patterns: vec!["*.rb"],
        },
        TypeInferenceCase {
            language: "php",
            scenario: "phpdoc",
            spec: FixtureSpec::php_type_inference_phpdoc(),
            patterns: vec!["*.php"],
        },
        TypeInferenceCase {
            language: "dart",
            scenario: "declarations",
            spec: FixtureSpec::dart_type_inference_declarations(),
            patterns: vec!["*.dart"],
        },
        TypeInferenceCase {
            language: "dart",
            scenario: "control_flow",
            spec: FixtureSpec::dart_type_inference_control_flow(),
            patterns: vec!["*.dart"],
        },
        TypeInferenceCase {
            language: "javascript",
            scenario: "narrowing",
            spec: FixtureSpec::javascript_type_inference_narrowing(),
            patterns: vec!["*.js"],
        },
        TypeInferenceCase {
            language: "javascript",
            scenario: "cross_file",
            spec: FixtureSpec::javascript_type_inference_cross_file(),
            patterns: vec!["*.js"],
        },
        TypeInferenceCase {
            language: "bash",
            scenario: "variables",
            spec: FixtureSpec::bash_type_inference_variables(),
            patterns: vec!["*.sh"],
        },
        TypeInferenceCase {
            language: "lua",
            scenario: "variables",
            spec: FixtureSpec::lua_type_inference_variables(),
            patterns: vec!["*.lua"],
        },
        TypeInferenceCase {
            language: "cpp",
            scenario: "overloads",
            spec: FixtureSpec::cpp_type_inference_overloads(),
            patterns: vec!["*.cpp", "*.hpp"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "cross_file",
            spec: FixtureSpec::csharp_type_inference_cross_file(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "visibility",
            spec: FixtureSpec::csharp_type_inference_visibility(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "dart",
            scenario: "cross_file",
            spec: FixtureSpec::dart_type_inference_cross_file(),
            patterns: vec!["*.dart"],
        },
        TypeInferenceCase {
            language: "dart",
            scenario: "overloads",
            spec: FixtureSpec::dart_type_inference_overloads(),
            patterns: vec!["*.dart"],
        },
        TypeInferenceCase {
            language: "go",
            scenario: "cross_file",
            spec: FixtureSpec::go_type_inference_cross_file(),
            patterns: vec!["*.go"],
        },
        TypeInferenceCase {
            language: "go",
            scenario: "visibility",
            spec: FixtureSpec::go_type_inference_visibility(),
            patterns: vec!["*.go"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "cross_file",
            spec: FixtureSpec::java_type_inference_cross_file(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "cross_file",
            spec: FixtureSpec::kotlin_type_inference_cross_file(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "overloads",
            spec: FixtureSpec::kotlin_type_inference_overloads(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "visibility",
            spec: FixtureSpec::kotlin_type_inference_visibility(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "php",
            scenario: "cross_file",
            spec: FixtureSpec::php_type_inference_cross_file(),
            patterns: vec!["*.php"],
        },
        TypeInferenceCase {
            language: "php",
            scenario: "overloads",
            spec: FixtureSpec::php_type_inference_overloads(),
            patterns: vec!["*.php"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "visibility",
            spec: FixtureSpec::python_type_inference_visibility(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "ruby",
            scenario: "cross_file",
            spec: FixtureSpec::ruby_type_inference_cross_file(),
            patterns: vec!["*.rb"],
        },
        TypeInferenceCase {
            language: "rust",
            scenario: "wildcard",
            spec: FixtureSpec::rust_type_inference_wildcard(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "scala",
            scenario: "cross_file",
            spec: FixtureSpec::scala_type_inference_cross_file(),
            patterns: vec!["*.scala"],
        },
        TypeInferenceCase {
            language: "scala",
            scenario: "overloads",
            spec: FixtureSpec::scala_type_inference_overloads(),
            patterns: vec!["*.scala"],
        },
        TypeInferenceCase {
            language: "scala",
            scenario: "visibility",
            spec: FixtureSpec::scala_type_inference_visibility(),
            patterns: vec!["*.scala"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "visibility",
            spec: FixtureSpec::typescript_type_inference_visibility(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "javascript",
            scenario: "wildcard",
            spec: FixtureSpec::javascript_type_inference_wildcard(),
            patterns: vec!["*.js"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "lambda",
            spec: FixtureSpec::python_type_inference_lambda(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "lambda",
            spec: FixtureSpec::typescript_type_inference_lambda(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "rust",
            scenario: "closure",
            spec: FixtureSpec::rust_type_inference_closure(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "lambda",
            spec: FixtureSpec::kotlin_type_inference_lambda(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "lambda",
            spec: FixtureSpec::csharp_type_inference_lambda(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "lambda",
            spec: FixtureSpec::java_type_inference_lambda(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "discriminated_union",
            spec: FixtureSpec::python_type_inference_discriminated_union(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "discriminated_union",
            spec: FixtureSpec::csharp_type_inference_discriminated_union(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "discriminated_union",
            spec: FixtureSpec::kotlin_type_inference_discriminated_union(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "dart",
            scenario: "discriminated_union",
            spec: FixtureSpec::dart_type_inference_discriminated_union(),
            patterns: vec!["*.dart"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "discriminated_union",
            spec: FixtureSpec::java_type_inference_discriminated_union(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "rust",
            scenario: "destructuring",
            spec: FixtureSpec::rust_type_inference_destructuring(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "destructuring",
            spec: FixtureSpec::typescript_type_inference_destructuring(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "destructuring",
            spec: FixtureSpec::python_type_inference_destructuring(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "rust",
            scenario: "lifetime",
            spec: FixtureSpec::rust_type_inference_lifetime(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "rust",
            scenario: "impl_self",
            spec: FixtureSpec::rust_type_inference_impl_self(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "rust",
            scenario: "reference",
            spec: FixtureSpec::rust_type_inference_reference(),
            patterns: vec!["*.rs"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "pattern_matching",
            spec: FixtureSpec::java_type_inference_pattern_matching(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "var_inference",
            spec: FixtureSpec::java_type_inference_var_inference(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "null_safety",
            spec: FixtureSpec::kotlin_type_inference_null_safety(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "scope_functions",
            spec: FixtureSpec::kotlin_type_inference_scope_functions(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "python",
            scenario: "negated_checks",
            spec: FixtureSpec::python_type_inference_negated_checks(),
            patterns: vec!["*.py"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "negated_checks",
            spec: FixtureSpec::typescript_type_inference_negated_checks(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "typescript",
            scenario: "control_positions",
            spec: FixtureSpec::typescript_type_inference_control_positions(),
            patterns: vec!["*.ts"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "negated_checks",
            spec: FixtureSpec::java_type_inference_negated_checks(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "java",
            scenario: "control_positions",
            spec: FixtureSpec::java_type_inference_control_positions(),
            patterns: vec!["*.java"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "is_not",
            spec: FixtureSpec::csharp_type_inference_is_not(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "csharp",
            scenario: "control_positions",
            spec: FixtureSpec::csharp_type_inference_control_positions(),
            patterns: vec!["*.cs"],
        },
        TypeInferenceCase {
            language: "kotlin",
            scenario: "negated_is",
            spec: FixtureSpec::kotlin_type_inference_negated_is(),
            patterns: vec!["*.kt"],
        },
        TypeInferenceCase {
            language: "dart",
            scenario: "is_negated",
            spec: FixtureSpec::dart_type_inference_is_negated(),
            patterns: vec!["*.dart"],
        },
        TypeInferenceCase {
            language: "dart",
            scenario: "control_positions",
            spec: FixtureSpec::dart_type_inference_control_positions(),
            patterns: vec!["*.dart"],
        },
        TypeInferenceCase {
            language: "scala",
            scenario: "for_comprehension",
            spec: FixtureSpec::scala_type_inference_for_comprehension(),
            patterns: vec!["*.scala"],
        },
        TypeInferenceCase {
            language: "go",
            scenario: "type_assertion",
            spec: FixtureSpec::go_type_inference_type_assertion(),
            patterns: vec!["*.go"],
        },
    ]
}
