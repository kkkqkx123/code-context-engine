//! Post-processing: modifier extraction from source text
//!
//! Thin dispatch to per-language submodules in `modifier/`.

use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;
use cce_types::language::Language;

pub fn extract_modifiers(mat: &QueryMatch, entity: &mut Entity, language: &Language) {
    // Reset first: no arm may leak stale modifiers when its match carries
    // nothing (several submodules return early on missing captures).
    entity.modifiers = Vec::new();
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
        Language::Php => {
            crate::parser::extractor::post_processing::modifier::php::extract_php_modifiers(
                mat, entity,
            )
        }
        // Conscious empty: Go signals visibility by name, Ruby by method
        // calls, Bash/Lua have no modifier concept.
        Language::Go | Language::Ruby | Language::Bash | Language::Lua => {
            entity.modifiers = Vec::new();
        }
        // Document/config/template languages, Unknown and Custom plugin
        // languages (whose query schemes are plugin-defined) carry no
        // built-in modifier extraction.
        _ => {
            entity.modifiers = Vec::new();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::executor::QueryMatch;
    use cce_types::entity::{EntityId, EntityKind};
    use std::collections::HashMap;

    fn empty_entity() -> Entity {
        Entity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: String::new(),
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: Default::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: vec!["stale".to_string()],
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    /// Sync guard: every parsable code language must have a conscious
    /// modifier decision (extractor or documented empty). A new language
    /// without a decision turns this red by design.
    #[test]
    fn test_modifiers_cover_all_parsable_languages() {
        use cce_types::language::Language;
        let with_extractor: &[Language] = &[
            Language::Rust,
            Language::Java,
            Language::Kotlin,
            Language::Scala,
            Language::CSharp,
            Language::TypeScript,
            Language::Tsx,
            Language::JavaScript,
            Language::Jsx,
            Language::Dart,
            Language::C,
            Language::Cpp,
            Language::Python,
            Language::Php,
        ];
        let conscious_empty: &[Language] = &[
            Language::Go,
            Language::Ruby,
            Language::Bash,
            Language::Lua,
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
        ];
        let empty_match = QueryMatch {
            captures: Vec::new(),
            pattern_index: 0,
            index: 0,
        };
        for language in Language::ast_parsable_languages() {
            assert!(
                with_extractor.contains(language) || conscious_empty.contains(language),
                "{language:?} is parsable but has no modifier decision: \\\n                 add an extractor arm or a conscious-empty entry"
            );
            // Every arm must reset stale modifiers, never leak them.
            let mut entity = empty_entity();
            extract_modifiers(&empty_match, &mut entity, language);
            assert!(
                entity.modifiers.is_empty(),
                "{language:?} leaked modifiers from an empty match"
            );
        }
    }
}
