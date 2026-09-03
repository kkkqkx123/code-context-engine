//! AST diagnosis implementation
//!
//! Main diagnosis logic that coordinates parsing, error collection, and refinement.

use cce_parser::parser::ast_parser::AstParser;
use cce_types::language::{Language, LanguageInfo};

use super::error_collector::ErrorCollector;
use super::strategies;
use super::{DiagnosisError, DiagnosisRequest, DiagnosisResponse, Result};

/// AST diagnosis handler
///
/// Provides syntax error detection based on tree-sitter parsing.
pub struct AstDiagnosis {
    /// AST parser instance
    parser: AstParser,
}

impl Default for AstDiagnosis {
    fn default() -> Self {
        Self::new()
    }
}

impl AstDiagnosis {
    /// Create a new AST diagnosis instance
    pub fn new() -> Self {
        Self {
            parser: AstParser::new(),
        }
    }

    /// Perform diagnosis on the given request
    ///
    /// # Arguments
    /// * `request` - Diagnosis request containing code and optional language/file info
    ///
    /// # Returns
    /// * `Result<DiagnosisResponse>` - Diagnosis result with validity and diagnostics
    pub fn diagnose(&mut self, request: DiagnosisRequest) -> Result<DiagnosisResponse> {
        // Step 1: Detect language
        let language = self.detect_language(&request)?;

        // Step 2: Check if language is supported
        if !AstParser::is_supported(&language) {
            return Err(DiagnosisError::UnsupportedLanguage(format!(
                "Language '{}' is not supported for AST diagnosis",
                language
            )));
        }

        // Step 3: Parse with tree-sitter
        let (tree, ast) = self
            .parser
            .parse_with_tree(&request.code, &language)
            .map_err(|e| DiagnosisError::ParseError(format!("Failed to parse code: {}", e)))?;

        // Step 4: Collect and process ERROR nodes
        let error_candidates = ErrorCollector::collect_and_process(&tree, &request.code);

        // Step 5: Apply language-specific strategy for refinement (static dispatch)
        let diagnostics =
            strategies::refine_diagnostics(&language, error_candidates, &request.code);

        // Step 6: Build response
        let is_valid = diagnostics.is_empty();

        Ok(DiagnosisResponse {
            language: language.to_string(),
            is_valid,
            ast: if request.include_ast { Some(ast) } else { None },
            diagnostics,
        })
    }

    /// Detect language from request
    fn detect_language(&self, request: &DiagnosisRequest) -> Result<Language> {
        // If language is explicitly specified, use it
        if let Some(ref lang) = request.language {
            return Ok(*lang);
        }

        // Try to detect from file name
        if let Some(ref file_name) = request.file_name {
            let info = LanguageInfo::detect_from_path(file_name);
            if info.language != Language::Unknown {
                return Ok(info.language);
            }
        }

        // Default to Unknown (will be rejected later)
        Err(DiagnosisError::LanguageDetection(
            "Cannot detect language. Please specify language or file_name.".to_string(),
        ))
    }

    /// Quick check if code is valid (without detailed diagnostics)
    ///
    /// This is a faster alternative when you only need to know if the code is valid.
    pub fn is_valid(&mut self, code: &str, language: &Language) -> Result<bool> {
        if !AstParser::is_supported(language) {
            return Err(DiagnosisError::UnsupportedLanguage(format!(
                "Language '{}' is not supported for AST diagnosis",
                language
            )));
        }

        let (tree, _) = self
            .parser
            .parse_with_tree(code, language)
            .map_err(|e| DiagnosisError::ParseError(format!("Failed to parse code: {}", e)))?;

        // Check if root has any ERROR children
        let root = tree.root_node();
        let has_error = Self::has_error_node(root);

        Ok(!has_error)
    }

    /// Check if a node or its descendants contain ERROR nodes
    fn has_error_node(node: tree_sitter::Node) -> bool {
        if node.kind() == "ERROR" || node.is_missing() {
            return true;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if Self::has_error_node(child) {
                return true;
            }
        }

        false
    }

    /// Get supported languages for AST diagnosis
    pub fn supported_languages() -> Vec<Language> {
        vec![
            Language::C,
            Language::Cpp,
            Language::CSharp,
            Language::JavaScript,
            Language::TypeScript,
            Language::Rust,
            Language::Go,
            Language::Java,
            Language::Python,
        ]
    }

    /// Check if a language is supported
    pub fn is_language_supported(language: &Language) -> bool {
        AstParser::is_supported(language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_languages() {
        let languages = AstDiagnosis::supported_languages();
        assert!(languages.contains(&Language::C));
        assert!(languages.contains(&Language::Cpp));
        assert!(languages.contains(&Language::Rust));
        assert!(languages.contains(&Language::Python));
        assert!(languages.contains(&Language::JavaScript));
    }

    #[test]
    fn test_is_language_supported() {
        assert!(AstDiagnosis::is_language_supported(&Language::C));
        assert!(AstDiagnosis::is_language_supported(&Language::Rust));
        assert!(!AstDiagnosis::is_language_supported(&Language::Unknown));
    }

    #[test]
    fn test_diagnose_valid_c_code() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("int main() { return 0; }").with_language(Language::C);

        let response = diagnosis.diagnose(request).expect("Diagnosis failed");
        assert!(response.is_valid);
        assert!(response.diagnostics.is_empty());
    }

    #[test]
    fn test_diagnose_valid_rust_code() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("fn main() {}").with_language(Language::Rust);

        let response = diagnosis.diagnose(request).expect("Diagnosis failed");
        assert!(response.is_valid);
        assert!(response.diagnostics.is_empty());
    }

    #[test]
    fn test_diagnose_valid_python_code() {
        let mut diagnosis = AstDiagnosis::new();
        let request =
            DiagnosisRequest::new("def hello():\n    pass").with_language(Language::Python);

        let response = diagnosis.diagnose(request).expect("Diagnosis failed");
        assert!(response.is_valid);
        assert!(response.diagnostics.is_empty());
    }

    #[test]
    fn test_diagnose_missing_semicolon_c() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("int x = 1").with_language(Language::C);

        let response = diagnosis.diagnose(request).expect("Diagnosis failed");
        assert!(!response.is_valid);
        assert!(!response.diagnostics.is_empty());
    }

    #[test]
    fn test_diagnose_unclosed_brace_c() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("int main() {").with_language(Language::C);

        let response = diagnosis.diagnose(request).expect("Diagnosis failed");
        assert!(!response.is_valid);
        assert!(!response.diagnostics.is_empty());
    }

    #[test]
    fn test_diagnose_with_file_name_detection() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("fn main() {}").with_file_name("test.rs");

        let response = diagnosis.diagnose(request).expect("Diagnosis failed");
        assert_eq!(response.language, "Rust");
        assert!(response.is_valid);
    }

    #[test]
    fn test_diagnose_unsupported_language() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("some code").with_language(Language::Unknown);

        let result = diagnosis.diagnose(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_diagnose_no_language_detection() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("some code");

        let result = diagnosis.diagnose(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_valid_quick_check() {
        let mut diagnosis = AstDiagnosis::new();

        let valid = diagnosis.is_valid("int main() { return 0; }", &Language::C);
        assert!(valid.expect("Check failed"));

        let invalid = diagnosis.is_valid("int main() {", &Language::C);
        assert!(!invalid.expect("Check failed"));
    }

    #[test]
    fn test_diagnose_with_ast() {
        let mut diagnosis = AstDiagnosis::new();
        let request = DiagnosisRequest::new("fn main() {}")
            .with_language(Language::Rust)
            .with_ast(true);

        let response = diagnosis.diagnose(request).expect("Diagnosis failed");
        assert!(response.ast.is_some());

        let ast = response.ast.expect("AST should be present");
        assert!(!ast.children.is_empty());
    }
}
