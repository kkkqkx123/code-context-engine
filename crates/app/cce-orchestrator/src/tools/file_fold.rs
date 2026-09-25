//! Stateless file fold tool for symbol skeleton extraction
//!
//! Sister tool to AST diagnosis: pure computation, no side effects, no index
//! access, no project context. Callers pass raw text plus language hints plus
//! a token budget and receive a lossy skeleton plus token accounting.
//!
//! All failure modes degrade instead of erroring: unknown language, parse
//! failure, empty input and over-limit input return a truncated text with
//! `structure_known=false`.

use serde::{Deserialize, Serialize};

use cce_parser::parser::coordinator::ParseCoordinator;
use cce_parser::summary::{FileFolder, FoldMode as ParserFoldMode};
use cce_types::language::{Language, LanguageInfo};
use cce_utils::token_estimation::estimate_tokens;

/// Default fold budget when the caller omits `max_tokens`.
pub const DEFAULT_FOLD_MAX_TOKENS: usize = 2000;
/// Hard ceiling for caller-requested budgets.
pub const MAX_FOLD_MAX_TOKENS: usize = 8000;
/// Inputs larger than this skip parsing and degrade directly.
pub const MAX_FOLD_TEXT_BYTES: usize = 200_000;

/// Fold detail level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FileFoldMode {
    /// Include full signatures.
    #[serde(rename = "detailed")]
    #[default]
    Detailed,
    /// Names only.
    #[serde(rename = "minimal")]
    Minimal,
}

impl FileFoldMode {
    /// Parse a caller-supplied mode name, defaulting to detailed.
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("detailed").trim().to_lowercase().as_str() {
            "minimal" | "names" | "short" => Self::Minimal,
            _ => Self::Detailed,
        }
    }

    fn as_parser_mode(self) -> ParserFoldMode {
        match self {
            Self::Detailed => ParserFoldMode::Detailed,
            Self::Minimal => ParserFoldMode::Minimal,
        }
    }
}

/// Stateless fold request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFoldRequest {
    /// Raw source text to fold.
    pub text: String,
    /// Explicit language hint (highest priority).
    pub language: Option<Language>,
    /// File name hint for suffix inference.
    pub file_name: Option<String>,
    /// Caller token budget for the folded text.
    pub max_tokens: Option<usize>,
    /// Fold detail level.
    pub mode: Option<FileFoldMode>,
}

impl FileFoldRequest {
    /// Create a request carrying only raw text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            language: None,
            file_name: None,
            max_tokens: None,
            mode: None,
        }
    }

    /// Set the explicit language hint.
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = Some(language);
        self
    }

    /// Set the file name hint.
    pub fn with_file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    /// Set the caller token budget.
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the fold detail level.
    pub fn with_mode(mut self, mode: FileFoldMode) -> Self {
        self.mode = Some(mode);
        self
    }
}

/// Stateless fold response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFoldResponse {
    /// Folded skeleton text, or truncated source on degraded paths.
    pub folded_text: String,
    /// Language actually used for folding (`Unknown` on degraded paths).
    pub language: String,
    /// Whether the skeleton carries parsed structure.
    pub structure_known: bool,
    /// Token estimate before folding.
    pub original_tokens: usize,
    /// Token estimate after folding.
    pub folded_tokens: usize,
    /// Sections kept in the skeleton.
    pub kept_sections: usize,
    /// Sections dropped by the token budget.
    pub dropped_sections: usize,
}

/// Stateless file fold tool.
pub struct FileFoldTool;

impl FileFoldTool {
    /// Fold raw text into a symbol skeleton.
    ///
    /// Borrows the shared parse coordinator (same mutex discipline as the
    /// diagnosis tool) and never propagates errors: every degenerate input
    /// maps to a truncated response.
    pub fn fold(coordinator: &mut ParseCoordinator, request: FileFoldRequest) -> FileFoldResponse {
        let budget = request
            .max_tokens
            .unwrap_or(DEFAULT_FOLD_MAX_TOKENS)
            .clamp(1, MAX_FOLD_MAX_TOKENS);
        let mode = request.mode.unwrap_or_default();
        let original_tokens = estimate_tokens(&request.text);
        let language = Self::resolve_language(&request);

        if request.text.is_empty() {
            return Self::degraded(&request.text, &language, budget, original_tokens);
        }

        if request.text.len() > MAX_FOLD_TEXT_BYTES {
            return Self::degraded(&request.text, &language, budget, original_tokens);
        }

        if language == Language::Unknown {
            return Self::degraded(&request.text, &language, budget, original_tokens);
        }

        let synthetic_path = Self::synthetic_path(&request, &language);
        let base_info = LanguageInfo::detect_from_path(&synthetic_path);
        let language_info = LanguageInfo {
            language,
            ..base_info
        };

        let parsed = match coordinator.parse_with_language_info(
            &synthetic_path,
            &request.text,
            &language_info,
        ) {
            Ok(parsed) => parsed,
            Err(_) => {
                return Self::degraded(&request.text, &language, budget, original_tokens);
            }
        };

        let folded = FileFolder::new()
            .with_max_tokens(budget)
            .with_mode(mode.as_parser_mode())
            .fold(&parsed);

        if folded.is_empty() {
            return Self::degraded(&request.text, &language, budget, original_tokens);
        }

        FileFoldResponse {
            folded_text: folded.content,
            language: language.to_string(),
            structure_known: true,
            original_tokens,
            folded_tokens: folded.estimated_tokens,
            kept_sections: folded.sections.len(),
            dropped_sections: folded.sections_dropped,
        }
    }

    /// Resolve explicit language first, then file-name suffix, then unknown.
    fn resolve_language(request: &FileFoldRequest) -> Language {
        if let Some(language) = request.language {
            if language != Language::Unknown {
                return language;
            }
        }
        if let Some(ref file_name) = request.file_name {
            let info = LanguageInfo::detect_from_path(file_name);
            if info.language != Language::Unknown {
                return info.language;
            }
        }
        Language::Unknown
    }

    /// Build a synthetic path so the parse pipeline has a stable file name.
    fn synthetic_path(request: &FileFoldRequest, language: &Language) -> String {
        if let Some(ref file_name) = request.file_name {
            if !file_name.trim().is_empty() {
                return file_name.clone();
            }
        }
        match language.common_extensions().first() {
            Some(ext) => format!("fold.{ext}"),
            None => "fold.txt".to_string(),
        }
    }

    /// Build the truncated degrade response.
    fn degraded(
        text: &str,
        language: &Language,
        budget: usize,
        original_tokens: usize,
    ) -> FileFoldResponse {
        let truncated = truncate_to_budget(text, budget);
        let folded_tokens = estimate_tokens(&truncated);
        FileFoldResponse {
            folded_text: truncated,
            language: language.to_string(),
            structure_known: false,
            original_tokens,
            folded_tokens,
            kept_sections: 0,
            dropped_sections: 0,
        }
    }
}

/// Truncate text so its token estimate fits the budget.
fn truncate_to_budget(text: &str, budget: usize) -> String {
    if estimate_tokens(text) <= budget {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut low = 0;
    let mut high = chars.len();
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect();
        if estimate_tokens(&candidate) <= budget {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    chars[..low].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinator() -> ParseCoordinator {
        ParseCoordinator::new()
    }

    #[test]
    fn fold_extracts_rust_skeleton() {
        let mut coordinator = coordinator();
        let code = "pub struct User { pub name: String }\n\
            pub fn normalize_name(input: &str) -> String { input.trim().to_string() }\n\
            pub fn format_user(name: &str) -> String { name.to_string() }\n";
        let request = FileFoldRequest::new(code)
            .with_language(Language::Rust)
            .with_max_tokens(2000);
        let response = FileFoldTool::fold(&mut coordinator, request);
        assert!(response.structure_known);
        assert_eq!(response.language, "Rust");
        assert!(response.folded_text.contains("User"));
        assert!(response.folded_text.contains("normalize_name"));
        assert!(response.folded_tokens <= 2000);
        assert!(response.kept_sections > 0);
    }

    #[test]
    fn fold_infers_language_from_file_name() {
        let mut coordinator = coordinator();
        let code = "def hello():\n    pass\n";
        let request = FileFoldRequest::new(code).with_file_name("hello.py");
        let response = FileFoldTool::fold(&mut coordinator, request);
        assert!(response.structure_known);
        assert_eq!(response.language, "Python");
    }

    #[test]
    fn unknown_language_degrades_without_error() {
        let mut coordinator = coordinator();
        let request = FileFoldRequest::new("some opaque text with no grammar");
        let response = FileFoldTool::fold(&mut coordinator, request);
        assert!(!response.structure_known);
        assert!(!response.folded_text.is_empty());
    }

    #[test]
    fn empty_input_degrades() {
        let mut coordinator = coordinator();
        let request = FileFoldRequest::new("").with_language(Language::Rust);
        let response = FileFoldTool::fold(&mut coordinator, request);
        assert!(!response.structure_known);
        assert!(response.folded_text.is_empty());
        assert_eq!(response.original_tokens, 0);
        assert_eq!(response.folded_tokens, 0);
    }

    #[test]
    fn over_limit_input_stays_within_budget() {
        let mut coordinator = coordinator();
        let code = "fn f() {}\n".repeat(5000);
        let request = FileFoldRequest::new(code).with_max_tokens(200);
        let response = FileFoldTool::fold(&mut coordinator, request);
        assert!(response.folded_tokens <= 200);
        assert!(response.original_tokens > response.folded_tokens);
    }
}
