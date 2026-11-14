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
use thiserror::Error;

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
/// Maximum entries accepted by a single batch fold request.
pub const MAX_FOLD_BATCH_ITEMS: usize = 32;
/// Maximum summed `text` bytes accepted by a single batch fold request.
pub const MAX_FOLD_BATCH_BYTES: usize = 2_000_000;

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

/// One entry of a batch fold request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFoldBatchItem {
    /// Caller-provided stable identifier, echoed verbatim in the result.
    pub id: String,
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

impl FileFoldBatchItem {
    /// Create an item carrying an id and raw text.
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
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

/// Batch fold request with optional global defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileFoldBatchRequest {
    /// Entries to fold; result order matches this order.
    pub items: Vec<FileFoldBatchItem>,
    /// Global language default for items without an explicit hint.
    pub language: Option<Language>,
    /// Global token budget default for items without an explicit budget.
    pub max_tokens: Option<usize>,
    /// Global fold mode default for items without an explicit mode.
    pub mode: Option<FileFoldMode>,
    /// Reserved for forward compatibility; the first version always runs sequentially.
    pub max_concurrency: Option<usize>,
}

impl FileFoldBatchRequest {
    /// Create a batch request from items.
    pub fn new(items: Vec<FileFoldBatchItem>) -> Self {
        Self {
            items,
            language: None,
            max_tokens: None,
            mode: None,
            max_concurrency: None,
        }
    }

    /// Set the global language default.
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = Some(language);
        self
    }

    /// Set the global token budget default.
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the global fold mode default.
    pub fn with_mode(mut self, mode: FileFoldMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Set the reserved concurrency hint (ignored, always sequential).
    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = Some(max_concurrency);
        self
    }
}

/// One entry of a batch fold response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFoldBatchItemResponse {
    /// Echo of the caller-provided identifier.
    pub id: String,
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

/// Aggregate accounting over a batch fold response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileFoldBatchStats {
    /// Summed `original_tokens` over all entries.
    pub total_original_tokens: usize,
    /// Summed `folded_tokens` over all entries.
    pub total_folded_tokens: usize,
    /// Entries folded with known structure.
    pub structure_known_count: usize,
}

/// Batch fold response: per-entry results plus aggregate stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFoldBatchResponse {
    /// Per-entry results in request order.
    pub results: Vec<FileFoldBatchItemResponse>,
    /// Aggregate token accounting for observation and budget checks.
    pub stats: FileFoldBatchStats,
}

/// Request-level batch fold failure.
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum FileFoldBatchError {
    /// Entry array is empty.
    #[error("fold batch items must not be empty")]
    Empty,
    /// Entry count exceeds the batch limit; split the request.
    #[error("fold batch holds {count} items, exceeding the limit of {max}; split the request")]
    TooManyItems { count: usize, max: usize },
    /// Summed text bytes exceed the batch limit; split the request.
    #[error("fold batch holds {bytes} text bytes, exceeding the limit of {max}; split the request")]
    TotalTooLarge { bytes: usize, max: usize },
}

/// Batch fold result type.
pub type FileFoldBatchResult<T> = std::result::Result<T, FileFoldBatchError>;

impl FileFoldTool {
    /// Fold a batch of texts sequentially.
    ///
    /// Each entry reuses the single-item fold logic with its effective
    /// options (explicit value first, then the batch global default), so
    /// per-entry semantics and degrade behavior match single folding.
    /// Request-level validation (empty, count, total size) rejects before
    /// any folding; entry-level issues always degrade in-band.
    pub fn fold_batch(request: FileFoldBatchRequest) -> FileFoldBatchResult<FileFoldBatchResponse> {
        if request.items.is_empty() {
            return Err(FileFoldBatchError::Empty);
        }
        if request.items.len() > MAX_FOLD_BATCH_ITEMS {
            return Err(FileFoldBatchError::TooManyItems {
                count: request.items.len(),
                max: MAX_FOLD_BATCH_ITEMS,
            });
        }
        let total_bytes: usize = request.items.iter().map(|item| item.text.len()).sum();
        if total_bytes > MAX_FOLD_BATCH_BYTES {
            return Err(FileFoldBatchError::TotalTooLarge {
                bytes: total_bytes,
                max: MAX_FOLD_BATCH_BYTES,
            });
        }

        let mut results = Vec::with_capacity(request.items.len());
        let mut stats = FileFoldBatchStats::default();
        for item in &request.items {
            let mut tool_request = FileFoldRequest::new(item.text.clone());
            if let Some(language) = item.language.or(request.language) {
                tool_request = tool_request.with_language(language);
            }
            if let Some(ref file_name) = item.file_name {
                tool_request = tool_request.with_file_name(file_name.clone());
            }
            if let Some(max_tokens) = item.max_tokens.or(request.max_tokens) {
                tool_request = tool_request.with_max_tokens(max_tokens);
            }
            if let Some(mode) = item.mode.or(request.mode) {
                tool_request = tool_request.with_mode(mode);
            }

            let mut coordinator = ParseCoordinator::new();
            let folded = Self::fold(&mut coordinator, tool_request);
            stats.total_original_tokens += folded.original_tokens;
            stats.total_folded_tokens += folded.folded_tokens;
            if folded.structure_known {
                stats.structure_known_count += 1;
            }
            results.push(FileFoldBatchItemResponse {
                id: item.id.clone(),
                folded_text: folded.folded_text,
                language: folded.language,
                structure_known: folded.structure_known,
                original_tokens: folded.original_tokens,
                folded_tokens: folded.folded_tokens,
                kept_sections: folded.kept_sections,
                dropped_sections: folded.dropped_sections,
            });
        }

        Ok(FileFoldBatchResponse { results, stats })
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

    #[test]
    fn batch_matches_sequential_single_results() {
        let rust_code = "pub struct User { pub name: String }\n\
            pub fn normalize_name(input: &str) -> String { input.trim().to_string() }\n";
        let python_code = "def hello():\n    pass\n";
        let request = FileFoldBatchRequest {
            items: vec![
                FileFoldBatchItem::new("msg-1", rust_code).with_language(Language::Rust),
                FileFoldBatchItem::new("msg-2", python_code)
                    .with_file_name("hello.py")
                    .with_max_tokens(500),
            ],
            language: None,
            max_tokens: Some(2000),
            mode: None,
            max_concurrency: None,
        };
        let batch = FileFoldTool::fold_batch(request).expect("batch must succeed");
        assert_eq!(batch.results.len(), 2);
        assert_eq!(batch.results[0].id, "msg-1");
        assert_eq!(batch.results[1].id, "msg-2");

        let mut first_coordinator = coordinator();
        let first = FileFoldTool::fold(
            &mut first_coordinator,
            FileFoldRequest::new(rust_code)
                .with_language(Language::Rust)
                .with_max_tokens(2000),
        );
        let mut second_coordinator = coordinator();
        let second = FileFoldTool::fold(
            &mut second_coordinator,
            FileFoldRequest::new(python_code)
                .with_file_name("hello.py")
                .with_max_tokens(500),
        );
        assert_eq!(batch.results[0].folded_text, first.folded_text);
        assert_eq!(batch.results[1].folded_text, second.folded_text);
        assert_eq!(
            batch.stats.total_original_tokens,
            first.original_tokens + second.original_tokens
        );
        assert_eq!(
            batch.stats.total_folded_tokens,
            first.folded_tokens + second.folded_tokens
        );
    }

    #[test]
    fn batch_degrades_per_entry_without_affecting_others() {
        let good = "pub fn format_user(name: &str) -> String { name.to_string() }\n";
        let request = FileFoldBatchRequest::new(vec![
            FileFoldBatchItem::new("good", good).with_language(Language::Rust),
            FileFoldBatchItem::new("unknown", "some opaque text with no grammar"),
            FileFoldBatchItem::new("empty", "").with_language(Language::Rust),
        ]);
        let batch = FileFoldTool::fold_batch(request).expect("batch must succeed");
        assert_eq!(batch.results.len(), 3);
        assert_eq!(
            batch
                .results
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["good", "unknown", "empty"]
        );
        assert!(batch.results[0].structure_known);
        assert!(!batch.results[1].structure_known);
        assert!(!batch.results[2].structure_known);
        assert_eq!(batch.stats.structure_known_count, 1);
    }

    #[test]
    fn batch_applies_global_defaults_with_explicit_override() {
        let code = "pub fn a() {}\n";
        let request = FileFoldBatchRequest {
            items: vec![
                FileFoldBatchItem::new("inherits", code),
                FileFoldBatchItem::new("overrides", code).with_mode(FileFoldMode::Minimal),
            ],
            language: Some(Language::Rust),
            max_tokens: Some(2000),
            mode: Some(FileFoldMode::Detailed),
            max_concurrency: None,
        };
        let batch = FileFoldTool::fold_batch(request).expect("batch must succeed");
        assert_eq!(batch.results.len(), 2);
        assert_eq!(batch.results[0].language, "Rust");
        assert_eq!(batch.results[1].language, "Rust");
        assert!(batch.results[0].structure_known);
        assert!(batch.results[1].structure_known);
    }

    #[test]
    fn batch_rejects_empty_and_oversized_requests() {
        assert!(matches!(
            FileFoldTool::fold_batch(FileFoldBatchRequest::new(Vec::new())),
            Err(FileFoldBatchError::Empty)
        ));

        let items = (0..MAX_FOLD_BATCH_ITEMS + 1)
            .map(|index| FileFoldBatchItem::new(format!("id-{index}"), "fn f() {}\n"))
            .collect();
        assert!(matches!(
            FileFoldTool::fold_batch(FileFoldBatchRequest::new(items)),
            Err(FileFoldBatchError::TooManyItems { .. })
        ));

        let oversized = "x".repeat(MAX_FOLD_BATCH_BYTES + 1);
        let request = FileFoldBatchRequest::new(vec![FileFoldBatchItem::new("big", oversized)]);
        assert!(matches!(
            FileFoldTool::fold_batch(request),
            Err(FileFoldBatchError::TotalTooLarge { .. })
        ));
    }

    #[test]
    fn batch_truncates_oversized_entry_in_band() {
        let code = "fn f() {}\n".repeat(5000);
        let request = FileFoldBatchRequest::new(vec![
            FileFoldBatchItem::new("big", code)
                .with_language(Language::Rust)
                .with_max_tokens(200),
        ]);
        let batch = FileFoldTool::fold_batch(request).expect("batch must succeed");
        assert_eq!(batch.results.len(), 1);
        assert!(batch.results[0].folded_tokens <= 200);
    }
}
