//! AST diagnosis module for syntax error detection
//!
//! This module provides syntax error detection based on tree-sitter parsing results.
//! It can locate code format issues such as unclosed brackets, unclosed strings,
//! missing semicolons, etc.
//!
//! # Overview
//!
//! The diagnosis process:
//! 1. Language detection (from file extension or explicit specification)
//! 2. AST parsing using tree-sitter
//! 3. ERROR node collection and filtering
//! 4. Language-specific refinement (optional)
//! 5. Diagnostic information generation
//!
//! # Example
//!
//! ```ignore
//! use code_context_engine::orchestrator::tools::ast_diagnosis::{AstDiagnosis, DiagnosisRequest};
//! use code_context_engine::types::language::Language;
//!
//! let mut diagnosis = AstDiagnosis::new();
//! let request = DiagnosisRequest {
//!     code: "int x = 1".to_string(),
//!     language: Some(Language::C),
//!     file_name: None,
//!     include_ast: false,
//! };
//!
//! // let response = diagnosis.diagnose(request)?;
//! // println!("Is valid: {}", response.is_valid);
//! ```

mod diagnosis;
mod error_collector;
mod strategies;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use cce_parser::parser::ast_parser::AstNode;
use cce_types::language::Language;
use cce_types::position::{Position, Span};

pub use diagnosis::AstDiagnosis;
pub use error_collector::{ErrorCandidate, ErrorCandidateKind};

/// AST diagnosis error type
#[derive(Error, Debug)]
pub enum DiagnosisError {
    /// Language detection error
    #[error("Language detection error: {0}")]
    LanguageDetection(String),

    /// Parse error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Unsupported language
    #[error("Unsupported language for AST diagnosis: {0}")]
    UnsupportedLanguage(String),
}

/// Result type for diagnosis operations
pub type Result<T> = std::result::Result<T, DiagnosisError>;

/// AST diagnosis request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisRequest {
    /// Code content to diagnose
    pub code: String,

    /// Programming language (optional, auto-detected from file_name if not specified)
    pub language: Option<Language>,

    /// File name (optional, used for language detection and error messages)
    pub file_name: Option<String>,

    /// Whether to include AST structure in response
    pub include_ast: bool,
}

impl DiagnosisRequest {
    /// Create a new diagnosis request with code only
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            language: None,
            file_name: None,
            include_ast: false,
        }
    }

    /// Specify the programming language
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = Some(language);
        self
    }

    /// Specify the file name for language detection
    pub fn with_file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    /// Include AST structure in the response
    pub fn with_ast(mut self, include: bool) -> Self {
        self.include_ast = include;
        self
    }
}

/// AST diagnosis response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisResponse {
    /// Detected or specified programming language
    pub language: String,

    /// Whether the code is valid (no syntax errors)
    pub is_valid: bool,

    /// AST structure (optional, only when include_ast=true)
    pub ast: Option<AstNode>,

    /// Diagnostic issues (only present when errors exist)
    pub diagnostics: Vec<Diagnostic>,
}

/// Diagnostic issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Issue type
    pub kind: DiagnosticKind,

    /// Error position (start)
    pub position: Position,

    /// Error span (optional, for range information)
    pub span: Option<Span>,

    /// Error message
    pub message: String,

    /// Positioning precision
    pub precision: DiagnosticPrecision,
}

impl Diagnostic {
    /// Create a new diagnostic
    pub fn new(
        kind: DiagnosticKind,
        position: Position,
        message: impl Into<String>,
        precision: DiagnosticPrecision,
    ) -> Self {
        Self {
            kind,
            position,
            span: None,
            message: message.into(),
            precision,
        }
    }

    /// Add span information
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

/// Diagnostic issue type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticKind {
    /// Missing semicolon
    MissingSemicolon,

    /// Unclosed string literal
    UnclosedString,

    /// Unclosed parenthesis
    UnclosedParenthesis,

    /// Unclosed brace
    UnclosedBrace,

    /// Unclosed bracket
    UnclosedBracket,

    /// Incomplete expression
    IncompleteExpression,

    /// Illegal token
    IllegalToken,

    /// Incomplete declaration
    IncompleteDeclaration,

    /// Preprocessor directive error
    PreprocessorError,

    /// Indentation error (Python-specific)
    IndentationError,

    /// Other syntax error
    SyntaxError,
}

impl std::fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticKind::MissingSemicolon => write!(f, "missing semicolon"),
            DiagnosticKind::UnclosedString => write!(f, "unclosed string"),
            DiagnosticKind::UnclosedParenthesis => write!(f, "unclosed parenthesis"),
            DiagnosticKind::UnclosedBrace => write!(f, "unclosed brace"),
            DiagnosticKind::UnclosedBracket => write!(f, "unclosed bracket"),
            DiagnosticKind::IncompleteExpression => write!(f, "incomplete expression"),
            DiagnosticKind::IllegalToken => write!(f, "illegal token"),
            DiagnosticKind::IncompleteDeclaration => write!(f, "incomplete declaration"),
            DiagnosticKind::PreprocessorError => write!(f, "preprocessor error"),
            DiagnosticKind::IndentationError => write!(f, "indentation error"),
            DiagnosticKind::SyntaxError => write!(f, "syntax error"),
        }
    }
}

/// Positioning precision level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticPrecision {
    /// High precision: ERROR node covers 1-2 tokens
    High,

    /// Medium precision: ERROR node covers larger range, needs inference
    Medium,

    /// Low precision: May not produce ERROR node
    Low,
}

impl std::fmt::Display for DiagnosticPrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticPrecision::High => write!(f, "high"),
            DiagnosticPrecision::Medium => write!(f, "medium"),
            DiagnosticPrecision::Low => write!(f, "low"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnosis_request_builder() {
        let request = DiagnosisRequest::new("int x = 1;")
            .with_language(Language::C)
            .with_file_name("test.c")
            .with_ast(true);

        assert_eq!(request.code, "int x = 1;");
        assert_eq!(request.language, Some(Language::C));
        assert_eq!(request.file_name, Some("test.c".to_string()));
        assert!(request.include_ast);
    }

    #[test]
    fn test_diagnostic_creation() {
        let diagnostic = Diagnostic::new(
            DiagnosticKind::MissingSemicolon,
            Position::new(0, 10),
            "expected ';' after expression",
            DiagnosticPrecision::High,
        );

        assert_eq!(diagnostic.kind, DiagnosticKind::MissingSemicolon);
        assert_eq!(diagnostic.position.row, 0);
        assert_eq!(diagnostic.position.column, 10);
        assert!(diagnostic.span.is_none());
    }

    #[test]
    fn test_diagnostic_with_span() {
        let span = Span::new(0, 10, 0, 0, 0, 10);
        let diagnostic = Diagnostic::new(
            DiagnosticKind::UnclosedString,
            Position::new(0, 5),
            "unclosed string literal",
            DiagnosticPrecision::High,
        )
        .with_span(span);

        assert!(diagnostic.span.is_some());
        assert_eq!(diagnostic.span.as_ref().map(|s| s.len()), Some(10));
    }

    #[test]
    fn test_diagnostic_kind_display() {
        assert_eq!(
            DiagnosticKind::MissingSemicolon.to_string(),
            "missing semicolon"
        );
        assert_eq!(
            DiagnosticKind::UnclosedString.to_string(),
            "unclosed string"
        );
        assert_eq!(DiagnosticKind::SyntaxError.to_string(), "syntax error");
    }

    #[test]
    fn test_diagnostic_precision_display() {
        assert_eq!(DiagnosticPrecision::High.to_string(), "high");
        assert_eq!(DiagnosticPrecision::Medium.to_string(), "medium");
        assert_eq!(DiagnosticPrecision::Low.to_string(), "low");
    }
}
