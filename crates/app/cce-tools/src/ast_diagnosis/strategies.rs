//! Diagnosis strategies for different languages
//!
//! This module provides language-specific refinement strategies for error diagnosis.
//! Uses static dispatch for zero-cost abstraction.

mod c_family;
mod common;
mod javascript;
mod python;
mod rust;

use cce_types::language::Language;

use super::{Diagnostic, ErrorCandidate};

/// Refine error candidates and generate diagnostics for a specific language
///
/// This function uses static dispatch to select the appropriate strategy
/// based on the language, avoiding dynamic dispatch overhead.
///
/// # Arguments
/// * `language` - The programming language
/// * `candidates` - Error candidates from tree-sitter parsing
/// * `source` - Original source code
///
/// # Returns
/// * `Vec<Diagnostic>` - Refined diagnostic information
pub fn refine_diagnostics(
    language: &Language,
    candidates: Vec<ErrorCandidate>,
    source: &str,
) -> Vec<Diagnostic> {
    match language {
        Language::C | Language::Cpp | Language::Java | Language::CSharp => {
            c_family::refine(candidates, source)
        }
        Language::Rust => rust::refine(candidates, source),
        Language::Python => python::refine(candidates, source),
        Language::JavaScript | Language::TypeScript => {
            javascript::refine(candidates, source, language)
        }
        _ => common::refine(candidates, source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refine_diagnostics_c() {
        let candidates = vec![];
        let source = "int main() { return 0; }";
        let diagnostics = refine_diagnostics(&Language::C, candidates, source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_refine_diagnostics_rust() {
        let candidates = vec![];
        let source = "fn main() {}";
        let diagnostics = refine_diagnostics(&Language::Rust, candidates, source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_refine_diagnostics_python() {
        let candidates = vec![];
        let source = "def hello():\n    pass";
        let diagnostics = refine_diagnostics(&Language::Python, candidates, source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_refine_diagnostics_javascript() {
        let candidates = vec![];
        let source = "function hello() { console.log('hello'); }";
        let diagnostics = refine_diagnostics(&Language::JavaScript, candidates, source);
        assert!(diagnostics.is_empty());
    }
}
