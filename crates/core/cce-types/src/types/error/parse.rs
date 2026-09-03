//! Parse domain error types
//!
//! This module defines error types related to parsing operations across the codebase.

use super::common::IoError;
use thiserror::Error;

/// Parse error type for domain-specific parsing operations
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    /// IO error - uses common IoError
    #[error("{0}")]
    Io(#[from] IoError),

    /// Language detection error
    #[error("Language detection failed: {0}")]
    LanguageDetection(String),

    /// AST parsing error
    #[error("AST parsing failed: {0}")]
    AstParsing(String),

    /// Code splitting error
    #[error("Code splitting failed: {0}")]
    CodeSplitting(String),

    /// Invalid file path
    #[error("Invalid file path: {0}")]
    InvalidFilePath(String),

    /// Unsupported language
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    /// Regular expression compilation error
    #[error("Failed to compile regex: {0}")]
    RegexCompilation(String),

    /// JSON parsing error
    #[error("JSON parsing failed: {0}")]
    JsonParsing(String),

    /// XML parsing error
    #[error("XML parsing failed: {0}")]
    XmlParsing(String),

    /// TOML parsing error
    #[error("TOML parsing failed: {0}")]
    TomlParsing(String),

    /// YAML parsing error
    #[error("YAML parsing failed: {0}")]
    YamlParsing(String),
}

impl ParseError {
    /// Create a language detection error
    pub fn language_detection(reason: impl Into<String>) -> Self {
        Self::LanguageDetection(reason.into())
    }

    /// Create an AST parsing error
    pub fn ast_parsing(reason: impl Into<String>) -> Self {
        Self::AstParsing(reason.into())
    }

    /// Create a code splitting error
    pub fn code_splitting(reason: impl Into<String>) -> Self {
        Self::CodeSplitting(reason.into())
    }

    /// Create an invalid file path error
    pub fn invalid_path(path: impl Into<String>) -> Self {
        Self::InvalidFilePath(path.into())
    }

    /// Create an unsupported language error
    pub fn unsupported_language(lang: impl Into<String>) -> Self {
        Self::UnsupportedLanguage(lang.into())
    }

    /// Create a regex compilation error
    pub fn regex_compilation(reason: impl Into<String>) -> Self {
        Self::RegexCompilation(reason.into())
    }

    /// Create a JSON parsing error
    pub fn json(reason: impl Into<String>) -> Self {
        Self::JsonParsing(reason.into())
    }

    /// Create an XML parsing error
    pub fn xml(reason: impl Into<String>) -> Self {
        Self::XmlParsing(reason.into())
    }

    /// Create a TOML parsing error
    pub fn toml(reason: impl Into<String>) -> Self {
        Self::TomlParsing(reason.into())
    }

    /// Create a YAML parsing error
    pub fn yaml(reason: impl Into<String>) -> Self {
        Self::YamlParsing(reason.into())
    }

    /// Get error code for programmatic error handling
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Io(_) => "PARSE_IO_ERROR",
            Self::LanguageDetection(_) => "PARSE_LANGUAGE_DETECTION_ERROR",
            Self::AstParsing(_) => "PARSE_AST_PARSING_ERROR",
            Self::CodeSplitting(_) => "PARSE_CODE_SPLITTING_ERROR",
            Self::InvalidFilePath(_) => "PARSE_INVALID_FILE_PATH_ERROR",
            Self::UnsupportedLanguage(_) => "PARSE_UNSUPPORTED_LANGUAGE_ERROR",
            Self::RegexCompilation(_) => "PARSE_REGEX_COMPILATION_ERROR",
            Self::JsonParsing(_) => "PARSE_JSON_PARSING_ERROR",
            Self::XmlParsing(_) => "PARSE_XML_PARSING_ERROR",
            Self::TomlParsing(_) => "PARSE_TOML_PARSING_ERROR",
            Self::YamlParsing(_) => "PARSE_YAML_PARSING_ERROR",
        }
    }
}

// Implement From<std::io::Error> for ParseError via IoError
impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(IoError::from(err))
    }
}
