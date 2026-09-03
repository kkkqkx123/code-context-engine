//! Enhanced error context for detailed operation failure diagnostics
//!
//! Provides rich error information including:
//! - Operation phase and context
//! - File and module information
//! - Retry count and suggested recovery actions

use super::context::OperationPhase;

/// Detailed error context for operation failures
#[derive(Debug, Clone)]
pub struct OperationErrorContext {
    /// Operation identifier
    pub operation_id: String,
    /// Operation type
    pub operation_type: String,
    /// Phase where error occurred
    pub phase: OperationPhase,
    /// File path if applicable
    pub file_path: Option<String>,
    /// Module name if applicable
    pub module: Option<String>,
    /// Error message
    pub error_message: String,
    /// Number of retries attempted
    pub retry_count: Option<u32>,
    /// Suggested recovery action
    pub suggested_recovery: String,
    /// Additional context information
    pub context: Vec<(String, String)>,
}

impl OperationErrorContext {
    /// Create new error context
    pub fn new(
        operation_id: String,
        operation_type: String,
        phase: OperationPhase,
        error: String,
    ) -> Self {
        let suggested_recovery = match phase {
            OperationPhase::Active => {
                "Operation was interrupted. It will automatically resume from the last checkpoint on restart."
                    .to_string()
            }
            OperationPhase::Queued => {
                "Operation failed before starting. Please retry the operation.".to_string()
            }
            OperationPhase::Paused => {
                "Operation is paused. Resume it or retry from the last checkpoint.".to_string()
            }
            OperationPhase::Completed | OperationPhase::Failed => {
                "Operation has reached terminal state. Check logs for details.".to_string()
            }
        };

        Self {
            operation_id,
            operation_type,
            phase,
            file_path: None,
            module: None,
            error_message: error,
            retry_count: None,
            suggested_recovery,
            context: Vec::new(),
        }
    }

    /// Add file information to context
    pub fn with_file(mut self, file_path: String) -> Self {
        self.file_path = Some(file_path);
        self
    }

    /// Add module information to context
    pub fn with_module(mut self, module: String) -> Self {
        self.module = Some(module);
        self
    }

    /// Add retry count information
    pub fn with_retry_count(mut self, count: u32) -> Self {
        self.retry_count = Some(count);
        self
    }

    /// Add custom context information
    pub fn with_context(mut self, key: String, value: String) -> Self {
        self.context.push((key, value));
        self
    }

    /// Add multiple context pairs
    pub fn with_contexts(mut self, contexts: Vec<(String, String)>) -> Self {
        self.context.extend(contexts);
        self
    }

    /// Format as detailed error report
    pub fn format_detailed(&self) -> String {
        let mut report = String::new();
        report.push_str("╭──── Operation Error ────────────────────────────────\n");
        report.push_str(&format!(
            "│ Operation: {} ({})\n",
            self.operation_id, self.operation_type
        ));
        report.push_str(&format!("│ Phase: {:?}\n", self.phase));

        if let Some(file) = &self.file_path {
            report.push_str(&format!("│ File: {}\n", file));
        }

        if let Some(module) = &self.module {
            report.push_str(&format!("│ Module: {}\n", module));
        }

        if let Some(retry) = self.retry_count {
            report.push_str(&format!("│ Retries: {}\n", retry));
        }

        report.push_str("│\n");
        report.push_str(&format!("│ Error: {}\n", self.error_message));

        if !self.context.is_empty() {
            report.push_str("│\n");
            report.push_str("│ Context:\n");
            for (key, value) in &self.context {
                report.push_str(&format!("│   {}: {}\n", key, value));
            }
        }

        report.push_str("│\n");
        report.push_str(&format!(
            "│ Suggested Recovery: {}\n",
            self.suggested_recovery
        ));
        report.push_str("╰────────────────────────────────────────────────────\n");

        report
    }

    /// Format as compact error summary
    pub fn format_compact(&self) -> String {
        let mut summary = format!(
            "Error in {} [{}]: {}",
            self.operation_id, self.operation_type, self.error_message
        );

        if let Some(file) = &self.file_path {
            summary.push_str(&format!(" (file: {})", file));
        }

        if let Some(module) = &self.module {
            summary.push_str(&format!(" [{}]", module));
        }

        if let Some(retry) = self.retry_count {
            summary.push_str(&format!(" retry#{}", retry));
        }

        summary
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self.phase,
            OperationPhase::Queued | OperationPhase::Active | OperationPhase::Paused
        )
    }
}

/// Error classification for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// File system related errors (IO, permissions, encoding)
    FileSystem,
    /// Database related errors (storage, persistence)
    Database,
    /// Parsing or processing errors
    Processing,
    /// Configuration or validation errors
    Configuration,
    /// Retry exhaustion or timeout
    Timeout,
    /// Unknown or unexpected errors
    Unknown,
}

impl ErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileSystem => "FileSystem",
            Self::Database => "Database",
            Self::Processing => "Processing",
            Self::Configuration => "Configuration",
            Self::Timeout => "Timeout",
            Self::Unknown => "Unknown",
        }
    }

    /// Classify error by message pattern
    pub fn from_message(msg: &str) -> Self {
        let msg_lower = msg.to_lowercase();
        let words: Vec<&str> = msg_lower.split(|c: char| !c.is_alphanumeric()).collect();

        // Check database first (more specific)
        for word in &words {
            if word.contains("database") || word.contains("sqlite") || word.contains("storage") {
                return Self::Database;
            }
        }

        // Then file system
        for word in &words {
            if word.contains("file") || word.contains("path") || *word == "io" {
                return Self::FileSystem;
            }
        }

        // Then processing
        for word in &words {
            if word.contains("parse") || word.contains("process") {
                return Self::Processing;
            }
        }

        // Configuration
        for word in &words {
            if word.contains("config") || word.contains("validation") {
                return Self::Configuration;
            }
        }

        // Timeout
        for word in &words {
            if word.contains("timeout") {
                return Self::Timeout;
            }
        }

        // Default
        Self::Unknown
    }
}

/// Recovery suggestion based on error category
#[derive(Debug, Clone)]
pub struct RecoverySuggestion {
    /// Error category
    pub category: ErrorCategory,
    /// Suggested action
    pub action: String,
    /// Priority level (1=highest, 3=lowest)
    pub priority: u8,
    /// Should retry immediately
    pub should_retry_immediately: bool,
    /// Suggested delay before retry in seconds
    pub retry_delay_secs: Option<u64>,
}

impl RecoverySuggestion {
    /// Generate suggestion for error category
    pub fn for_category(category: ErrorCategory) -> Self {
        match category {
            ErrorCategory::FileSystem => Self {
                category,
                action: "Check file system permissions and disk space. Retry the operation.".to_string(),
                priority: 1,
                should_retry_immediately: false,
                retry_delay_secs: Some(5),
            },
            ErrorCategory::Database => Self {
                category,
                action: "Check database connection and integrity. Restart the application.".to_string(),
                priority: 1,
                should_retry_immediately: false,
                retry_delay_secs: Some(10),
            },
            ErrorCategory::Processing => Self {
                category,
                action: "Review error logs and retry the operation. Contact support if issue persists.".to_string(),
                priority: 2,
                should_retry_immediately: true,
                retry_delay_secs: None,
            },
            ErrorCategory::Configuration => Self {
                category,
                action: "Fix configuration errors and restart the application.".to_string(),
                priority: 1,
                should_retry_immediately: false,
                retry_delay_secs: None,
            },
            ErrorCategory::Timeout => Self {
                category,
                action: "Increase timeout limits or check system resources. Retry with extended timeout.".to_string(),
                priority: 2,
                should_retry_immediately: false,
                retry_delay_secs: Some(30),
            },
            ErrorCategory::Unknown => Self {
                category,
                action: "Review detailed error logs. Contact support for assistance.".to_string(),
                priority: 3,
                should_retry_immediately: false,
                retry_delay_secs: Some(60),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_creation() {
        let ctx = OperationErrorContext::new(
            "op1".to_string(),
            "full_index".to_string(),
            OperationPhase::Active,
            "IO error".to_string(),
        );

        assert_eq!(ctx.operation_id, "op1");
        assert_eq!(ctx.operation_type, "full_index");
        assert_eq!(ctx.phase, OperationPhase::Active);
        assert!(ctx.is_recoverable());
    }

    #[test]
    fn test_error_context_with_file() {
        let ctx = OperationErrorContext::new(
            "op1".to_string(),
            "full_index".to_string(),
            OperationPhase::Active,
            "error".to_string(),
        )
        .with_file("/path/to/file.rs".to_string());

        assert_eq!(ctx.file_path, Some("/path/to/file.rs".to_string()));
    }

    #[test]
    fn test_error_category_classification() {
        let cat1 = ErrorCategory::from_message("Failed to open file");
        assert_eq!(cat1, ErrorCategory::FileSystem);

        let cat2 = ErrorCategory::from_message("Database connection error");
        assert_eq!(cat2, ErrorCategory::Database);

        let cat3 = ErrorCategory::from_message("Operation timeout");
        assert_eq!(cat3, ErrorCategory::Timeout);
    }

    #[test]
    fn test_recovery_suggestion() {
        let suggestion = RecoverySuggestion::for_category(ErrorCategory::Timeout);
        assert_eq!(suggestion.category, ErrorCategory::Timeout);
        assert_eq!(suggestion.retry_delay_secs, Some(30));
    }

    #[test]
    fn test_error_context_format() {
        let ctx = OperationErrorContext::new(
            "op1".to_string(),
            "full_index".to_string(),
            OperationPhase::Active,
            "Test error".to_string(),
        );

        let detailed = ctx.format_detailed();
        assert!(detailed.contains("op1"));
        assert!(detailed.contains("Test error"));

        let compact = ctx.format_compact();
        assert!(compact.contains("op1"));
        assert!(compact.contains("full_index"));
    }
}
