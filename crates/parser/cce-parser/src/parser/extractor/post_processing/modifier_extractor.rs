//! Post-processing: modifier extraction from source text
//!
//! Thin dispatch to per-language submodules in `modifier/`.

use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;
use cce_types::language::Language;

pub fn extract_modifiers(mat: &QueryMatch, entity: &mut Entity, language: &Language) {
    match *language {
        Language::Rust => {
            crate::parser::extractor::post_processing::modifier::rust::extract_rust_modifiers(
                mat, entity,
            )
        }
        Language::Java | Language::Kotlin | Language::Scala | Language::CSharp => {
            crate::parser::extractor::post_processing::modifier::jvm::extract_jvm_modifiers(
                mat, entity,
            )
        }
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => {
            crate::parser::extractor::post_processing::modifier::typescript::extract_typescript_modifiers(
                mat, entity,
            )
        }
        Language::Dart => {
            crate::parser::extractor::post_processing::modifier::dart::extract_dart_modifiers(
                mat, entity,
            )
        }
        Language::C | Language::Cpp => {
            crate::parser::extractor::post_processing::modifier::c::extract_c_modifiers(mat, entity)
        }
        Language::Python => {
            crate::parser::extractor::post_processing::modifier::python::extract_python_modifiers(
                mat, entity,
            )
        }
        _ => {
            entity.modifiers = Vec::new();
        }
    }
}
