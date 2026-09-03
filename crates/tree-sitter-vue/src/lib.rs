//! This crate provides Vue grammar for the tree-sitter parsing library.

use tree_sitter::Language;

extern "C" {
    fn tree_sitter_vue() -> Language;
}

/// Get the tree-sitter [Language][] for Vue.
///
/// [Language]: https://docs.rs/tree-sitter/*/tree_sitter/struct.Language.html
pub fn language() -> Language {
    unsafe { tree_sitter_vue() }
}
