//! Parse domain error types
//!
//! This module defines error types related to parsing operations across the codebase.

use super::common::{ErrorClassify, IoError};
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

    /// Content changed between the scan phase and processing (stale snapshot)
    #[error("Content changed since scan: {0}")]
    ContentChanged(String),

    /// Content could not be decoded with the detected encoding
    #[error("Encoding detection failed: {0}")]
    Encoding(String),
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

    /// Create a content-drift error
    pub fn content_changed(reason: impl Into<String>) -> Self {
        Self::ContentChanged(reason.into())
    }

    /// Create an encoding detection error
    pub fn encoding(reason: impl Into<String>) -> Self {
        Self::Encoding(reason.into())
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
            Self::ContentChanged(_) => "PARSE_CONTENT_CHANGED_ERROR",
            Self::Encoding(_) => "PARSE_ENCODING_ERROR",
        }
    }
}

impl ErrorClassify for ParseError {
    fn is_retryable(&self) -> bool {
        self.is_transient()
    }

    fn is_transient(&self) -> bool {
        match self {
            // IO outcome depends on the failure kind (interruptions and
            // resource pressure may succeed on retry).
            Self::Io(err) => err.is_transient(),
            // Every other variant is deterministic for the same file content
            // (unsupported language, bad path, malformed document, splitter
            // bug, undecodable bytes); retrying without a re-scan never helps.
            _ => false,
        }
    }

    fn is_permanent(&self) -> bool {
        !self.is_transient()
    }
}

// Implement From<std::io::Error> for ParseError via IoError
impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(IoError::from(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn io_error(kind: std::io::ErrorKind) -> ParseError {
        ParseError::Io(IoError::from(std::io::Error::new(kind, "test")))
    }

    #[test]
    fn content_deterministic_variants_are_permanent() {
        let cases = [
            ParseError::unsupported_language("foo"),
            ParseError::language_detection("ambiguous"),
            ParseError::invalid_path("/x"),
            ParseError::ast_parsing("boom"),
            ParseError::code_splitting("boom"),
            ParseError::regex_compilation("boom"),
            ParseError::json("bad json"),
            ParseError::xml("bad xml"),
            ParseError::toml("bad toml"),
            ParseError::yaml("bad yaml"),
            ParseError::content_changed("drifted"),
            ParseError::encoding("undecodable"),
            io_error(std::io::ErrorKind::NotFound),
            io_error(std::io::ErrorKind::PermissionDenied),
            io_error(std::io::ErrorKind::InvalidData),
        ];
        for err in cases {
            assert!(!err.is_retryable(), "{err} must not be retryable");
            assert!(err.is_permanent(), "{err} must be permanent");
        }
    }

    #[test]
    fn transient_io_kinds_are_retryable() {
        let cases = [
            io_error(std::io::ErrorKind::TimedOut),
            io_error(std::io::ErrorKind::Interrupted),
            io_error(std::io::ErrorKind::WouldBlock),
            io_error(std::io::ErrorKind::ResourceBusy),
            io_error(std::io::ErrorKind::ConnectionReset),
            io_error(std::io::ErrorKind::Other),
        ];
        for err in cases {
            assert!(err.is_transient(), "{err} must be transient");
            assert!(!err.is_permanent(), "{err} must not be permanent");
        }
    }
}
